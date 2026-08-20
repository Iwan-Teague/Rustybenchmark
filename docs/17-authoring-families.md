# 17 — Authoring a family

This is the practical guide to adding a generator family. It exists because family authoring is the
project's long pole — 272 families at 1–3 days each ([14-roadmap.md](14-roadmap.md)) — so the pattern
that three families now share is written down once, here, rather than re-derived each time. Gate **G3**
is "a new contributor authors a validated family in ≤ 2 days"; this guide is what makes that plausible.

Three reference families, in rising order of how much of this guide they exercise:

- [`window_op.rs`](../crates/bench-gen/src/window_op.rs) — `borrow-lifetimes`, in-place slice mutation,
  with an allocation (L3) constraint.
- [`error_handling.rs`](../crates/bench-gen/src/error_handling.rs) — `error-handling`, parse → validate →
  fold returning `Result`, with a forbidden-method (L3) constraint.
- [`stack_machine.rs`](../crates/bench-gen/src/stack_machine.rs) — `pattern-matching`, exhaustive `match`
  over a provided enum.

Copy the closest one and adapt it. The rest of this document is *why* each part is shaped the way it is.

## The contract

A family implements `bench_gen::Generator`. Everything is a pure function of the `seed` — same seed →
byte-identical instance — because resume and replay depend on it ([09-resume-and-checkpointing.md](09-resume-and-checkpointing.md)).
Never touch the OS RNG; use `bench_gen::Rng` (SplitMix64) seeded from the task seed.

```
fn id(&self) -> &str                       // family id, e.g. "stack-machine"
fn category(&self) -> &str                 // one of the 11 categories (04-categories.md)
fn generate(&self, seed) -> GeneratedTask  // the model's view + hidden oracle + grading config
fn reference_code(&self, seed) -> String   // the correct solution; must score 1.000
fn skeleton_code(&self, seed) -> String    // the ablated body (todo!()); must fail
fn trivial_baselines(&self, seed) -> Vec<(label, code)>  // right shape, wrong content; each must fail
fn spec_signature(&self, seed) -> Vec<String>            // the structural identity (Q31)
```

## The recipe (solution-first)

The order matters — it is what makes the oracle correct by construction (ADR-0003). Do **not** write a
prompt and then a checker; derive both from a `Spec`.

1. **`Spec` + `sample(seed)`.** Draw the *structural* choices from the seed: which operation, which rule,
   which stride — the things that define the skill. This is the family's variable surface.
2. **`eval` — the native reference.** Write the answer as ordinary Rust that computes the correct result
   for a `Spec` and an input. This is the source of truth.
3. **`reference_src` — the emitted reference.** Render `eval`'s logic as a source string. It must be a
   *mirror* of `eval`: the same arithmetic, the same edge-case handling. This is the pair you must keep
   in lockstep — the differential gate below is what catches you when you don't.
4. **`skeleton_src`.** The reference with the body replaced by `todo!()`, plus the worked examples as a
   doc comment. Keep any provided types (enums, signatures) that the model must not re-invent.
5. **`prompt`.** Describe the task from the `Spec`. State edge cases explicitly (empty input, underflow).
   End with the canary (`mint_canary(family, seed)`), which must appear verbatim in the prompt.
6. **Oracle tests** (hidden): a `behavior` test from the worked examples, and a `differential` test that
   fuzzes thousands of random inputs comparing the model against an inlined copy of the reference. Add an
   `alloc` test only if the category's signal is allocation (see `window-op`).

## Worked examples: seed-vary them, and include one guaranteed-non-trivial case

Examples appear in both the prompt and the skeleton doc, so they are the family's largest per-instance
text. Seed-vary them (fresh numbers each instance) — it is good for contamination-resistance and it lifts
view-distance. **But** always include one *canonical* case constructed to be non-trivial under every
`Spec`, because the `identity`/`echo` baseline only fails if at least one example is actually changed by
the operation. `window-op` uses a strictly-increasing input with a width above the max rotate amount;
`stack-machine` uses two pushes then a `Combine`+`Map`. Compute every example with `eval`, never by hand.

## The two numbers that are *not* the same thing (Q31)

Do not confuse these, or you will ship a family that looks diverse and isn't:

- **View-distance / contamination.** Whether two instances' *prompts* differ. Seed-varied examples make
  this easy — too easy: it saturates and is **gameable**. It guards only "has the model seen this exact
  prompt", nothing more.
- **`spec_signature` / diversity.** The count of distinct signatures (`bench_gen::spec_diversity`) is the
  **authoritative, ungameable** task-diversity number, and the one a family is authored against. Include
  the structural choices; **exclude** numeric constants and the function name (`AtMost(10)` and
  `AtMost(50)` are the *same skill*). Your family's spec-diversity must comfortably exceed the per-epoch
  seed count. Reference-distance is reported by `validate-family` as a diagnostic only — it is deflated by
  boilerplate and is *not* a diversity measure (`stack-machine` has the most skills yet the lowest
  reference-capacity).

## Validate before you commit

```bash
rustybench validate-family --family <id> --seeds 8
```

Every seed must print `OK`, and the report must show a spec-diversity above your intended per-epoch
count. The gates, and why each exists:

| Gate | Catches |
|---|---|
| `reference = 1.000` | The reference passes its own oracle — the load-bearing self-consistency check |
| `skeleton_behavior < 0.5` | Ablation actually removed the answer |
| `baselines_caught` | Each degenerate answer (const / echo / unwrap) fails — the oracle is not too weak |
| `determinism` | Same seed → identical instance (resume/replay depend on it) |
| `canary` | The prompt carries its leak-detection canary, and no oracle content leaks in |
| differential (inside grading) | The emitted reference matches the native `eval` on 3000 random inputs — this is what keeps step 2 and step 3 honest |

If the reference does not score 1.000, the emitted source and native `eval` have drifted, or a test
target does not build against the reference — fix the mirror, do not weaken the test.

## Pitfalls the three families already hit

- **Emitted-source landmines.** `i64::MIN` has no negative-literal form; `window-op` omits `Min`/`Max`
  for this reason. Prefer identities and constants that render as plain literals.
- **Overflow in the differential.** Debug builds panic on overflow, which fails the differential
  spuriously. Bound the fuzzer's values and program length so no legal input overflows.
- **Empty test targets.** Leave `alloc_test` (etc.) as `""` to opt a layer out; the CLI maps empty → not
  configured. A configured target that produces no test summary scores **0.0**, not "absent".
- **Low genuine diversity hiding behind fresh prompts.** If `spec_diversity` is small, widen the
  structural surface (more operations / rules), not the example noise. See Q30/Q31.
