//! `bench-core` — the shared vocabulary of Rustybenchmark.
//!
//! This crate is pure: no I/O, no async, no process spawning. It depends on
//! nothing else in the workspace, and everything else depends on it. That
//! invariant (docs/13-architecture.md) is what keeps the type system coherent
//! as the leaf crates grow.
//!
//! Scope so far: identifiers, the instance a model is graded on, the layered
//! oracle *vector*, the scoring arithmetic, the rustc-error-code →
//! `FailureClass` lookup, and per-layer weight renormalisation. L2 behaviour and
//! the L3 allocation constraint are populated; L2 property/differential and L4
//! quality are declared but not yet filled.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// A task family identifier, e.g. `"borrowck/split-mut-window"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A per-instance seed. Frozen tasks use a fixed value; generated tasks derive
/// it from the epoch / challenge nonce (docs/02-task-format.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seed(pub u64);

/// The blake3 identity of a work unit, rendered as `"blake3:<hex>"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnitId(pub String);

/// The atom of execution and checkpointing: one `(task, seed)` at a fixed plan
/// position. Independent and idempotent — re-running one reproduces both the
/// instance (generation is deterministic) and the grade (the oracle is).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUnit {
    pub task_id: TaskId,
    pub seed: Seed,
    pub index: u32,
}

impl WorkUnit {
    /// Deterministic identity. Same inputs → same id, which is what makes
    /// resume and replay-verification trivially correct.
    pub fn unit_id(&self) -> UnitId {
        let mut h = blake3::Hasher::new();
        h.update(self.task_id.0.as_bytes());
        h.update(&[0u8]); // domain separator between the two length-varying fields
        h.update(&self.seed.0.to_le_bytes());
        h.update(&self.index.to_le_bytes());
        UnitId(format!("blake3:{}", h.finalize().to_hex()))
    }
}

// ---------------------------------------------------------------------------
// Task manifest (the struct; parsing lives in the crate that reads task.toml)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// A fixed instance with no generation. Development and smoke only — refused
    /// by scored suites (docs/02-task-format.md).
    Frozen,
    Seeded,
    Mined,
}

/// Per-category oracle weights. Global defaults are wrong for several
/// categories (docs/04-categories.md); they are overridable per family
/// (`[weights]` in task.toml).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OracleWeights {
    pub behavior: f32,
    pub constraint: f32,
    pub quality: f32,
}

impl Default for OracleWeights {
    fn default() -> Self {
        // docs/03-oracle.md global default. behavior 0.70 / constraint 0.20 /
        // quality 0.10. Per-category overrides come in P2.
        OracleWeights {
            behavior: 0.70,
            constraint: 0.20,
            quality: 0.10,
        }
    }
}

// ---------------------------------------------------------------------------
// Instance — the concrete problem handed to a model
// ---------------------------------------------------------------------------

/// A materialised problem. `files` are shown to the model; `hidden` (the
/// oracle) is injected into a *separate* grading workspace after the model's
/// turn, never present while the model runs (docs/03-oracle.md).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub prompt: String,
    pub files: BTreeMap<PathBuf, String>,
    pub hidden: BTreeMap<PathBuf, String>,
    /// A unique low-frequency string embedded in the prompt; its later
    /// appearance in a public corpus is direct evidence of leakage.
    pub canary: String,
}

// ---------------------------------------------------------------------------
// The oracle vector — a graded result is a vector, never a bit
// ---------------------------------------------------------------------------

/// Behaviour layer (L2). Sub-oracles are `Option` because a gate failure below
/// them short-circuits: a solution that does not compile has no behaviour score.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BehaviorScore {
    /// Fraction of hidden example tests passed.
    pub unit: Option<f32>,
    /// Fraction of invariant properties held.
    pub property: Option<f32>,
    /// Agreement with the hidden reference over generated inputs. This is what
    /// catches a solution that passes every visible test and is still wrong —
    /// unit tests overstate correctness by 30–32% (docs/01, docs/03).
    pub differential: Option<f32>,
    /// Weighted combination of whichever sub-oracles ran, in [0, 1].
    pub score: Option<f32>,
}

impl BehaviorScore {
    /// Recompute `score` from the sub-oracles that ran, using the docs/03
    /// weights (unit 0.3 / property 0.5 / differential 0.2) renormalised over
    /// those present. `None` if none ran.
    pub fn recompute(&mut self) {
        const W_UNIT: f32 = 0.3;
        const W_PROP: f32 = 0.5;
        const W_DIFF: f32 = 0.2;
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        if let Some(u) = self.unit {
            num += W_UNIT * u;
            den += W_UNIT;
        }
        if let Some(p) = self.property {
            num += W_PROP * p;
            den += W_PROP;
        }
        if let Some(d) = self.differential {
            num += W_DIFF * d;
            den += W_DIFF;
        }
        self.score = (den > f32::EPSILON).then(|| num / den);
    }
}

/// Constraint layer (L3). Each check is optional — it contributes to the layer
/// score only when it ran. The layer score is the mean of the boolean checks
/// that produced a verdict. Allocation is the first check implemented; clippy,
/// fmt and `syn`-based checks follow.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ConstraintScore {
    /// The hot path stayed within its allocation budget (docs/03-oracle.md:
    /// allocation is measured, not name-blacklisted).
    pub alloc_ok: Option<bool>,
    pub clippy_clean: Option<bool>,
    pub fmt_ok: Option<bool>,
    pub unsafe_blocks: Option<u32>,
    /// Human-readable violations, e.g. `"alloc: hot path allocated"`.
    pub violations: Vec<String>,
    /// Mean of the boolean checks that ran, in [0, 1]; `None` if none ran.
    pub score: Option<f32>,
}

impl ConstraintScore {
    /// Recompute `score` as the mean of the boolean checks present. `unsafe_blocks`
    /// is recorded but not folded in here — a task that forbids `unsafe` expresses
    /// that as its own check in a later increment.
    pub fn recompute(&mut self) {
        let mut sum = 0.0f32;
        let mut n = 0u32;
        for b in [self.alloc_ok, self.clippy_clean, self.fmt_ok]
            .into_iter()
            .flatten()
        {
            sum += if b { 1.0 } else { 0.0 };
            n += 1;
        }
        self.score = (n > 0).then(|| sum / n as f32);
    }
}

/// Derived from rustc error codes. This is the per-category diagnostic no
/// general-purpose coding benchmark can produce (docs/03-oracle.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureClass {
    Borrowck,
    Trait,
    Type,
    Lifetime,
    AsyncSend,
    Syntax,
    Resolve,
    Idiom,
    /// Compiled, failed L2 behaviour.
    Logic,
    /// Compiled, passed L2, failed L3 constraint.
    Constraint,
    Other,
    /// No failure — the unit passed.
    None,
}

/// The full graded result for one attempt. Layers run in order; a failed gate
/// short-circuits later layers but every field is recorded.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OracleVector {
    // L0 apply gate
    pub apply_ok: bool,
    // L1 compile gate
    pub compile_ok: bool,
    pub error_codes: Vec<String>,
    pub warn_count: u32,
    // L2 behavior
    pub behavior: BehaviorScore,
    // L3 constraint
    pub constraint: ConstraintScore,
    // Composite in [0, 1]; 0.0 unless both gates pass.
    pub score: f32,
    pub failure_class: FailureClass,
}

impl OracleVector {
    /// A vector for a unit whose model response could not even be applied.
    pub fn apply_failed() -> Self {
        OracleVector {
            apply_ok: false,
            compile_ok: false,
            error_codes: Vec::new(),
            warn_count: 0,
            behavior: BehaviorScore::default(),
            constraint: ConstraintScore::default(),
            score: 0.0,
            failure_class: FailureClass::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// The composite score, gated on apply + compile.
///
/// ```text
/// task_score = (apply_ok && compile_ok) ? Σ w_layer·score_layer / Σ w_layer : 0.0
/// ```
///
/// The sum runs over whichever layers actually produced a score, renormalised by
/// their weights. So a task with no L3 constraint check scores purely on
/// behaviour, and a constraint-dominant task (docs/04, `borrow-lifetimes`:
/// behavior 0.35 / constraint 0.55) penalises a behaviourally-correct but
/// allocation-heavy solution — the fix for REVIEW.md S6. Quality (L4) slots into
/// the same sum when it arrives, without changing the gate.
pub fn composite_score(v: &OracleVector, w: &OracleWeights) -> f32 {
    if !(v.apply_ok && v.compile_ok) {
        return 0.0;
    }
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    if let Some(b) = v.behavior.score {
        num += w.behavior * b;
        den += w.behavior;
    }
    if let Some(c) = v.constraint.score {
        num += w.constraint * c;
        den += w.constraint;
    }
    // quality (L4) joins here in a later increment.
    if den <= f32::EPSILON {
        return 0.0;
    }
    (num / den).clamp(0.0, 1.0)
}

/// Map rustc error codes to a `FailureClass`, most-specific first. When a unit
/// compiled, the caller passes the behaviour outcome instead of codes.
pub fn classify_error_codes(codes: &[String]) -> FailureClass {
    // Order matters: the first matching family wins, so borrow/lifetime beats
    // the generic type bucket.
    const BORROWCK: &[&str] = &[
        "E0499", "E0502", "E0503", "E0505", "E0506", "E0382", "E0384",
    ];
    const LIFETIME: &[&str] = &["E0597", "E0515", "E0521", "E0623", "E0495", "E0700"];
    const TRAIT: &[&str] = &["E0277", "E0119", "E0210", "E0271", "E0599"];
    const TYPE: &[&str] = &["E0308", "E0053", "E0061", "E0069"];
    const RESOLVE: &[&str] = &["E0425", "E0433", "E0412", "E0405"];
    const SYNTAX: &[&str] = &["E0001"];

    let has = |set: &[&str]| codes.iter().any(|c| set.contains(&c.as_str()));

    if has(BORROWCK) {
        FailureClass::Borrowck
    } else if has(LIFETIME) {
        FailureClass::Lifetime
    } else if has(TRAIT) {
        FailureClass::Trait
    } else if has(TYPE) {
        FailureClass::Type
    } else if has(RESOLVE) {
        FailureClass::Resolve
    } else if has(SYNTAX) {
        FailureClass::Syntax
    } else if codes.is_empty() {
        FailureClass::None
    } else {
        FailureClass::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_id_is_deterministic() {
        let u = WorkUnit {
            task_id: TaskId("borrowck/x".into()),
            seed: Seed(42),
            index: 0,
        };
        assert_eq!(u.unit_id(), u.unit_id());
    }

    #[test]
    fn unit_id_separates_task_from_seed() {
        // "a" + seed 0 must not collide with "" + seed derived from "a"'s bytes.
        let a = WorkUnit {
            task_id: TaskId("a".into()),
            seed: Seed(0),
            index: 0,
        };
        let b = WorkUnit {
            task_id: TaskId("".into()),
            seed: Seed(0),
            index: 0,
        };
        assert_ne!(a.unit_id(), b.unit_id());
    }

    #[test]
    fn gate_zeroes_the_score() {
        let mut v = OracleVector::apply_failed();
        v.behavior.score = Some(1.0);
        assert_eq!(composite_score(&v, &OracleWeights::default()), 0.0);
    }

    fn vector(behavior: Option<f32>, constraint: Option<f32>) -> OracleVector {
        let constraint_score = ConstraintScore {
            alloc_ok: constraint.map(|s| s >= 1.0),
            score: constraint,
            ..Default::default()
        };
        OracleVector {
            apply_ok: true,
            compile_ok: true,
            error_codes: vec![],
            warn_count: 0,
            behavior: BehaviorScore {
                unit: behavior,
                property: None,
                differential: None,
                score: behavior,
            },
            constraint: constraint_score,
            score: 0.0,
            failure_class: FailureClass::None,
        }
    }

    #[test]
    fn passing_behavior_scores_full_when_gates_pass() {
        assert!(
            (composite_score(&vector(Some(1.0), None), &OracleWeights::default()) - 1.0).abs()
                < 1e-6
        );
    }

    #[test]
    fn half_behavior_scores_half() {
        assert!(
            (composite_score(&vector(Some(0.5), None), &OracleWeights::default()) - 0.5).abs()
                < 1e-6
        );
    }

    #[test]
    fn constraint_dominant_weights_penalise_clone_everything() {
        // A behaviourally-correct (1.0) but allocation-failing (0.0) solution
        // under borrow-lifetimes weights (behavior 0.35 / constraint 0.55).
        let w = OracleWeights {
            behavior: 0.35,
            constraint: 0.55,
            quality: 0.10,
        };
        let v = vector(Some(1.0), Some(0.0));
        let s = composite_score(&v, &w);
        // (0.35*1 + 0.55*0) / (0.35 + 0.55) = 0.389
        assert!((s - 0.35 / 0.90).abs() < 1e-4, "got {s}");
        // The same answer under behaviour-dominant defaults scores much higher
        // (0.70/0.90 = 0.778): constraint weighting roughly halves it.
        let behav_dom = composite_score(&v, &OracleWeights::default());
        assert!(
            behav_dom > s + 0.3,
            "constraint weighting must move the score: {behav_dom} vs {s}"
        );
    }

    #[test]
    fn differential_catches_unit_passing_but_wrong() {
        // The headline: a solution that passes every example test (unit 1.0) but
        // disagrees with the reference (differential 0.0). docs/03 weights make
        // behaviour 0.6, not the 1.0 a unit-only oracle would report.
        let mut b = BehaviorScore {
            unit: Some(1.0),
            differential: Some(0.0),
            ..Default::default()
        };
        b.recompute();
        // (0.3*1 + 0.2*0) / (0.3 + 0.2) = 0.6
        assert!((b.score.unwrap() - 0.6).abs() < 1e-4, "got {:?}", b.score);
    }

    #[test]
    fn behavior_score_renormalises_over_present_suboracles() {
        // unit only: behaviour == unit.
        let mut b = BehaviorScore {
            unit: Some(0.8),
            ..Default::default()
        };
        b.recompute();
        assert!((b.score.unwrap() - 0.8).abs() < 1e-6);
        // none ran: None.
        let mut empty = BehaviorScore::default();
        empty.recompute();
        assert_eq!(empty.score, None);
    }

    #[test]
    fn constraint_score_is_mean_of_present_checks() {
        let mut c = ConstraintScore {
            alloc_ok: Some(true),
            fmt_ok: Some(false),
            ..Default::default()
        };
        c.recompute();
        assert_eq!(c.score, Some(0.5));
    }

    #[test]
    fn borrowck_beats_type_bucket() {
        // A response that trips both E0499 (borrow) and E0308 (type) is a
        // borrow-checker failure first.
        assert_eq!(
            classify_error_codes(&["E0308".into(), "E0499".into()]),
            FailureClass::Borrowck
        );
    }

    #[test]
    fn empty_codes_is_none() {
        assert_eq!(classify_error_codes(&[]), FailureClass::None);
    }

    #[test]
    fn unknown_code_is_other() {
        assert_eq!(classify_error_codes(&["E9999".into()]), FailureClass::Other);
    }
}
