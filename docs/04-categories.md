# 04 — Categories

Ten categories. Each scored independently, each with its own confidence interval, rendered as a radar chart on the leaderboard.

Ten is a taxonomy, not a sample size. The number that matters is **families per category** — see [07-statistics.md](07-statistics.md). Target is **20–25 families per category, 200–250 total**. Below ~15 families a category score is decoration, not data.

| # | Category | Skill probed | Primary oracle emphasis | Suite | Families (target) |
|---|---|---|---|---|---|
| 1 | `borrow-lifetimes` | aliasing, NLL, lifetime elision, avoiding self-reference, `split_at_mut`-shaped problems | L1 error codes E04xx/E05xx; L3 forbidden `clone`/`to_vec` | synth | 25 |
| 2 | `traits-generics` | trait impls, coherence, GATs, blanket impls, associated types, where-clauses | L1 + L3 signature match | synth | 25 |
| 3 | `error-handling` | `?`, custom `From`, `thiserror`/`anyhow`, exhaustive matching, error type design | L2 behavior + L3 | synth | 20 |
| 4 | `unsafe-ffi` | raw pointers, `repr(C)`, UB avoidance, safe wrappers over unsafe cores | **miri mandatory**; UB = hard behavior failure | synth | 20 |
| 5 | `async-concurrency` | `Send`/`Sync` bounds, tokio, channels, cancellation, deadlock avoidance | L2 property; loom where feasible | synth | 20 |
| 6 | `idiom-refactor` | imperative → iterator, `Option`/`Result` combinators, clippy-clean rewrites | L3 constraint-dominant | synth | 20 |
| 7 | `perf-optimization` | allocation removal, `&str` vs `String`, iterator fusion, SIMD-friendly loops | L4 criterion ratio primary | synth | 15 |
| 8 | `api-evolution` | post-cutoff std stabilisations, crate API migration, deprecation handling | L1 + L2; freshness-rotated | wild | 20 |
| 9 | `test-authoring` | writing tests for a given implementation | **L4 mutation score primary** | synth | 20 |
| 10 | `multi-file-repo` | cross-module change in a 3–10 file crate; repo navigation | full stack; slowest tier | wild | 15 |

**Total: ~200 families.** ~165 synthetic (hand-written, solution-first), ~35 mined.

## Why these ten

Categories 1, 2, 4, 5 cover the semantics that Rust-SWE-bench attributes **32.6%** of agent failures to and that no general-purpose coding benchmark can see. Category 10 covers the **43.7%** attributed to repo-wide structure comprehension. Category 8 is the contamination probe — RustEvo² measured a **56.1% → 32.5%** before/after-cutoff cliff on exactly this axis, which makes it a direct, quantified memorisation detector.

Categories 3, 6, 7, 9 cover the parts of daily Rust work that determine whether a model is *pleasant* to use rather than merely correct.

## Category 10 is in the headline number

Earlier draft treated `multi-file-repo` as an opt-in extra. That was wrong, and the reversal is recorded in [ADR-0004](adr/0004-multi-file-repo-in-headline.md).

Real Rust fix patches average **9.8 files and 139.9 lines** (Python SWE-bench Verified: 1.25 files / 14.3 lines). A Rust benchmark composed only of single-file puzzles measures something meaningfully different from Rust work. Excluding the category would bias the entire benchmark toward toy tasks.

Cost is managed rather than avoided:

- **Fewer families (15), equal category weight.** Depth traded for breadth elsewhere.
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
- `unsafe-ffi`: `repr-c`, `raw-ptr`, `transmute`, `send-sync-impl`, `ffi-boundary`
- `async-concurrency`: `send-bounds`, `cancellation`, `channels`, `shared-state`, `executor`

Once there are enough submissions, subcategory scores become the mechanism for retuning difficulty and spotting families that carry no information (everyone at 0% or 100%).

## Weighting

Default composite: **equal weight per category.** Not per family — otherwise a category with 25 families would silently outweigh one with 15.

```
capability_score = mean over categories of (mean task_score within category)
```

Alternative weightings (usage-frequency-weighted, difficulty-weighted) are published as secondary columns once there is data to justify them, never as the default.
