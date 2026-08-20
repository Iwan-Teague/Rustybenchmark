# Build log

A running record of what has actually been built, verified, and decided —
distinct from the design docs (which specify the target) and the review rounds
(which attack it). One entry per meaningful increment. Newest first.

The roadmap phases referenced here are in [14-roadmap.md](14-roadmap.md).

---

## 2026-08-20 · Q28 + Q29 decided — the pass predicate and the estimation spec

Worked through the two blocking statistical questions with the user and wrote the decisions into code and
[07-statistics.md](07-statistics.md).

**Q28 — the pass predicate is structural, not a threshold.** A task passes iff
`applied ∧ compiled ∧ behaviour == 1.0 ∧ (unsafe_ok ∧ paths_ok ∧ alloc_ok)`, where a constraint the family
did not declare is not a barrier and quality (clippy/fmt, L4) is excluded. Binary, weight-independent
(re-tuning composite weights cannot move pass rates — kills REVIEW-6's swept-cut 23.3% type-I), and
pre-registered because it defines "correct" rather than tuning a cutoff. A clone-everything answer fails
`borrow-lifetimes` by the alloc clause. Implemented as `OracleVector::passed()` in `bench-core` (+2 tests);
the continuous `capability_score` stays the headline, `passed` feeds only the six binary consumers.

**Q29 — the estimation spec, resolved around one reframe:** the published CI is *always* the cluster
bootstrap; the design-effect formula and ICC are sizing/diagnostic only (so a bad/negative ICC can't
narrow a published interval). Decisions written into doc 07:

- **Bootstrap unit = coarsest cluster** — shapes once labelled (needs Q24), family-level and flagged as
  under-covering until then; `idiom-refactor` is crossed → directional-only.
- **Few clusters → wild cluster bootstrap** (Cameron–Gelbach–Miller), coverage validated by simulation;
  categories below a cluster floor are directional-only (fixes the 92%/84% under-coverage).
- **ICC** — variance-components, clamped to [0,1], `design_effect = max(1, …)`, empirical-Bayes shrink
  toward pooled; diagnostic/sizing only.
- **Precomputation sign test → pick-one collapse** (core seed index 0): the only rule preserving the null
  (`any` 100% / `majority` 54% / `pick-one` 4.2% false-accusation).
- **Multiplicity → FWER**: simultaneous radar CIs at 1−0.05/11, Holm for pairwise model comparisons; FDR
  considered and rejected for a public leaderboard.

Both marked DECIDED in OPEN-QUESTIONS. Residual dependencies noted: the shape-level bootstrap and the
per-category cluster floor need the Q24 shape audit; the real ICC needs Phase 3.5. `bench-core` 14 tests.

---

## 2026-08-20 · Fourth family: `seq-transform` (category `iterators`)

A fourth family, authored against [17-authoring-families.md](17-authoring-families.md) with `stack-machine` as the
closest template: the model implements `transform(xs: &[i64]) -> Vec<i64>`, applying a **seed-selected
three-stage pipeline** — Filter (Positive / Even / AboveThreshold(t) / NonZero) → Map (Double / Negate /
Square / AddK(k)) → Terminal (Collect / RunningSum / DedupConsecutive). Native `eval` and the emitted
reference are mirrored one-to-one; the differential fuzzes 3000 random slices (values ∈ -9..=9, length
0..12, so no debug-build overflow); a fixed canonical case `[2, 5, 8]` is changed and rendered non-empty
by **every** combination — including every threshold `t` and constant `k` — so both trivial baselines
(`identity`, `empty`) fail on every seed. The structural surface is 4 × 4 × 3 = **48 distinct skills**,
the widest of the four families; `t`, `k` and the function name are excluded from `spec_signature` (Q31).

```
view       min=0.392 median=0.591 near-twins 0/28
reference  min=0.286 median=0.545 near-twins 0/28   capacity=43
spec-diversity: 48 distinct skills
distinct-skills epoch: served 8 covering 8 distinct skills
```

`cargo test -p bench-gen` — 40 passed (incl. `spec_diversity == 48`); `validate-family --seeds 8` — all
gates green; clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-20 · Authoring guide (doc 17) + README refresh

With three families now sharing a stable pattern, distilled it into [17-authoring-families.md](17-authoring-families.md):
the `Generator` contract, the solution-first recipe (Spec → native `eval` → mirrored emitted reference →
skeleton → prompt → oracle), the worked-example rule (seed-vary + one guaranteed-non-trivial case), the
view-vs-spec-diversity distinction (Q31), the `validate-family` gate table, and the pitfalls the three
families already hit (`i64::MIN` literals, differential overflow, empty test targets). This is the G3
lever — a contributor copies the nearest of the three references and adapts it. Refreshed the README
status (it still described P0 / 14 tests) to the current 7 crates, 71 tests, three families, macOS
sandbox, and added doc 17 to the read-order table. Docs only.

---

## 2026-08-20 · Third family: `stack-machine` (category `pattern-matching`)

A third family in a new category, to keep testing whether solution-first generation generalises and to
cover a genuinely different Rust skill: an **exhaustive `match` over a provided enum** driving a
`Vec<i64>` stack. The model implements `run(program: &[Op]) -> Vec<i64>`; the `Combine` / `Map` /
`Reorder` operations have seed-selected semantics described in the prompt (5 × 4 × 2). Native `eval` and
the emitted reference are mirrored; the differential fuzzes 3000 random programs; the two baselines
(`const-empty`, `echo-pushes`) are both caught.

`validate-family --seeds 8` — all five construction gates green on every seed (reference 1.000, skeleton
fails, baselines caught, canary, determinism). Numbers:

```
view       min=0.349 median=0.453 near-twins 0/28
reference  min=0.000 median=0.207 near-twins 16/28   capacity=2
spec-diversity: 40 distinct skills
distinct-skills epoch: served 8 covering 8 distinct skills
```

**A recorded surprise that further validates Q31.** I built this family expecting *more* solution
diversity than `error-handling` (whose reference-capacity was 7). On the authoritative measure it has it
— **spec-diversity 40, the highest of the three**. But its *reference*-capacity came out **2**, *lower*
than error-handling's 7, because the fixed `match`/loop scaffolding dominates the solution text even more
than the parse-plumbing did. So the family with the *most* skills has the *lowest* reference-distance
proxy — a third, independent confirmation that reference-distance is a poor diversity measure and that
spec-diversity (the decided gate, Q31) is the right one. Left the honest number in the module doc rather
than tuning the family to make a misleading proxy look better.

Three families now span three categories (borrow-lifetimes, error-handling, pattern-matching) and three
distinct task shapes; solution-first generation holds for all three. 71 workspace tests; clippy/fmt
clean.

---

## 2026-08-20 · Epoch sampler serves distinct *skills* (Q31 follow-on)

The Q31 decision left one piece unbuilt: the epoch sampler served on view-distance, which post-finding is
near-vacuous, so within an epoch it did not guarantee distinct *skills* — it could spend several of its
`n` slots on the same skill with different constants. Built `plan_epoch_distinct_skills`: it rejects a
candidate whose `spec_signature` is already served (the point — cover different skills), and still
enforces view-distance for prompt freshness (contamination). It `Exhausted`s once the family's distinct
skills run out.

- `EpochPlan` now carries the served `specs` and a `distinct_skills()` count.
- A test cross-checks the serve-path against `spec_diversity`: asking window-op for 13 distinct skills
  exhausts at exactly **12**, its diversity. Plus determinism and unique-specs tests.
- `validate-family` demonstrates it: "distinct-skills epoch: served 6 seed(s) covering 6 distinct skills".

This surfaced a **new tension for Phase 4 / P3.5**, recorded under Q31: "one seed per skill per epoch"
maximises coverage, but the ICC / repeated-measures design may want *several* seeds of the same skill to
estimate within-skill variance. Different serving policies for different purposes; likely an epoch covers
distinct skills while a separate repeated-measures pass samples same-skill seeds. Not settled here — and
neither planner is wired into a run protocol yet, because there is no run protocol yet. 66 workspace
tests; clippy/fmt clean.

---

## 2026-08-20 · Q31 decided — spec-diversity as the authoritative task-diversity measure

Acting on the Q31 finding (previous entry), and on the "two gates" decision: added a
`spec_signature(seed)` method to the `Generator` trait — the structural choices that define the *skill*,
with numeric constants and identifiers excluded — plus `spec_diversity(gen, n)` counting distinct
signatures. That count is now the authoritative, ungameable diversity number, reported by
`validate-family` and pinned as a regression test.

Granularity: `AtMost(10)` and `AtMost(50)` share a signature (same skill, different constant); constants
still vary the prompt so they still count under the view (contamination) gate, they just don't inflate
diversity. Measured and pinned:

- `window-op`: **12** distinct skills (6 in-place ops × 2 strides)
- `error-handling`: **30** distinct skills (5 combines × 6 rule-types)

Both clear any plausible per-epoch seed count, and — unlike the text proxies — these numbers can be
neither inflated by example noise nor deflated by shared boilerplate. `validate-family` now prints, per
family: view-distance, reference-distance, view/reference capacity, and spec-diversity — the full honest
picture in one place. Q31 marked decided; the per-family diversity *floor* stays open (it depends on the
Phase-4 per-epoch seed count) and folds back into Q30. 63 workspace tests; clippy/fmt clean.

---

## 2026-08-20 · Widened `window-op` — and it exposed that the anti-twin metric is unreliable (Q31)

Set out to raise `window-op`'s tight capacity (8) with the same lever that "fixed" `error-handling`:
two new in-place ops (`Negate`, `AddConst(k)` — chosen because they change every generic window, so the
`identity` baseline stays caught; `Min`/`Max` avoided for the `i64::MIN` literal landmine) plus
seed-varied worked examples. All construction gates stayed green.

**Then measuring capacity two ways broke the earlier story.** The epoch sampler measures distance on the
prompt+skeleton — what the model sees. Seed-varying the examples makes every prompt textually distinct
*without changing the task*, so `window-op` saturated: 100 % of raw-index seeds accepted, view-capacity
effectively unbounded. Measuring on the **reference** (the solution) instead told the truth:

| family | view near-twins | view-capacity | reference near-twins | reference-capacity |
|---|---|---|---|---|
| window-op | 0/28 | saturated | 1/28 | **22** |
| error-handling | 0/28 | ~250–326 | **12/28** | **7** |

So `error-handling`'s "3 → 326" win from the previous entry was mostly illusory — its genuine
solution-diversity is **7**. View-distance over-counts (gameable by example noise); reference-distance
under-counts (deflated by shared boilerplate). Neither is task diversity, which is the structural spec
count. Written up as **[Q31](OPEN-QUESTIONS.md)** (BLOCKING) with three options; the honest correction is
recorded against Q30 and doc 04 too, rather than quietly overwriting the earlier numbers.

Changes made in response, all honest-by-construction:
- `validate-family` now prints **both** view- and reference-distance lines and **both** capacities, so
  the gap is visible in CI. Live: window-op `reference min=0.146 cap=22`; error-handling
  `reference min=0.094 near-twins 12/28 cap=7`.
- `bench_gen::epoch` gained `greedy_distinct_count` and `reference_capacity` (public, documented with the
  over/under-count caveats).
- Capacity regression tests re-pinned to **reference**-capacity: `window_op_reference_capacity_is_healthy`
  (≥18) and `error_handling_reference_capacity_is_low_by_design` (5..=12, a documented limitation, not a
  bug). New `view_distance_saturates_but_reference_distance_does_not` pins the Q31 finding itself.

`window-op` keeps the two new ops (genuine solution diversity, reference-capacity 22) and the seed-varied
examples (contamination-resistance). 62 workspace tests; clippy/fmt clean. The epoch sampler still serves
on view-distance — post-finding that means it rarely rejects, so which basis it should serve on is part
of the Q31 decision.

---

## 2026-08-20 · Widened `error-handling` (Q30 lever 2): capacity 3 → ~326

The sampler flagged `error-handling`'s distinct-at-floor capacity as **3** — unusable, below any
per-epoch seed count. Acted on it directly (Q30 lever 2: enlarge the variable surface):

- **Combine operations 3 → 5**: added `Count` (`acc + 1`) and `SumSquares` (`acc + n*n`). `Min`/`Max`
  were deliberately *not* added — their `i64::MIN` identity has no safe negative-literal form, and the
  emitted reference/tests print literals.
- **Validation rules 3 → 6**: added `AtLeast(b)`, `InRange(lo,hi)`, `Even`. Counting bound choices that
  is 12 distinct rule-instances, so 5 × 12 = 60 combine/rule logics (up from ~9).
- **Seed-varied worked examples**: the examples in the prompt and skeleton are now generated per seed
  (two varied numeric cases + a constructed rule-failure + a parse-failure + empty), where before they
  were four fixed lists. This also fixes a real gap — the old examples never illustrated a *rule*
  failure. Native `eval` and the emitted source stay mirrored, so the reference-passes and differential
  gates still hold.

Verified end to end — `validate-family --family error-handling --seeds 8`, all gates green:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
anti-twin (prompt+skeleton): min=0.287 median=0.438 near-twin pairs (<0.25)=0/28
epoch sampler: served 8 pairwise-distant seed(s) (rejected 0 collision(s), min=0.287)
```

| `error-handling` | median | near-twins | distinct-at-floor capacity |
|---|---|---|---|
| before | 0.263 | 18/45 | 3 |
| after | **0.438** | **0/28** | **~326** |

**Honest caveat.** ~326 is *view*-capacity, and part of the lift is example-text variation that the
shingle metric rewards. That legitimately freshens prompts against exact-text recall but is not skill
diversity; the genuine distinct-logic surface is the 60 combine/rule combinations. Both clear any
per-epoch seed count, and acceptance ran ~22% (not ~100%), so it is not pure metric-gaming. The capacity
regression test was re-pinned to a stable lower bound (`error_handling_capacity_clears_a_per_epoch_count`,
seats 50) rather than the RNG-sensitive exact figure; `window_op_capacity_at_floor_is_8` stays exact
(window-op is a true structural ceiling). 61 workspace tests; clippy/fmt clean.

---

## 2026-08-20 · Distance-aware epoch sampler (Q30) — and it measured a hard capacity ceiling

Built `bench_gen::epoch`, the Q30 second-order fix: a run must not *serve* two near-twins even when a
family's average distance is healthy. `plan_epoch` draws candidate seeds in order and rejects any whose
model-view is closer than the floor to an already-accepted sibling, so the seeds it serves are
pairwise-distant by construction. It is deterministic in `(family, epoch, n, threshold)` — replay-safe —
and measures distance on the exact `view_of` string the CI gate uses, so a plan that clears the floor
here clears the gate. If a family cannot supply the requested count it returns `Exhausted` rather than
serving a twin. `MIN_INSTANCE_DISTANCE = 0.25` is now a named constant both the sampler and
`validate-family`'s near-twin gate read, so they cannot drift.

**Running it turned the soft near-twin count into a hard ceiling — the finding.** The sampler's
*distinct-at-floor capacity* (most pairwise-≥0.25 instances a family can seat) is far below what the
median implies, because pairwise-mutual distance is a much stronger constraint than average distance:

| family | median | near-twin pairs | **distinct-at-floor capacity** |
|---|---|---|---|
| window-op | 0.433 | 7/45 | **8** |
| error-handling | 0.263 | 18/45 | **3** |

`error-handling`'s capacity of **3** is below any usable per-epoch seed count: as designed it cannot
outlast repeated epochs without repeating an instance, so Q30's "enlarge the variable surface" lever is
effectively forced for it. `window-op`'s 8 is workable but tight. Both capacities are pinned as
regression tests (`window_op_capacity_at_floor_is_8`, `error_handling_capacity_at_floor_is_3`) so a
family change that moves them fails loudly rather than silently degrading anti-memorisation.

`validate-family` now closes the loop in one output: it reports median/near-twins (detection) **and**
the sampler's capacity (prevention). Live:

```
window-op:       near-twin pairs (<0.25)=7/45   · epoch sampler: capacity 8 … enlarge the variable surface (Q30)
error-handling:  near-twin pairs (<0.25)=18/45  · epoch sampler: capacity 3 … enlarge the variable surface (Q30)
```

Docs updated: Q3/Q30 (capacity table + the "capacity is the real ceiling" reframing),
[02-task-format.md](02-task-format.md) (distinct-at-floor capacity as the measure families are authored
against), [04-categories.md](04-categories.md) (`error-handling` capacity 3 → must be enlarged before it
ships). 7 new tests; workspace 59 tests; clippy/fmt clean.

---

## 2026-08-20 · Folded the Q3 variance finding into the design docs

The second-family measurement (below) produced a design-level conclusion that lived only in this build
log. Promoted it into the durable specs so the 272-family plan carries it:

- **[Q3](OPEN-QUESTIONS.md)** marked *partly resolved*. Records the settled half — solution-first
  seeding generalises in **correctness** across two different-shape families (both pass all five
  construction gates on every seed) — and the unsettled half: it does **not** generalise automatically
  in **variance**. Carries the distance table (`borrow-lifetimes` 0.433 / 7-of-45 vs `error-handling`
  0.263 / 18-of-45).
- **[Q30](OPEN-QUESTIONS.md)** opened for the mechanism the finding leaves undecided: anti-twin
  variance is per-family, not global, so a single `min_instance_distance` means different things across
  categories. Two prevention levers stated (per-category floor vs. forced variable surface), plus the
  second-order epoch-seed-collision problem and its fix (a distance-aware epoch sampler).
- **[02-task-format.md](02-task-format.md)** gained "Varying the four axes is necessary, not
  sufficient" — the rule that variance is measured per family, and that clearing the floor *on average*
  does not clear it *pairwise*.
- **[04-categories.md](04-categories.md)** notes that pinning the error enum makes `error-handling`
  correct-by-construction but naturally low-variance, cross-linking Q3/Q30.

No code changed; `bench-invariants` still green (the edits are prose, not stats-table cells).

---

## 2026-08-19 · P3 — second family (`error-handling`): seeding generalises, and two oracle bugs it caught

**The question this answers.** The docs' biggest open risk (Q3, R2-S6): does
solution-first seeding generalise beyond `window-op`'s in-place-mutation shape,
or collapse to cosmetic variation? A second family in a genuinely different
shape settles it.

**`error-handling`** (category `error-handling`): parse `&[&str]` into `i64`,
validate each against a seed-selected rule (non-negative / at-most-N / non-zero),
combine with a seed-selected operation (sum / product / sum-of-abs), returning
`Result<i64, ParseError>` with `?` propagation. Different machinery entirely —
error paths, a custom enum the model must keep, string parsing. Its constraint is
error-handling-specific: **no `.unwrap()` / `.expect()`** (propagate, don't
panic), which needed the AST forbidden-path check extended to match *method
calls*, not just type paths.

**Verdict: seeding generalises.** All five construction gates pass across every
seed — reference 1.000, skeleton fails, both trivial baselines caught (including
the `.unwrap()` one, proving the extended AST check), canary present, determinism.
The reference-passes-its-own-oracle gate confirms the generator is self-consistent
for this shape too.

**But the anti-twin numbers are a real, quantified caveat:**

| family | min | median | near-twins |
|---|---|---|---|
| window-op | 0.122 | 0.433 | 7/45 |
| **error-handling** | **0.042** | **0.263** | **18/45** |

Error-handling instances are markedly *more similar* — median 0.263 vs 0.433,
barely above the near-twin threshold. The cause is structural: the shared
boilerplate (the `ParseError` enum, the parse-propagate skeleton, the constraint
list) dominates the prompt, so the seed-varied part (op + rule) is a small
fraction of what the model sees. **Solution-first seeding generalises in
correctness but not automatically in variance** — a family with heavy fixed
scaffolding needs a larger variable surface, or a per-category anti-twin
threshold. That is a concrete design conclusion for the 272-family plan, and it
is the honest answer to Q3: *yes, but variance must be designed for, not
assumed.*

**Two oracle bugs, both found by running the second family — not by review.**

1. **Score inflation on non-conforming answers.** The live 3B rewrote the
   provided enum as *private* and dropped `#[derive(PartialEq)]`. The lib still
   compiled (a private enum is legal), so L1 passed — but the behaviour and
   differential test targets could not build against it (`E0603`, `E0369`), so
   they produced no summary, which the oracle recorded as `None`. The composite
   then renormalised over the *remaining* layers and scored the broken answer
   **1.000**. Fixed: a configured test that yields no summary is a **failure
   (0.0)**, never absent, and flags `behavior:no_summary` / `differential:no_summary`.
   The same answer now scores **0.222** — behaviour 0, with only the style
   constraint contributing. (That 0.222 for a non-functional-but-tidy answer
   reinforces the Q28 "should behaviour be a hard floor" question already on
   file; it is the documented weighting, not a new bug.)
2. **Empty test target treated as configured.** A family opts out of the alloc
   layer by leaving its target blank; the CLI wrapped `""` as `Some("")`, so the
   oracle ran `cargo test --test ""`, failed, and (after fix 1) scored it a
   constraint failure — which broke `error-handling` validation. Fixed: empty
   target → `None`.

Window-op could not have surfaced either: its answers conform to a fixed
signature with no enum, so the interface-mismatch path was never exercised. This
is precisely why a second family of a *different shape* was the right next step.

52 tests across seven crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · P3 — anti-twin distance + trivial-baseline gates

**What.** The gate that actually tests the project's central claim — that
generated instances resist memorisation — plus two more construction gates, and
a strengthened `window-op`.

`window-op` gained a fourth operation (RotateRight(k)) and a **stride** modifier
(every window / every other window), taking the distinct-logic space from 3 to
~16, so a seed now varies genuinely different logic. The reference-passes gate
confirms every one of these still grades to 1.0 — a buggy new variant would have
been caught there.

`validate-family` now runs **five gates** per seed and reports the anti-twin
distance:

```
seed 0..9: OK  determinism=true reference=1.000 skeleton_behavior=0.000 baselines_caught=true canary=true
anti-twin (prompt+skeleton): min=0.122 median=0.433 near-twin pairs (<0.25)=7/45
```

- **Trivial-baseline gate** — `const-zero` and `identity` (right signature, wrong
  body) must each fail. They do: the oracle is not fooled by a right-shaped
  answer. This is the check that a family's oracle is not trivially beatable.
- **Canary gate** — the prompt carries its per-instance canary.
- **Anti-twin distance** (normalised token-shingle Jaccard over prompt+skeleton,
  the R4-S4 metric): reported, not hard-gated, because a finite task space
  eventually collides by pigeonhole. The useful signals are the *minimum*
  distance and the near-twin count.

**The measurement earns its keep immediately.** Median 0.433 says most seed-pairs
are genuinely different tasks — the anti-memorisation property holding. But
**7 of 45 pairs are near-twins** (min 0.122): two seeds landing on the same
(operation, stride) produce prompts differing only in the function name. That is
a real, quantified limitation of a ~16-variant family, and it drives a design
conclusion the docs should absorb: **an epoch's N seeds within a family should be
chosen (or the structural axis derived from the seed index) to guarantee N
distinct variants**, rather than trusting independent hashing not to collide.
The gate turned "does generation resist memorisation?" from an assertion into a
number.

47 tests across seven crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · P3 (first increment) — bench-gen: seeded solution-first generation

**What.** The pivotal phase begins: tasks stop being frozen files and become
**seeded generators**. A family is a function from a seed to a fresh instance,
its reference, its oracle, and the skeleton — all built from the same seed,
solution-first, so the oracle is correct by construction (ADR-0003).

`bench-gen` provides seed derivation (blake3), canary minting, a SplitMix64 PRNG
(pure in the seed), the `Generator` trait, and one real family: **`window-op`**
(category `borrow-lifetimes`). Crucially the *operation itself* is seed-selected
— Reverse / RotateLeft(k) / SwapEnds — so a seed changes the logic a model must
write, not just identifiers. From the sampled op the generator derives the
reference source, the worked example outputs (computed natively so they are
correct), the differential oracle (embedded reference + generated inputs), the
alloc test, and the ablated `todo!()` skeleton.

**`rustybench validate-family` runs the construction gates** (ADR-0003), over
every seed and with no model:

```
seed   0..7: OK  determinism=true  reference=1.000  skeleton_behavior=0.000
all gates passed
```

- **Determinism** — same seed → byte-identical instance.
- **Reference passes its own oracle (1.000)** — the synthesised reference,
  graded through the *full* pipeline (compile · behaviour · differential ·
  alloc · unsafe), scores perfect. This is the load-bearing self-consistency
  check: reference, example outputs and differential all agree, so a buggy
  generator is caught here, not in the field.
- **Skeleton fails (behaviour 0.0)** — the ablation genuinely removed the answer.

**Full loop against the live 3B, on tasks it has never seen.** `rustybench run
--family window-op --seed N` generates → prompts the model → grades under the
sandbox. Two seeds, genuinely different operations:

```
seed 3  op=reverse       fn=rework_segments  score 0.751  unit 0.6  diff 0.0  logic
seed 5  op=rotate_left(3) fn=map_frames       score 0.751  unit 0.6  diff 0.0  logic
```

Different operations and names — the anti-memorisation property. The 3B wrote a
windowed reverse whose `(w..v.len()).step_by(w)` loop **misses the last full
window** — a real off-by-one, caught partially by the unit tests (3/5) and fully
by the differential oracle (which is exactly why the differential oracle exists).
The identical 0.751 is coincidental.

**Scope of this increment:** one parametric family, three construction gates. Not
yet: the compositional archetype, the anti-twin distance gate, the remaining
`validate-family` checks (trivial-baselines-fail, prompt has one canary), and the
seed→instance server issuance the integrity model needs. 43 tests across seven
crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · bench-invariants — the doc-arithmetic CI gate (round 6's ask)

**What.** Round 6 asked for "one script that recomputes every published table
from its stated formulae and fails CI on a hand-edited cell", because the same
defect class recurred across three rounds: R1-S1 (statistics tables
arithmetically wrong), round 5's arithmetic block, and R6-S1/R6-S7 (`standard`
published two different CIs for one quantity). This is that gate.

`bench-invariants` holds the canonical parameters (corpus 5×40 + 6×12, ICC 0.3,
per-suite seeds, the timing split) and the formulae (`design_effect`, `ci_pct`,
the stratified `overall_ci`, `F/ICC` ceiling) in **one place**. Four tests parse
the actual markdown tables in `07-statistics.md` and `04-categories.md` and
assert every cell follows from the math: suite families/seeds/units, pooled
eff N, overall CI, per-category CIs, and wall-clock hours (unit-aware — smoke is
in minutes); the 272 = 248 + 24 corpus split; the 44.5-hour headline (with a
guard that a stale "39 hour" cannot reappear); and the precision ceiling.

**Proven both directions.** Passes clean on the current docs. Then I corrupted
`deep`'s overall CI to **±4.1%** — which is *exactly* the stale pre-R6 value —
and the gate failed with `deep overall CI: doc says 4.1, formula gives 4.8542`.
It catches the precise drift round 6 found, and it runs under the normal
`cargo test`, so a hand-edited cell now breaks CI.

The canonical model is now the source of truth: when Phase 3.5 measures ICC, it
changes in `bench-invariants` and every doc table must move with it or the build
fails. 36 tests across six crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · P2 — L3 `syn` AST checks: unsafe counting + forbidden paths

**What.** The structural L3 constraints, done on the parsed tree via `syn`, never
by grepping text (docs/03). A new `bench-oracle::ast` module — also the parsing
machinery P3 generation reuses — exposes `count_unsafe` (every `unsafe {}` block
and `unsafe fn`, free or impl method) and `find_forbidden_paths` (any path
segment matching `RefCell`, `transmute`, … regardless of import style). Tasks
declare `[constraint] max_unsafe` and `forbidden_paths`; `ConstraintScore` gains
`unsafe_ok` / `paths_ok` and the raw `unsafe_blocks` count.

**Demonstrated.** `split-mut-window` now sets `max_unsafe = 0` — solving a
borrow-checker task with raw-pointer `unsafe` sidesteps the very skill it probes.
An `unsafeptr` answer (correct, zero-alloc, but `unsafe { std::ptr::swap(...) }`):

```
unsafe_blocks: 1, unsafe_ok: false, violations: ["unsafe: 1 usage(s), limit 0"]
score 0.694  failure=Constraint
```

Behaviour is perfect (1.0) and it does not allocate — the unsafe is invisible to
L1/L2/L3-alloc. Only the AST check sees it. You cannot hide `unsafe` from the
parse tree.

**A calibration finding, logged not smoothed over.** Adding the unsafe check
*raised* the clone-everything score from **0.389 to 0.694**. Both `clone`
(allocates, no unsafe) and `unsafeptr` (no alloc, uses unsafe) now score 0.694 —
each violates exactly one of the two constraint checks, and the layer is their
**mean** (docs/03). So mean-aggregation gives a cheating solution half credit for
the constraint it happened *not* to violate, which **partially undoes the
constraint-dominant weighting the S6 fix relied on** — the more constraint
checks a task has, the more a single-constraint violation is diluted. This is a
real design question: for a category where every constraint expresses the same
"respect the borrow checker" intent, the layer arguably wants **min** (worst
violation) or per-check weights, not mean. Filed for P2-proper alongside the
pass-predicate decision (Q28); it is the same class of question — how strict the
aggregation should be. The machinery is correct; the aggregation policy is the
open call.

32 tests across five crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · P1 — bench-sandbox: wall-clock timeout + rlimits

**What.** Finishes the sandbox's resource controls. Model code that spins,
hangs, or fork-bombs is now stopped by a **harness-owned wall-clock timeout**,
with `RLIMIT_CPU` as a far backstop. Every cargo stage runs under it.

**Mechanism (Unix).** The child leads its own process group (`process_group(0)`),
so on the deadline the whole tree — cargo → rustc → the test binary — is killed
with `kill(-pgid, SIGKILL)`, not just the direct child. `setrlimit` runs in a
`pre_exec` hook and inherits to every descendant. Stdio is captured to temp files
so a full pipe can't deadlock the manual wait loop. `--wall-timeout-secs` makes
the deadline harness-owned and suite-tunable (docs/08: never submitter-set).

**Demonstrated.** An `infiniteloop` answer (`loop {}`) compiles, then every test
stage hangs; at `--wall-timeout-secs 5` all three are killed and recorded:

```
score 0.000  failure=Logic
flags: timeout:test, timeout:differential, timeout:alloc
```

The journal carries the `flags` (docs/12, R6-S8: a timeout is a *flag*, not a
failure class). Normal cases are unaffected — pass 1.000, subtlebug 0.844.

**A real bug, found by running it — not by review.** The first cut tied
`RLIMIT_CPU` to the wall value. The behaviour stage runs **five tests on five
threads**, so it accumulated CPU 5× faster than wall time and was killed by
`SIGXCPU` at ~1s — *before* the wall clock — so that kill was **not** recorded as
a timeout (its flag was silently missing while differential/alloc, single-test,
raced to the deadline and were flagged). The fix decouples the CPU limit to a
generous backstop (`wall × 30`) so the **wall clock is always the primary
control** for any realistic thread count. A single flat `RLIMIT_CPU == wall`
would have shipped a sandbox that under-reports timeouts on exactly the
multi-test stages that matter. Escape tests (`wall_timeout_kills_runaway`,
`normal_command_does_not_time_out`) now cover it in CI.

**Still deferred:** `RLIMIT_AS` is opt-in (macOS enforces it unreliably); Linux
netns and Windows job objects remain the cross-platform gap.

---

## 2026-08-19 · P1 (partial) — bench-sandbox: containing model-authored code

**What.** Until now the oracle ran arbitrary model-authored Rust under plain
`std::process::Command` — no containment. `bench-sandbox` closes that. Every
`cargo` invocation the oracle makes (compile, since proc macros run at *build*
time, and every test binary) now routes through one `run` seam.

**macOS (implemented, tested here).** A seatbelt profile via `sandbox-exec`:
permit by default, then subtract the two things that matter —

- **network denied** (`deny network*`): the documented Terminal-Bench "agent
  fetched solutions online" threat; and
- **writes confined** (`deny file-write*` + allow only the workspace, cargo
  caches, and temp): model code cannot write or delete outside the grading
  workspace — no `rm -rf ~`.

Reads stay broad because the toolchain needs them and reads are not the threat.
The profile was found empirically: a clean `cargo build`/`test` of the frozen
task runs fine under it.

**The escape tests are the proof, and they run in CI.** From inside the sandbox:

| probe | sandboxed result |
|---|---|
| `TcpStream` connect to 1.1.1.1:80 | **blocked** (`Operation not permitted`) — reaches it unsandboxed |
| write to `~/rb-escape-probe.txt` | **blocked** |
| write inside the workspace | allowed (grading must still work) |

**Grading is identical under containment** — pass 1.000, subtlebug 0.844, clone
0.389, byte-for-byte the same as before the sandbox. The containment contains
without perturbing the measurement.

**Recorded for integrity.** The journal now carries `sandbox: "seatbelt"` (or
`"unsupported"`), and the CLI prints `[sandbox: seatbelt]`. A leaderboard must
know whether model code was contained (docs/10).

**Linux / Windows: honestly not yet.** `available()` returns `Unsupported` and
`run` executes uncontained with a loud warning and an `unsupported` journal flag.
The netns (Linux) and job-object (Windows) paths are the remaining P1 work; the
crate never pretends to contain what it cannot. Also deferred: rlimits
(address-space / CPU-time / pid caps) and the harness-owned wall-clock timeout.

---

## 2026-08-19 · P2 (partial) — L2 differential sub-oracle

**What.** The other half of the P2 exit criterion: *"a deliberately-wrong-but-
tests-passing solution is caught by the property oracle."* The hidden oracle
ships a differential target (`tests/differential.rs`) carrying a known-correct
reference and a seeded LCG generator; it compares the candidate against the
reference over 3000 inputs including widths ≥ 3 that the example tests never
exercise. `behavior.score` now combines unit + differential with the docs/03
weights (unit 0.3 / property 0.5 / differential 0.2, renormalised over those
present). No external crate — the LCG keeps grading deterministic and offline;
`proptest` is the P2-proper upgrade once dep-vendoring is set up.

**The headline, measured end to end:**

| response | unit | differential | behavior | constraint | score | failure_class |
|---|---|---|---|---|---|---|
| correct | 1.0 | 1.0 | 1.0 | ok | **1.000** | none |
| **subtlebug** (swaps only window ends) | **1.0** | **0.0** | 0.6 | ok | **0.844** | **logic** |
| clone-everything | 1.0 | 1.0 | 1.0 | fail | 0.389 | constraint |
| logic-fail (whole-slice reverse) | 0.2 | 0.0 | 0.12 | ok | 0.658 | logic |

The `subtlebug` row is the point. It passes **all five example tests** — a
`unit` of **1.0**, a perfect score under any unit-only oracle — but it only
swaps the first two elements of each window, so it is wrong for every width ≥ 3,
which no example covers. The differential oracle catches it (`differential` 0.0),
dropping behaviour to 0.6 and the composite to 0.844, and flipping
`failure_class` from `none` to `logic`. **Without the differential oracle this
wrong solution would score a perfect 1.000.** That is the 30–32%-overstatement
finding (docs/01) fixed with real toolchain execution.

**Deferred within L2:** the property sub-oracle (metamorphic invariants) and
`proptest` shrinking of the first failing input. `differential` alone already
delivers the exit criterion.

---

## 2026-08-19 · First real model — llama-server + Qwen2.5-3B-Instruct-Q4_K_M

**What.** The first non-mock run: `rustybench run` against a live `llama-server`
(build 10470) serving Qwen2.5-3B-Instruct-Q4_K_M on the M5's Metal GPU, full
offload, concurrency pinned to 1. The HTTP path, token accounting, and the whole
oracle stack exercised against a real model.

**Result — the thesis on a real model.** Score **0.922**, `failure_class` Logic.
The model wrote a genuinely good answer: in-place `slice.reverse()`, **no
allocation** (L3 constraint 1.0), four of five behaviour tests pass. It fails one
— and the failure is a real, subtle Rust bug:

```rust
for i in (0..v.len() - w + 1).step_by(w) { ... }
//            ^^^^^^^^^^^^^^ usize underflow when v.len() < w
```

On the empty slice with `w = 3`, `0usize - 3` panics. The oracle located it
exactly: behaviour 0.8, failing input = `empty_slice`. That is the entire value
proposition demonstrated end to end — not a pass/fail bit, but "compiles,
memory-clean, one usize-underflow edge case." No general-purpose benchmark
produces that shape of result.

**Numbers:** prompt 290 tok, completion 285 tok, gen 5137 ms, grade 1775 ms.

**Throughput is NOT representative and is labelled so.** The ~55 tok/s here was
measured with 15 Chrome + 8 Safari + VS Code + Electron all compositing through
the same GPU and sharing unified-memory bandwidth, Bitdefender hooking every
cargo file write (inflating `grade_ms`), and this agent working alongside.
Correctness is robust to all of it — the oracle is deterministic — which is the
verifiable/unverifiable split (docs/10) confirmed in practice: the score is
trustworthy from a busy machine; the tok/s is not. Calibration-quality
throughput needs a quiesced host (a P1 concern), not a smoke test.

**Host:** Apple M5, 24 GB unified, macOS 26.5.1, AC power, ~81% memory free.

---

## 2026-08-19 · P2 (partial) — L3 constraint layer: allocation instrumentation

**What.** The first L3 constraint check, and per-category oracle weights, wired
through `bench-core` → `bench-oracle` → `bench-cli`.

**The mechanism.** Allocation is *measured*, not name-blacklisted. Round 5
established that `forbidden_calls = ["clone", "to_vec"]` cannot work — there are
unboundedly many ways to copy data. Instead the hidden oracle ships a test
target (`tests/alloc.rs`) carrying a counting `#[global_allocator]`; it snapshots
the allocation count, calls the function under test, and asserts zero
allocations in the hot path. A clone-everything solution allocates and fails it.
Because each integration test is its own binary, the allocator in `alloc.rs`
governs only that target and does not perturb the behaviour tests.

**Per-category weights.** `composite_score` now renormalises over whichever
layers produced a score, so `borrow-lifetimes` can be constraint-dominant
(behavior 0.35 / constraint 0.55) per docs/04. That is the fix for REVIEW.md S6:
under the old behaviour-dominant default a clone-everything solution scored
near-identically to a proper one.

**Verified** on the frozen borrow task, four model responses, weights
behavior 0.35 / constraint 0.55 (measured end to end against a mock):

| response | apply | compile | behavior | constraint(alloc) | score | failure_class |
|---|---|---|---|---|---|---|
| correct (`chunks_exact_mut`) | ✓ | ✓ | 1.0 | ok | **1.000** | none |
| clone-everything (`to_vec` + copy back) | ✓ | ✓ | 1.0 | **fail** | **0.389** | constraint |
| second mutable borrow | ✓ | ✗ (E0499) | — | — | **0.000** | borrowck |
| whole-slice reverse | ✓ | ✓ | 0.2 | ok | **0.689** | logic |

The clone-everything row is the point: behaviourally correct, all five
correctness tests pass, but the allocation it performs is now visible in the
score. Under the behaviour-dominant default weights the same answer scores
**0.778**; constraint-dominant weighting roughly halves it to **0.389**. That is
the REVIEW.md S6 fix, demonstrated with real toolchain execution.

**Observed tension, logged rather than hidden.** The logically-wrong
whole-slice-reverse (0.689) now *outscores* the behaviourally-correct
clone-everything (0.389). That is a genuine consequence of constraint-dominant
weighting: whole-slice-reverse manipulates in place (0 alloc → full 0.55
constraint credit) while clone-everything sidesteps the borrow skill entirely
(0 constraint credit) despite passing correctness. Whether "wrong but in-place"
should beat "right but allocating" on a *borrow-checker* skill probe is a real
design question, not a bug in the machinery — the composite is doing exactly
what the weights say. It bears on the pass-predicate decision (Q28) and on
whether behaviour should be a floor rather than a weighted term. Flagged for
P2-proper; the roadmap's "near zero" for clone-everything is reached more fully
once that is settled and/or L4 quality is present.

**Deferred within L3:** clippy, fmt, and `syn`-based unsafe/forbidden-path
checks. Allocation was done first because it is the flagship and hits the
exit criterion; the others are cheap follow-ups.

---

## 2026-08-19 · P0 — the spine

**What.** `rustybench run` end to end: load a frozen task → call an
OpenAI-compatible model → grade with the real `cargo`/`rustc` toolchain →
append a scored JSONL journal line.

**Crates.** `bench-core` (pure types + scoring + rustc-code→`FailureClass`),
`bench-model` (blocking `/v1/chat/completions`), `bench-oracle` (L0 apply, L1
compile with diagnostic capture, L2 unit), `bench-cli` (the `run` binary). Plus
one hand-written frozen borrow-checker task with five hidden tests.

**Verified** against a mock model: correct → 1.000/none, second-mutable-borrow →
0.000/borrowck (from a real `E0499`), whole-slice-reverse → 0.200/logic.

**Two bugs found by running it, not by review:**
- A materialised task crate was adopted by the outer cargo workspace. Fixed with
  a standalone `[workspace]` table in the task `Cargo.toml` — also how generated
  tasks will ship.
- The test-summary parser read the last `test result:` line (empty doc tests)
  instead of summing lib + integration + doc sections.

14 unit tests, clippy clean under `-D warnings`, rustfmt clean.

**Not yet, by design:** generation, seeding, the sandbox, resume, submission,
and the blocking measurements the reviews flagged (ρ / Q22, the pass predicate /
Q28, the statistical machinery / Q29).
