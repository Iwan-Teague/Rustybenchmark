# 14 — Roadmap

## Honest cost statement

The corpus is the project. ~165 hand-written solution-first generators, each with reference synthesis, property derivation, ablation, and a CI validation harness — call it **1–3 days each once the framework exists**. Plus ~35 mined families and the mining pipeline.

Everything else — sandbox, oracle, hardware profiling, statistics, resume, attestation, server — is perhaps 3–4 months of focused work and then mostly stable.

The conclusion that follows: **over-invest in the framework and in `bench-tasks` ergonomics.** Generators 2 through 200 are only tractable if generator 1 established a good pattern and good helpers. Every hour spent making family authoring pleasant pays back ~200 times.

## Phase 0 — Spine (weeks 1–3)

One `frozen` task, no generation. Prove the loop end to end.

- `bench-core` types
- `bench-model` OpenAI-compatible client against a local llama.cpp
- `bench-oracle` L0 + L1 + L2-unit only
- `bench-cli run` writing JSONL

**Exit criterion:** `rustybench run` against a real local model produces a scored journal line.

## Phase 1 — Containment and hardware (weeks 3–6)

Independent of task content, so it can proceed in parallel with Phase 2 thinking.

- `bench-sandbox` on all three platforms, with the escape-attempt test suite
- `bench-hw` inventory + calibration + sustained phase + pre-run gates
- `ExecClass` and `MemProfile` recorded from the first run onward

**Exit criterion:** the escape tests fail correctly on macOS, Linux, and Windows; calibration numbers are reproducible within ±5% across repeat runs on the same machine.

## Phase 2 — Oracle depth (weeks 5–8)

- L3 constraint layer with `syn`-based AST checks — cheap, high signal, very Rust
- L2 property + differential sub-oracles with seeded proptest
- `failure_class` derivation from rustc error codes

**Exit criterion:** a deliberately-wrong-but-test-passing solution is caught by the property oracle.

## Phase 3 — Generation (weeks 7–12)

The pivotal phase. Get this right and the rest is repetition.

- `bench-gen`: seed derivation, canary minting, ablation helpers, tree-edit distance
- **One exemplary family**, `borrowck/split-mut-window`, done to full quality
- `validate-family` with all nine CI gates
- Second and third families, authored by following the first, to test whether the pattern generalises

**Exit criterion:** a family can be authored in under two days by following the exemplar, and passes 1000-seed validation.

## Phase 4 — Runs that survive reality (weeks 10–14)

- `bench-run`: plan freezing, journal, segments, resume, validity gates
- Crash-injection test suite
- Scheduling controls (`--max-duration`, `--until`, `--pause-on-battery`)
- `status` with partial scores and ETA
- `bench-stats`: cluster bootstrap, effective N, honest CIs

**Exit criterion:** a run killed at every stage boundary resumes to bit-identical final results.

## Phase 5 — First real data (weeks 12–20)

- **3 categories × 20 families = 60 families.** `deep` tier on that subset gives roughly ±14% per category and a defensible overall number — already better-grounded than most published local-model coding comparisons.
- Suggested first three: `borrow-lifetimes`, `traits-generics`, `error-handling`. Most Rust-distinctive, all synthesisable, no miri/criterion dependency.
- Run across several models and several machines. Publish the analysis, not a leaderboard.

**Exit criterion:** a written report with real numbers and honest error bars. This is the first thing the outside world sees, and it should be a paper-shaped artifact rather than a website.

## Phase 6 — Attestation and submission (weeks 18–24)

- `bench-attest`: manifest, canonical CBOR, signing, redaction, canary screening
- Challenge/batch protocol client (server may stub to `local` initially)
- Consent flow, three separate consents
- Server: `/submit`, `/dump`, minimal leaderboard
- T1 replay verification server-side

**Exit criterion:** an independent third party can download the dump and re-derive the leaderboard exactly.

## Phase 7 — Scale the corpus (ongoing, months 6–18)

- Remaining synthetic categories, ~20 families each
- Mining pipeline for `multi-file-repo` and `api-evolution`
- L4 quality layer (mutation, criterion) where categories need it
- Epoch rotation in production

## Phase 8 — Maturity (months 12+)

- `calibrate-suite`: empirical ICC, per-family IRT parameters
- Adaptive item allocation
- T3 audit process and remedies document
- Agentic track as a separate leaderboard

## What ships first, publicly

Not a leaderboard. **A report.** Phase 5's output — three categories, several models, several machines, real confidence intervals, published methodology, published generators, published raw data.

A leaderboard launched before there is enough corpus to support category-level claims would be making claims the statistics cannot back, and would be very hard to walk back. The report establishes the method; the leaderboard follows once the corpus justifies it.

## Sequencing rules

1. **Schema decisions before code.** The submission manifest, `ExecClass`, `MemProfile`, and the IRT placeholder fields are cheap now and expensive later. See [12-schemas.md](12-schemas.md).
2. **Sandbox before corpus.** A corpus built against a leaky sandbox has to be revalidated.
3. **Resume before long suites.** Do not discover resume bugs in someone's 40-hour run.
4. **Local runner before server.** The server is small and additive; the runner is the product.
5. **Report before leaderboard.** Method credibility first.
