# 10 — Integrity

## The honest starting point

**Client-side attestation cannot be made sound.** The attacker owns the machine. Hash the binary, embed a signing key, sign the payload — they patch the binary, or extract the key with a debugger and sign whatever they like. Every "only submittable through the program" scheme is obfuscation, not security.

Build it anyway as a speed bump against casual tampering. Design assuming it will be broken.

This is not theoretical. Terminal-Bench found real leaderboard misconduct: one submission **stored encrypted solutions inside the agent binary** and modified timeouts; another **shipped the task test folder** in the agent setup; a third had the agent **fetch solutions from the internet**. All three passed submission until humans looked, and all three were surfaced by independent community analysis of published data.

## The asymmetry we have and most benchmarks do not

Split claims by verifiability:

| Claim | Verifiable? | Mechanism |
|---|---|---|
| "The model produced this code" | **No** | Unfalsifiable in principle |
| **"This code scores 0.83 on task X seed Y"** | **Yes, exactly** | Oracle is deterministic → server re-grades |
| **"These are the tasks I was given"** | **Yes** | Server issues the seeds |
| "This ran on an RTX 4090 at 41 tok/s" | **No** | Plausibility-checkable only |

That split is the whole design. **Correctness is fully verifiable server-side**, because the oracle is deterministic and instances are server-seeded. **Hardware throughput is never verifiable.** Publish them as two different things, with two different confidence labels. Never blend them into one trust-me number.

## Trust tiers

Every leaderboard row is labelled with its tier.

### T0 — Self-reported

Client computes scores, signs with an embedded key, uploads. Verifies nothing. Grey badge. Useful for volume ("I tried this on my laptop"); never headline. This is what offline/local runs get.

### T1 — Replayed

Client uploads the model's raw output artifacts alongside the scores. The server re-materialises each instance from its seed, runs the identical oracle in its own sandbox, and compares. Any mismatch → rejected and flagged.

**This eliminates score fabrication entirely.** It costs the server CPU and nothing else. It is the single highest-leverage control in the system.

### T2 — Challenged

Client requests a run; the server returns `{challenge_nonce, epoch, expires_at}`. Seeds derive as:

```
seed(task_id, i) = blake3(challenge_nonce || epoch || task_id || i)[..8]
```

Since the nonce did not exist before the request, precomputed or binary-embedded solutions cannot apply. Combined with T1 replay this is strong: it proves both that *the answers are right* and that *they were not prepared in advance*.

**T2 + T1 is the default requirement for a ranked leaderboard row.**

#### Batched issuance (required for long runs)

A `full` suite takes ~49 hours; it cannot sit inside a short anti-precomputation window. So the nonce is issued per batch, not per run:

```
client: POST /challenge/batch { run_id, batch_index }
server: { batch_nonce, unit_range, expires_at }   # window measured in hours
client: executes the batch, submits results
client: may not request batch k+1 before submitting batch k
```

Precomputation exposure is bounded to one batch. Batches compose freely with execution segments — see [09-resume-and-checkpointing.md](09-resume-and-checkpointing.md).

Trade-off: T2 requires network access *between* batches. The sandbox still denies network *during* every unit. Fully offline runs are possible but are capped at T0/T1.

### T3 — Audited

MLPerf's model. Per epoch, audit two submissions — one selected at random, one selected by the review committee. Examine logs, configuration, and any custom code; reproduce on reference hardware; results valid if within **5%** of submitted numbers; **90-day** audit window; a failed audit retracts published material.

Gold badge. Worth building only once there are reputations on the line. It is a policy document plus a small amount of tooling, not a large engineering effort.

## Plausibility checks — the hardware half

Hardware claims cannot be verified, but they are strongly coupled, which makes lying detectable.

| Check | Detects |
|---|---|
| **Throughput consistency** — per-unit `completion_tokens / gen_ms` must match calibrated `tg128` within a band | Fabricated timings, or a calibration run on different hardware |
| **Memory physics** — `peak_accel_mem_mb` must fit the claimed device and be consistent with `model_size × quant + kv_cache(ctx)` | Misreported model, quant, or device |
| **Thermal signature** — laptops throttle 12–18% cold→stabilised | A "laptop" with perfectly flat 10-minute sustained throughput |
| **Error-code fingerprint** — each model family has a characteristic rustc-diagnostic distribution | Output not actually produced by the declared model |
| **Segment coherence** — calibration across segments, `build_overhead_ratio` drift | A run stitched together from different machines |
| **Canary leakage** — screen submitted output for canaries belonging to *other* instances | An agent that scraped or was fed a solution corpus |

None is proof. All are cheap. Together they make a consistent fake require sustained effort, and every one of them produces a flag that a human reviewer can act on.

## Anti-gaming controls in the harness

Each maps to a documented real-world failure mode.

| Control | Enforcement | Addresses |
|---|---|---|
| **No network during generation or grading** | OS-level (netns / seatbelt / job object), not policy. Attempted connection = hard fail | Terminal-Bench case 3 (agent fetched solutions) |
| **Oracle never in the model's filesystem view** | Separate grading workspace; oracle injected after the turn; pre-turn content-hash assertion | Terminal-Bench case 2 (test folder shipped in agent setup) |
| **Timeouts and limits are harness-owned** | Read from the suite definition; every submitter-settable knob is in the signed manifest and displayed | Terminal-Bench case 1 (modified timeouts) |
| **Full trajectory required for passing units** | Prompt, response, and tool calls stored for every passing unit; T1 rejects passing units without them | Makes audit possible at all |
| **Canary per instance** | Minted at generation, embedded in the prompt, screened at submission and against public corpora | Leakage detection, contamination monitoring |
| **Frozen tasks excluded from scored suites** | Harness refuses `kind = "frozen"` in `standard` and above | Prevents a static, memorisable core |
| **Published raw corpus** | Everything except redacted personal data | Independent re-derivation — the strongest control available |

That last row deserves emphasis: all three Terminal-Bench cases were caught by community analysis of published results, not by the platform's own checks. Publishing the corpus is not a nice-to-have.

## Reconciling replay with leakage

T1 replay requires the model's output. Publishing that output could leak solved instances back into training corpora — poisoning our own benchmark.

Resolution:

- Output artifacts are uploaded **encrypted to a server key**, retained privately, **never published**, never included in the public dump.
- Retention window matches the audit period (90 days), then deleted.
- The public dump contains scores, error codes, timings, and hardware class — no source.
- The consent screen states this **separately** from the statistics consent, because it is a materially different thing to agree to.

See [11-submission-and-privacy.md](11-submission-and-privacy.md).

## What to build, and when

**Before first ship** (expensive to retrofit):

- Canonical manifest format and serialization (`bench-attest/manifest.rs`)
- Seed-from-nonce derivation
- Challenge/batch protocol shape, even if the server initially always returns `local`
- Canary minting and embedding
- Trajectory capture for passing units

**Shortly after** (server-side, additive):

- T1 replay verification
- Plausibility check suite
- Canary cross-screening

**Later** (policy, not code):

- T3 audit process and remedies document

## Components

```
bench-attest/
  challenge.rs   # request nonce, derive seeds, enforce batch ordering and expiry
  manifest.rs    # canonical payload: run_id, challenge, suite_hash, generator_commit,
                 # harness_version, binary_hash, submitter-settable config, hw, calib, results
  sign.rs        # ed25519 over canonical CBOR; embedded key = T0 only, no illusions
  redact.rs      # bucket hardware, strip paths / hostnames / usernames / serials
  canary.rs      # mint, embed, screen
```

Server side:

```
verify/replay.rs      # re-materialise seed, re-run oracle, diff against submitted scores
verify/plausible.rs   # throughput / memory / thermal / histogram checks
verify/canary.rs      # cross-instance and cross-corpus canary screening
verify/audit.rs       # T3 workflow support
```
