//! `rustybench` — the CLI. P0 spine: a single `run` subcommand that loads a
//! frozen task, sends its prompt to an OpenAI-compatible model, grades the
//! response with the real toolchain, and appends one line to a JSONL journal.
//!
//! This proves the loop end to end (docs/14-roadmap.md P0 exit criterion).
//! Generation, seeding, the sandbox, resume, and submission are later phases.

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
    about = "Rust coding benchmark for local LLMs (P0 spine)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run one frozen task against a model and append a graded journal line.
    Run {
        /// Path to a frozen task directory (contains task.toml).
        #[arg(long)]
        task: PathBuf,
        /// OpenAI-compatible base URL, e.g. http://localhost:8080
        #[arg(long)]
        model: String,
        /// Model name to send in the request body.
        #[arg(long, default_value = "local")]
        model_name: String,
        /// Journal file to append to.
        #[arg(long, default_value = "runs/journal.jsonl")]
        out: PathBuf,
        /// Scratch root for grading workspaces.
        #[arg(long, default_value = "runs/ws")]
        scratch: PathBuf,
    },
}

/// The parsed task.toml. A subset of docs/02-task-format.md sufficient for the
/// spine's frozen tasks.
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

/// One journal line — a subset of docs/12-schemas.md journal.jsonl.
#[derive(Serialize)]
struct JournalLine {
    schema: u32,
    unit_id: String,
    task_id: String,
    category: String,
    seed: u64,
    index: u32,
    model: ModelInfo,
    /// What containment grading ran under: "seatbelt" (macOS) or "unsupported".
    /// Integrity-relevant — a leaderboard must know whether model code was
    /// contained (docs/10-integrity.md).
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

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            task,
            model,
            model_name,
            out,
            scratch,
        } => run(&task, &model, &model_name, &out, &scratch),
    }
}

fn run(
    task_dir: &Path,
    base_url: &str,
    model_name: &str,
    out: &Path,
    scratch_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // --- load the frozen task ---
    let manifest_text = std::fs::read_to_string(task_dir.join("task.toml"))
        .map_err(|e| format!("reading task.toml in {}: {e}", task_dir.display()))?;
    let manifest: TaskManifest = toml::from_str(&manifest_text)?;

    let files = read_tree(&task_dir.join("template"))?;
    let hidden = read_tree(&task_dir.join("oracle"))?;
    let prompt = std::fs::read_to_string(task_dir.join("prompt.md"))
        .map_err(|e| format!("reading prompt.md: {e}"))?;

    let instance = Instance {
        prompt: prompt.clone(),
        files,
        hidden,
        canary: format!("rb-frozen-{}", manifest.id),
    };

    // Frozen tasks have no generation; the work unit is (task, seed 0).
    let unit = WorkUnit {
        task_id: TaskId(manifest.id.clone()),
        seed: Seed(0),
        index: 0,
    };

    // --- the model turn (harness process, not sandboxed) ---
    println!(
        "→ {} against {base_url} ({model_name}) [sandbox: {}]",
        manifest.id,
        match bench_sandbox::available() {
            bench_sandbox::Containment::Seatbelt => "seatbelt",
            _ => "none",
        }
    );
    let client = ModelClient::new(base_url, model_name);
    let completion =
        client.complete(&manifest.system_prompt, &prompt, &SamplingConfig::default())?;
    println!(
        "  ← {} completion tokens, finish={}, {} ms",
        completion.completion_tokens, completion.finish_reason, completion.elapsed_ms
    );

    // --- grade ---
    let ws = scratch_root.join(unit.unit_id().0.replace(':', "_"));
    if ws.exists() {
        std::fs::remove_dir_all(&ws)?;
    }
    std::fs::create_dir_all(&ws)?;

    let weights = manifest
        .weights
        .as_ref()
        .map(|w| OracleWeights {
            behavior: w.behavior,
            constraint: w.constraint,
            quality: w.quality,
        })
        .unwrap_or_default();
    let ocfg = manifest.oracle.unwrap_or_default();
    let spec = bench_oracle::GradeSpec {
        answer_path: Path::new(&manifest.answer_path),
        weights: &weights,
        behavior_test: ocfg.behavior_test.as_deref(),
        differential_test: ocfg.differential_test.as_deref(),
        alloc_test: ocfg.alloc_test.as_deref(),
    };

    let containment = match bench_sandbox::available() {
        bench_sandbox::Containment::Seatbelt => "seatbelt",
        bench_sandbox::Containment::Unsupported => "unsupported",
    };
    if containment == "unsupported" {
        eprintln!("  ! warning: no sandbox on this platform — model code runs uncontained");
    }

    let grade_start = std::time::Instant::now();
    let vector = bench_oracle::grade(&instance, &completion.text, &spec, &ws)?;
    let grade_ms = grade_start.elapsed().as_millis() as u64;

    // --- write the journal line ---
    let line = JournalLine {
        schema: 1,
        unit_id: unit.unit_id().0,
        task_id: manifest.id.clone(),
        category: manifest.category.clone(),
        seed: unit.seed.0,
        index: unit.index,
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
    };

    append_journal(out, &line)?;

    println!(
        "  score {:.3}  apply={} compile={} unit={:?} diff={:?} behavior={:?} constraint={:?} failure={:?}",
        vector.score,
        vector.apply_ok,
        vector.compile_ok,
        vector.behavior.unit,
        vector.behavior.differential,
        vector.behavior.score,
        vector.constraint.score,
        vector.failure_class
    );
    if !vector.error_codes.is_empty() {
        println!("  rustc: {}", vector.error_codes.join(", "));
    }
    println!("  journal → {}", out.display());
    Ok(())
}

/// Read a directory tree into a `path → contents` map, keyed relative to `root`.
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
    let json = serde_json::to_string(line)?;
    writeln!(f, "{json}")?;
    f.flush()
}
