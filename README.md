# Rustybenchmark

A benchmark harness for measuring what **local LLMs can actually do in Rust**, and what **your specific machine** can deliver while doing it.

Two numbers that do not currently exist side by side anywhere:

- **Capability** — what a model can solve, time-unbounded, broken down by Rust skill area (borrow checker, traits, unsafe, async, idiom, perf, …).
- **Throughput** — what a given machine + model + quantisation combination actually delivers per hour, per GB of VRAM, per watt-hour.

Every run profiles the host, calibrates inference on it, runs a seeded, contamination-resistant task suite in a sandbox, grades with a layered oracle, and (opt-in) submits redacted results to a public leaderboard.

Written in Rust. Runs on consumer hardware. Talks to any OpenAI-compatible endpoint (llama.cpp, Ollama, vLLM, LM Studio).

---

## Status

**Exploratory / design phase.** No code yet. This repository currently holds the design documents that specify the system.

## Read order

| # | Document | What it settles |
|---|---|---|
| — | [docs/00-overview.md](docs/00-overview.md) | Goals, non-goals, the two-number thesis |
| — | [docs/01-prior-art.md](docs/01-prior-art.md) | What already exists, with figures, and the gap we fill |
| 1 | [docs/02-task-format.md](docs/02-task-format.md) | What a task *is* — manifest, generator contract, solution-first generation |
| 2 | [docs/03-oracle.md](docs/03-oracle.md) | How a solution is graded — the five layers |
| 3 | [docs/04-categories.md](docs/04-categories.md) | The ten Rust skill categories and their family budgets |
| 4 | [docs/05-hardware-and-calibration.md](docs/05-hardware-and-calibration.md) | Host inventory, inference calibration, derived metrics |
| 5 | [docs/06-execution-classes.md](docs/06-execution-classes.md) | GPU / hybrid / CPU classification and memory accounting |
| 6 | [docs/07-statistics.md](docs/07-statistics.md) | Suite sizing, ICC, power analysis, confidence intervals |
| 7 | [docs/08-run-protocol.md](docs/08-run-protocol.md) | The run lifecycle and the sandbox |
| 8 | [docs/09-resume-and-checkpointing.md](docs/09-resume-and-checkpointing.md) | Multi-session runs, segments, crash safety |
| 9 | [docs/10-integrity.md](docs/10-integrity.md) | Trust tiers, challenge/replay, anti-gaming |
| 10 | [docs/11-submission-and-privacy.md](docs/11-submission-and-privacy.md) | What is uploaded, what is redacted, consent |
| 11 | [docs/12-schemas.md](docs/12-schemas.md) | Every on-disk and on-wire schema in one place |
| 12 | [docs/13-architecture.md](docs/13-architecture.md) | Crate layout and dependency direction |
| 13 | [docs/14-roadmap.md](docs/14-roadmap.md) | Build order and the minimum shippable product |

Supporting:

- **[docs/REVIEW.md](docs/REVIEW.md)** — adversarial review round 1. Read this alongside the docs it corrects; several published figures were wrong and the category design changed as a result
- **[docs/REVIEW-2.md](docs/REVIEW-2.md)** — adversarial review round 2, empirical against rustc 1.97. Found a direct contradiction between the statistical and integrity designs, and measured the diagnostic instrumentation to be blind for a third of realistic failures
- [docs/GLOSSARY.md](docs/GLOSSARY.md) — terms used precisely throughout
- [docs/OPEN-QUESTIONS.md](docs/OPEN-QUESTIONS.md) — unresolved decisions, with the phase each blocks
- [docs/adr/](docs/adr/) — architecture decision records, including reversals

## The one-paragraph version

Static coding benchmarks rot: models memorise them. Rustybenchmark ships **generators**, not answers. Each task family is a function from a seed to a fresh problem instance, its reference implementation, and its property-based oracle — all constructed from the same seed, so the oracle is correct by construction and the instance has never been seen. Grading is deterministic given the seed, which means the server can independently re-verify any submitted correctness claim. Hardware claims cannot be verified, so they are reported separately and at a lower trust level. Runs checkpoint continuously and resume across sessions, because the largest suite takes around 39 hours on consumer hardware.

## License

TBD — see [docs/OPEN-QUESTIONS.md](docs/OPEN-QUESTIONS.md).
