//! `bench-oracle` — the layered grader.
//!
//! P0 spine: L0 apply, L1 compile (with rustc-error-code extraction), and the
//! L2 unit sub-oracle. L2 property/differential and the L3/L4 layers arrive in
//! P2. Grading runs the real `cargo`/`rustc` toolchain in a materialised
//! workspace that is separate from anything the model ever saw — the oracle
//! files are injected only here (docs/03-oracle.md).
//!
//! Model-authored code runs under containment: every `cargo` invocation goes
//! through `bench-sandbox` (P1), which on macOS denies network and confines
//! writes to the workspace. The seam is deliberately narrow — one `run_cargo`
//! helper — so containment is applied in exactly one place.

pub mod ast;

use bench_core::{
    classify_error_codes, composite_score, BehaviorScore, ConstraintScore, FailureClass, Instance,
    OracleVector, OracleWeights,
};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace {0} is not an empty directory")]
    DirtyWorkspace(String),
}

/// Everything the oracle needs beyond the instance and the response. Grows one
/// field per layer as the oracle deepens.
pub struct GradeSpec<'a> {
    /// Where in the crate the model's answer is written, e.g. `src/lib.rs`.
    pub answer_path: &'a Path,
    pub weights: &'a OracleWeights,
    /// The `cargo test --test <name>` target for the L2 behaviour tests. `None`
    /// runs every test, which is fine when the task has no separate L3 target.
    pub behavior_test: Option<&'a str>,
    /// The `cargo test --test <name>` target for the L2 differential sub-oracle
    /// (candidate vs hidden reference over generated inputs). `None` skips it.
    pub differential_test: Option<&'a str>,
    /// The `cargo test --test <name>` target carrying the L3 allocation
    /// instrumentation. `None` skips the constraint layer.
    pub alloc_test: Option<&'a str>,
    /// Resource limits (harness-owned wall clock, rlimits) for every cargo call.
    pub limits: bench_sandbox::Limits,
    /// L3 AST constraint: maximum `unsafe` usages allowed. `None` = unchecked.
    pub max_unsafe: Option<u32>,
    /// L3 AST constraint: forbidden type/function paths (`RefCell`, `transmute`).
    pub forbidden_paths: Vec<String>,
    /// L3 constraint: run `cargo clippy --lib` on the answer and score its
    /// cleanliness (docs/03 — the idiomaticity signal). `false` skips it.
    pub check_clippy: bool,
    /// Clippy lints not counted against cleanliness, e.g.
    /// `"clippy::needless_range_loop"`.
    pub clippy_allow: Vec<String>,
}

/// Grade one model response for one instance.
///
/// `workspace` must be an existing empty directory; the oracle materialises the
/// crate there (skeleton files + the model's applied answer + hidden oracle
/// files), builds it, and tests it.
pub fn grade(
    instance: &Instance,
    response: &str,
    spec: &GradeSpec,
    workspace: &Path,
) -> Result<OracleVector, OracleError> {
    if workspace.read_dir()?.next().is_some() {
        return Err(OracleError::DirtyWorkspace(workspace.display().to_string()));
    }

    // ---- L0: apply ----
    let code = match extract_code(response) {
        Some(c) => c,
        None => return Ok(OracleVector::apply_failed()),
    };

    // Materialise the model's view (skeleton), overwrite the answer file with
    // the extracted code, then inject the hidden oracle files.
    for (rel, contents) in &instance.files {
        write_under(workspace, rel, contents)?;
    }
    write_under(workspace, spec.answer_path, &code)?;
    for (rel, contents) in &instance.hidden {
        write_under(workspace, rel, contents)?;
    }

    // Containment for every model-code execution below. Built once; the model
    // has already produced its response (unsandboxed HTTP), and nothing it
    // wrote runs until now.
    let policy = bench_sandbox::Policy::for_workspace(workspace)?.with_limits(spec.limits);
    let mut flags: Vec<String> = Vec::new();

    // ---- L1: compile ----
    let build = run_cargo(&policy, &["build", "--offline", "--message-format=json"])?;
    if build.timed_out {
        flags.push("timeout:build".into());
    }
    let (error_codes, warn_count) = parse_diagnostics(&build.stdout);
    let compile_ok = build.success;

    if !compile_ok {
        let failure_class = classify_error_codes(&error_codes);
        let mut v = OracleVector {
            apply_ok: true,
            compile_ok: false,
            error_codes,
            warn_count,
            behavior: BehaviorScore::default(),
            constraint: ConstraintScore::default(),
            score: 0.0,
            failure_class,
            flags: flags.clone(),
        };
        v.score = composite_score(&v, spec.weights);
        return Ok(v);
    }

    // ---- L2: behaviour ----
    let mut targs = vec!["test"];
    if let Some(name) = spec.behavior_test {
        targs.push("--test");
        targs.push(name);
    }
    targs.extend_from_slice(&["--offline", "--quiet"]);
    let test = run_cargo(&policy, &targs)?;
    if test.timed_out {
        flags.push("timeout:test".into());
    }
    let unit = parse_test_summary(&test.stdout).or_else(|| parse_test_summary(&test.stderr));
    // A configured behaviour stage that yields no summary means the test target
    // did not build against this answer (e.g. it changed a required interface,
    // or it timed out). That is a behaviour *failure* scored 0.0 — never absent,
    // or a non-conforming answer would inflate its score on the remaining layers.
    let unit_score = match unit {
        Some((_, 0)) => Some(0.0),
        Some((passed, total)) => Some(passed as f32 / total as f32),
        None => {
            if !test.timed_out {
                flags.push("behavior:no_summary".into());
            }
            Some(0.0)
        }
    };

    let mut behavior = BehaviorScore {
        unit: unit_score,
        property: None,
        differential: None,
        score: None,
    };

    // ---- L2 differential: candidate vs hidden reference over generated inputs ----
    if let Some(name) = spec.differential_test {
        let out = run_cargo(&policy, &["test", "--test", name, "--offline", "--quiet"])?;
        if out.timed_out {
            flags.push("timeout:differential".into());
        }
        behavior.differential =
            match parse_test_summary(&out.stdout).or_else(|| parse_test_summary(&out.stderr)) {
                Some((_, 0)) => Some(0.0),
                Some((passed, total)) => Some(passed as f32 / total as f32),
                None => {
                    if !out.timed_out {
                        flags.push("differential:no_summary".into());
                    }
                    Some(0.0)
                }
            };
    }
    behavior.recompute();

    // ---- L3: constraint (allocation) ----
    let mut constraint = ConstraintScore::default();
    if let Some(name) = spec.alloc_test {
        let out = run_cargo(&policy, &["test", "--test", name, "--offline", "--quiet"])?;
        if out.timed_out {
            flags.push("timeout:alloc".into());
        }
        match parse_test_summary(&out.stdout).or_else(|| parse_test_summary(&out.stderr)) {
            Some((passed, total)) if total > 0 => {
                let ok = passed == total;
                constraint.alloc_ok = Some(ok);
                if !ok {
                    constraint
                        .violations
                        .push("alloc: hot path allocated".into());
                }
            }
            // The allocation target did not build against this answer — the
            // answer does not conform, so it fails the check (not "absent").
            _ => {
                constraint.alloc_ok = Some(false);
                constraint
                    .violations
                    .push("alloc: test target did not run".into());
                if !out.timed_out {
                    flags.push("alloc:no_summary".into());
                }
            }
        }
        constraint.recompute();
    }

    // ---- L3 constraint: AST checks (unsafe, forbidden paths) ----
    if spec.max_unsafe.is_some() || !spec.forbidden_paths.is_empty() {
        if let Some(limit) = spec.max_unsafe {
            if let Some(n) = ast::count_unsafe(&code) {
                constraint.unsafe_blocks = Some(n);
                let ok = n <= limit;
                constraint.unsafe_ok = Some(ok);
                if !ok {
                    constraint
                        .violations
                        .push(format!("unsafe: {n} usage(s), limit {limit}"));
                }
            }
        }
        if !spec.forbidden_paths.is_empty() {
            if let Some(hits) = ast::find_forbidden_paths(&code, &spec.forbidden_paths) {
                let ok = hits.is_empty();
                constraint.paths_ok = Some(ok);
                if !ok {
                    constraint
                        .violations
                        .push(format!("forbidden path(s): {}", hits.join(", ")));
                }
            }
        }
        constraint.recompute();
    }

    // ---- L3 constraint: clippy (idiomaticity, docs/03) ----
    // The dominant signal for `idiom-refactor`: non-idiomatic code compiles and is
    // behaviourally correct, so only clippy distinguishes it. Runs on the answer's
    // library only (`--lib`), so the hidden test targets never contribute lints.
    if spec.check_clippy {
        let mut args: Vec<String> = vec![
            "clippy".into(),
            "--lib".into(),
            "--offline".into(),
            "--message-format=json".into(),
        ];
        if !spec.clippy_allow.is_empty() {
            args.push("--".into());
            for lint in &spec.clippy_allow {
                args.push("-A".into());
                args.push(lint.clone());
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = run_cargo(&policy, &arg_refs)?;
        if out.timed_out {
            flags.push("timeout:clippy".into());
        }
        let lints = parse_clippy_lints(&out.stdout);
        let clean = lints.is_empty();
        constraint.clippy_clean = Some(clean);
        if !clean {
            constraint
                .violations
                .push(format!("clippy: {}", lints.join(", ")));
        }
        constraint.recompute();
    }

    // Failure class, most-specific first: compiled, so it is a behaviour (unit
    // or differential), then constraint, then clean.
    let failure_class = if behavior.score.map(|s| s < 1.0).unwrap_or(true) {
        FailureClass::Logic
    } else if constraint.score.map(|s| s < 1.0).unwrap_or(false) {
        FailureClass::Constraint
    } else {
        FailureClass::None
    };

    let mut v = OracleVector {
        apply_ok: true,
        compile_ok: true,
        error_codes,
        warn_count,
        behavior,
        constraint,
        score: 0.0,
        failure_class,
        flags,
    };
    v.score = composite_score(&v, spec.weights);
    Ok(v)
}

/// Extract the answer from a model response. Prefers the first fenced code
/// block (``` optionally tagged `rust`); falls back to the whole trimmed
/// response when the model returned bare code.
pub fn extract_code(response: &str) -> Option<String> {
    if let Some(block) = first_fenced_block(response) {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }
    let trimmed = response.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn first_fenced_block(s: &str) -> Option<String> {
    let mut lines = s.lines();
    let mut inside = false;
    let mut buf = String::new();
    for line in &mut lines {
        let is_fence = line.trim_start().starts_with("```");
        if is_fence {
            if inside {
                return Some(buf);
            } else {
                inside = true;
                continue;
            }
        }
        if inside {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    None
}

fn write_under(root: &Path, rel: &Path, contents: &str) -> std::io::Result<()> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

struct CargoRun {
    success: bool,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn run_cargo(policy: &bench_sandbox::Policy, args: &[&str]) -> std::io::Result<CargoRun> {
    // The single external-command seam — every model-code execution the oracle
    // performs (compile, since proc macros run at build time; and every test
    // binary) goes through here, and here alone gets containment.
    let out = bench_sandbox::run(policy, "cargo", args, &[("CARGO_TERM_COLOR", "never")])?;
    Ok(CargoRun {
        success: out.success(),
        stdout: out.stdout,
        stderr: out.stderr,
        timed_out: out.timed_out,
    })
}

/// Parse `cargo build --message-format=json` output for every rustc diagnostic
/// code, and count warnings. Every code is kept, not just success/failure —
/// the histogram is the richest Rust-specific signal (docs/03-oracle.md).
fn parse_diagnostics(stdout: &str) -> (Vec<String>, u32) {
    let mut codes = Vec::new();
    let mut warns = 0u32;
    for line in stdout.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("");
        if level == "warning" {
            warns += 1;
        }
        if let Some(code) = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
        {
            if level == "error" {
                codes.push(code.to_string());
            }
        }
    }
    codes.sort();
    codes.dedup();
    (codes, warns)
}

/// Parse `cargo clippy --message-format=json` output for the clippy lint codes it
/// emitted at `warning` level — the `clippy::*` codes only, so ordinary rustc
/// warnings (unused variables) do not count against idiomaticity. Deduplicated;
/// empty means clippy-clean. Allowed lints were suppressed by `-A` upstream and so
/// never appear here.
fn parse_clippy_lints(stdout: &str) -> Vec<String> {
    let mut lints = Vec::new();
    for line in stdout.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        if msg.get("level").and_then(|l| l.as_str()) != Some("warning") {
            continue;
        }
        if let Some(code) = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
        {
            if code.starts_with("clippy::") {
                lints.push(code.to_string());
            }
        }
    }
    lints.sort();
    lints.dedup();
    lints
}

/// Parse libtest's human summary lines: `test result: ok. 3 passed; 0 failed; …`.
/// Returns `(passed, passed + failed)` **summed across every section** — a
/// `cargo test` run emits one summary per target (lib unit tests, each
/// integration test, and doc tests), so taking only the last would read the
/// empty doc-test section and score a passing solution zero. libtest JSON
/// output needs nightly, so the spine scans the stable text; P4 can switch to
/// JSON once the toolchain pin is decided.
fn parse_test_summary(s: &str) -> Option<(u32, u32)> {
    let marker = "test result:";
    let mut passed = 0u32;
    let mut total = 0u32;
    let mut seen = false;
    for line in s.lines() {
        let Some(pos) = line.find(marker) else {
            continue;
        };
        let after = &line[pos + marker.len()..];
        if let (Some(p), Some(fail)) = (
            extract_count(after, "passed"),
            extract_count(after, "failed"),
        ) {
            passed += p;
            total += p + fail;
            seen = true;
        }
    }
    seen.then_some((passed, total))
}

fn extract_count(s: &str, label: &str) -> Option<u32> {
    let idx = s.find(label)?;
    // Walk backwards over whitespace and digits to read the number before `label`.
    let prefix = s[..idx].trim_end();
    let num: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num.chars().rev().collect::<String>().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_rust_block() {
        let r = "Here you go:\n```rust\nfn f() {}\n```\nDone.";
        assert_eq!(extract_code(r).unwrap(), "fn f() {}");
    }

    #[test]
    fn extracts_untagged_fence() {
        let r = "```\nfn f() {}\n```";
        assert_eq!(extract_code(r).unwrap(), "fn f() {}");
    }

    #[test]
    fn falls_back_to_bare_code() {
        assert_eq!(extract_code("fn f() {}").unwrap(), "fn f() {}");
    }

    #[test]
    fn empty_response_has_no_code() {
        assert!(extract_code("   \n  ").is_none());
    }

    #[test]
    fn parses_error_codes_and_warns() {
        let json = r#"{"reason":"compiler-message","message":{"level":"error","code":{"code":"E0499"},"message":"x"}}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"unused_variables"},"message":"y"}}
{"reason":"compiler-artifact"}"#;
        let (codes, warns) = parse_diagnostics(json);
        assert_eq!(codes, vec!["E0499".to_string()]);
        assert_eq!(warns, 1);
    }

    #[test]
    fn parses_test_summary() {
        assert_eq!(
            parse_test_summary("test result: ok. 3 passed; 0 failed; 0 ignored"),
            Some((3, 3))
        );
        assert_eq!(
            parse_test_summary("test result: FAILED. 1 passed; 2 failed; 0 ignored"),
            Some((1, 3))
        );
    }

    #[test]
    fn sums_across_sections() {
        // lib unit tests (0) + integration (5 passed) + doc tests (0): the
        // real shape of a `cargo test` run. Must be (5, 5), not the last (0, 0).
        let out = "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored\n\
                   running 5 tests\ntest result: ok. 5 passed; 0 failed; 0 ignored\n\
                   running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored";
        assert_eq!(parse_test_summary(out), Some((5, 5)));
    }

    #[test]
    fn no_summary_is_none() {
        assert_eq!(parse_test_summary("error: could not compile"), None);
    }

    #[test]
    fn parses_only_clippy_warnings() {
        // A clippy lint, an ordinary rustc warning (must be ignored), a second
        // clippy lint, a duplicate clippy lint (deduped), and a non-message line.
        let json = r#"{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::needless_range_loop"},"message":"x"}}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"unused_variables"},"message":"y"}}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::manual_map"},"message":"z"}}
{"reason":"compiler-message","message":{"level":"warning","code":{"code":"clippy::needless_range_loop"},"message":"dup"}}
{"reason":"compiler-artifact"}"#;
        assert_eq!(
            parse_clippy_lints(json),
            vec![
                "clippy::manual_map".to_string(),
                "clippy::needless_range_loop".to_string()
            ]
        );
        // Clean output → no lints.
        assert!(parse_clippy_lints(r#"{"reason":"compiler-artifact"}"#).is_empty());
    }
}
