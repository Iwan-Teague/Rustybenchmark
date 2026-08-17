# 04 — Categories

Eleven categories in two classes. Each scored independently, each with its own confidence interval, rendered as a radar chart on the leaderboard.

The category count is a taxonomy. The number that decides whether a category score means anything is **families per category** — and per-category precision is *bounded by family count alone*, no matter how many seeds are run ([07-statistics.md](07-statistics.md), the S3 ceiling). At 20 families a category carries ±12–16% forever. At 40 it reaches ±8.5%.

Hence two classes:

- **Core** — 40 families. Rankable at `deep` (±10.7%). Five of them.
- **Probe** — 12 families. **Directional indicators only, never ranked** (±19.6%). Six of them.

Both contribute to `capability_score`. Only core categories may be compared against each other or across models. Decision recorded in [ADR-0008](adr/0008-core-and-probe-categories.md).

## Core categories (40 families each)

| # | Category | Skill probed | Oracle emphasis | Suite |
|---|---|---|---|---|
| 1 | `borrow-lifetimes` | aliasing, NLL, lifetime elision, avoiding self-reference, `split_at_mut`-shaped problems | **constraint-dominant** — allocation instrumentation + L1 error codes E04xx/E05xx | synth |
| 2 | `traits-generics` | trait impls, coherence, GATs, blanket impls, associated types, where-clauses | L1 + L3 signature match | synth |
| 3 | `error-handling` | `?`, custom `From`, `thiserror`/`anyhow`, exhaustive matching, error type design | L2 behavior + L3 | synth |
| 4 | `idiom-refactor` | imperative → iterator, `Option`/`Result` combinators, clippy-clean rewrites | **constraint-dominant** | synth |
| 5 | `unsafe-core` | raw pointers, transmute, aliasing rules, `Send`/`Sync` impls, safe wrappers over unsafe cores | **miri mandatory**; UB = hard behavior failure | synth |

**200 families.** All synthetic, all solution-first, all with tractable oracles.

## Probe categories (12 families each)

| # | Category | Skill probed | Oracle emphasis | Suite | Why probe, not core |
|---|---|---|---|---|---|
| 6 | `async-concurrency` | `Send`/`Sync` bounds, tokio, channels, cancellation, deadlock avoidance | loom linearizability where state space permits; otherwise constraint-dominant | synth | Oracle unresolved — see [REVIEW.md](REVIEW.md) S10 |
| 7 | `perf-optimization` | allocation removal, `&str` vs `String`, iterator fusion, SIMD-friendly loops | L4 criterion ratio primary | synth | Hardware-gated; unmeasurable on many machines |
| 8 | `api-evolution` | post-cutoff std stabilisations, crate API migration, deprecation handling | L1 + L2; freshness-rotated | wild | Mined; supply-limited by the release calendar |
| 9 | `test-authoring` | writing tests for a given implementation | **L4 mutation score primary** | synth | L4-dependent, so not replay-verifiable |
| 10 | `multi-file-repo` | cross-module change in a 3–10 file crate; repo navigation | full stack; slowest tier | wild | Mined; yield for small commits unproven |
| 11 | `ffi-boundary` | `repr(C)`, C interop, ABI correctness, ownership across the boundary | real C shim linked at test time; **no miri** | synth | Miri cannot execute foreign calls |

**72 families.** Split from the original category 4: miri can check `unsafe-core` but cannot execute FFI, which is what `ffi-boundary` is definitionally about.

**Corpus total: 272 families.** ~236 synthetic, ~36 mined.

## Why these ten

Categories 1, 2, 4, 5 cover the semantics that Rust-SWE-bench attributes **32.6%** of agent failures to and that no general-purpose coding benchmark can see. Category 10 covers the **43.7%** attributed to repo-wide structure comprehension. Category 8 is the contamination probe — RustEvo² measured a **56.1% → 32.5%** before/after-cutoff cliff on exactly this axis, which makes it a direct, quantified memorisation detector.

Categories 3, 6, 7, 9 cover the parts of daily Rust work that determine whether a model is *pleasant* to use rather than merely correct.

## Category 10 is in the headline number

Earlier draft treated `multi-file-repo` as an opt-in extra. That was wrong, and the reversal is recorded in [ADR-0004](adr/0004-multi-file-repo-in-headline.md).

Real Rust fix patches average **9.8 files and 139.9 lines** (Python SWE-bench Verified: 1.25 files / 14.3 lines). A Rust benchmark composed only of single-file puzzles measures something meaningfully different from Rust work. Excluding the category would bias the entire benchmark toward toy tasks.

Cost is managed rather than avoided:

- **Probe class (12 families), equal category weight.** Its own score is directional only, but it still contributes to the composite.
- **Bounded repos.** 3–10 files, ≤2k LoC, pre-vendored dependencies, prebuilt dependency workspace. Mining targets *small* fail-to-pass commits specifically; they exist in quantity.
- **Separate reporting axis.** Both `capability_score` (all ten) and `capability_score_lite` (categories 1–9) are published side by side. A slow machine can run lite and still appear, with the omission explicit rather than silently biasing the table.
- **Time-boxed, not turn-boxed.** Wall-clock cap per task; timeout vs genuine failure is recorded separately, because on this benchmark the distinction is a *hardware* fact and separating it is the point.

## Tiers within a category

Each family carries `tier = smoke | standard | deep`:

- `smoke` — fast, low-variance families used for the sub-hour suite. Present in every category so the smoke run still produces a shape, even though it produces no defensible category CIs.
- `standard` — the working set.
- `deep` — expensive families (miri, criterion, multi-file) admitted only above the `standard` suite.

## Subcategories

Recorded per family for drill-down, not scored independently at launch. Examples:

- `borrow-lifetimes`: `aliasing`, `elision`, `variance`, `self-reference`, `reborrow`
- `traits-generics`: `coherence`, `gat`, `blanket`, `assoc-type`, `object-safety`
- `unsafe-core`: `raw-ptr`, `transmute`, `aliasing`, `send-sync-impl`, `drop-order`
- `ffi-boundary`: `repr-c`, `abi`, `ownership-transfer`, `callback`, `string-marshalling`
- `async-concurrency`: `send-bounds`, `cancellation`, `channels`, `shared-state`, `executor`

Once there are enough submissions, subcategory scores become the mechanism for retuning difficulty and spotting families that carry no information (everyone at 0% or 100%).

## Weighting

Default composite: **equal weight per category.** Not per family — otherwise a 40-family category would silently outweigh a 12-family one.

```
capability_score      = mean over all 11 categories of (mean task_score within category)
capability_score_lite = mean over categories 1-9 and 11 (excludes multi-file-repo)
```

Probe categories are noisier individually but contribute acceptably to an 11-way mean. Alternative weightings (usage-frequency, difficulty) are published as secondary columns once there is data to justify them, never as the default.

## Per-category oracle weights

The global default (`behavior 0.70 / constraint 0.20 / quality 0.10`) is **wrong for several categories** and is overridable per category and per family.

The clearest case is `borrow-lifetimes`. A solution that clones everything is *semantically correct* — it passes every behavioural property. The entire signal is in L3. Grading that category on the global weights would score a clone-everything model almost identically to one that actually understands borrows, in the project's own flagship category.

| Category | behavior | constraint | quality | Rationale |
|---|---|---|---|---|
| `borrow-lifetimes` | 0.35 | **0.55** | 0.10 | Cloning is behaviourally correct; constraints carry the signal |
| `idiom-refactor` | 0.30 | **0.60** | 0.10 | The task *is* the constraint |
| `perf-optimization` | 0.30 | 0.10 | **0.60** | Criterion ratio is the point |
| `test-authoring` | 0.20 | 0.10 | **0.70** | Mutation score is the point |
| `unsafe-core` | **0.70** | 0.30 | 0.00 | UB under miri is a behavior failure, not a style deduction |
| all others | 0.70 | 0.20 | 0.10 | Default |

Weights are declared in the category definition, overridable in `task.toml`, and **published on the leaderboard** — a hidden weighting is as much an attack surface as a hidden timeout.
