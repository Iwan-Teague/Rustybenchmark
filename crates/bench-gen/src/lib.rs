//! `bench-gen` — turns a seed into a fresh problem instance.
//!
//! Tasks are *generators*, not files. A family is a function from a seed to a
//! concrete instance, its reference implementation, its oracle, and the skeleton
//! the model completes — all built from the same seed, **solution-first**, so
//! the oracle is correct by construction (docs/02-task-format.md, ADR-0003).
//!
//! Scope of this first P3 increment: seed derivation, canary minting, the
//! `Generator` trait, and one parametric family (`window-op`) where the *operation
//! itself* varies by seed, not just identifiers — so seeds resist memorisation
//! rather than merely renaming. `validate-family`'s core gates
//! (reference-passes-its-own-oracle, skeleton-fails, determinism) are exercised
//! by `rustybench validate-family` in the CLI.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod distance;
pub mod epoch;
pub mod error_handling;
pub mod seq_transform;
pub mod stack_machine;
pub mod window_op;

/// Derive an instance seed from the epoch, task id and index (docs/02):
/// `blake3(epoch || task_id || index)[..8]`. Local/offline runs pass the seed
/// directly instead.
pub fn derive_seed(epoch: &str, task_id: &str, index: u32) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(epoch.as_bytes());
    h.update(&[0u8]);
    h.update(task_id.as_bytes());
    h.update(&[0u8]);
    h.update(&index.to_le_bytes());
    let bytes = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&bytes.as_bytes()[..8]);
    u64::from_le_bytes(b)
}

/// A unique low-frequency string embedded in the prompt. Its later appearance in
/// a public corpus is direct evidence the instance leaked (docs/02, ADR-0001).
pub fn mint_canary(family: &str, seed: u64) -> String {
    let mut h = blake3::Hasher::new();
    h.update(family.as_bytes());
    h.update(&seed.to_le_bytes());
    let hex = h.finalize().to_hex();
    format!("rb-{}", &hex[..12])
}

/// A small deterministic PRNG (SplitMix64). Generation must be pure in the seed
/// — same seed → byte-identical instance — so we never touch the OS RNG.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    /// Uniform in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// A fully materialised generated task: the model's view, the hidden oracle, the
/// prompt, and the grading configuration derived from the same seed.
pub struct GeneratedTask {
    pub id: String,
    pub category: String,
    pub prompt: String,
    pub canary: String,
    /// Where the model's answer is written, e.g. `src/lib.rs`.
    pub answer_path: String,
    /// Files given to the model (Cargo.toml + skeleton).
    pub files: BTreeMap<PathBuf, String>,
    /// Hidden oracle files (behaviour, differential, alloc tests).
    pub hidden: BTreeMap<PathBuf, String>,
    /// `cargo test --test` target names.
    pub behavior_test: String,
    pub differential_test: String,
    pub alloc_test: String,
    /// L3 AST constraint.
    pub max_unsafe: u32,
    /// L3 AST constraint: forbidden type/function/method names.
    pub forbidden_paths: Vec<String>,
    /// Per-category weights (behavior, constraint, quality).
    pub weights: (f32, f32, f32),
}

impl GeneratedTask {
    pub fn instance(&self) -> bench_core::Instance {
        bench_core::Instance {
            prompt: self.prompt.clone(),
            files: self.files.clone(),
            hidden: self.hidden.clone(),
            canary: self.canary.clone(),
        }
    }

    /// The same task with the model's answer replaced by a given source — used
    /// by the validation gates to grade the reference and the skeleton.
    pub fn files_with_answer(&self, answer: &str) -> BTreeMap<PathBuf, String> {
        let mut f = self.files.clone();
        f.insert(PathBuf::from(&self.answer_path), answer.to_string());
        f
    }
}

/// A task family: a deterministic function from seed to instance, plus the two
/// artefacts the validation gates need.
pub trait Generator {
    fn id(&self) -> &str;
    fn category(&self) -> &str;
    /// Generate the instance for `seed`. Must be pure in `seed`.
    fn generate(&self, seed: u64) -> GeneratedTask;
    /// The correct reference implementation for `seed`. Grading it must score
    /// 1.0 (ADR-0003: correct by construction).
    fn reference_code(&self, seed: u64) -> String;
    /// The ablated skeleton (`todo!()`). Grading it must fail.
    fn skeleton_code(&self, seed: u64) -> String;
    /// Degenerate answers (label, code) that have the right shape but the wrong
    /// content. Each must fail grading, or the oracle is too weak. Default: none.
    fn trivial_baselines(&self, _seed: u64) -> Vec<(String, String)> {
        Vec::new()
    }

    /// The structural identity of the task `seed` produces: the generative choices
    /// that define the *skill*, excluding cosmetic variation (identifiers, numeric
    /// constants, worked-example data). Two seeds with the same signature test the
    /// same skill; the count of distinct signatures is the family's genuine task
    /// diversity — the measure that is neither inflatable by example noise nor
    /// deflatable by shared boilerplate (docs/OPEN-QUESTIONS.md Q31). Returned as a
    /// set of feature tokens; order is not significant.
    fn spec_signature(&self, seed: u64) -> Vec<String>;
}

/// The number of distinct [`Generator::spec_signature`]s over `seed = 0..upto` —
/// a family's genuine, ungameable task diversity (Q31). This is a family-quality
/// measure checked at authoring time, distinct from the per-epoch view-distance
/// the sampler enforces to keep served prompts fresh.
pub fn spec_diversity(gen: &dyn Generator, upto: u64) -> usize {
    let mut seen = std::collections::HashSet::new();
    for s in 0..upto {
        let mut sig = gen.spec_signature(s);
        sig.sort();
        seen.insert(sig.join("|"));
    }
    seen.len()
}

/// Every registered family id. The run protocol serves these; keep it in sync with
/// [`family`] (a test asserts every id here resolves).
pub const FAMILY_IDS: &[&str] = &[
    "window-op",
    "error-handling",
    "stack-machine",
    "seq-transform",
];

/// Look up a family by id.
pub fn family(id: &str) -> Option<Box<dyn Generator>> {
    match id {
        "window-op" => Some(Box::new(window_op::WindowOpFamily)),
        "error-handling" => Some(Box::new(error_handling::ErrorHandlingFamily)),
        "stack-machine" => Some(Box::new(stack_machine::StackMachineFamily)),
        "seq-transform" => Some(Box::new(seq_transform::SeqTransformFamily)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_derivation_is_deterministic() {
        assert_eq!(
            derive_seed("2026-08", "window-op", 3),
            derive_seed("2026-08", "window-op", 3)
        );
        assert_ne!(
            derive_seed("2026-08", "window-op", 3),
            derive_seed("2026-08", "window-op", 4)
        );
    }

    #[test]
    fn canary_is_stable_and_seed_specific() {
        assert_eq!(mint_canary("window-op", 7), mint_canary("window-op", 7));
        assert_ne!(mint_canary("window-op", 7), mint_canary("window-op", 8));
        assert!(mint_canary("window-op", 7).starts_with("rb-"));
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn every_family_id_resolves() {
        // FAMILY_IDS must not drift from the family() registry.
        for id in FAMILY_IDS {
            assert!(
                family(id).is_some(),
                "FAMILY_IDS entry {id} does not resolve"
            );
        }
    }

    #[test]
    fn spec_diversity_is_the_structural_count() {
        // The honest, ungameable task-diversity ceiling (Q31): distinct structural
        // specs, constants excluded. window-op = 6 ops x 2 strides; error-handling
        // = 5 combines x 6 rule-types; stack-machine = 5 combines x 4 maps x 2
        // reorders; seq-transform = 4 filters x 4 maps x 3 terminals. Pinned so
        // narrowing a surface fails loudly.
        assert_eq!(
            spec_diversity(family("window-op").unwrap().as_ref(), 4000),
            12
        );
        assert_eq!(
            spec_diversity(family("error-handling").unwrap().as_ref(), 4000),
            30
        );
        assert_eq!(
            spec_diversity(family("stack-machine").unwrap().as_ref(), 4000),
            40
        );
        assert_eq!(
            spec_diversity(family("seq-transform").unwrap().as_ref(), 4000),
            48
        );
    }
}
