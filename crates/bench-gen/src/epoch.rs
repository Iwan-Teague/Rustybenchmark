//! Distance-aware epoch sampling — the Q30 second-order fix.
//!
//! The anti-twin *gate* ([`crate::distance`], docs/02) measures average distance
//! and reports near-twin pairs, but nothing stops a run from *serving* two of
//! them: a family's distinct-task space is finite, so `N` seeds drawn by raw
//! index can collide even when the family's median distance is healthy
//! (measured: `error-handling` at median 0.263 still emits 18/45 near-twin pairs
//! — see docs/OPEN-QUESTIONS.md Q3/Q30).
//!
//! This module closes that gap. [`plan_epoch`] draws candidate seeds in order and
//! *rejects* any candidate whose model-view is closer than the floor to an
//! already-accepted sibling, so the `N` seeds it returns are pairwise-distant by
//! construction. It is a deterministic function of `(family, epoch, n, threshold)`,
//! so resume and replay serve the identical set (docs/02: same seed → same run).
//!
//! Crucially, the distance is measured on exactly the string the gate uses —
//! [`view_of`] — so a plan that clears `threshold` here also clears the CI gate.
//! If a family genuinely cannot supply `N` distinct instances within the attempt
//! budget, [`plan_epoch`] returns [`Exhausted`] rather than silently serving
//! twins: that is a real family defect and is meant to be loud.
//!
//! Post-Q31, view-distance is a weak per-epoch constraint (seed-varied examples
//! saturate it). The stronger, ungameable one is *skill* distinctness:
//! [`plan_epoch_distinct_skills`] serves `n` seeds covering `n` different
//! `spec_signature`s (docs/OPEN-QUESTIONS.md Q31), still enforcing view-distance
//! for prompt freshness. It `Exhausted`s once the family's distinct skills run
//! out — the loud signal that the family is too narrow for the per-epoch count.

use crate::{derive_seed, distance, GeneratedTask, Generator};
use std::path::Path;

/// The canonical anti-twin floor for parametric families: `min_instance_distance`
/// in docs/02-task-format.md. Kept here so the gate and the sampler cannot drift.
pub const MIN_INSTANCE_DISTANCE: f64 = 0.25;

/// The model-visible text of an already-generated instance: prompt + skeleton,
/// exactly what the anti-twin gate measures distance on. One definition so the
/// sampler's guarantee and `validate-family`'s report can never diverge.
pub fn view_of(task: &GeneratedTask) -> String {
    let skeleton = task
        .files
        .get(Path::new(&task.answer_path))
        .cloned()
        .unwrap_or_default();
    format!("{}\n{}", task.prompt, skeleton)
}

/// Convenience: generate `seed` and return its model-view.
pub fn model_view(gen: &dyn Generator, seed: u64) -> String {
    view_of(&gen.generate(seed))
}

/// A deterministic epoch plan: `seeds` whose pairwise model-view distances all
/// clear the threshold. `attempts` is how many candidates were examined to find
/// them; `min_pairwise` is the realised minimum (>= threshold by construction);
/// `specs` is the canonical spec-signature of each served seed (Q31).
#[derive(Debug, Clone)]
pub struct EpochPlan {
    pub seeds: Vec<u64>,
    pub views: Vec<String>,
    pub specs: Vec<String>,
    pub attempts: u32,
    pub min_pairwise: f64,
}

impl EpochPlan {
    /// Candidates examined but rejected as too close to an accepted sibling.
    pub fn rejected(&self) -> u32 {
        self.attempts - self.seeds.len() as u32
    }

    /// How many distinct skills the served seeds cover. Equal to `seeds.len()`
    /// for a plan built by [`plan_epoch_distinct_skills`]; may be fewer for the
    /// view-only [`plan_epoch_from`], which does not reject spec-collisions.
    pub fn distinct_skills(&self) -> usize {
        let mut set: Vec<&String> = self.specs.iter().collect();
        set.sort();
        set.dedup();
        set.len()
    }
}

/// The canonical (order-independent) spec-signature key for `seed` — the family's
/// structural identity (Q31), used to detect within-epoch skill collisions.
pub fn spec_key(gen: &dyn Generator, seed: u64) -> String {
    let mut sig = gen.spec_signature(seed);
    sig.sort();
    sig.join("|")
}

/// The family could not supply `wanted` pairwise-distant instances within the
/// attempt budget — a finite-variant-space defect, not a transient error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhausted {
    pub accepted: usize,
    pub wanted: usize,
    pub attempts: u32,
}

impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "epoch sampling exhausted: accepted {}/{} distinct instances after {} attempts \
             — the family's distinct-task space is too small for this threshold",
            self.accepted, self.wanted, self.attempts
        )
    }
}

impl std::error::Error for Exhausted {}

/// Select `n` seeds from `candidates` whose model-views are all at least
/// `threshold` apart, rejecting collisions greedily in candidate order. Stops at
/// `n` acceptances or `max_attempts` candidates examined, whichever comes first.
///
/// Greedy and order-dependent by design: the result is reproducible from the
/// candidate order, which is what makes an epoch replayable.
pub fn plan_epoch_from<I>(
    gen: &dyn Generator,
    candidates: I,
    n: usize,
    threshold: f64,
    max_attempts: u32,
) -> Result<EpochPlan, Exhausted>
where
    I: IntoIterator<Item = u64>,
{
    let mut seeds: Vec<u64> = Vec::with_capacity(n);
    let mut views: Vec<String> = Vec::with_capacity(n);
    let mut attempts = 0u32;

    for seed in candidates {
        if seeds.len() >= n || attempts >= max_attempts {
            break;
        }
        attempts += 1;
        let view = model_view(gen, seed);
        let far_enough = views
            .iter()
            .all(|v| distance::shingle_distance(v, &view, distance::K) >= threshold);
        if far_enough {
            seeds.push(seed);
            views.push(view);
        }
    }

    if seeds.len() < n {
        return Err(Exhausted {
            accepted: seeds.len(),
            wanted: n,
            attempts,
        });
    }

    let min_pairwise = min_pairwise_distance(&views);
    debug_assert!(
        min_pairwise >= threshold || seeds.len() < 2,
        "accepted set must satisfy the floor by construction"
    );
    let specs = seeds.iter().map(|&s| spec_key(gen, s)).collect();
    Ok(EpochPlan {
        seeds,
        views,
        specs,
        attempts,
        min_pairwise,
    })
}

/// Select `n` seeds that cover `n` **distinct skills** (Q31): a candidate is
/// rejected if its spec-signature is already served, *or* if its model-view is
/// within `view_floor` of an accepted sibling. The first constraint is the point
/// — an epoch should test different skills, not the same skill `n` times with
/// different constants; the second keeps prompts fresh (contamination). Exhausts
/// if the family has fewer than `n` distinct skills, which is the loud signal
/// that the family is too narrow for this per-epoch count.
///
/// Greedy and order-dependent, so reproducible from the candidate order.
pub fn plan_epoch_distinct_skills<I>(
    gen: &dyn Generator,
    candidates: I,
    n: usize,
    view_floor: f64,
    max_attempts: u32,
) -> Result<EpochPlan, Exhausted>
where
    I: IntoIterator<Item = u64>,
{
    let mut seeds: Vec<u64> = Vec::with_capacity(n);
    let mut views: Vec<String> = Vec::with_capacity(n);
    let mut specs: Vec<String> = Vec::with_capacity(n);
    let mut attempts = 0u32;

    for seed in candidates {
        if seeds.len() >= n || attempts >= max_attempts {
            break;
        }
        attempts += 1;
        let key = spec_key(gen, seed);
        if specs.contains(&key) {
            continue; // skill-collision: this epoch already covers it
        }
        let view = model_view(gen, seed);
        let fresh = views
            .iter()
            .all(|v| distance::shingle_distance(v, &view, distance::K) >= view_floor);
        if fresh {
            seeds.push(seed);
            views.push(view);
            specs.push(key);
        }
    }

    if seeds.len() < n {
        return Err(Exhausted {
            accepted: seeds.len(),
            wanted: n,
            attempts,
        });
    }

    let min_pairwise = min_pairwise_distance(&views);
    Ok(EpochPlan {
        seeds,
        views,
        specs,
        attempts,
        min_pairwise,
    })
}

/// [`plan_epoch_from`] over the production seed pool: `derive_seed(epoch, id, i)`
/// for `i = 0, 1, 2, …`. This is what a real epoch serves.
pub fn plan_epoch(
    gen: &dyn Generator,
    epoch: &str,
    n: usize,
    threshold: f64,
    max_attempts: u32,
) -> Result<EpochPlan, Exhausted> {
    let id = gen.id().to_string();
    let candidates = (0u32..).map(move |i| derive_seed(epoch, &id, i));
    plan_epoch_from(gen, candidates, n, threshold, max_attempts)
}

/// Minimum pairwise distance over a set of views. `1.0` (vacuously satisfied) for
/// fewer than two views.
pub fn min_pairwise_distance(views: &[String]) -> f64 {
    let mut min = 1.0f64;
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let d = distance::shingle_distance(&views[i], &views[j], distance::K);
            if d < min {
                min = d;
            }
        }
    }
    min
}

/// Greedy count of items that are all pairwise `>= threshold`, taken in order —
/// the distinct-at-floor capacity for whatever text the caller measured on.
pub fn greedy_distinct_count(views: &[String], threshold: f64) -> usize {
    let mut kept: Vec<&String> = Vec::new();
    for v in views {
        if kept
            .iter()
            .all(|k| distance::shingle_distance(k, v, distance::K) >= threshold)
        {
            kept.push(v);
        }
    }
    kept.len()
}

/// Distinct-at-floor capacity measured on the **reference** (the solution) for
/// `seed = 0..upto`. This is the honest anti-*memorisation-of-solution* measure:
/// unlike view-capacity it is not inflated by seed-varied worked examples (which
/// change the prompt without changing the answer). It is, however, *deflated* by
/// shared solution boilerplate — two references that differ only in a constant or
/// one expression read as near-twins even when the underlying skill differs — so
/// it under-counts genuine task diversity for heavily-scaffolded families. Neither
/// text measure is the true diversity, which is the structural spec count (Q31).
pub fn reference_capacity(gen: &dyn Generator, upto: u64, threshold: f64) -> usize {
    let refs: Vec<String> = (0..upto).map(|s| gen.reference_code(s)).collect();
    greedy_distinct_count(&refs, threshold)
}

// ---------------------------------------------------------------------------
// Run plan — the paired-core / fresh-probe seed sets for one epoch (ADR-0009)
// ---------------------------------------------------------------------------

/// Which seed set a unit belongs to (ADR-0009).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    /// Paired core — `blake3(epoch ‖ family ‖ i)`, identical for every submitter,
    /// **scored**; the basis of all published figures and McNemar pairing.
    Core,
    /// Fresh probe — `blake3(probe_nonce ‖ family ‖ i)`, **never scored**; the
    /// precomputation detector (the sign test pairs each probe with its family's core).
    Probe,
}

impl UnitKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnitKind::Core => "core",
            UnitKind::Probe => "probe",
        }
    }
}

/// One planned unit of an epoch run: a family, its seed set, the index within that set,
/// and the derived seed. Idempotent — the seed fully determines the instance and grade
/// (docs/08), which is what makes resume trivially correct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunUnit {
    pub family: String,
    pub kind: UnitKind,
    pub index: u32,
    pub seed: u64,
}

impl RunUnit {
    /// Stable within-epoch key for resume dedup. The epoch is implicit (a plan and the
    /// journal filter share one epoch), so `family | kind | index` identifies the unit.
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.family, self.kind.as_str(), self.index)
    }
}

/// The probe nonce for an epoch. In production this is a per-batch challenge nonce
/// ([docs/10](../../../docs/10-integrity.md), ADR-0009), so probe seeds do not exist
/// before the run is requested. Locally it is derived from the epoch so a run is
/// reproducible and resumable while still occupying a seed space disjoint from the core.
pub fn probe_nonce(epoch: &str) -> String {
    format!("{epoch}::probe")
}

/// Plan one epoch: `n_core` paired-core plus `n_probe` fresh-probe seeds for each family,
/// in a deterministic order. Core seeds derive from `epoch` (shared across submitters);
/// probe seeds derive from [`probe_nonce`] (the detector set).
pub fn plan_run(families: &[&str], epoch: &str, n_core: u32, n_probe: u32) -> Vec<RunUnit> {
    let probe = probe_nonce(epoch);
    let mut out = Vec::new();
    for &family in families {
        for index in 0..n_core {
            out.push(RunUnit {
                family: family.to_string(),
                kind: UnitKind::Core,
                index,
                seed: derive_seed(epoch, family, index),
            });
        }
        for index in 0..n_probe {
            out.push(RunUnit {
                family: family.to_string(),
                kind: UnitKind::Probe,
                index,
                seed: derive_seed(&probe, family, index),
            });
        }
    }
    out
}

/// Filter a plan to the units not yet recorded (by [`RunUnit::key`]) — the resume step.
/// Because units are idempotent, skipping the done ones and running the rest reproduces
/// exactly the same journal a single uninterrupted run would have (docs/09).
pub fn remaining(plan: &[RunUnit], done: &std::collections::HashSet<String>) -> Vec<RunUnit> {
    plan.iter()
        .filter(|u| !done.contains(&u.key()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error_handling::ErrorHandlingFamily, window_op::WindowOpFamily};

    #[test]
    fn plan_is_deterministic() {
        let g = WindowOpFamily;
        let a = plan_epoch(&g, "2026-08", 6, MIN_INSTANCE_DISTANCE, 400).unwrap();
        let b = plan_epoch(&g, "2026-08", 6, MIN_INSTANCE_DISTANCE, 400).unwrap();
        assert_eq!(a.seeds, b.seeds, "same inputs must serve the same seeds");
    }

    #[test]
    fn accepted_set_clears_the_floor() {
        // The guarantee: within a family's capacity, every served pair is at least
        // the floor apart. 6 is inside window-op's measured capacity of 8.
        let g = WindowOpFamily;
        let plan = plan_epoch(&g, "e1", 6, MIN_INSTANCE_DISTANCE, 500).unwrap();
        assert_eq!(plan.seeds.len(), 6);
        assert!(
            plan.min_pairwise >= MIN_INSTANCE_DISTANCE,
            "min pairwise {} < floor {}",
            plan.min_pairwise,
            MIN_INSTANCE_DISTANCE
        );
    }

    #[test]
    fn view_distance_saturates_but_reference_distance_does_not() {
        // The Q31 finding, pinned. Seed-varied worked examples make every *prompt*
        // distinct, so view-distance no longer collides even at tiny N — while the
        // *solutions* still contain near-twins (few genuine logics, shared plumbing).
        // A sampler measuring view-distance therefore rejects nothing and masks low
        // solution diversity; reference-distance is where the real collisions live.
        let g = WindowOpFamily;
        let views: Vec<String> = (0..10u64).map(|s| model_view(&g, s)).collect();
        let refs: Vec<String> = (0..10u64).map(|s| g.reference_code(s)).collect();
        assert!(
            min_pairwise_distance(&views) >= MIN_INSTANCE_DISTANCE,
            "view-distance is saturated by example variation"
        );
        assert!(
            min_pairwise_distance(&refs) < MIN_INSTANCE_DISTANCE,
            "but the solutions still contain near-twins"
        );
    }

    #[test]
    fn exhaustion_is_reported_not_hidden() {
        // No two instances of one family are fully disjoint (shared boilerplate),
        // so a threshold of 1.0 can never seat a second seed. The sampler must
        // surface that as Exhausted, never serve a twin.
        let g = ErrorHandlingFamily;
        let err = plan_epoch_from(&g, 0u64..1000, 3, 1.0, 200).unwrap_err();
        assert!(err.accepted < err.wanted);
        assert_eq!(err.accepted, 1, "only the first seed can ever be seated");
    }

    // Distinct-at-floor capacity, pinned as a regression guard and as the
    // quantified evidence behind Q30. Capacity is far below what a family's
    // *median* distance suggests: pairwise-mutual distance is a much stronger
    // constraint than average distance. If a family change drops capacity below
    // the per-epoch seed count, the family is memorisation-vulnerable — so these
    // are meant to fail loudly, not be bumped silently.

    // Capacity is pinned on the REFERENCE (the solution), not the model-view.
    // View-capacity is gameable: seed-varied worked examples make every prompt
    // distinct without changing the answer (window-op saturates at 100% acceptance).
    // Reference-capacity measures how many genuinely different *solutions* a family
    // serves — the honest anti-memorisation number. It under-counts (shared
    // boilerplate reads as twin), so these are lower bounds / documented ceilings,
    // not diversity itself, which is the structural spec count (docs Q31).

    #[test]
    fn window_op_reference_capacity_is_healthy() {
        // 6 in-place operations (incl. Negate, AddConst) x rotate amounts x strides.
        let g = WindowOpFamily;
        let cap = reference_capacity(&g, 400, MIN_INSTANCE_DISTANCE);
        assert!(
            cap >= 18,
            "window-op should serve many distinct solutions, got {cap}"
        );
    }

    #[test]
    fn error_handling_reference_capacity_is_low_by_design() {
        // The pinned-enum plumbing dominates every solution, so 5 combines x 6 rules
        // collapse to a single-digit reference-capacity: the family tests plumbing,
        // not error design (docs/04), and its genuine solution diversity is small.
        // Widening the surface raised *view*-capacity (fresh prompts) but not this.
        // Pinned as a standing, honest limitation — see Q31.
        let g = ErrorHandlingFamily;
        let cap = reference_capacity(&g, 400, MIN_INSTANCE_DISTANCE);
        assert!(
            (5..=12).contains(&cap),
            "error-handling reference-capacity is expected to be low, got {cap}"
        );
    }

    #[test]
    fn view_matches_prompt_plus_skeleton() {
        let g = WindowOpFamily;
        let task = g.generate(3);
        let v = view_of(&task);
        assert!(v.starts_with(&task.prompt));
        assert!(v.contains("todo!"), "view must include the skeleton body");
    }

    #[test]
    fn run_plan_is_deterministic_and_sized() {
        let fams = ["window-op", "error-handling"];
        let a = plan_run(&fams, "2026-08", 4, 1);
        let b = plan_run(&fams, "2026-08", 4, 1);
        assert_eq!(a, b, "same epoch → identical plan (resume depends on it)");
        assert_eq!(a.len(), 2 * (4 + 1), "n_families × (core + probe)");
        assert_eq!(a.iter().filter(|u| u.kind == UnitKind::Core).count(), 8);
        assert_eq!(a.iter().filter(|u| u.kind == UnitKind::Probe).count(), 2);
    }

    #[test]
    fn core_and_probe_seed_spaces_are_disjoint() {
        // Same family+index, but core and probe must derive different seeds — the whole
        // point of ADR-0009 (probe seeds unpredictable from the public epoch).
        let plan = plan_run(&["window-op"], "e", 3, 3);
        for i in 0..3 {
            let core = plan
                .iter()
                .find(|u| u.kind == UnitKind::Core && u.index == i)
                .unwrap();
            let probe = plan
                .iter()
                .find(|u| u.kind == UnitKind::Probe && u.index == i)
                .unwrap();
            assert_ne!(core.seed, probe.seed);
        }
    }

    #[test]
    fn different_epochs_give_different_core_seeds() {
        let a = plan_run(&["window-op"], "2026-08", 2, 0);
        let b = plan_run(&["window-op"], "2026-09", 2, 0);
        assert_ne!(a[0].seed, b[0].seed, "core seeds rotate with the epoch");
    }

    #[test]
    fn remaining_skips_done_units() {
        let plan = plan_run(&["window-op", "error-handling"], "e", 2, 1);
        // Mark the first two units done.
        let done: std::collections::HashSet<String> =
            plan.iter().take(2).map(|u| u.key()).collect();
        let left = remaining(&plan, &done);
        assert_eq!(left.len(), plan.len() - 2);
        assert!(left.iter().all(|u| !done.contains(&u.key())));
        // Resuming an already-complete run leaves nothing to do.
        let all: std::collections::HashSet<String> = plan.iter().map(|u| u.key()).collect();
        assert!(remaining(&plan, &all).is_empty());
    }

    // The Q31 follow-on: an epoch should cover distinct *skills*, not just distinct
    // prompts. plan_epoch_distinct_skills rejects within-epoch spec-collisions.

    #[test]
    fn distinct_skills_plan_serves_unique_specs() {
        let g = ErrorHandlingFamily;
        let plan =
            plan_epoch_distinct_skills(&g, 0u64.., 10, MIN_INSTANCE_DISTANCE, 2_000).unwrap();
        assert_eq!(plan.seeds.len(), 10);
        assert_eq!(
            plan.distinct_skills(),
            10,
            "every served seed must cover a different skill"
        );
    }

    #[test]
    fn distinct_skills_plan_exhausts_past_spec_diversity() {
        // window-op has exactly 12 distinct skills (crate::spec_diversity). Asking
        // for 13 distinct skills must exhaust at 12 — the loud signal that the family
        // is too narrow for the requested per-epoch count.
        let g = WindowOpFamily;
        let err = plan_epoch_distinct_skills(&g, 0u64..5_000, 13, MIN_INSTANCE_DISTANCE, 5_000)
            .unwrap_err();
        assert_eq!(
            err.accepted, 12,
            "window-op serves 12 distinct skills, no more"
        );
    }

    #[test]
    fn distinct_skills_plan_is_deterministic() {
        let g = ErrorHandlingFamily;
        let a = plan_epoch_distinct_skills(&g, 0u64.., 8, MIN_INSTANCE_DISTANCE, 2_000).unwrap();
        let b = plan_epoch_distinct_skills(&g, 0u64.., 8, MIN_INSTANCE_DISTANCE, 2_000).unwrap();
        assert_eq!(a.seeds, b.seeds);
    }
}
