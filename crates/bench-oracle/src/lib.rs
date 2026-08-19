//! `bench-oracle` — the layered grader.
//!
//! P0 spine: L0 apply, L1 compile (with rustc-error-code extraction), and the
//! L2 unit sub-oracle. L2 property/differential and the L3/L4 layers arrive in
//! P2. Grading runs the real `cargo`/`rustc` toolchain in a materialised
//! workspace that is separate from anything the model ever saw — the oracle
//! files are injected only here (docs/03-oracle.md).
//!
//! Not yet sandboxed: model-authored code runs under the plain process API.
//! `bench-sandbox` (P1) wraps these invocations with network denial and
//! rlimits. The seam is deliberately narrow — every external command goes
//! through `run_cargo`, so P1 has exactly one place to harden.

use bench_core::{
    classify_error_codes, composite_score, BehaviorScore, FailureClass, Instance, OracleVector,
    OracleWeights,
};
use std::path::Path;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace {0} is not an empty directory")]
    DirtyWorkspace(String),
}

/// Grade one model response for one instance.
///
/// `workspace` must be an existing empty directory; the oracle materialises the
/// crate there (skeleton files + the model's applied answer + hidden oracle
/// files), builds it, and tests it.
pub fn grade(
    instance: &Instance,
    response: &str,
    answer_path: &Path,
    weights: &OracleWeights,
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
    write_under(workspace, answer_path, &code)?;
    for (rel, contents) in &instance.hidden {
        write_under(workspace, rel, contents)?;
    }

    // ---- L1: compile ----
    let build = run_cargo(workspace, &["build", "--offline", "--message-format=json"])?;
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
            score: 0.0,
            failure_class,
        };
        v.score = composite_score(&v, weights);
        return Ok(v);
    }

    // ---- L2: unit ----
    let test = run_cargo(workspace, &["test", "--offline", "--quiet"])?;
    let unit = parse_test_summary(&test.stdout).or_else(|| parse_test_summary(&test.stderr));
    let unit_score = unit.map(|(passed, total)| {
        if total == 0 {
            0.0
        } else {
            passed as f32 / total as f32
        }
    });

    let behavior = BehaviorScore {
        unit: unit_score,
        property: None,
        differential: None,
        score: unit_score,
    };
    let failure_class = match unit_score {
        Some(s) if s >= 1.0 => FailureClass::None,
        _ => FailureClass::Logic,
    };

    let mut v = OracleVector {
        apply_ok: true,
        compile_ok: true,
        error_codes,
        warn_count,
        behavior,
        score: 0.0,
        failure_class,
    };
    v.score = composite_score(&v, weights);
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
}

fn run_cargo(workspace: &Path, args: &[&str]) -> std::io::Result<CargoRun> {
    // The single external-command seam. P1's sandbox wraps exactly this.
    let out = Command::new("cargo")
        .args(args)
        .current_dir(workspace)
        .env("CARGO_TERM_COLOR", "never")
        .output()?;
    Ok(CargoRun {
        success: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
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
}
