//! `bench-stats` — turn a journal of graded units into the published figures.
//!
//! This crate is the concrete implementation of the decisions in
//! [docs/07-statistics.md](../../../docs/07-statistics.md):
//!
//! - **`capability_score`** is the *equal-weight mean of the per-category means*
//!   (docs/04), not a pooled mean of units — so a small probe category counts the
//!   same as a large core one.
//! - **Pass** is the structural predicate `OracleVector::passed()` (Q28), never a
//!   threshold on the continuous score. It drives the binary metrics (pass-rate);
//!   the continuous `oracle.score` drives capability.
//! - **Confidence intervals are the cluster bootstrap** (Q29), never the
//!   design-effect formula. Specifically the **studentised (percentile-t) wild
//!   cluster bootstrap** (Q29.5): Rademacher sign-flips on per-family residual
//!   sums, with each replicate carrying its own cluster-robust SE so the CI
//!   inverts bootstrap t-quantiles. Measured coverage 0.95 at 12 clusters, versus
//!   0.90 for the raw percentile form and worse for naive resampling. The cluster
//!   is the **family** for now — shapes are not labelled yet (needs Q24), so every
//!   CI is still a lower bound on width (it cannot see the shape clustering).
//!   Categories with too few families are marked **directional-only**.
//! - **Multiplicity**: per-category CIs shown together are **simultaneous** —
//!   Bonferroni level `1 − α/K` across the `K` categories reported (Q29.4).
//!
//! Not yet implemented (tracked): the wild cluster bootstrap for few-cluster
//! coverage, shape-level resampling (Q24), the paired McNemar / sign-test
//! detectors, and ICC estimation. This increment establishes the load-bearing
//! path — capability, pass-rate, and honest cluster-bootstrap CIs — that those
//! extend.

use bench_core::OracleVector;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// The confidence level for the overall figure.
pub const ALPHA: f64 = 0.05;

/// A category with fewer than this many families cannot support a family cluster
/// bootstrap (resampling one or two clusters gives a near-zero-width, over-confident
/// interval). Such categories are reported **directional-only**. Provisional — the
/// real floor is fixed at the Q24 shape audit against measured coverage.
pub const CLUSTER_FLOOR: usize = 8;

/// Default bootstrap resamples.
pub const BOOTSTRAP_ITERS: usize = 10_000;

/// One graded unit, as read from a journal line. Only the fields statistics needs
/// are declared; serde ignores the rest (`model`, `cost`, `sandbox`, …).
#[derive(Clone, Debug, Deserialize)]
pub struct Record {
    pub task_id: String,
    pub category: String,
    pub oracle: OracleVector,
}

impl Record {
    /// The family a unit belongs to: the `task_id` up to the first `/` (generated
    /// ids are `family/{seed}`), or the whole id for a frozen task.
    pub fn family(&self) -> &str {
        self.task_id.split('/').next().unwrap_or(&self.task_id)
    }

    /// The continuous capability contribution — the composite computed at grade time.
    pub fn score(&self) -> f64 {
        self.oracle.score as f64
    }

    /// The structural pass bit (Q28).
    pub fn passed(&self) -> bool {
        self.oracle.passed()
    }

    /// Build a synthetic record for tests and examples: a passing record has
    /// `behaviour == 1.0` and no failing constraint; a failing one has behaviour
    /// `0.5`. `score` is the (independent) composite value.
    pub fn synthetic(category: &str, family: &str, unit: u64, score: f64, passed: bool) -> Record {
        let mut oracle = OracleVector::apply_failed();
        oracle.apply_ok = true;
        oracle.compile_ok = true;
        oracle.behavior.score = Some(if passed { 1.0 } else { 0.5 });
        oracle.score = score as f32;
        Record {
            task_id: format!("{family}/{unit:016x}"),
            category: category.to_string(),
            oracle,
        }
    }
}

/// Read a JSONL journal into records, skipping blank lines.
pub fn load_journal(path: &Path) -> std::io::Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)?;
    parse_journal(&text)
}

/// Parse JSONL text into records.
pub fn parse_journal(text: &str) -> std::io::Result<Vec<Record>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec: Record = serde_json::from_str(line).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line {}: {e}", i + 1),
            )
        })?;
        out.push(rec);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

/// `category -> family -> [unit values]`, for a chosen per-unit metric.
type Grouped = BTreeMap<String, BTreeMap<String, Vec<f64>>>;

fn group_by(records: &[Record], value: impl Fn(&Record) -> f64) -> Grouped {
    let mut g: Grouped = BTreeMap::new();
    for r in records {
        g.entry(r.category.clone())
            .or_default()
            .entry(r.family().to_string())
            .or_default()
            .push(value(r));
    }
    g
}

/// Mean over every unit in a category (flattening families). `None` if empty.
fn category_mean(families: &BTreeMap<String, Vec<f64>>) -> Option<f64> {
    let mut sum = 0.0;
    let mut n = 0usize;
    for units in families.values() {
        for &v in units {
            sum += v;
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f64)
}

/// The equal-weight mean of category means — `capability_score` (docs/04).
fn equal_weight_overall(g: &Grouped) -> Option<f64> {
    let means: Vec<f64> = g.values().filter_map(category_mean).collect();
    (!means.is_empty()).then(|| means.iter().sum::<f64>() / means.len() as f64)
}

// ---------------------------------------------------------------------------
// Deterministic RNG (SplitMix64) — same journal → same CI.
// ---------------------------------------------------------------------------

struct Rng {
    state: u64,
}
impl Rng {
    fn new(seed: u64) -> Self {
        Rng {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

// ---------------------------------------------------------------------------
// Cluster bootstrap
// ---------------------------------------------------------------------------

/// Negligible-SE guard — below this a cluster set is treated as having no variance.
const SE_EPS: f64 = 1e-12;

/// Precomputed **wild cluster bootstrap** state for one cluster set (the families of a
/// category): the point mean, each family's residual sum `e_g = Σ_{i∈g}(y_i − μ)`, each
/// family's unit count, and the total. The wild cluster bootstrap (Cameron–Gelbach–Miller,
/// Q29.5) flips each `e_g` by an i.i.d. Rademacher sign per replicate.
///
/// The interval is the **studentised** (percentile-t) form: each replicate carries its
/// *own* cluster-robust SE computed from the sign-flipped residuals, and the CI inverts the
/// quantiles of the bootstrap t-statistic. Studentising is what lifts few-cluster coverage
/// from ~0.90 (raw percentile) toward the nominal 0.95, because the t-pivot cancels the
/// small-sample noise in the SE estimate. A homogeneous cluster set has zero SE and gets a
/// zero-width interval, correctly.
struct WildCat {
    mu: f64,
    e: Vec<f64>,
    n_g: Vec<usize>,
    n: usize,
}

impl WildCat {
    fn new(families: &[&Vec<f64>]) -> Option<WildCat> {
        let n: usize = families.iter().map(|f| f.len()).sum();
        if n == 0 {
            return None;
        }
        let mu = families.iter().flat_map(|f| f.iter()).sum::<f64>() / n as f64;
        let e: Vec<f64> = families
            .iter()
            .map(|f| f.iter().map(|&y| y - mu).sum::<f64>())
            .collect();
        let n_g = families.iter().map(|f| f.len()).collect();
        Some(WildCat { mu, e, n_g, n })
    }

    /// Cluster-robust variance of the point mean: `(Σ_g e_g²) / N²`.
    fn point_var(&self) -> f64 {
        self.e.iter().map(|&eg| eg * eg).sum::<f64>() / (self.n as f64).powi(2)
    }

    /// One Rademacher sign-flip replicate. Returns `(Δ, var*)` where `Δ = μ* − μ` is the
    /// shift of the category mean and `var*` is the bootstrap cluster-robust variance of
    /// `μ*` under the flip: with `e*_g = w_g e_g − n_g Δ`, `var* = (Σ_g e*_g²)/N²`.
    fn draw_parts(&self, rng: &mut Rng) -> (f64, f64) {
        let w: Vec<f64> = self
            .e
            .iter()
            .map(|_| if rng.next_u64() & 1 == 0 { 1.0 } else { -1.0 })
            .collect();
        let n = self.n as f64;
        let delta = w.iter().zip(&self.e).map(|(wi, ei)| wi * ei).sum::<f64>() / n;
        let var_star = w
            .iter()
            .zip(&self.e)
            .zip(&self.n_g)
            .map(|((wi, ei), &ng)| {
                let eg_star = wi * ei - ng as f64 * delta;
                eg_star * eg_star
            })
            .sum::<f64>()
            / (n * n);
        (delta, var_star)
    }
}

fn wildcat(families: &BTreeMap<String, Vec<f64>>) -> Option<WildCat> {
    let fams: Vec<&Vec<f64>> = families.values().collect();
    WildCat::new(&fams)
}

/// Turn a set of bootstrap t-statistics and the point `(mu, se)` into a studentised
/// (equal-tailed percentile-t) CI: `[μ − se·q_{1−α/2}, μ − se·q_{α/2}]`.
fn studentized_ci(mu: f64, se: f64, mut ts: Vec<f64>, alpha: f64) -> (f64, f64) {
    if se <= SE_EPS {
        return (mu, mu); // no variance to report
    }
    if ts.len() < 2 {
        // Degenerate bootstrap (too few informative replicates): fall back to normal.
        return (mu - 1.959_964 * se, mu + 1.959_964 * se);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| ts[(((ts.len() - 1) as f64) * p).round() as usize];
    (mu - se * q(1.0 - alpha / 2.0), mu - se * q(alpha / 2.0))
}

/// Studentised wild cluster bootstrap for the overall equal-weight statistic. Each
/// replicate flips family signs within every category, forms the equal-weight mean shift
/// `Δ` and its bootstrap SE, and records the t-pivot; the CI inverts those quantiles.
fn bootstrap_overall(g: &Grouped, iters: usize, alpha: f64, rng: &mut Rng) -> (f64, f64) {
    let cats: Vec<WildCat> = g.values().filter_map(wildcat).collect();
    if cats.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let c = cats.len() as f64;
    let theta = cats.iter().map(|w| w.mu).sum::<f64>() / c;
    let se = (cats.iter().map(|w| w.point_var()).sum::<f64>() / (c * c)).sqrt();
    let mut ts = Vec::with_capacity(iters);
    for _ in 0..iters {
        let mut dsum = 0.0;
        let mut vsum = 0.0;
        for w in &cats {
            let (d, v) = w.draw_parts(rng);
            dsum += d;
            vsum += v;
        }
        let delta = dsum / c;
        let se_star = (vsum / (c * c)).sqrt();
        if se_star > SE_EPS {
            ts.push(delta / se_star);
        }
    }
    studentized_ci(theta, se, ts, alpha)
}

/// Studentised wild cluster bootstrap for a single category mean.
fn bootstrap_category(
    families: &BTreeMap<String, Vec<f64>>,
    iters: usize,
    alpha: f64,
    rng: &mut Rng,
) -> (f64, f64) {
    let wc = match wildcat(families) {
        None => return (f64::NAN, f64::NAN),
        Some(wc) => wc,
    };
    let se = wc.point_var().sqrt();
    let mut ts = Vec::with_capacity(iters);
    for _ in 0..iters {
        let (delta, var_star) = wc.draw_parts(rng);
        let se_star = var_star.sqrt();
        if se_star > SE_EPS {
            ts.push(delta / se_star);
        }
    }
    studentized_ci(wc.mu, se, ts, alpha)
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CategoryReport {
    pub category: String,
    pub mean_score: f64,
    pub pass_rate: f64,
    pub families: usize,
    pub units: usize,
    /// Simultaneous CI on `mean_score` (Bonferroni across the reported categories).
    pub score_ci: (f64, f64),
    /// True when the category has too few families to bootstrap honestly
    /// (`families < CLUSTER_FLOOR`): its CI is not trustworthy and it must be shown
    /// directional-only, never ranked.
    pub directional_only: bool,
}

#[derive(Clone, Debug)]
pub struct StatReport {
    pub capability_score: f64,
    /// Overall CI at level `1 − ALPHA` (not simultaneity-adjusted — it is one figure).
    pub capability_ci: (f64, f64),
    pub pass_rate: f64,
    pub categories: Vec<CategoryReport>,
    pub units: usize,
    pub bootstrap_iters: usize,
    /// Number of categories the per-category CIs are corrected over (the `K` in
    /// the simultaneous `α/K`).
    pub simultaneous_k: usize,
}

/// Compute the full report from a set of graded records.
pub fn report(records: &[Record]) -> StatReport {
    report_with(records, BOOTSTRAP_ITERS)
}

/// As [`report`], with an explicit bootstrap resample count (tests use a smaller one).
pub fn report_with(records: &[Record], iters: usize) -> StatReport {
    let by_score = group_by(records, Record::score);
    let by_pass = group_by(records, |r| if r.passed() { 1.0 } else { 0.0 });

    let capability_score = equal_weight_overall(&by_score).unwrap_or(f64::NAN);
    let pass_rate = equal_weight_overall(&by_pass).unwrap_or(f64::NAN);

    let k = by_score.len().max(1);
    let cat_alpha = ALPHA / k as f64; // Bonferroni simultaneous

    // One deterministic RNG stream for the whole report → reproducible CIs.
    let mut rng = Rng::new(0x5EED_5747_5747_5747);
    let capability_ci = bootstrap_overall(&by_score, iters, ALPHA, &mut rng);

    let mut categories = Vec::new();
    for (cat, fams) in &by_score {
        let mean_score = category_mean(fams).unwrap_or(f64::NAN);
        let pass_rate = by_pass.get(cat).and_then(category_mean).unwrap_or(f64::NAN);
        let families = fams.len();
        let units = fams.values().map(|u| u.len()).sum();
        let score_ci = bootstrap_category(fams, iters, cat_alpha, &mut rng);
        categories.push(CategoryReport {
            category: cat.clone(),
            mean_score,
            pass_rate,
            families,
            units,
            score_ci,
            directional_only: families < CLUSTER_FLOOR,
        });
    }

    StatReport {
        capability_score,
        capability_ci,
        pass_rate,
        categories,
        units: records.len(),
        bootstrap_iters: iters,
        simultaneous_k: k,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dataset() -> Vec<Record> {
        // Two categories. Cat A: 2 families × 3 units, all passing, score 1.0.
        // Cat B: 1 family × 3 units, none passing, score 0.0.
        let mut v = Vec::new();
        for fam in ["a1", "a2"] {
            for u in 0..3 {
                v.push(Record::synthetic("cat-a", fam, u, 1.0, true));
            }
        }
        for u in 0..3 {
            v.push(Record::synthetic("cat-b", "b1", u, 0.0, false));
        }
        v
    }

    #[test]
    fn capability_is_equal_weight_over_categories() {
        // Cat A mean 1.0, Cat B mean 0.0 → equal-weight overall 0.5, NOT the pooled
        // 6/9 = 0.67 that unit-weighting would give.
        let r = report_with(&dataset(), 200);
        assert!(
            (r.capability_score - 0.5).abs() < 1e-9,
            "got {}",
            r.capability_score
        );
        assert!((r.pass_rate - 0.5).abs() < 1e-9, "got {}", r.pass_rate);
    }

    #[test]
    fn family_is_derived_from_task_id() {
        let r = Record::synthetic("c", "window-op", 7, 1.0, true);
        assert_eq!(r.family(), "window-op");
    }

    #[test]
    fn pass_uses_the_structural_predicate() {
        assert!(Record::synthetic("c", "f", 0, 0.9, true).passed());
        // A high composite score does not imply a pass — behaviour must be 1.0.
        assert!(!Record::synthetic("c", "f", 0, 0.99, false).passed());
    }

    #[test]
    fn few_family_categories_are_directional_only() {
        let r = report_with(&dataset(), 200);
        let a = r.categories.iter().find(|c| c.category == "cat-a").unwrap();
        let b = r.categories.iter().find(|c| c.category == "cat-b").unwrap();
        // Both have < CLUSTER_FLOOR families, so both are flagged.
        assert!(a.directional_only && b.directional_only);
        assert_eq!(a.families, 2);
        assert_eq!(b.families, 1);
    }

    #[test]
    fn per_category_ci_is_simultaneous_over_k() {
        let r = report_with(&dataset(), 200);
        assert_eq!(r.simultaneous_k, 2, "two categories → corrected over 2");
    }

    #[test]
    fn ci_brackets_the_point_estimate() {
        let r = report_with(&dataset(), 2000);
        let (lo, hi) = r.capability_ci;
        assert!(lo <= r.capability_score + 1e-9 && r.capability_score <= hi + 1e-9);
        assert!(lo <= hi);
    }

    #[test]
    fn homogeneous_category_has_zero_width_ci() {
        // Cat A is all-1.0: every family residual is zero, so the wild bootstrap can
        // never move the mean and the CI has zero width — correct (there is no
        // variance to report), and the category is still flagged directional-only.
        let r = report_with(&dataset(), 500);
        let a = r.categories.iter().find(|c| c.category == "cat-a").unwrap();
        assert!((a.score_ci.0 - 1.0).abs() < 1e-9 && (a.score_ci.1 - 1.0).abs() < 1e-9);
        assert!(a.directional_only, "and it is flagged as not trustworthy");
    }

    #[test]
    fn wild_ci_reflects_between_family_variance() {
        // One category, six families spread evenly 0.0..1.0 (mean 0.5). The
        // studentised wild bootstrap produces a non-trivial interval bracketing the
        // mean — it picks up the between-family spread rather than collapsing.
        // (Studentising is degenerate at 2 clusters, which is exactly why sub-floor
        // categories are directional-only; six clusters is enough to be well-defined.)
        let mut v = Vec::new();
        for (i, s) in [0.0, 0.2, 0.4, 0.6, 0.8, 1.0].into_iter().enumerate() {
            for u in 0..2 {
                v.push(Record::synthetic("c", &format!("f{i}"), u, s, s >= 0.5));
            }
        }
        let r = report_with(&v, 4000);
        let c = &r.categories[0];
        assert!((c.mean_score - 0.5).abs() < 1e-6); // f32-origin scores
        let width = c.score_ci.1 - c.score_ci.0;
        assert!(width > 0.05, "expected a non-trivial interval, got {width}");
        assert!(c.score_ci.0 <= 0.5 && 0.5 <= c.score_ci.1);
    }

    #[test]
    fn wild_bootstrap_coverage_is_near_nominal() {
        // Q29.5's validation: simulate clustered data whose population mean is 0
        // (cluster effects and noise both mean-zero), and check the 95% studentised
        // wild CI covers 0 close to 95% of the time. Measured 0.953 at G=12 (the raw
        // percentile form managed only 0.90, and the naive resample-families
        // bootstrap under-covers this design further). Bound at 0.90 to guard the
        // studentised improvement while tolerating simulation noise.
        let mut rng = Rng::new(0xC0FF_EE12_3400);
        let uni = |r: &mut Rng| (r.next_u64() >> 11) as f64 / (1u64 << 53) as f64; // [0,1)
        let trials = 600;
        let (g, n) = (12usize, 4usize);
        let mut covered = 0;
        for _ in 0..trials {
            let mut fams: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for gi in 0..g {
                let eff = uni(&mut rng) * 2.0 - 1.0; // cluster effect in [-1, 1)
                let units: Vec<f64> = (0..n).map(|_| eff + uni(&mut rng) * 0.6 - 0.3).collect();
                fams.insert(format!("f{gi:02}"), units);
            }
            let (lo, hi) = bootstrap_category(&fams, 400, 0.05, &mut rng);
            if lo <= 0.0 && 0.0 <= hi {
                covered += 1;
            }
        }
        let coverage = covered as f64 / trials as f64;
        assert!(
            coverage >= 0.90,
            "studentised wild CI coverage {coverage} is too low (nominal 0.95)"
        );
    }

    #[test]
    fn parse_journal_reads_real_shaped_lines() {
        // A minimal journal line with extra fields serde must ignore.
        let line = r#"{"schema":1,"unit_id":"x","task_id":"window-op/0000000000000001","category":"borrow-lifetimes","seed":1,"index":0,"model":{"name":"m"},"sandbox":"seatbelt","oracle":{"apply_ok":true,"compile_ok":true,"error_codes":[],"warn_count":0,"behavior":{"unit":null,"property":null,"differential":null,"score":1.0},"constraint":{"alloc_ok":null,"clippy_clean":null,"fmt_ok":null,"unsafe_blocks":null,"unsafe_ok":null,"paths_ok":null,"violations":[],"score":null},"score":0.9,"failure_class":"none","flags":[]},"cost":{},"failure_class":"none"}"#;
        let recs = parse_journal(line).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].family(), "window-op");
        assert!(
            recs[0].passed(),
            "behaviour 1.0, no failing constraint → pass"
        );
        assert!((recs[0].score() - 0.9).abs() < 1e-6); // score is f32-origin
    }
}
