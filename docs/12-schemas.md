# 12 — Schemas

Every on-disk and on-wire schema, in one place. All carry an integer `schema` field; readers reject unknown majors rather than guessing.

---

## `plan.json` — frozen work list

```json
{
  "schema": 1,
  "run_id": "01K3F8XQ2M7VZ0J4T6N9P1R5CD",
  "plan_hash": "blake3:9f3c1a...",
  "suite": "deep",
  "suite_hash": "blake3:4b21ef...",
  "generator_commit": "a17c93d",
  "harness_version": "0.4.1",
  "epoch": "2026-08",
  "challenge": { "mode": "batched", "batch_size": 50 },
  "epoch_seed": "...",
  "core": [
    { "index": 0, "unit_id": "blake3:...", "task_id": "borrowck/split-mut-window", "seed": 8412739123 }
  ],
  "probe": [
    { "index": 0, "task_id": "borrowck/split-mut-window", "batch": 0, "seed": null }
  ]
}
```

## `state.json` — mutable run state

```json
{
  "schema": 1,
  "run_id": "01K3F8XQ2M7VZ0J4T6N9P1R5CD",
  "status": "paused",
  "tainted": false,
  "taint_reasons": [],
  "created_at": "2026-08-17T19:02:11Z",
  "updated_at": "2026-08-19T22:02:55Z",
  "current_segment": 3,
  "units_completed": 743,
  "units_planned": 1200,
  "identity": {
    "plan_hash": "blake3:9f3c1a...",
    "suite_hash": "blake3:4b21ef...",
    "generator_commit": "a17c93d",
    "harness_version": "0.4.1",
    "model": { "name": "qwen3-coder-30b-a3b", "quant": "Q4_K_M",
               "backend": "llama.cpp", "backend_version": "b7412",
               "ctx": 32768, "ngl": 999 },
    "sampling": { "temp": 0.0, "top_p": 1.0, "top_k": 0, "seed": 42 },
    "exec_class": "GpuFull",
    "hw_fingerprint": "blake3:..."
  },
  "batches": [ { "index": 0, "nonce": "...", "expires_at": "...", "submitted": true } ]
}
```

`identity` is the block checked on resume. Any mismatch is fatal unless `--force-heterogeneous`, which sets `tainted` and appends to `taint_reasons` irreversibly.

## `journal.jsonl` — one line per completed unit

```json
{
  "schema": 1,
  "run_id": "01K3F8XQ2M7VZ0J4T6N9P1R5CD",
  "unit_id": "blake3:...",
  "index": 417,
  "segment": 3,
  "segment_position": 22,
  "completed_at": "2026-08-19T21:14:03Z",

  "task_id": "borrowck/split-mut-window",
  "category": "borrow-lifetimes",
  "subcategory": "aliasing",
  "seed": 8412739123,
  "set": "core",                  // core | probe -- core is scored, probe is detector-only
  "batch_nonce": null,            // probe units only; lets the server verify seed derivation
  "canary": "rb-9f3c1a7e",
  "attempt": 2,

  "oracle": {
    "apply_ok": true,
    "compile_ok": false,
    "error_codes": ["E0499"],
    "warn_count": 2,
    "diagnostic_completeness": "typeck_only",   // full | typeck_only -- was borrowck reached?
    "compile_rate_contrib": 0,
    "behavior": { "unit": null, "property": null, "differential": null, "score": null },
    "constraint": { "clippy": null, "fmt": null, "unsafe_blocks": 0,
                    "forbidden": [], "score": null },
    "quality": { "mutation": null, "perf_ratio": null, "size_ratio": null, "score": null },
    "score": 0.0,
    "first_try_score": 0.0
  },

  "cost": {
    "prompt_tokens": 1842,
    "completion_tokens": 611,
    "prefill_ms": 9120,
    "gen_ms": 30410,
    "build_ms": 4310,
    "grade_ms": 1880,
    "peak_accel_mem_mb": 18930,
    "peak_host_rss_mb": 3210
  },

  "failure_class": "borrowck",
  "classified": true,               // false when classify() fell through to `other`
  "flags": []
}
```

`flags` may contain: `timeout`, `network_attempt`, `context_overflow`, `budget_overflow`, `format_error`, `nondeterministic_repeat`, `skipped_context`, `perf_unavailable`.

## `hw.json` — static inventory

```json
{
  "schema": 1,
  "cpu": { "model": "...", "vendor": "amd", "physical_cores": 16, "logical_cores": 32,
           "base_mhz": 4500, "boost_mhz": 5700, "arch": "x86_64",
           "features": ["avx2","avx512f"], "source": "sysinfo" },
  "ram": { "total_mb": 65536, "available_mb": 51200, "type": "ddr5", "channels": 2,
           "source": "sysinfo" },
  "gpus": [ { "vendor": "nvidia", "model": "...", "mem_total_mb": 24564, "mem_free_mb": 23980,
              "driver": "560.35.03", "compute_units": 128, "cuda_cc": "8.9",
              "source": "nvml" } ],
  "os": { "family": "linux", "kernel": "6.11.0", "version": "..." },
  "storage": { "scratch_fs": "ext4", "media": "nvme", "free_mb": 412000 },
  "power": { "source": "ac", "chassis": "desktop" },
  "unified_memory": false
}
```

Every field carries `source`, so a value from `wgpu` fallback is distinguishable from one from NVML.

## `segments/<n>/calib.json`

```json
{
  "schema": 1,
  "segment": 3,
  "warmup_s": 60,
  "pp512": { "median": 812.4, "p5": 795.1, "p95": 828.9, "reps": 10 },
  "tg128": { "median": 41.2,  "p5": 40.1,  "p95": 42.0,  "reps": 10 },
  "sustained": { "minute_1_tps": 41.2, "minute_10_tps": 40.6, "delta_pct": -1.5,
                 "clock_start_mhz": 2520, "clock_end_mhz": 2505, "throttle_events": 0 },
  "peak_accel_mem_mb": 18930,
  "peak_host_rss_mb": 3210,
  "exec_class": "GpuFull",
  "offload_ratio": 1.0,
  "kv_cache_location": "Accel"
}
```

## `report.json` — aggregated result

```json
{
  "schema": 1,
  "run_id": "...",
  "completeness": 1.0,
  "tier": { "suite": "deep", "trust": "T2" },
  "flags": ["throughput_stable"],
  "categories_scored": ["borrow-lifetimes","traits-generics","error-handling","idiom-refactor",
                        "unsafe-core","async-concurrency","api-evolution","test-authoring",
                        "cross-module","ffi-boundary"],
  "capability_score":       { "value": 0.412, "ci95": [0.363, 0.461], "effective_n_equal_weight": 408,
                              "effective_n_pooled": 573 },
  "capability_score_synth": { "value": 0.437, "ci95": [0.386, 0.488], "effective_n_equal_weight": 355 },
  "capability_score_core5": { "value": 0.401, "ci95": [0.352, 0.450], "effective_n_equal_weight": 421 },
  "categories": {
    "borrow-lifetimes": { "value": 0.31, "ci95": [0.20, 0.42], "n": 160, "effective_n": 84,
                          "shapes": 11, "icc_within_family": 0.31, "icc_within_shape": null }
  },
  "throughput_score": { "value": 21.4, "unit": "units_passed_per_hour",
                        "exec_class": "GpuFull", "stability": "stable" },
  "vram_efficiency": 0.0224,
  "time_to_first_pass_s": 47.3,
  "efficiency_score": null,
  "error_histogram": { "E0499": 41, "E0308": 96, "E0277": 133 },
  "compile_rate": 0.61,
  "classified_rate": { "borrow-lifetimes": 0.88, "idiom-refactor": 0.94 },
  "harness_overhead_ratio": 0.50,
  "l4_share_of_grade": 0.62,
  "icc_measured": { "borrow-lifetimes": 0.31, "unsafe-core": 0.52 },
  "failure_classes": { "borrowck": 88, "trait": 133, "type": 96, "logic": 210, "constraint": 41 },
  "segments": 4,
  "_note": "icc_estimates was a duplicate of icc_measured and is removed -- R4-S5 requires the\n            measured per-category value, so there is only one field for it."
}
```

## Submission manifest (on-wire)

Canonical CBOR, ed25519-signed. JSON shown for readability.

```json
{
  "schema": 1,
  "run_id": "...",
  "machine_uuid": "random, locally generated, not hardware-derived",
  "epoch": "2026-08",
  "batches": [ { "index": 0, "nonce": "...", "submitted_at": "..." } ],
  "identity": { "suite_hash": "...", "generator_commit": "...",
                "harness_version": "...", "binary_hash": "..." },
  "submitter_config": { "wall_timeout_s": 900, "budget_tokens": 16000,
                        "max_attempts": 2, "quality_enabled": true },
  "model": { "...": "as in state.json" },
  "hw_public": { "hw_class": "vram-24", "vendor": "nvidia", "gen": "ada",
                 "cpu_class": "desktop-16c", "mem_bandwidth_class": "high",
                 "chassis": "desktop", "power_source": "ac", "unified_memory": false },
  "calibration": [ "per-segment calib summaries" ],
  "units": [ "journal lines, redacted" ],
  "report": { "...": "report.json" },
  "consents": { "public_scores": true, "replay_artifacts": true, "audit_contact": false },
  "signature": "ed25519:..."
}
```

`submitter_config` is signed and **displayed on the leaderboard**. Every knob a submitter can turn must be visible, because a hidden knob is an attack surface (Terminal-Bench case 1 was a modified timeout).

## Task manifest

See [02-task-format.md](02-task-format.md) for the annotated `task.toml`.

## Reserved for v2

Fields that must exist in the schema now even though nothing writes them yet, to avoid a migration:

```json
"irt": { "difficulty": null, "discrimination": null, "guessing": null }
```

## Family-validation fields (`task.toml`)

Referenced across [02-task-format.md](02-task-format.md) and [03-oracle.md](03-oracle.md); recorded
here so the schema stops drifting behind the prose.

```toml
min_instance_distance   = 0.25    # prompt+skeleton, parametric families
min_reference_distance  = 0.20    # reference impl, secondary
min_transform_jaccard   = 0.50    # transform set, compositional families
forbidden_calls         = []      # specific-API constraints only, never an allocation proxy
[constraint.allocation] max_allocs = "reference"
[quality.perf]          max_ratio  = 2.0
```

## Segment fields

```json
{ "exit_reason": "max_duration" }   // max_duration | until_time | sigint | battery
                                    // | thermal | crash | complete | aborted
```

Per-family IRT parameters, populated by `rustybench calibrate-suite` once enough submissions exist, and used for adaptive item allocation. See [07-statistics.md](07-statistics.md).
