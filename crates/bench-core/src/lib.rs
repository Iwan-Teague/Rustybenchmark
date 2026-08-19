//! `bench-core` — the shared vocabulary of Rustybenchmark.
//!
//! This crate is pure: no I/O, no async, no process spawning. It depends on
//! nothing else in the workspace, and everything else depends on it. That
//! invariant (docs/13-architecture.md) is what keeps the type system coherent
//! as the leaf crates grow.
//!
//! Scope for the P0 spine: identifiers, the instance a model is graded on, the
//! layered oracle *vector*, the scoring arithmetic, and the rustc-error-code →
//! `FailureClass` lookup. Constraint (L3) and quality (L4) layers are declared
//! in the vector but not yet populated — that is P2.

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
/// categories (docs/04-categories.md); they are overridable per family. Only
/// `behavior` is populated in the P0 spine.
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
    pub unit: Option<f32>,
    pub property: Option<f32>,
    pub differential: Option<f32>,
    /// Weighted combination of whichever sub-oracles ran, in [0, 1].
    pub score: Option<f32>,
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
    // L2 behavior (0.70 weight by default)
    pub behavior: BehaviorScore,
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
/// task_score = (apply_ok && compile_ok) ? w_b*behavior + w_c*constraint + w_q*quality : 0.0
/// ```
///
/// In the P0 spine only the behaviour layer is populated, so the constraint and
/// quality terms contribute nothing and the behaviour weight is renormalised to
/// carry the whole score. When L3/L4 arrive (P2) this becomes the full weighted
/// sum without changing the gate.
pub fn composite_score(v: &OracleVector, w: &OracleWeights) -> f32 {
    if !(v.apply_ok && v.compile_ok) {
        return 0.0;
    }
    let b = v.behavior.score.unwrap_or(0.0);
    // Renormalise over the layers that actually ran. Spine: behaviour only.
    let present = w.behavior; // + w.constraint + w.quality once those land
    if present <= f32::EPSILON {
        return 0.0;
    }
    (w.behavior * b / present).clamp(0.0, 1.0)
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

    #[test]
    fn passing_behavior_scores_full_when_gates_pass() {
        let v = OracleVector {
            apply_ok: true,
            compile_ok: true,
            error_codes: vec![],
            warn_count: 0,
            behavior: BehaviorScore {
                unit: Some(1.0),
                property: None,
                differential: None,
                score: Some(1.0),
            },
            score: 0.0,
            failure_class: FailureClass::None,
        };
        assert!((composite_score(&v, &OracleWeights::default()) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn half_behavior_scores_half() {
        let v = OracleVector {
            apply_ok: true,
            compile_ok: true,
            error_codes: vec![],
            warn_count: 0,
            behavior: BehaviorScore {
                unit: Some(0.5),
                property: None,
                differential: None,
                score: Some(0.5),
            },
            score: 0.0,
            failure_class: FailureClass::Logic,
        };
        assert!((composite_score(&v, &OracleWeights::default()) - 0.5).abs() < 1e-6);
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
