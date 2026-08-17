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

## Q6 — Server hosting and cost

**Blocks:** Phase 6.

T1 replay means the server re-runs the oracle for every submitted unit. At 1200 units per `deep` run, that is real CPU. Options: verify a random sample (say 20%) rather than everything, verify everything but queue it asynchronously, or require submitters to fund verification for large suites. Sampling weakens the guarantee; queuing does not. Leaning toward: verify everything, asynchronously, with the tier badge upgrading from T0 to T1 when verification completes.

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

## Q13 — Probe-subset detector power

**Blocks:** Phase 6. Raised by [REVIEW-2.md](REVIEW-2.md) R2-S1 / [ADR-0009](adr/0009-paired-core-and-fresh-probe-seeds.md).

The fresh-probe subset (~15% of units) detects precomputation by comparing probe score to core score. But 15% of a `deep` run is ~163 units, clustered by family — so its effective N is well under 100, and the same design-effect arithmetic that governs scoring applies to the detector.

Unvalidated: can a gap of the size a cheater would produce be distinguished from ordinary sampling noise at useful confidence? The 15% figure is provisional and should be derived from a power calculation on the detector, not chosen for looking reasonable.

---

## Q14 — Are `de_idiomatize` transforms invertible in practice?

**Blocks:** authoring `idiom-refactor` (Phase 5). Raised by [REVIEW-2.md](REVIEW-2.md) R2-S6.

Each clippy lint has a conceptual inverse, but applying several mechanically may produce code that reads as obviously machine-mangled rather than as plausible human-written non-idiomatic Rust. A prompt that looks generated is a prompt models will treat differently — and possibly one they pattern-match to "this is a benchmark" rather than reasoning about.

Needs a Phase 3 spike alongside the exemplar family: generate 20 instances, have a Rust developer judge whether they read as plausible real code.

---

## Q15 — Server-side replay cost under the corrected timing model

**Blocks:** Phase 6. Supersedes part of Q6.

Q6 was written before the round-1 timing correction. A `deep` run is 1088 scored units, and T1 replay re-runs L0–L3 for every one. At corrected build-and-grade costs this is materially more server CPU than Q6 assumed. Re-derive before committing to "verify everything asynchronously".
