# 06 — Execution classes and memory accounting

## The problem with "GPU only"

A strict GPU-only rule sounds like it simplifies memory measurement. It does not, and it excludes the machines this project exists to serve.

**Partial offload is the normal consumer configuration.** An 8 GB laptop GPU running a 19 GB Q4 model puts some layers on the GPU and the rest on the CPU. That is not "CPU inference" — it is the majority setup in the 8–16 GB band. A GPU-only rule either excludes those machines entirely or forces them onto models small enough for full offload, which quietly biases the whole leaderboard toward tiny models.

**MoE changed the CPU maths.** Qwen3-Coder-30B-A3B is 30B total with ~3.3B active parameters. Sparse-active models are far more viable on CPU than dense models of the same nominal size, so "CPU inference is pointless" is materially less true than it was.

**Support cost is near zero.** The harness speaks OpenAI-compatible HTTP; it does not know or care where the layers live. The only work is *reporting*, and llama.cpp already exposes the offload split.

Decision recorded in [ADR-0005](adr/0005-execution-classes-not-gpu-only.md): **classify, do not ban.**

## Classes

```rust
pub enum ExecClass {
    /// 100% of layers offloaded, KV cache on accelerator.
    GpuFull,
    /// Partial offload. offload_ratio in (0.0, 1.0).
    Hybrid,
    /// Zero layers offloaded.
    CpuOnly,
}
```

Derived from the backend's reported layer split, not self-declared. For llama.cpp: `ngl` vs total layers, plus KV cache location. For vLLM/others: backend-specific probe, falling back to `Unknown` — and `Unknown` is not leaderboard-eligible.

## Memory accounting

Two numbers plus a ratio, replacing the single "VRAM" field. This is cleaner even for pure-GPU runs, because Apple unified memory does not fit a VRAM-shaped field at all.

```rust
pub struct MemProfile {
    pub peak_accel_mem_mb: u64,   // VRAM, or unified-memory attribution on Apple
    pub peak_host_rss_mb:  u64,   // system RAM
    pub offload_ratio:     f32,   // ngl_layers / total_layers, 0.0 ..= 1.0
    pub kv_cache_location: KvLoc, // Accel | Host | Split
    pub mem_bandwidth_class: BwClass, // derived from calibrated tg128
}
```

`mem_bandwidth_class` is the real predictor of generation speed regardless of where the memory physically lives, which is why it is derived and recorded rather than inferred by readers from the GPU model name.

## Leaderboard policy

- **`GpuFull` is the default view.** Cleanest, most comparable, the primary table.
- **`Hybrid` and `CpuOnly` are collected, tagged, and shown behind a toggle.** Never merged into the same ranking.
- **Timing-derived metrics are comparable only within a class.** `throughput_score`, `time_to_first_pass`, `efficiency_score` — all class-scoped.
- **`capability_score_core5` is comparable across all classes.** Correctness does not care where the layers ran — but `perf-optimization` is gated to `GpuFull`, so the full eleven-category `capability_score` is *not* cross-class comparable ([REVIEW-5.md](REVIEW-5.md) `capability-score-denominator`).

That last point is worth stating on the leaderboard itself: a `CpuOnly` row with the same `capability_score` as a `GpuFull` row is making the same claim about the model and a very different claim about the machine.

## Implementation order

Build `GpuFull` end to end first. `Hybrid` and `CpuOnly` fall out for free the moment `offload_ratio` is being recorded — which should happen from day one whether or not those submissions are accepted. One field now, no schema migration later.

## Where CPU genuinely hurts

`perf-optimization` (category 7). Criterion ratios against a reference implementation are noisy on a thermally unstable, contended CPU. That category is gated:

- execution class `GpuFull` only
- AC power required
- stability precheck: repeated timing of the reference must vary <5% across repetitions

Failing the precheck skips the category and marks the run `perf_unavailable`. It does not score the model badly for the user's thermal situation.

## Backend metadata to capture

Recorded for every run; displayed on the leaderboard because these are the knobs that make results incomparable if hidden.

```
backend           llama.cpp | ollama | vllm | lmstudio | other
backend_version   e.g. b7412
model_name        canonical
quant             Q4_K_M | Q5_K_M | Q8_0 | fp16 | awq | gptq | ...
ctx               context window actually configured
ngl               layers offloaded
batch / ubatch
flash_attn        on | off
kv_cache_type     f16 | q8_0 | q4_0
threads
rope / yarn settings if non-default
```

Quantisation especially: comparing a Q4_K_M row to a Q8_0 row of the "same model" as though they were the same thing is one of the most common errors in local-model discourse, and the leaderboard should make it impossible to do by accident.
