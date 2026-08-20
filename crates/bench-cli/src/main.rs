//! `rustybench` — the CLI. Grades a task (frozen file or seeded generator)
//! against an OpenAI-compatible model, under the sandbox, and appends a scored
//! JSONL line. Also `validate-family`, which runs a family's own construction
//! gates: the reference must score 1.0, the skeleton must fail, generation must
//! be deterministic (ADR-0003).

use bench_core::{FailureClass, Instance, OracleVector, OracleWeights, Seed, TaskId, WorkUnit};
use bench_model::{ModelClient, SamplingConfig};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "rustybench",
    version,
    about = "Rust coding benchmark for local LLMs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a task against a model and append a graded journal line.
    Run {
        /// A frozen task directory (contains task.toml). Mutually exclusive with --family.
        #[arg(long)]
        task: Option<PathBuf>,
        /// A generator family id (e.g. `window-op`), used with --seed.
        #[arg(long)]
        family: Option<String>,
        /// The seed for --family.
        #[arg(long)]
        seed: Option<u64>,
        /// OpenAI-compatible base URL, e.g. http://localhost:8080
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "local")]
        model_name: String,
        #[arg(long, default_value = "runs/journal.jsonl")]
        out: PathBuf,
        #[arg(long, default_value = "runs/ws")]
        scratch: PathBuf,
        #[arg(long, default_value_t = 120)]
        wall_timeout_secs: u64,
    },
    /// Run a family's construction gates over a range of seeds (no model).
    ValidateFamily {
        #[arg(long)]
        family: String,
        /// Number of seeds to check, starting from 0.
        #[arg(long, default_value_t = 8)]
        seeds: u64,
        #[arg(long, default_value = "runs/validate")]
        scratch: PathBuf,
    },
    /// Aggregate a graded journal into capability/pass-rate and cluster-bootstrap CIs.
    Stats {
        /// Path to a JSONL journal written by `run`.
        #[arg(long, default_value = "runs/journal.jsonl")]
        journal: PathBuf,
    },
    /// Compare two models (McNemar on the shared pass bits + paired pass-rate CI).
    Compare {
        #[arg(long)]
        journal_a: PathBuf,
        #[arg(long)]
        journal_b: PathBuf,
    },
    /// Run a whole epoch over every family: paired-core + fresh-probe seeds, resumable.
    RunSuite {
        #[arg(long)]
        model: String,
        #[arg(long, default_value = "local")]
        model_name: String,
        /// Epoch label — fixes the paired-core seed set (ADR-0009).
        #[arg(long)]
        epoch: String,
        /// Paired-core seeds per family (scored).
        #[arg(long, default_value_t = 4)]
        seeds_core: u32,
        /// Fresh-probe seeds per family (precomputation detector; never scored).
        #[arg(long, default_value_t = 1)]
        seeds_probe: u32,
        #[arg(long, default_value = "runs/journal.jsonl")]
        out: PathBuf,
        #[arg(long, default_value = "runs/ws")]
        scratch: PathBuf,
        #[arg(long, default_value_t = 120)]
        wall_timeout_secs: u64,
        /// Print the plan (after resume filtering) without calling the model.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Everything needed to grade one task, from either source.
struct Task {
    id: String,
    category: String,
    system_prompt: String,
    prompt: String,
    instance: Instance,
    answer_path: String,
    weights: OracleWeights,
    behavior_test: Option<String>,
    differential_test: Option<String>,
    alloc_test: Option<String>,
    max_unsafe: Option<u32>,
    forbidden_paths: Vec<String>,
}

const GENERIC_SYSTEM_PROMPT: &str = "You are an expert Rust programmer. Respond with a SINGLE ```rust code block containing the complete contents of src/lib.rs. No prose, no explanation, no other text.";

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Run {
            task,
            family,
            seed,
            model,
            model_name,
            out,
            scratch,
            wall_timeout_secs,
        } => {
            let t = load_task(task.as_deref(), family.as_deref(), seed)?;
            run(t, &model, &model_name, &out, &scratch, wall_timeout_secs)
        }
        Command::ValidateFamily {
            family,
            seeds,
            scratch,
        } => validate_family(&family, seeds, &scratch),
        Command::Stats { journal } => stats(&journal),
        Command::Compare {
            journal_a,
            journal_b,
        } => compare(&journal_a, &journal_b),
        Command::RunSuite {
            model,
            model_name,
            epoch,
            seeds_core,
            seeds_probe,
            out,
            scratch,
            wall_timeout_secs,
            dry_run,
        } => run_suite(
            &model,
            &model_name,
            &epoch,
            seeds_core,
            seeds_probe,
            &out,
            &scratch,
            wall_timeout_secs,
            dry_run,
        ),
    }
}

// ---------------------------------------------------------------------------
// Task loading
// ---------------------------------------------------------------------------

fn load_task(
    task_dir: Option<&Path>,
    family: Option<&str>,
    seed: Option<u64>,
) -> Result<Task, Box<dyn std::error::Error>> {
    match (task_dir, family) {
        (Some(dir), None) => load_frozen(dir),
        (None, Some(fam)) => {
            let seed = seed.ok_or("--family requires --seed")?;
            load_generated(fam, seed)
        }
        _ => Err("provide exactly one of --task or --family".into()),
    }
}

#[derive(Deserialize)]
struct TaskManifest {
    id: String,
    category: String,
    #[allow(dead_code)]
    kind: String,
    answer_path: String,
    system_prompt: String,
    #[serde(default)]
    weights: Option<Weights>,
    #[serde(default)]
    oracle: Option<OracleCfg>,
    #[serde(default)]
    constraint: Option<ConstraintCfg>,
}
#[derive(Deserialize)]
struct Weights {
    behavior: f32,
    constraint: f32,
    quality: f32,
}
#[derive(Deserialize, Default)]
struct OracleCfg {
    behavior_test: Option<String>,
    differential_test: Option<String>,
    alloc_test: Option<String>,
}
#[derive(Deserialize, Default)]
struct ConstraintCfg {
    max_unsafe: Option<u32>,
    #[serde(default)]
    forbidden_paths: Vec<String>,
}

fn load_frozen(task_dir: &Path) -> Result<Task, Box<dyn std::error::Error>> {
    let manifest_text = std::fs::read_to_string(task_dir.join("task.toml"))
        .map_err(|e| format!("reading task.toml in {}: {e}", task_dir.display()))?;
    let m: TaskManifest = toml::from_str(&manifest_text)?;
    let files = read_tree(&task_dir.join("template"))?;
    let hidden = read_tree(&task_dir.join("oracle"))?;
    let prompt = std::fs::read_to_string(task_dir.join("prompt.md"))
        .map_err(|e| format!("reading prompt.md: {e}"))?;
    let ocfg = m.oracle.unwrap_or_default();
    let ccfg = m.constraint.unwrap_or_default();
    let weights = m
        .weights
        .map(|w| OracleWeights {
            behavior: w.behavior,
            constraint: w.constraint,
            quality: w.quality,
        })
        .unwrap_or_default();
    let canary = format!("rb-frozen-{}", m.id);
    Ok(Task {
        id: m.id.clone(),
        category: m.category,
        system_prompt: m.system_prompt,
        prompt: prompt.clone(),
        instance: Instance {
            prompt,
            files,
            hidden,
            canary,
        },
        answer_path: m.answer_path,
        weights,
        behavior_test: ocfg.behavior_test,
        differential_test: ocfg.differential_test,
        alloc_test: ocfg.alloc_test,
        max_unsafe: ccfg.max_unsafe,
        forbidden_paths: ccfg.forbidden_paths,
    })
}

fn load_generated(fam: &str, seed: u64) -> Result<Task, Box<dyn std::error::Error>> {
    let g = bench_gen::family(fam).ok_or_else(|| format!("unknown family `{fam}`"))?;
    Ok(task_from_generated(&g.generate(seed)))
}

/// Map an empty test-target name to `None` (a family opts out of a layer by
/// leaving its target blank).
fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn task_from_generated(gt: &bench_gen::GeneratedTask) -> Task {
    Task {
        id: gt.id.clone(),
        category: gt.category.clone(),
        system_prompt: GENERIC_SYSTEM_PROMPT.to_string(),
        prompt: gt.prompt.clone(),
        instance: gt.instance(),
        answer_path: gt.answer_path.clone(),
        weights: OracleWeights {
            behavior: gt.weights.0,
            constraint: gt.weights.1,
            quality: gt.weights.2,
        },
        behavior_test: non_empty(&gt.behavior_test),
        differential_test: non_empty(&gt.differential_test),
        alloc_test: non_empty(&gt.alloc_test),
        max_unsafe: Some(gt.max_unsafe),
        forbidden_paths: gt.forbidden_paths.clone(),
    }
}

// ---------------------------------------------------------------------------
// Grading
// ---------------------------------------------------------------------------

/// Grade a response string against a task in a fresh workspace under `scratch`.
fn grade(
    task: &Task,
    response: &str,
    scratch_root: &Path,
    tag: &str,
    wall_timeout_secs: u64,
) -> Result<OracleVector, Box<dyn std::error::Error>> {
    let ws = scratch_root.join(tag);
    if ws.exists() {
        std::fs::remove_dir_all(&ws)?;
    }
    std::fs::create_dir_all(&ws)?;
    let limits = bench_sandbox::Limits {
        wall: std::time::Duration::from_secs(wall_timeout_secs),
        cpu: std::time::Duration::from_secs(wall_timeout_secs.saturating_mul(30)),
        address_space: None,
    };
    let spec = bench_oracle::GradeSpec {
        answer_path: Path::new(&task.answer_path),
        weights: &task.weights,
        behavior_test: task.behavior_test.as_deref(),
        differential_test: task.differential_test.as_deref(),
        alloc_test: task.alloc_test.as_deref(),
        limits,
        max_unsafe: task.max_unsafe,
        forbidden_paths: task.forbidden_paths.clone(),
    };
    Ok(bench_oracle::grade(&task.instance, response, &spec, &ws)?)
}

fn run(
    task: Task,
    base_url: &str,
    model_name: &str,
    out: &Path,
    scratch_root: &Path,
    wall_timeout_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let containment = match bench_sandbox::available() {
        bench_sandbox::Containment::Seatbelt => "seatbelt",
        bench_sandbox::Containment::Unsupported => "unsupported",
    };
    if containment == "unsupported" {
        eprintln!("  ! warning: no sandbox on this platform — model code runs uncontained");
    }
    println!(
        "→ {} against {base_url} ({model_name}) [sandbox: {containment}]",
        task.id
    );

    let line = grade_and_line(
        &task,
        base_url,
        model_name,
        scratch_root,
        wall_timeout_secs,
        "local",
        "core",
        0,
        0,
    )?;
    append_journal(out, &line)?;

    let v = &line.oracle;
    println!(
        "  score {:.3}  apply={} compile={} unit={:?} diff={:?} behavior={:?} constraint={:?} failure={:?}",
        v.score,
        v.apply_ok,
        v.compile_ok,
        v.behavior.unit,
        v.behavior.differential,
        v.behavior.score,
        v.constraint.score,
        v.failure_class
    );
    if !v.error_codes.is_empty() {
        println!("  rustc: {}", v.error_codes.join(", "));
    }
    if !v.flags.is_empty() {
        println!("  flags: {}", v.flags.join(", "));
    }
    println!("  journal → {}", out.display());
    Ok(())
}

/// Call the model on one task, grade the response under the sandbox, and build the
/// journal line — the single-unit core shared by `run` and `run-suite`.
#[allow(clippy::too_many_arguments)]
fn grade_and_line(
    task: &Task,
    base_url: &str,
    model_name: &str,
    scratch_root: &Path,
    wall_timeout_secs: u64,
    epoch: &str,
    kind: &str,
    seed: u64,
    index: u32,
) -> Result<JournalLine, Box<dyn std::error::Error>> {
    let containment = match bench_sandbox::available() {
        bench_sandbox::Containment::Seatbelt => "seatbelt",
        bench_sandbox::Containment::Unsupported => "unsupported",
    };
    let client = ModelClient::new(base_url, model_name);
    let completion = client.complete(
        &task.system_prompt,
        &task.prompt,
        &SamplingConfig::default(),
    )?;
    println!(
        "  ← {} completion tokens, finish={}, {} ms",
        completion.completion_tokens, completion.finish_reason, completion.elapsed_ms
    );

    let unit = WorkUnit {
        task_id: TaskId(task.id.clone()),
        seed: Seed(seed),
        index,
    };
    let tag = unit.unit_id().0.replace(':', "_");
    let grade_start = std::time::Instant::now();
    let vector = grade(
        task,
        &completion.text,
        scratch_root,
        &tag,
        wall_timeout_secs,
    )?;
    let grade_ms = grade_start.elapsed().as_millis() as u64;

    Ok(JournalLine {
        schema: 1,
        unit_id: unit.unit_id().0,
        task_id: task.id.clone(),
        category: task.category.clone(),
        seed,
        index,
        epoch: epoch.to_string(),
        kind: kind.to_string(),
        model: ModelInfo {
            name: model_name.to_string(),
            base_url: base_url.to_string(),
            finish_reason: completion.finish_reason.clone(),
        },
        sandbox: containment.to_string(),
        oracle: vector.clone(),
        cost: Cost {
            prompt_tokens: completion.prompt_tokens,
            completion_tokens: completion.completion_tokens,
            gen_ms: completion.elapsed_ms,
            grade_ms,
        },
        failure_class: vector.failure_class,
    })
}

// ---------------------------------------------------------------------------
// run-suite — epoch orchestration with paired-core / fresh-probe + resume
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn run_suite(
    base_url: &str,
    model_name: &str,
    epoch: &str,
    n_core: u32,
    n_probe: u32,
    out: &Path,
    scratch_root: &Path,
    wall_timeout_secs: u64,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let plan = bench_gen::epoch::plan_run(bench_gen::FAMILY_IDS, epoch, n_core, n_probe);
    let done = read_done_keys(out, epoch)?;
    let todo = bench_gen::epoch::remaining(&plan, &done);
    println!(
        "epoch {epoch}: {} units planned ({} core + {} probe over {} families), {} done, {} to run",
        plan.len(),
        n_core as usize * bench_gen::FAMILY_IDS.len(),
        n_probe as usize * bench_gen::FAMILY_IDS.len(),
        bench_gen::FAMILY_IDS.len(),
        plan.len() - todo.len(),
        todo.len(),
    );

    if dry_run {
        for u in &todo {
            println!(
                "  [plan] {:<14} {:<5} idx={} seed={:016x}",
                u.family,
                u.kind.as_str(),
                u.index,
                u.seed
            );
        }
        println!("(dry run — no model calls)");
        return Ok(());
    }

    let containment = matches!(
        bench_sandbox::available(),
        bench_sandbox::Containment::Seatbelt
    );
    if !containment {
        eprintln!("  ! warning: no sandbox on this platform — model code runs uncontained");
    }
    let mut ran = 0usize;
    for u in &todo {
        let g =
            bench_gen::family(&u.family).ok_or_else(|| format!("unknown family {}", u.family))?;
        let task = task_from_generated(&g.generate(u.seed));
        println!(
            "→ {} {} idx={} (seed {:016x})",
            u.family,
            u.kind.as_str(),
            u.index,
            u.seed
        );
        let line = grade_and_line(
            &task,
            base_url,
            model_name,
            scratch_root,
            wall_timeout_secs,
            epoch,
            u.kind.as_str(),
            u.seed,
            u.index,
        )?;
        println!(
            "  score {:.3} pass={}",
            line.oracle.score,
            line.oracle.passed()
        );
        append_journal(out, &line)?;
        ran += 1;
    }
    println!(
        "epoch {epoch}: ran {ran} unit(s); journal → {}",
        out.display()
    );
    Ok(())
}

/// Read the resume set: the `family|kind|index` keys already recorded for `epoch`.
fn read_done_keys(
    out: &Path,
    epoch: &str,
) -> Result<std::collections::HashSet<String>, Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct DoneLine {
        task_id: String,
        index: u32,
        #[serde(default)]
        epoch: String,
        #[serde(default)]
        kind: String,
    }
    let mut done = std::collections::HashSet::new();
    let text = match std::fs::read_to_string(out) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(done),
        Err(e) => return Err(e.into()),
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let d: DoneLine = serde_json::from_str(line)?;
        if d.epoch != epoch {
            continue;
        }
        let family = d.task_id.split('/').next().unwrap_or(&d.task_id);
        done.insert(format!("{}|{}|{}", family, d.kind, d.index));
    }
    Ok(done)
}

// ---------------------------------------------------------------------------
// validate-family
// ---------------------------------------------------------------------------

fn validate_family(
    fam: &str,
    seeds: u64,
    scratch: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let g = bench_gen::family(fam).ok_or_else(|| format!("unknown family `{fam}`"))?;
    println!("validate-family {fam}: {seeds} seed(s)");
    let mut failures = 0u32;
    let mut views: Vec<String> = Vec::new();

    for seed in 0..seeds {
        let gt = g.generate(seed);
        let task = task_from_generated(&gt);
        views.push(bench_gen::epoch::view_of(&gt));

        let gt2 = g.generate(seed);
        let deterministic =
            gt.prompt == gt2.prompt && gt.files == gt2.files && gt.hidden == gt2.hidden;

        let ref_v = grade(
            &task,
            &g.reference_code(seed),
            scratch,
            &format!("ref-{seed}"),
            120,
        )?;

        let skel_v = grade(
            &task,
            &g.skeleton_code(seed),
            scratch,
            &format!("skel-{seed}"),
            120,
        )?;
        let skel_behavior = skel_v.behavior.score.unwrap_or(0.0);

        let mut baselines_ok = true;
        let mut baseline_note = String::new();
        for (label, code) in g.trivial_baselines(seed) {
            let v = grade(&task, &code, scratch, &format!("base-{seed}-{label}"), 120)?;
            let behav = v.behavior.score.unwrap_or(0.0);
            if behav >= 0.999 {
                baselines_ok = false;
                baseline_note = format!(" <-- baseline `{label}` not caught (behavior {behav:.3})");
            }
        }

        let canary_ok = gt.prompt.contains(&gt.canary);

        let ref_ok = ref_v.score >= 0.99;
        let skel_ok = skel_behavior < 0.5;
        let all = deterministic && ref_ok && skel_ok && baselines_ok && canary_ok;
        if !all {
            failures += 1;
        }
        println!(
            "  seed {seed:>3}: {} determinism={} reference={:.3}{} skeleton_behavior={:.3}{} baselines_caught={}{} canary={}",
            if all { "OK  " } else { "FAIL" },
            deterministic,
            ref_v.score,
            if ref_ok { "" } else { " <-- expected 1.0" },
            skel_behavior,
            if skel_ok { "" } else { " <-- expected <0.5" },
            baselines_ok,
            baseline_note,
            canary_ok,
        );
    }

    if views.len() >= 2 {
        let floor = bench_gen::epoch::MIN_INSTANCE_DISTANCE;

        // (min, median, near-twin pairs, total pairs) over a set of texts.
        fn dist_stats(items: &[String], floor: f64) -> (f64, f64, u32, usize) {
            let mut all_d: Vec<f64> = Vec::new();
            let mut near = 0u32;
            for i in 0..items.len() {
                for j in (i + 1)..items.len() {
                    let d = bench_gen::distance::shingle_distance(
                        &items[i],
                        &items[j],
                        bench_gen::distance::K,
                    );
                    all_d.push(d);
                    if d < floor {
                        near += 1;
                    }
                }
            }
            all_d.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let min = all_d.first().copied().unwrap_or(1.0);
            let median = all_d[all_d.len() / 2];
            (min, median, near, all_d.len())
        }

        // What the model sees. Gameable: seed-varied worked examples inflate this
        // without changing the answer (Q31), so a healthy line here is necessary
        // but NOT sufficient.
        let (vmin, vmed, vnear, vtot) = dist_stats(&views, floor);
        println!(
            "  anti-twin  view (prompt+skeleton): min={vmin:.3} median={vmed:.3} near-twin pairs (<{floor})={vnear}/{vtot}"
        );

        // The honest anti-memorisation-of-solution measure: distance between the
        // references. Not inflated by example variation (but deflated by shared
        // boilerplate — see Q31).
        let refs: Vec<String> = (0..seeds).map(|s| g.reference_code(s)).collect();
        let (rmin, rmed, rnear, rtot) = dist_stats(&refs, floor);
        println!(
            "  anti-twin  reference (solution) : min={rmin:.3} median={rmed:.3} near-twin pairs (<{floor})={rnear}/{rtot}"
        );

        // Distinct-at-floor capacity on each basis. view-capacity via the epoch
        // sampler (what it currently serves on); reference-capacity is the honest
        // solution-diversity ceiling.
        let want = views.len();
        let budget = ((want as u32) * 100).clamp(500, 4000);
        let view_cap =
            match bench_gen::epoch::plan_epoch_from(g.as_ref(), 0u64.., want, floor, budget) {
                Ok(plan) => format!(
                    "{}+ (asked {want}, rejected {})",
                    plan.seeds.len(),
                    plan.rejected()
                ),
                Err(e) => format!("{} (exhausted at {} candidates)", e.accepted, e.attempts),
            };
        let ref_cap = bench_gen::epoch::reference_capacity(g.as_ref(), 400, floor);
        println!(
            "  capacity   view={view_cap}  reference={ref_cap}  (text proxies — view over-counts, reference under-counts)"
        );

        // The authoritative diversity number (Q31, decided): distinct structural
        // specs, ungameable by example noise and undeflated by boilerplate. This is
        // the count a family is authored against; view-distance is a separate gate
        // for prompt freshness (contamination-resistance).
        let diversity = bench_gen::spec_diversity(g.as_ref(), 4000);
        println!("  spec-diversity: {diversity} distinct skills (the authoritative task-diversity measure — Q31)");

        // The spec-aware epoch serve-path: cover distinct skills, not just distinct
        // prompts. Request the validated count; it should serve that many skills so
        // long as the count is within the family's diversity.
        match bench_gen::epoch::plan_epoch_distinct_skills(g.as_ref(), 0u64.., want, floor, 5000) {
            Ok(plan) => println!(
                "  distinct-skills epoch: served {} seed(s) covering {} distinct skills",
                plan.seeds.len(),
                plan.distinct_skills(),
            ),
            Err(e) => println!(
                "  distinct-skills epoch: only {} distinct skills available (asked {want}) — family too narrow for this per-epoch count",
                e.accepted,
            ),
        }
    }

    let _ = std::fs::remove_dir_all(scratch);
    if failures == 0 {
        println!("all gates passed");
        Ok(())
    } else {
        Err(format!("{failures} seed(s) failed validation").into())
    }
}

// ---------------------------------------------------------------------------
// stats
// ---------------------------------------------------------------------------

fn stats(journal: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let records = bench_stats::load_journal(journal)
        .map_err(|e| format!("reading {}: {e}", journal.display()))?;
    if records.is_empty() {
        return Err(format!("no graded units in {}", journal.display()).into());
    }
    let r = bench_stats::report(&records);

    println!(
        "capability_score = {:.3}  [{:.3}, {:.3}]  (95% overall CI, cluster bootstrap)",
        r.capability_score, r.capability_ci.0, r.capability_ci.1
    );
    println!(
        "pass_rate        = {:.3}  over {} units in {} categor{}",
        r.pass_rate,
        r.units,
        r.categories.len(),
        if r.categories.len() == 1 { "y" } else { "ies" }
    );
    println!(
        "per-category (simultaneous CIs at 1 - 0.05/{}):",
        r.simultaneous_k
    );
    for c in &r.categories {
        let icc = c
            .icc
            .map(|i| format!("icc={i:.2}"))
            .unwrap_or_else(|| "icc=n/a".to_string());
        let de = c
            .design_effect
            .map(|d| format!(" de={d:.2}"))
            .unwrap_or_default();
        println!(
            "  {:<18} score={:.3} [{:.3}, {:.3}]  pass={:.3}  fams={} units={}  {icc}{de}{}",
            c.category,
            c.mean_score,
            c.score_ci.0,
            c.score_ci.1,
            c.pass_rate,
            c.families,
            c.units,
            if c.directional_only {
                "  <-- directional-only (too few families)"
            } else {
                ""
            },
        );
    }
    match r.pooled_icc {
        Some(icc) => {
            println!("pooled ICC = {icc:.3} (diagnostic/sizing only, not in any CI — Q29.2)")
        }
        None => println!("pooled ICC = n/a (needs >=2 families with >=2 seeds each to estimate)"),
    }
    println!(
        "note: family-level cluster bootstrap ({} resamples) — a lower bound on CI width until shapes are labelled (Q24).",
        r.bootstrap_iters
    );
    Ok(())
}

fn compare(a: &Path, b: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ra = bench_stats::load_journal(a).map_err(|e| format!("reading {}: {e}", a.display()))?;
    let rb = bench_stats::load_journal(b).map_err(|e| format!("reading {}: {e}", b.display()))?;
    let cmp = bench_stats::compare_models(&ra, &rb);
    if cmp.n_paired == 0 {
        return Err("the two journals share no units (compare needs the paired seed set)".into());
    }
    let m = &cmp.mcnemar;
    println!("paired over {} shared units", cmp.n_paired);
    println!(
        "McNemar: A-only={} B-only={} (discordant {}), chi2={:.3}, p={:.4}",
        m.discordant_a_only,
        m.discordant_b_only,
        m.discordant_a_only + m.discordant_b_only,
        m.statistic,
        m.p_value,
    );
    println!(
        "pass-rate difference (B - A) = {:+.3}  [{:+.3}, {:+.3}]  (95% paired wild cluster bootstrap)",
        cmp.delta_pass_rate, cmp.delta_ci.0, cmp.delta_ci.1
    );
    let verdict = if m.p_value >= 0.05 {
        "no significant difference (p >= 0.05)"
    } else if cmp.delta_pass_rate > 0.0 {
        "B significantly better (p < 0.05)"
    } else {
        "A significantly better (p < 0.05)"
    };
    println!("verdict: {verdict}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Journal + fs
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JournalLine {
    schema: u32,
    unit_id: String,
    task_id: String,
    category: String,
    seed: u64,
    index: u32,
    /// Epoch label; `"local"` for a single-unit `run`.
    epoch: String,
    /// `"core"` or `"probe"` (ADR-0009).
    kind: String,
    model: ModelInfo,
    sandbox: String,
    oracle: OracleVector,
    cost: Cost,
    failure_class: FailureClass,
}
#[derive(Serialize)]
struct ModelInfo {
    name: String,
    base_url: String,
    finish_reason: String,
}
#[derive(Serialize)]
struct Cost {
    prompt_tokens: u32,
    completion_tokens: u32,
    gen_ms: u64,
    grade_ms: u64,
}

fn read_tree(root: &Path) -> std::io::Result<BTreeMap<PathBuf, String>> {
    let mut map = BTreeMap::new();
    if !root.exists() {
        return Ok(map);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                map.insert(rel, std::fs::read_to_string(&path)?);
            }
        }
    }
    Ok(map)
}

fn append_journal(out: &Path, line: &JournalLine) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)?;
    writeln!(f, "{}", serde_json::to_string(line)?)?;
    f.flush()
}
