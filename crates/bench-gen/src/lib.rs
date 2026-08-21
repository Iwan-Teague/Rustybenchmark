//! `bench-gen` — turns a seed into a fresh problem instance.
//!
//! Tasks are *generators*, not files. A family is a function from a seed to a
//! concrete instance, its reference implementation, its oracle, and the skeleton
//! the model completes — all built from the same seed, **solution-first**, so
//! the oracle is correct by construction (docs/02-task-format.md, ADR-0003).
//!
//! This crate provides seed derivation, canary minting, the `Generator` trait, the
//! distance-aware [`epoch`] sampler, and the family registry ([`FAMILY_IDS`] /
//! [`family`]). Each family draws its *structural* choices from the seed — the
//! operation, not just identifiers — so seeds resist memorisation rather than
//! merely renaming; [`spec_diversity`] counts those distinct skills (Q31).
//! `validate-family` in the CLI runs the compile-dependent construction gates
//! (reference-passes-its-own-oracle, skeleton-fails, baselines-caught); the pure
//! invariants (determinism, canary, category, spec-signature, the diversity floor)
//! are guarded generically over the whole registry in this crate's tests.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod bit_manipulation;
pub mod checked_eval;
pub mod distance;
pub mod dual_region;
pub mod epoch;
pub mod error_handling;
pub mod generic_select;
pub mod grid_reduce;
pub mod idiom_refactor;
pub mod seq_transform;
pub mod stack_machine;
pub mod string_processing;
pub mod traits_generics;
pub mod unsafe_core;
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
    /// L3 AST constraint: max `unsafe` usages. `None` opts the check out entirely
    /// (the constraint layer then reflects only the other checks) — used where
    /// `unsafe` is irrelevant (`idiom-refactor`) or mandatory (`unsafe-core`).
    pub max_unsafe: Option<u32>,
    /// L3 AST constraint: forbidden type/function/method names.
    pub forbidden_paths: Vec<String>,
    /// L3 constraint: run `cargo clippy` on the answer and score its cleanliness
    /// (docs/03 — the idiomaticity signal, dominant for `idiom-refactor`).
    pub check_clippy: bool,
    /// Clippy lints to allow (not counted against cleanliness), e.g.
    /// `clippy::needless_range_loop` where an index loop is legitimate.
    pub clippy_allow: Vec<String>,
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

/// The minimum [`spec_diversity`] a family must clear to ship — docs/17's
/// "comfortably above [a per-epoch seed count of] 8". Provisional, like
/// `bench_stats::CLUSTER_FLOOR`: the real value is fixed once Phase 4 sets the
/// per-epoch seed count (docs/OPEN-QUESTIONS.md Q30/Q31). Enforced generically over
/// [`FAMILY_IDS`] in this crate's tests; every current family clears it with
/// headroom (the smallest ships at 12).
pub const MIN_SPEC_DIVERSITY: usize = 8;

/// Every registered family id. The run protocol serves these; keep it in sync with
/// [`family`] (a test asserts every id here resolves).
pub const FAMILY_IDS: &[&str] = &[
    "window-op",
    "error-handling",
    "stack-machine",
    "seq-transform",
    "grid-reduce",
    "trait-impl",
    "bit-ops",
    "raw-ptr",
    "str-transform",
    "dual-region",
    "generic-select",
    "checked-eval",
    "idiom-loop",
];

/// Look up a family by id.
pub fn family(id: &str) -> Option<Box<dyn Generator>> {
    match id {
        "window-op" => Some(Box::new(window_op::WindowOpFamily)),
        "error-handling" => Some(Box::new(error_handling::ErrorHandlingFamily)),
        "stack-machine" => Some(Box::new(stack_machine::StackMachineFamily)),
        "seq-transform" => Some(Box::new(seq_transform::SeqTransformFamily)),
        "grid-reduce" => Some(Box::new(grid_reduce::GridReduceFamily)),
        "trait-impl" => Some(Box::new(traits_generics::TraitsGenericsFamily)),
        "bit-ops" => Some(Box::new(bit_manipulation::BitManipulationFamily)),
        "raw-ptr" => Some(Box::new(unsafe_core::UnsafeCoreFamily)),
        "str-transform" => Some(Box::new(string_processing::StringProcessingFamily)),
        "dual-region" => Some(Box::new(dual_region::DualRegionFamily)),
        "generic-select" => Some(Box::new(generic_select::GenericSelectFamily)),
        "checked-eval" => Some(Box::new(checked_eval::CheckedEvalFamily)),
        "idiom-loop" => Some(Box::new(idiom_refactor::IdiomRefactorFamily)),
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
    fn every_family_meets_the_pure_construction_invariants() {
        // A generic drift-guard over the whole registry: the invariants that need no
        // toolchain (the compile/differential gates stay in `validate-family`). A new
        // family that forgets determinism, its canary, a category, a spec-signature,
        // or enough diversity fails here in `cargo test`, not only in the manual CLI.
        for id in FAMILY_IDS {
            let g = family(id).unwrap();
            assert!(!g.category().is_empty(), "{id}: empty category");
            assert!(
                spec_diversity(g.as_ref(), 4000) >= MIN_SPEC_DIVERSITY,
                "{id}: spec-diversity below the floor of {MIN_SPEC_DIVERSITY}"
            );
            for seed in [0u64, 1, 7, 42, 1000] {
                let a = g.generate(seed);
                let b = g.generate(seed);
                assert_eq!(
                    a.prompt, b.prompt,
                    "{id} seed {seed}: prompt not deterministic"
                );
                assert_eq!(
                    a.files, b.files,
                    "{id} seed {seed}: files not deterministic"
                );
                assert_eq!(
                    a.hidden, b.hidden,
                    "{id} seed {seed}: hidden not deterministic"
                );
                assert!(
                    a.prompt.contains(&a.canary),
                    "{id} seed {seed}: prompt is missing its canary"
                );
                assert!(
                    !g.spec_signature(seed).is_empty(),
                    "{id} seed {seed}: empty spec_signature"
                );
                assert_eq!(
                    a.category,
                    g.category(),
                    "{id} seed {seed}: instance category disagrees with the family"
                );
            }
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
        // grid-reduce = 2 axes x 6 reductions.
        assert_eq!(
            spec_diversity(family("grid-reduce").unwrap().as_ref(), 4000),
            12
        );
        assert_eq!(
            spec_diversity(family("seq-transform").unwrap().as_ref(), 4000),
            48
        );
        // trait-impl = 4 keep predicates x 5 reductions.
        assert_eq!(
            spec_diversity(family("trait-impl").unwrap().as_ref(), 4000),
            20
        );
        // bit-ops = 5 masks x 4 transforms.
        assert_eq!(
            spec_diversity(family("bit-ops").unwrap().as_ref(), 4000),
            20
        );
        // raw-ptr = 4 access patterns x 5 reductions.
        assert_eq!(
            spec_diversity(family("raw-ptr").unwrap().as_ref(), 4000),
            20
        );
        // str-transform = 4 filters x 3 case-maps x 2 orders.
        assert_eq!(
            spec_diversity(family("str-transform").unwrap().as_ref(), 4000),
            24
        );
        // dual-region = 6 pairwise ops x 2 pairings (a second borrow-lifetimes family).
        assert_eq!(
            spec_diversity(family("dual-region").unwrap().as_ref(), 4000),
            12
        );
        // generic-select = 4 selects x 4 projections (a second traits-generics family).
        assert_eq!(
            spec_diversity(family("generic-select").unwrap().as_ref(), 4000),
            16
        );
        // checked-eval = 3 checked folds x 4 guard types (a second error-handling family).
        assert_eq!(
            spec_diversity(family("checked-eval").unwrap().as_ref(), 4000),
            12
        );
        // idiom-loop = 4 filters x 4 maps (the compositional idiom-refactor family).
        assert_eq!(
            spec_diversity(family("idiom-loop").unwrap().as_ref(), 4000),
            16
        );
    }
}
