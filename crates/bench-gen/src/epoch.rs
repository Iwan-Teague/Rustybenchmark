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
/// them; `min_pairwise` is the realised minimum (>= threshold by construction).
#[derive(Debug, Clone)]
pub struct EpochPlan {
    pub seeds: Vec<u64>,
    pub views: Vec<String>,
    pub attempts: u32,
    pub min_pairwise: f64,
}

impl EpochPlan {
    /// Candidates examined but rejected as too close to an accepted sibling.
    pub fn rejected(&self) -> u32 {
        self.attempts - self.seeds.len() as u32
    }
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
    Ok(EpochPlan {
        seeds,
        views,
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
    fn sampler_beats_naive_on_the_hard_family() {
        // The Q30 story on error-handling, in one test:
        //  (a) taking 6 seeds by raw index yields a set with a near-twin (min < floor);
        //  (b) a clean 6-set is impossible — the family's capacity at the floor is 3,
        //      so asking for 6 exhausts, having rejected the collisions rather than
        //      serving them;
        //  (c) the 3 it does serve are genuinely floor-clean.
        let g = ErrorHandlingFamily;

        let naive: Vec<String> = (0..6u64).map(|s| model_view(&g, s)).collect();
        let naive_min = min_pairwise_distance(&naive);
        assert!(
            naive_min < MIN_INSTANCE_DISTANCE,
            "precondition: raw-index 6-set should contain a near-twin (min {naive_min})"
        );

        let err = plan_epoch_from(&g, 0u64..2_000, 6, MIN_INSTANCE_DISTANCE, 2_000).unwrap_err();
        assert_eq!(err.accepted, 3, "capacity at floor is 3");
        assert!(
            err.attempts > err.accepted as u32,
            "sampler must have examined and rejected collisions ({} attempts for {} seated)",
            err.attempts,
            err.accepted
        );

        let clean = plan_epoch_from(&g, 0u64..2_000, 3, MIN_INSTANCE_DISTANCE, 2_000).unwrap();
        assert!(
            clean.min_pairwise >= MIN_INSTANCE_DISTANCE,
            "served set must clear the floor (min {})",
            clean.min_pairwise
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

    // The two families' greedy distinct-at-floor capacity, pinned as a regression
    // guard and as the quantified evidence behind Q30. These are far below the
    // families' *median* distance would suggest: pairwise-mutual distance is a much
    // stronger constraint than average distance. If a family change moves these,
    // the design conversation (per-category floor vs. larger variable surface) must
    // be revisited — so the test is meant to fail loudly, not be bumped silently.

    #[test]
    fn window_op_capacity_at_floor_is_8() {
        let g = WindowOpFamily;
        // Ask for 9 (one past capacity) so the sampler must exhaust to prove 8.
        let err = plan_epoch_from(&g, 0u64..2_000, 9, MIN_INSTANCE_DISTANCE, 2_000).unwrap_err();
        assert_eq!(
            err.accepted, 8,
            "window-op seats 8 distinct instances at 0.25"
        );
    }

    #[test]
    fn error_handling_capacity_at_floor_is_3() {
        let g = ErrorHandlingFamily;
        let err = plan_epoch_from(&g, 0u64..2_000, 4, MIN_INSTANCE_DISTANCE, 2_000).unwrap_err();
        assert_eq!(
            err.accepted, 3,
            "error-handling seats only 3 distinct instances at 0.25 — below any \
             per-epoch seed count; the family needs a larger variable surface (Q30)"
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
}
