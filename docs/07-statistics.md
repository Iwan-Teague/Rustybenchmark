# 07 — Statistics: suite sizing, power, and confidence

This document decides how many task families exist, how many seeds each gets, how many times a model is sampled, and what confidence can honestly be claimed. Every one of those is the same question: how to spend a fixed compute budget for the tightest unbiased estimate.

## Variance sources

Three, and they are not equal:

1. **Item variance** — which task families are in the suite. **Dominant.**
2. **Instance variance** — which seed within a family. Moderate, and *correlated within family*.
3. **Sampling variance** — temperature and backend nondeterminism. Small at temperature 0.

## The clustering result that drives everything

Seeds within a family are **not independent observations**. A model that cannot reason about lifetime elision fails seeds 1–16 of `borrowck/elide-nested` together. That is intra-class correlation (ICC), and it shrinks effective sample size:

```
design_effect = 1 + (seeds_per_family − 1) × ICC
effective_N   = total_instances / design_effect
```

For seeded variants of one family, ICC is plausibly **0.3–0.5**. At ICC = 0.3:

| Seeds/family | design effect | Effective items per family |
|---|---|---|
| 1 | 1.0 | 1.00 |
| 2 | 1.3 | 1.54 |
| 4 | 1.9 | 2.11 |
| 8 | 3.1 | 2.58 |
| 16 | 5.5 | 2.91 |

**Sixteen seeds of one family buys roughly 2.9 independent items. Four buys 2.1.** Diminishing returns arrive fast.

Therefore: **breadth beats depth, decisively.**

> 200 families × 3 seeds = 600 instances ≈ 400 effective
> 50 families × 12 seeds = 600 instances ≈ 155 effective

Same compute, **2.6× the statistical power**. This is the single most consequential number in the design, and it is why the family count (272) matters far more than the seed count.

It also answers "surely we need an average?" — yes, but the average that carries information is *across families*, not across repeats of the same family. Repeats mostly re-measure something already known.

## Precision achieved

95% CI half-width at p = 0.5 (worst case), on **effective** N:

| Effective N | ±CI |
|---|---|
| 25 | ±19.6% |
| 50 | ±13.9% |
| 100 | ±9.8% |
| 200 | ±6.9% |
| 400 | ±4.9% |
| 800 | ±3.5% |

To separate two models **10 points apart** (unpaired, 80% power, α = 0.05): ~385 effective items per arm.
To separate them at **5 points**: ~1,530 effective items per arm.

That second number is why pairing matters.

> **Note on the formula.** `task_score` is continuous on [0, 1], not binary, so the binomial
> expression is not exactly right. It is a *conservative upper bound*: the variance of any
> variable bounded on [0, 1] is at most 0.25, so these CIs are never too narrow. The reported
> CI is always the cluster bootstrap, not this formula; the table exists for sizing decisions.

## The ceiling that seeds cannot raise

As seeds per family → ∞, the design effect grows linearly, so effective N per category converges:

```
lim (m × F) / (1 + (m−1) × ICC)  =  F / ICC
m→∞
```

**Per-category precision is bounded by family count alone.** No amount of seeding escapes it.

| Families per category | ICC 0.2 | ICC 0.3 | ICC 0.5 | ICC 0.7 |
|---|---|---|---|---|
| 12 | ±12.7% | ±15.5% | ±20.0% | ±23.7% |
| 20 | ±9.8% | **±12.0%** | **±15.5%** | ±18.3% |
| 30 | ±8.0% | ±9.8% | ±12.7% | ±15.0% |
| 40 | ±6.9% | **±8.5%** | ±11.0% | ±13.0% |
| 45 | ±6.5% | ±8.0% | ±10.3% | ±12.2% |

This is the finding that reshaped the category design. At the originally planned 20 families per
category, the radar chart — the product's most distinctive feature — would carry ±12–16% error bars
at *any* suite size, forever. "Better at traits than at lifetimes" would never be supportable.

Resolution: categories split into **core** (40 families, rankable) and **probe** (12 families,
explicitly directional-only). See [04-categories.md](04-categories.md) and
[ADR-0008](adr/0008-core-and-probe-categories.md).

## Paired design — free 2–4× power

**Every model in an epoch runs the identical scored seed set.** Comparison then uses McNemar on discordant pairs rather than two independent proportions. Models fail on correlated items, so pairing cancels most of the item variance — typically a 2–4× reduction in the N required for the same power.

This requirement collides head-on with the anti-precomputation design, which wants seeds that did not exist before the run was requested. A single derivation cannot serve both. Resolved by splitting each epoch into a **paired core** (~85%, fixed per epoch, scored) and a **fresh probe** (~15%, per-batch nonce, never scored, used only as a precomputation detector) — see [ADR-0009](adr/0009-paired-core-and-fresh-probe-seeds.md).

All precision figures in this document are computed on the **core** set only. Probe units are **additional**, not carved out of the scored count — so a `deep` run executes 1088 scored core units *plus* ~163 probe units, and its wall clock is ~15% above the scored-only figure. The suite table below states both.

## Suite sizing

### Timing model

Per-instance cost, on a 20 tok/s local model:

```
prefill        ~10 s     (2k-token prompt at ~200 tok/s)
generate       ~30 s     (600 completion tokens)
build + grade  ~12 s     (warm cargo cache, L0-L3)
               ------
single-shot     ~55 s
repair mode     ~88 s    (2 attempts on the ~60% that fail attempt 1)
+ L4 quality   ~128 s    (cargo-mutants + criterion)
```

`repair` is the **default** interaction mode and L4 is **mandatory at `deep`**, so both must be in
the estimate. The earlier draft costed only the 55 s single-shot path and understated every suite
by 2–3×.

### Suites

Corpus: **272 families** — 5 core × 40, 6 probe × 12. See [04-categories.md](04-categories.md).

| Suite | Families | Seeds | Scored units | + probe | Mode | Eff. N | ±CI overall | ±CI core cat | ±CI probe cat | @20 tok/s | @60 tok/s |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `smoke` | 60 | 1 | 60 | — | single-shot | 60 | ±12.7% | n/a | n/a | ~55 min | ~25 min |
| `standard` | 272 | 2 | 544 | +82 | repair | ~418 | ±4.8% | ±15.1% | ±27.6% | ~15.3 h | ~7.0 h |
| `deep` | 272 | 4 | 1088 | +163 | repair + L4 | ~573 | ±4.1% | **±10.7%** | ±19.6% | ~44.5 h | ~20.2 h |

All figures at ICC = 0.3. Precision columns are computed on scored units only; wall-clock columns include the probe. `smoke` carries no probe — it is not submittable for ranking, so precomputation detection is moot. See the sensitivity grid below.

### ICC sensitivity

ICC is an **assumption, not a measurement** (see [REVIEW.md](REVIEW.md) S2). Overall effective N and
±CI for `deep` (272 × 4 = 1088 instances) across plausible values:

| ICC | Design effect | Eff. N | ±CI overall | ±CI core cat |
|---|---|---|---|---|
| 0.2 | 1.6 | 680 | ±3.8% | ±9.6% |
| 0.3 | 1.9 | 573 | ±4.1% | ±10.7% |
| 0.5 | 2.5 | 435 | ±4.7% | ±12.3% |
| 0.7 | 3.1 | 351 | ±5.2% | ±13.7% |

Suite sizing is **provisional until Phase 3.5 measures the real ICC**. That experiment is a hard
gate before corpus scale-up.

**ICC is estimated and published per category, not pooled.** Round 4 found that `unsafe-core`'s
40 families must spread across only ~16 miri-checkable task shapes, so families within a shape are
correlated and the category's effective ICC exceeds the corpus-wide 0.3. Its honest CI is nearer
±12.3% than the ±10.7% a pooled value implies. Any category whose families cluster into few shapes
has the same problem, so the estimate belongs per category. See [REVIEW-4.md](REVIEW-4.md) R4-S5.

### There is no `full` suite

An earlier draft specified `full` at 16 seeds. It was deleted. Going 4 → 16 seeds improves
per-core-category CI from ±10.7% to roughly ±9% for **4× the wall clock** — over 110 hours at
20 tok/s. The S3 ceiling means the money is in families, never in seeds.

If someone wants more precision than `deep`, the answer is more families, not more seeds.

### Leaderboard policy

- **`deep` is the minimum tier for a ranked row.** `standard` and `smoke` are accepted, displayed
  greyed, marked *insufficient precision for ranking*.
- **Core category scores are rankable at `deep`** (±10.7%). Two models must differ by roughly
  15 points before a core-category difference is claimable, and the claim uses the paired
  bootstrap, not overlapping CIs.
- **Probe category scores are never ranked.** They are displayed as directional indicators with
  their CIs shown, and the UI does not offer sorting on them. A ±19.6% error bar cannot support
  an ordering and should not be presented as if it could.

## Sampling protocol

```
primary        temp = 0.0, 1 sample per instance, all families × N seeds
               -> capability_score, category scores, clustered-bootstrap CIs

variance probe 10% instance subsample × 5 samples @ temp = 0.8
               -> pass@5, self-consistency rate, sampling-variance estimate
               cost: +5% of total runtime

determinism    20 instances × 3 identical greedy calls
probe          -> detects backend nondeterminism; reported as a hardware/backend fact
```

Greedy repeats are near-worthless for capability estimation — that budget belongs to more families. Sampled repeats on a subsample give the variance estimate needed to put honest error bars on everything else.

Backend nondeterminism is real: llama.cpp greedy output can vary with batch size and backend. Any case where identical `(model, seed, sampling)` produced different output is recorded and published as a backend fact, not silently averaged away.

## Confidence interval computation

**Cluster-bootstrap at the family level. Never resample instances independently.**

```
repeat 10_000 times:
    resample task families with replacement
    carry all seeds of each resampled family along
    recompute the statistic
report 2.5th and 97.5th percentiles
```

Naive per-instance bootstrap understates the CI by roughly 40% at ICC = 0.3. That produces confident nonsense, which is worse than no number.

The same clustering applies to category scores (resample families within category) and to paired comparisons (resample families, recompute the paired difference).

## Measure the real ICC, then adapt

Ship `rustybench calibrate-suite`. Once a few hundred submissions exist:

1. **Compute per-family ICC empirically** and retune seeds-per-family instead of guessing 0.3.
2. **Adaptive allocation.** Families where every model scores 0% or 100% carry no information — they are wasted compute. Families near a 50% pass rate carry the most. Item-response-theory and adaptive-testing approaches reach the same precision with substantially fewer items.

Adaptive allocation is a version-2 feature, but the schema must have a home for per-family difficulty and discrimination parameters **from day one**. See [12-schemas.md](12-schemas.md).

## Reporting rules

- Every published score carries a CI. A score without one is not published.
- Every published score states its suite tier and effective N.
- Category comparisons within a model use the paired bootstrap, not overlapping-CI eyeballing.
- Model comparisons use McNemar on the shared seed set, and state the discordant-pair count.
- Any metric flagged unstable (see [05-hardware-and-calibration.md](05-hardware-and-calibration.md)) is displayed struck through, not omitted.
