# 03 — The oracle

Grading produces a **vector**, not a bit. Layers run in order; a failed gate short-circuits later layers but is always recorded.

```
L0 apply      gate
L1 compile    gate            + rustc diagnostic codes  <- richest Rust-specific signal
L2 behavior   weight 0.7      unit + property + differential
L3 constraint weight 0.2      clippy, unsafe, forbidden APIs, signature, miri
L4 quality    weight 0.1      mutation score, perf ratio, size ratio
```

```
task_score = (apply_ok && compile_ok)
           ? 0.7*behavior + 0.2*constraint + 0.1*quality
           : 0.0
```

The composite exists for sorting. **Always report the vector.** `compile_rate` alone is a headline-worthy number, and the error-code histogram is the diagnostic that tells a user *which part of Rust* a model is weak at.

---

## L0 — Apply (gate)

The model's response is materialised into the workspace.

- Patch applies cleanly, or whole-file replacement is well-formed.
- No writes outside the workspace root.
- No new files outside declared-writable paths.
- No modification of `Cargo.toml` dependency section unless the family permits it.

**Emits:** `apply_ok: bool`, `apply_error: Option<String>`

Response-format failures land here. Track them: a model that can solve the problem but cannot emit a valid diff is a real and separately interesting result. Aider's benchmark demonstrates that strict diff-edit format is itself a discriminator.

## L1 — Compile (gate)

```
cargo build --offline --locked --message-format=json
```

Capture **every** diagnostic, not just success/failure.

**Emits:**
- `compile_ok: bool`
- `error_codes: Vec<String>` — e.g. `["E0499", "E0597"]`
- `warn_count: u32`
- `build_ms: u64`
- `failure_class` (derived): `borrowck | trait | type | lifetime | syntax | resolve | other`

The `failure_class` derivation is a lookup table from rustc error codes. This is what produces the per-category diagnostic that no general-purpose benchmark can offer. Rust-SWE-bench attributes **32.6%** of agent failures to type/trait/borrow semantics — we make that number visible per model, per category, automatically.

## L2 — Behavior (weight 0.7)

Three sub-oracles, all reproducible from the instance seed.

```toml
[behavior.unit]
weight = 0.3
# hidden example tests, seed-parameterised

[behavior.property]
weight = 0.5
runner        = "proptest"
cases         = 512
proptest_seed = "derive:instance"    # blake3(instance_seed || "prop")
properties    = ["order_preserved", "no_element_lost", "idempotent_on_empty"]

[behavior.differential]
weight  = 0.2
inputs  = 2000
compare = "eq"        # eq | approx(1e-9) | set_eq | order_insensitive
# candidate vs oracle/reference.rs on seeded generated inputs
```

Property and differential carry 70% of the behavior weight deliberately. Published evidence: property-based evaluation of StarCoder and CodeLlama on MBPP/HumanEval found **30–32% of solutions only partially satisfy correctness properties and 18–23% fail outright** — all of which example tests scored as passes. Example-only grading would bias our numbers upward in exactly the same way as everyone else's.

**Emits:** `behavior_score: f32` (0.0–1.0), per-sub-oracle breakdown, and `first_failing_input` — the proptest-shrunk minimal counterexample. Publish that last one; it is the single most useful artifact for a human reading a failure.

Property definitions live in `oracle/props.rs` and are **derived from the `Spec`**, not written against the reference implementation. Deriving them from the code would make them tautological.

## L3 — Constraint (weight 0.2)

Rust-specific static gates. Each is binary; the layer score is their weighted mean.

```toml
[constraint]
clippy          = { level = "deny", allow = ["clippy::needless_range_loop"] }
fmt             = true
unsafe_blocks   = { max = 0 }
forbidden_calls = ["clone", "to_vec", "Rc::new", "RefCell"]
required_traits = ["Iterator"]
miri            = { enabled = false }          # true for the unsafe-ffi category
no_std          = false
public_api      = "must_match_signature"       # exported signatures unchanged
alloc_free      = false                        # optional: no heap allocation in hot path
```

**All checks are AST-based via `syn`, never textual.** Grepping for `clone` is defeated by `<[T]>::to_vec` and false-positives on comments and string literals. The forbidden-call checker resolves method calls against the type where it can and falls back to path matching where it cannot; it records which mode it used so borderline results are auditable.

**Emits:** `constraint_score: f32`, `violations: Vec<String>` with file:line — e.g. `"forbidden: clone @ src/lib.rs:41"`.

`miri` is mandatory for the `unsafe-ffi` category and off elsewhere (it is slow). When on, a miri UB report is a hard behavior failure, not a constraint deduction — undefined behaviour is not a style issue.

## L4 — Quality (weight 0.1)

```toml
[quality]
mutation = { tool = "cargo-mutants", timeout_s = 120, weight = 0.5 }
perf     = { tool = "criterion", baseline = "oracle/reference.rs", max_ratio = 2.0, weight = 0.3 }
size     = { max_ratio_vs_reference = 3.0, weight = 0.2 }
```

- **Mutation score** is the primary oracle for the `test-authoring` category: did the model's *tests* kill the mutants? For other categories it measures whether the solution is meaningfully specified rather than accidentally passing.
- **Perf ratio** is the primary oracle for `perf-optimization`. Criterion against the reference implementation, ratio capped at `max_ratio`.
- **Size ratio** penalises LoC bloat relative to the reference.

L4 is expensive. **Off by default**; enabled with `--quality`. Required for the `deep` and `full` suites, optional for `standard`, unavailable in `smoke`.

### Perf measurement caveat

Criterion ratios are noisy on thermally unstable or contended hardware. The `perf-optimization` category is therefore gated to:

- execution class `GpuFull`
- AC power (not battery)
- a stability precheck: repeated timing of the reference must vary by <5% across repetitions

If the precheck fails, perf tasks are skipped and the run is marked `perf_unavailable` rather than scored badly. See [06-execution-classes.md](06-execution-classes.md).

---

## Oracle isolation — non-negotiable

The single highest-value integrity control in the whole system.

1. The model's workspace and the grading workspace are **separate directories**.
2. `oracle/` content is materialised into the grading workspace **only after** the model's turn has completed and its response has been captured.
3. Before handing the workspace to the model, the harness asserts that no file in it hashes to any oracle file's content hash.
4. The sandbox denies network access for the entire turn, enforced by the OS, not by policy.

Terminal-Bench's three documented misconduct cases were: **shipping the test folder in the agent setup**, **modifying timeouts**, and **fetching solutions from the internet**. Controls 2, 4, and harness-owned timeouts respectively address each.

## Repair mode

When `interaction.mode = "repair"`, attempt 2 receives:

- the exact rustc diagnostics from L1 (rendered, not JSON)
- the names of failing tests and the shrunk counterexample from L2, **without the property source**
- constraint violations from L3 with file:line

It does **not** receive: the reference implementation, hidden test source, property source, or expected outputs. Both attempts are scored and both are recorded; the reported task score is attempt 2's. Attempt-1 score is retained as `first_try_score`, which is a separately interesting metric.
