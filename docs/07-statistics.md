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

Same compute, **2.6× the statistical power**. This is the single most consequential number in the design, and it is why the family count (200–250) matters far more than the seed count.

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

## Paired design — free 2–4× power

**Every model in an epoch runs the identical seed set.** Comparison then uses McNemar on discordant pairs rather than two independent proportions. Models fail on correlated items, so pairing cancels most of the item variance — typically a 2–4× reduction in the N required for the same power.

Cost: one line of policy. Seeds are fixed per epoch, not per submission. This dovetails exactly with the T2 challenge-nonce issuance scheme in [10-integrity.md](10-integrity.md): a per-epoch nonce produces a per-epoch seed set.

## Suite sizing

Assumptions: ~55 s per instance single-shot on a 20 tok/s local model (30 s generate + 10 s prefill + 12 s warm cargo build and grade); ~110 s with repair. Roughly 2.2× faster on a 4090-class box at ~60 tok/s.

| Suite | Families | Seeds | Instances | Eff. N | ±CI overall | ±CI per category | @20 tok/s | @60 tok/s |
|---|---|---|---|---|---|---|---|---|
| `smoke` | 60 | 1 | 60 | 60 | ±12.6% | n/a | ~55 min | ~25 min |
| `standard` | 200 | 2 | 400 | ~308 | ±5.6% | ±17% | ~6.1 h | ~2.8 h |
| `deep` | 200 | 6 | 1200 | ~632 | ±3.9% | ±12% | ~18.3 h | ~8.4 h |
| `full` | 200 | 16 | 3200 | ~872 | ±3.3% | ±10% | ~49 h | ~22 h |

Read the two precision columns carefully.

**Overall score converges fast. Per-category scores do not.** At `standard`, a category score of 45% carries roughly ±17 points. You cannot honestly say "this model is better at traits than at lifetimes" from that. **Category-level claims require `deep` minimum.**

### Leaderboard policy

**`deep` is the minimum tier for a ranked row.** `standard` and `smoke` submissions are accepted and displayed, greyed, marked *insufficient precision for ranking*.

This single rule is what stops the leaderboard degenerating into noise, and it is far easier to enforce from day one than to introduce later.

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
