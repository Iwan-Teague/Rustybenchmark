# ADR-0009 — Paired core and fresh probe seed sets

**Status:** Accepted · 2026-08-17 · Arising from [REVIEW-2.md](../REVIEW-2.md) R2-S1

## Context

Two corrected documents specified incompatible seed derivation, and the conflict was invisible until they were read against each other.

**[07-statistics.md](../07-statistics.md)** requires every model in an epoch to run the **identical seed set**. Paired comparison via McNemar on discordant pairs is where the 2–4× power gain comes from. Without pairing, separating two models 5 points apart needs ~1,530 effective items per arm — beyond any suite we will ship.

**[10-integrity.md](../10-integrity.md)** derives seeds from a **per-batch, per-run challenge nonce**, specifically so the seeds did not exist before the run was requested and precomputed solutions cannot apply.

Both cannot hold:

- Per-run nonce ⇒ every submitter gets different seeds ⇒ **no pairing** ⇒ the statistical design collapses.
- Per-epoch shared nonce ⇒ public the moment the first submitter receives it ⇒ **precomputation returns**.

## Options

1. **Pairing wins; drop T2.** Seeds fixed per epoch, published. Precomputation possible all epoch. Rotation is the only defence.
2. **Freshness wins; drop pairing.** Per-run nonces, no cross-model comparison, ~4× larger suites needed for the same discriminating power. Unaffordable.
3. **Alternate by epoch.** Some epochs paired, some fresh. Halves the data for both purposes.
4. **Split the seed set within every epoch.**

## Decision

**Option 4.** Every epoch issues two disjoint sets of units.

| Set | Share | Derivation | Role |
|---|---|---|---|
| **Paired core** | ~85% | `blake3(epoch_seed \|\| task_id \|\| i)` — fixed per epoch, identical for every submitter | **Scored.** Cross-model comparison, McNemar pairing, all published figures |
| **Fresh probe** | ~15% | `blake3(batch_nonce \|\| task_id \|\| i)` — per batch, per run | **Never scored.** Precomputation detector |

The reported score comes from the paired core. The probe is a **detector**: a submission whose probe score falls materially below its core score did not earn the core score honestly.

```
precompute_signal = core_score − probe_score
```

Both sets draw from the same families with the same seed-space, so under honest execution the two scores should agree within sampling error. The gap is the signal.

## Consequences

**Good**

- **Pairing is exact.** The full 2–4× power gain is preserved, and it is the reason the suites are affordable at all.
- **Precomputation becomes detectable rather than merely bounded.** Strictly stronger than the original T2: a cheater must now solve the probe honestly *and* match their own inflated core score, which is self-defeating.
- The detector needs no trust in the client and no server-side replay — it is a comparison of two numbers the submitter provides.
- Composes cleanly with batching and with resumable segments: core units need no network, probe units need a batch nonce.

**Bad**

- Epoch seeds are effectively public within the epoch. A determined attacker can precompute the core set for the current epoch — and will be caught by the probe only if they do not also solve the probe. Epoch rotation remains necessary; this does not replace it.
- The probe costs ~15% of every run and produces no score.
- **The detector's statistical power is unvalidated.** 15% of units may be too few to distinguish precomputation from ordinary sampling noise at useful confidence. The same effective-N arithmetic that round 1 applied to scoring has not been applied to the detector. Tracked as round 3 item 3; the 15% figure is provisional.

## Related

This supersedes the seed derivation in [10-integrity.md](../10-integrity.md) T2 and the "seeds are fixed per epoch" statement in [07-statistics.md](../07-statistics.md). Both documents now reference this ADR rather than specifying derivation themselves.
