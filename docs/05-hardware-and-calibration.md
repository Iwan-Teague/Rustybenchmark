# 05 — Hardware profiling and calibration

Two phases run before any task. Both are mandatory; a run without them is not scorable.

## Phase A — Static inventory

```rust
pub struct HwProfile {
    pub schema:  u32,
    pub cpu:     CpuInfo,      // model, physical/logical cores, base+boost MHz, arch,
                               // features (avx2/avx512/neon/sve)
    pub ram:     RamInfo,      // total, available, type + channels where obtainable
    pub gpus:    Vec<GpuInfo>, // vendor, model, mem_total, mem_free, driver version,
                               // compute units, cuda_cc / rocm_arch / metal_family
    pub os:      OsInfo,       // kernel, version, build
    pub storage: DiskInfo,     // nvme | sata | spinning | unknown, free space on scratch
    pub power:   PowerHint,    // ac | battery, chassis: laptop | desktop | server | unknown
    pub unified_memory: bool,  // Apple Silicon and similar
}
```

### Crates

| Need | Crate | Notes |
|---|---|---|
| CPU, RAM, OS, disk | `sysinfo` | now also exposes GPU info on Linux, macOS, and Windows |
| NVIDIA detail | `nvml-wrapper` | safe wrapper over NVML; `device.memory_info()` for used/total |
| Cross-vendor VRAM | `gpu-probe` | no vendor SDKs required beyond the driver; uses nvml-wrapper for NVIDIA |
| Universal fallback | `wgpu` adapter enumeration | covers Apple Metal, Intel Arc, AMD where nothing else does |

Strategy: try vendor-specific first (most detail), fall back to `gpu-probe`, fall back to `wgpu` adapter info, fall back to `unknown` with a warning. Record which path produced each field — a leaderboard row derived from `wgpu` fallback deserves a different confidence marker than one from NVML.

## Phase B — Inference calibration

Adopt the llama-bench protocol so numbers are comparable to the wider ecosystem.

```
warm-up:     60 s sustained load, results discarded
measure:     pp512   prompt processing, 512 tokens   -> compute-bound throughput
             tg128   token generation, 128 tokens    -> memory-bandwidth-bound latency
             repetitions = 10, report median + p5 + p95
also record: peak accelerator memory, peak host RSS,
             clock at start vs end, wall-clock drift across repetitions
```

`pp512` measures a single forward pass over a 512-token prompt, reported as `(B × 512) / median_time` — big GEMMs and SDPA, compute-bound. `tg128` measures generating 128 tokens one at a time from a KV cache warmed by a short prefill, reported as `(B × 128) / median_time` of the decode loop only — frequent small matmuls and KV traffic, memory-bound. They fail differently and must be reported separately.

### The sustained phase — our addition

Standard tooling stops at the short burst. That hides the fact that matters most for a two-day benchmark run.

```
sustained:   10 minutes of bench-shaped load
             (2k-token prompts, 1k-token generations, back to back)
record:      tok/s at minute 1 vs minute 10
             clock throttle delta
             any thermal event flags the OS exposes
```

Thermal throttling alone swings llama.cpp results **12–18%** between cold and stabilised runs. Laptops fall off a cliff over ten minutes; desktops mostly do not. That delta is a genuine hardware fact almost nobody publishes, and it is one of the more useful things this project can contribute.

**Warm-up is not optional.** A calibration without it is not comparable to anything.

### Calibration is per-segment, not per-run

A run that spans multiple sessions (see [09-resume-and-checkpointing.md](09-resume-and-checkpointing.md)) recalibrates at the start of every segment. Thermal and power state do not persist across a lid close. Timing metrics aggregate across segments with segment tags, and the run reports inter-segment calibration variance.

If calibrated `tg128` varies more than **±10%** across segments, the run's **throughput metrics are flagged unstable**. Correctness metrics are unaffected — they do not depend on speed.

## Derived metrics

```
capability_score     = weighted mean task score across categories   (model property)
throughput_score     = tasks_passed / wall_clock_hour               (machine property)
efficiency_score     = tasks_passed / kWh                           (where telemetry exists)
vram_efficiency      = capability_score / peak_accel_gb
time_to_first_pass   = median seconds from prompt to a passing solution
build_overhead_ratio = build_ms / (build_ms + gen_ms)
```

`capability_score` is comparable across all machines and execution classes. `throughput_score`, `time_to_first_pass`, and `efficiency_score` are **only comparable within an execution class** — see [06-execution-classes.md](06-execution-classes.md).

`build_overhead_ratio` is a harness-health metric: if it climbs above ~0.3 the timing numbers are measuring the user's disk more than their accelerator, and the cargo caching setup needs attention.

## Hardware classes

Auto-assigned so leaderboard rows are comparable without the user choosing anything.

```
cpu-only
igpu
vram-8      vram-12     vram-16     vram-24     vram-32     vram-48     vram-80+
unified-16  unified-32  unified-64  unified-128            (Apple Silicon and similar)
multi-gpu-<n>x<class>
```

Buckets, not exact models, both for comparability and because an exact GPU model plus driver plus core count plus serial is an identifying fingerprint. See [11-submission-and-privacy.md](11-submission-and-privacy.md).

## Power telemetry

Best-effort, never required.

| Platform | Source |
|---|---|
| NVIDIA | NVML power draw sampling |
| Apple Silicon | `powermetrics` (requires elevated privileges — ask, do not demand) |
| Linux | RAPL where readable |
| Windows / AMD / Intel Arc | frequently unavailable |

If unavailable, `efficiency_score` is `null`. Never estimate it from TDP — a made-up number on a leaderboard is worse than a missing one.

## Pre-run gates

The harness refuses to start, or warns loudly, when:

| Condition | Action |
|---|---|
| Calibrated `tg128` below the suite's floor (suite would exceed 72 h) | refuse, suggest a smaller suite |
| Free scratch space below the suite's estimate ×1.5 | refuse |
| On battery power | warn; refuse for `perf-optimization` |
| Cold/warm calibration delta >15% | warn; mark throughput metrics unstable |
| Model context window smaller than the largest family's prompt | refuse that family, record as `skipped_context` |
| Network reachable from inside the sandbox | refuse — the sandbox is broken |
