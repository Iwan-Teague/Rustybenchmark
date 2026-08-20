# Build log

A running record of what has actually been built, verified, and decided —
distinct from the design docs (which specify the target) and the review rounds
(which attack it). One entry per meaningful increment. Newest first.

The roadmap phases referenced here are in [14-roadmap.md](14-roadmap.md).

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
