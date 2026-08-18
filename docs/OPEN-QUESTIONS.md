# Open questions

Unresolved as of 2026-08-17. Each needs a decision before the phase that depends on it.

---

## Q1 — License

**Blocks:** first public commit.

Generators must be public for the benchmark to be credible, and the raw corpus must be publishable. Candidates: Apache-2.0, MIT/Apache dual (Rust ecosystem norm), or a permissive code license with the task corpus under CC-BY.

Consideration: do we want to discourage the corpus being used as *training data*? A license cannot really prevent it, and a restrictive license would undercut the "publish everything" integrity argument. Probably dual MIT/Apache-2.0 with a stated norm rather than a legal restriction.

---

## Q2 — Mining pipeline design for the `wild` suite

**Blocks:** Phase 7. **Not** Phase 5.

Rust-SWE-bench's pipeline is the model: >1k-star repos, PRs linked to issues and touching tests, Docker + cargo snapshot per PR, execution-based fail-to-pass validation, then human review. Their yield was 500 tasks from ~80k scraped PRs — roughly 0.6%.

Open: can we get a usable yield restricted to *small* commits (3–10 files, ≤2k LoC repos)? If the yield collapses, category 10's family budget needs revisiting.

---

## Q3 — How much does `Spec` need to carry per category?

**Blocks:** Phase 3.

`borrow-lifetimes` has an obvious structural parameterisation. `error-handling` and `idiom-refactor` are less obvious — what is the structural axis that a seed varies? Risk: some categories reduce to cosmetic variation and fail the `min_instance_distance` gate.

Mitigation to test in Phase 3: author one family in a *hard* category (suggest `idiom-refactor`) alongside the exemplar, specifically to find out whether the pattern generalises or whether some categories need a different generation strategy.

---

## Q4 — Agentic track: in or out?

**Blocks:** nothing near-term; affects Phase 8.

Single-shot and repair measure the model. Agentic measures model + scaffolding, which is what people actually use but is much harder to attribute and multiplies cost. Current position: separate track, separate leaderboard, not in the headline number. Revisit once the core is stable.

---

## Q5 — Which quantisations to canonicalise?

**Blocks:** leaderboard presentation, Phase 6.

If every quant of every model is its own row, the table is unreadable. If they are merged, the numbers are wrong. Options: canonical quant per model (Q4_K_M as the default row, others behind a toggle), or full matrix with aggressive filtering. Leaning toward the former with the latter available.

---

## Q6 — Server hosting and cost · **CLOSED**

**Blocks:** Phase 6.

T1 replay means the server re-runs the oracle for every submitted unit. At 1200 units per `deep` run, that is real CPU. Options: verify a random sample (say 20%) rather than everything, verify everything but queue it asynchronously, or require submitters to fund verification for large suites. Sampling weakens the guarantee; queuing does not. Leaning toward: verify everything, asynchronously, with the tier badge upgrading from T0 to T1 when verification completes.


**Closed 2026-08-17 by measurement** ([REVIEW-3.md](REVIEW-3.md) R3-S6). See Q15.

---

## Q7 — Windows sandbox parity

**Blocks:** Phase 1.

Network denial and memory limits via job objects and WFP are less well-trodden than netns and seatbelt. If parity is not achievable, the options are: Windows runs at a lower trust tier, Windows requires WSL2, or Windows is unsupported at launch. Needs a spike early — it affects a large fraction of the consumer audience.

---

## Q8 — Power telemetry on Apple Silicon

**Blocks:** `efficiency_score` on a major platform.

`powermetrics` requires elevated privileges. Asking users to run the harness with sudo is unacceptable. Options: prompt once for a helper installation, skip `efficiency_score` on macOS, or find a non-privileged source. Default for now: skip and report `null` rather than estimate.

---

## Q9 — Do we run hosted frontier models as reference points?

**Blocks:** Phase 5 report framing.

Arguments for: gives readers an anchor, makes the local-model numbers interpretable. Against: costs money, invites "why is your leaderboard not about frontier models", and their sampling/config is not controllable. Leaning toward: yes, a small number, clearly marked as reference anchors and excluded from the main ranking.

---

## Q11 — What is the async oracle?

**Blocks:** authoring `async-concurrency` (Phase 7). Raised by [REVIEW.md](REVIEW.md) S10.

The differential oracle compares candidate and reference outputs on generated inputs. Under nondeterministic scheduling there is no stable output to compare, so the specification as written either produces spurious failures from scheduling variance or degenerates into a compile check.

Working hypothesis: grade on **linearizability properties under loom's exhaustive scheduling** where the state space is small enough, and on `Send`/`Sync`/cancellation-safety **constraints** otherwise — i.e. treat the category as constraint-dominant like `borrow-lifetimes`. Loom's state-space explosion is the binding constraint on how large a task can be.

Needs a spike before the category is authored. Until resolved, `async-concurrency` stays a probe category.

---

## Q12 — Context limits for `cross-module`

**Blocks:** Phase 7. Raised by [REVIEW.md](REVIEW.md) S13.

A 3–10 file, ≤2k LoC repo plus instructions plus repair diagnostics can exceed a 32k context. Small-context models would be scored on a category they were structurally prevented from attempting, conflating context window with capability.

Options: declare a minimum context per suite and refuse below it; provide a retrieval interface so the model requests files; score `skipped_context` as a separate outcome rather than as failure. Leaning toward the first plus the third — refusing is honest, and a retrieval interface turns the category into an agentic benchmark, which is a different measurement.

---

## Q10 — Epoch length

**Blocks:** Phase 6.

Monthly is the working assumption. Shorter epochs mean fresher seeds but fewer models per epoch, which weakens paired comparison. Longer epochs mean better pairing but more precomputation exposure. Needs a decision informed by actual submission volume, so: start monthly, revisit with data.

---

## Q13 — Probe-subset detector power · **CLOSED**

**Blocks:** Phase 6. Raised by [REVIEW-2.md](REVIEW-2.md) R2-S1 / [ADR-0009](adr/0009-paired-core-and-fresh-probe-seeds.md).

The fresh-probe subset (~15% of units) detects precomputation by comparing probe score to core score. But 15% of a `deep` run is ~163 units, clustered by family — so its effective N is well under 100, and the same design-effect arithmetic that governs scoring applies to the detector.

Unvalidated: can a gap of the size a cheater would produce be distinguished from ordinary sampling noise at useful confidence? The 15% figure is provisional and should be derived from a power calculation on the detector, not chosen for looking reasonable.


**Closed 2026-08-17 by calculation** ([REVIEW-3.md](REVIEW-3.md) R3-S3). The power arithmetic was
done. At 15% probe the originally specified score-comparison detects only **12.4 points** of
inflation; a **sign test on family-paired discordance** detects **~5.2 points** at identical cost.
The detector is now the sign test, and probe seeds are drawn on families already present in core so
the pairing exists. 15% stands.

**Residual, published rather than hidden:** inflation below roughly **4–5 points** is undetectable at
any probe share we can afford. The probe is a screening test with a stated floor.

---

## Q14 — Are `de_idiomatize` transforms invertible in practice? · **CLOSED, badly**

**Blocks:** authoring `idiom-refactor` (Phase 5). Raised by [REVIEW-2.md](REVIEW-2.md) R2-S6.

Each clippy lint has a conceptual inverse, but applying several mechanically may produce code that reads as obviously machine-mangled rather than as plausible human-written non-idiomatic Rust. A prompt that looks generated is a prompt models will treat differently — and possibly one they pattern-match to "this is a benchmark" rather than reasoning about.

Needs a Phase 3 spike alongside the exemplar family: generate 20 instances, have a Rust developer judge whether they read as plausible real code.


**Closed 2026-08-17 by test** ([REVIEW-3.md](REVIEW-3.md) R3-S2). Two answers.

**Invertibility: yes.** Three transform classes (iterator→index loop, `Option::map`→`match`,
`?`→`match`) round-trip with identical behaviour across empty, negative, boundary, and repeated
inputs. The output reads as ordinary novice Rust, not machine-mangled.

**But the task is not hard.** `cargo clippy --fix` solved **2 of the 3** unaided, with equivalence
tests still passing — clippy's `help: try:` text on machine-applicable lints *is* the answer. This
compromised a core, rankable category. Three fixes landed: a new family-validation gate (`clippy
--fix` must not solve the instance), exclusion of machine-applicable inverses from the transform
catalogue, and stripping suggestion text from repair feedback.

The legitimate task space is the transform classes clippy can *detect but not mechanically apply*.
It is smaller than assumed, and authoring for this category is correspondingly harder.

---

## Q15 — Server-side replay cost under the corrected timing model · **CLOSED**

**Blocks:** Phase 6. Supersedes part of Q6.

Q6 was written before the round-1 timing correction. A `deep` run is 1088 scored units, and T1 replay re-runs L0–L3 for every one. At corrected build-and-grade costs this is materially more server CPU than Q6 assumed. Re-derive before committing to "verify everything asynchronously".


**Closed 2026-08-17 by measurement** ([REVIEW-3.md](REVIEW-3.md) R3-S6). A warm `cargo build` +
`test` + `clippy` cycle measures **0.65–0.68 s** and barely scales with crate size (0.68 s at 15
lines / 2 tests; 0.65 s at 365 lines / 60 tests) — incremental compilation dominates. At a
conservative 3 s per non-miri unit, one `deep` submission costs **2.1–6.1 CPU-hours** depending on
miri cost, or under 23 minutes wall on a 16-core box.

**Decision: verify every unit of every submission.** No sampling, no asynchronous badge upgrade, no
submitter-funded verification. A small cloud bill, not an architectural constraint.

---

## Q16 — Does the sign-test detector survive an adaptive adversary? · **CLOSED — it does not**

**Blocks:** Phase 6. Raised by [REVIEW-3.md](REVIEW-3.md) round 4 scope.

R3-S3 computed the detector's power against a *naive* cheater who precomputes a random subset. An adaptive one has options: precompute only families they predict will be probed, or deliberately fail some core units to flatten the discordance signal and stay under the threshold.

The second is the interesting one, because it is self-limiting — suppressing core passes to hide inflation gives back the points the inflation bought. Needs the arithmetic done properly: is there a strategy that nets a gain while staying below the ~5-point detection floor?


**Closed 2026-08-17 by simulation** ([REVIEW-4.md](REVIEW-4.md) R4-S1). **No.** An adversary who
precomputes *and* deliberately fails core units to rebalance the discordance keeps **+9 to +10 points
of inflation at 0% detection** (20% precomputation, 300 trials). Family-aggregate pairing and
probe/core indistinguishability were both tested; both failed.

The ratio is structural: the cheat pays off across 1088 scored units while the detector observes 163
family-level pairs, so hiding is ~6.7× cheaper than the gain. No test statistic fixes that.

**Consequence:** the probe is demoted to a screening test and **seed secrecy becomes the primary
control** — core seeds are not published until their epoch closes. The self-cancelling intuition that
suppression must cost what it gains is wrong, and was only caught by simulating rather than trusting
the algebra.

---

## Q17 — Is `min_instance_distance` measurable for compositional families? · **CLOSED**

**Blocks:** authoring `idiom-refactor` (Phase 5). Raised by [REVIEW-3.md](REVIEW-3.md) round 4 scope.

For parametric families the skeleton is an *ablation* of the reference, so prompt-to-prompt tree-edit distance is well defined. For compositional families the skeleton is a *transform* of the reference, and two instances applying the same three transforms to different code may be structurally near-identical by tree-edit distance while being genuinely different tasks — or the reverse.

The gate may be measuring the wrong thing for half the corpus. Needs validation against real generated instances before the gate is trusted for compositional families.


**Closed 2026-08-17 by test** ([REVIEW-4.md](REVIEW-4.md) R4-S4). The hypothesised *inversion* did
not reproduce — surface distance returned the correct verdict on both probes. But the margins were
0.146 and 0.331 either side of a 0.25 threshold, where transform-set Jaccard gives an unambiguous
0.00 and 1.00. Surface distance is a correlate that holds only while reskins are shallow.

**Resolution:** compositional families gate on `min_transform_jaccard` as primary, surface distance
secondary.

---

## Q18 — Does miri leave enough task space for 40 `unsafe-core` families? · **CLOSED — barely**

**Blocks:** Phase 7. Raised by [REVIEW-3.md](REVIEW-3.md) round 4 scope.

R3 split `unsafe-ffi` because miri cannot execute foreign calls, moving FFI to a probe category and keeping `unsafe-core` as a core category with miri mandatory. But miri has further limitations — restricted `libc`, no real syscalls, some `std` internals unsupported — and `unsafe-core` needs **40 families**, the full core budget.

If miri-checkable unsafe Rust does not span 40 genuinely distinct family shapes, the category either shrinks below core size or admits families miri cannot check, which defeats the split. Needs a feasibility survey before P7.


**Closed 2026-08-17 by survey** ([REVIEW-4.md](REVIEW-4.md) R4-S5). About **16 distinct
miri-checkable shapes** exist (provenance, transmute, `MaybeUninit`, `&mut` aliasing, use-after-free,
custom allocators, `Pin`, unions, drop order, packed refs, `from_utf8_unchecked`, `NonNull`, threads
and data races, atomics (partial), alignment, int-to-ptr provenance). At 2–3 families per shape that
is 32–48 — so 40 fits, but near the ceiling.

**The consequence that matters:** 40 families across ~16 shapes means families cluster, so the
category's effective **ICC exceeds the corpus-wide 0.3** and its honest CI is nearer ±12.3% than the
±10.7% claimed. ICC is now estimated and published **per category**, not pooled. Miri's ~60×
slowdown independently validates the 30–120 s/unit figure used in [R3-S6](REVIEW-3.md).

---

## Process note — schema drift

Both R3-S1 and M3-1 were schema gaps created by prose changes in other documents: probe units were specified in [ADR-0009](adr/0009-paired-core-and-fresh-probe-seeds.md) and [10-integrity.md](10-integrity.md) without ever reaching [12-schemas.md](12-schemas.md), and the resulting contradiction with the frozen plan went unnoticed for a full review round.

**The schemas are drifting behind the design.** Round 4 should cross-check [12-schemas.md](12-schemas.md) against every other document, and thereafter any change introducing a new field or unit kind must patch the schema in the same commit.

---

## Q19 — Can seed secrecy survive a confederate inside an open epoch?

**Blocks:** Phase 6. Raised by [REVIEW-4.md](REVIEW-4.md) R4-S2.

Seed secrecy is now the primary precomputation control, but a submitter who has already run holds the epoch's core seeds and can pass them to a confederate before the epoch closes.

Open: is per-submitter seed **salting** possible while preserving enough pairing — for example, pairing only across submitters who share a salt cohort, accepting a smaller effective comparison set in exchange for removing the leak channel? Needs the power arithmetic doing on cohort sizes.

---

## Q20 — Do the plausibility checks survive the same suppression attack?

**Blocks:** Phase 6. Raised by [REVIEW-4.md](REVIEW-4.md) round 5 scope.

R4-S1 showed a fresh-subset detector is defeated by an adversary willing to throw units. The plausibility checks in [10-integrity.md](10-integrity.md) — throughput consistency, memory physics, thermal signature, error-code fingerprint, canary cross-screening — have **never been modelled adversarially at all**. They were designed against carelessness, not against someone optimising against them.
