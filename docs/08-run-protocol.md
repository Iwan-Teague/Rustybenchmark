# 08 — Run protocol and sandbox

## Lifecycle

```
rustybench run --suite deep --model http://localhost:8080/v1 --epoch 2026-08
```

1. **Preflight** — verify toolchain, scratch space, backend reachability, sandbox integrity. See gates in [05-hardware-and-calibration.md](05-hardware-and-calibration.md).
2. **Profile** — static hardware inventory → `hw.json`.
3. **Challenge** — request `{challenge_nonce, epoch, expires_at}` from the server (or `local` for offline T0 runs).
4. **Plan** — expand the suite into an ordered, frozen list of work units → `plan.json`, hashed.
5. **Calibrate** — warm-up, `pp512`, `tg128`, sustained phase → `segments/000/calib.json`.
6. **Execute** — for each work unit: materialise → prompt → model → attempt 1 → grade → (repair) → attempt 2 → grade → append to journal.
7. **Aggregate** — fold the journal into scores with clustered-bootstrap CIs.
8. **Submit** — optional, consented, redacted. See [11-submission-and-privacy.md](11-submission-and-privacy.md).

Steps 5–6 repeat per segment when a run spans multiple sessions — see [09-resume-and-checkpointing.md](09-resume-and-checkpointing.md).

## Work unit

The atom of execution and of checkpointing.

```rust
pub struct WorkUnit {
    pub unit_id:  UnitId,     // blake3(run_id || task_id || seed || attempt_policy)
    pub task_id:  TaskId,
    pub seed:     u64,
    pub index:    u32,        // position in the plan; fixed
}
```

Work units are **independent and idempotent**. Re-running one produces the same instance (generation is deterministic from the seed) and the same grade (the oracle is deterministic). This property is what makes resume, replay verification, and crash recovery all trivially correct.

## Per-unit sequence

```
1. materialise model workspace       (skeleton + Cargo.toml + vendored deps symlink)
2. assert no oracle content present  (content-hash check)
3. enter sandbox
4. render prompt, send to backend, capture response + token counts + timings
5. exit sandbox
6. materialise grading workspace     (fresh dir; skeleton + model response + oracle/)
7. run L0..L4                        (each in the sandbox, each with its own timeout)
8. write artifacts, append journal line, fsync
9. wipe workspaces
```

Steps 1–2 and 6 being separate directories is the core oracle-isolation control. See [03-oracle.md](03-oracle.md).

## Sandbox

A full VM is the wrong tool for consumer hardware — heavy, slow to start, and hostile to the "just run this on your laptop" goal. Wasm is also wrong: wasmtime's isolation is excellent, but anything touching `std::fs`, threads, or tokio breaks, which removes half the categories.

**Chosen: OS-level containment around the cargo invocation.**

| Control | Linux | macOS | Windows |
|---|---|---|---|
| Network deny | network namespace, no veth | `sandbox-exec` seatbelt profile | **AppContainer** without network capabilities |
| Filesystem scope | mount namespace, bind-mount workspace only | seatbelt path allowlist | AppContainer SID + explicit workspace ACLs |
| Memory cap | `RLIMIT_AS` + cgroup v2 | `RLIMIT_AS` | job object memory limit |
| CPU time cap | `RLIMIT_CPU` | `RLIMIT_CPU` | job object |
| Wall-clock cap | harness supervisor | harness supervisor | harness supervisor |
| Process cap | `RLIMIT_NPROC` / cgroup pids | `RLIMIT_NPROC` | job object active-process limit |

Firecracker remains an option for a future hosted verification tier, where a ~50k-line Rust VMM with a deliberately small device model is the right trade. Not for the local runner.

### Network policy

**Zero network access during generation and grading**, enforced by the OS rather than by policy. `cargo` runs `--offline` with pre-vendored dependencies. Any attempted connection is logged and fails the unit with `reason = "network_attempt"`.

This directly addresses one of the three documented Terminal-Bench misconduct cases (an agent fetching solutions from the internet).

### Timeouts are harness-owned

Every timeout — per-attempt generation, per-stage build, per-stage test, per-unit wall clock — comes from the suite definition, not from anything the submitter can set. Any submitter-settable knob that does exist goes into the signed manifest and is displayed on the leaderboard.

Modifying timeouts was another of the three documented Terminal-Bench cases.

## Cargo cost control

Cold builds dominate wall-clock and would make the benchmark measure the user's disk rather than the model.

- **Shared `CARGO_HOME`** across the whole run, persisted between segments.
- **Pre-vendored `.crate` files** shipped with the suite; `--offline --locked` always.
- **Prebuilt dependency workspace** per suite: dependencies compiled once during preflight, `target/` reused by every unit via a shared target directory.
- **`sccache`** where available.
- Record `build_ms` separately from `gen_ms` in every journal line, and publish `build_overhead_ratio`. If it exceeds ~0.3 the caching setup is broken and timing metrics are suspect.
- **Exclude the first N units of each segment from timing aggregates** (cache warmth); record `segment_position` on every unit so this is auditable rather than magic.

## Determinism controls

Grading is fully reproducible: seed → instance → proptest seed → verdict. Generation is not — greedy decoding in llama.cpp still varies with batch size and backend build.

Therefore:

- Record complete sampling configuration in every journal line: `{temp, top_p, top_k, seed, backend, backend_version, quant, ngl, ctx, batch, flash_attn, kv_cache_type}`.
- Default primary run: `temp = 0.0`, single sample.
- `--pass-k 5 --temp 0.8` for the variance probe on a 10% subsample.
- Flag and publish any case where identical `(model, seed, sampling)` produced different output.

## Interaction modes

| Mode | Turns | Notes |
|---|---|---|
| `single-shot` | 1 | Cheapest; cleanest measurement of raw capability |
| `repair` | 2 | Attempt 2 sees diagnostics only, never oracle source. **Default.** |
| `agentic` | n | Model drives tools (read/write/run). Separate reported track — the scaffolding confounds the model measurement, and cost multiplies |

`repair` is default because it mirrors real use and because Aider's benchmark established the protocol (two attempts, test output fed back) — comparability is worth something.

Both attempts are scored. `first_try_score` is retained alongside the final score; the gap between them is a genuinely interesting model property.

## CLI surface

```
rustybench profile                       # hardware inventory only
rustybench calibrate                     # calibration only
rustybench run --suite <s> --model <url> [--epoch <e>] [--quality] [--max-duration 4h]
rustybench status [<run_id>]             # progress, ETA, segment history
rustybench resume <run_id>               # continue a paused or crashed run
rustybench abandon <run_id>              # mark dead, keep journal
rustybench report <run_id> [--format md|json|html]
rustybench submit <run_id>               # explicit; never automatic on first use
rustybench validate-family <id> --seeds 1000
rustybench calibrate-suite               # ICC / difficulty analysis over a result corpus
rustybench verify <submission.json>      # local replay of someone else's submission
```
