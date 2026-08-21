# Build log

A running record of what has actually been built, verified, and decided —
distinct from the design docs (which specify the target) and the review rounds
(which attack it). One entry per meaningful increment. Newest first.

The roadmap phases referenced here are in [14-roadmap.md](14-roadmap.md).

---

## 2026-08-21 · Fifteenth family: `idiom-counter` — the second idiom-refactor family, a different lint

`idiom-refactor` had exactly one family (`idiom-loop`, built around
`clippy::needless_range_loop`). This adds a second, built on a different
detectable non-idiom: the **explicit running counter** —
`let mut i = 0; for &x in xs { … i += 1; }` — which trips
`clippy::explicit_counter_loop`.

Same de-idiomatisation contract as `idiom-loop`: the skeleton is *behaviourally
complete* and only clippy distinguishes it; weights are docs/04's
behavior .30 / constraint .60 / quality .10 (`check_clippy=true`,
`max_unsafe=None`).

Two axes × = **12 distinct specs**: how index contributes (**weight**: linear
`i+1` / reverse `n-i` / parity `±1`) and what each element becomes before
weighting (**map**: identity / double / square / negate). The idiomatic answers:
`.enumerate()` for linear, `.rev().enumerate()` for reverse (the reversed
position *is* the weight — no length arithmetic), an `if i % 2` inside the
closure for parity.

**Q14 verified twice over.** Before authoring, both directions were measured in
a scratch crate:

- all three de-idiom shapes fire `explicit_counter_loop`, whose suggestion is
  MaybeIncorrect — after `cargo clippy --fix` every `i += 1;` survives (and the
  automated gate confirms per-seed below);
- all 12 emitted references are clippy-clean under `-D warnings` (dumped and
  linted verbatim), so the reference scores 1.000 while the unchanged counter
  loop scores ~0.33.

Baselines unchanged from `idiom-loop`: the load-bearing `unchanged` (correct,
unidiomatic) plus `const-zero`; the pinned canonical `[3,1,4,2]` is non-zero
under all 12 spec combinations.

```
validate-family idiom-counter: 8 seed(s)
  seed   0: OK   determinism=true reference=1.000 skeleton=0.333
                 (behavior 1.000) baselines_caught=true canary=true
                 clippy_fix_safe=true (1 lint(s) survive --fix)
  seed   1..7: same OK shape; seeds 2 and 6 report 2 surviving lints  [×8]
  anti-twin  view (prompt+skeleton): min=0.286 median=0.399 near-twin pairs (<0.25)=0/28
  anti-twin  reference (solution) : min=0.120 median=0.833 near-twin pairs (<0.25)=2/28
  capacity   view=8+ (asked 8, rejected 0)  reference=15
  spec-diversity: 12 distinct skills (the authoritative task-diversity measure — Q31)
  distinct-skills epoch: served 8 seed(s) covering 8 distinct skills
all gates passed
```

Honest caveats:

- Skeleton composite 0.333 with behavior 1.000 is the family working as
  designed — the model's starting code is correct, and only the clippy layer
  separates it from full credit.
- Reference anti-twin min=0.120 (two near-twin pairs of 28): with one shared
  signature and three weight strategies, two seeds' references can be close;
  view-distance stays clean and spec-diversity (12) is the gated measure.
- Like `idiom-loop`, there is no unsafe layer here by design.

186 workspace tests (+8); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · Fourteenth family: `raw-ptr-mut` — unsafe *writes*, the second unsafe-core family (P7)

The corpus had exactly one unsafe-core family (`raw-ptr`, reads through
`*const i64`). This adds the write half of the unsafe story: **in-place mutation
through a raw mutable pointer** —

```rust
pub fn {name}(ptr: *mut i64, len: usize) -> usize  // returns the number of elements written
```

The model must derive addresses with `ptr.add(i)`, dereference-**assign** under
`unsafe`, and return how many slots it touched — a genuinely different skill from
read-only traversal (aliasing discipline: never read a slot you are about to
overwrite, and the count is part of the contract, not an afterthought).

Two structural axes × four transforms = **16 distinct specs**: which positions are
written (`all` / even indices / odd indices / first half — via start/stride/bound,
same shape as `raw-ptr`) and what is stored (`x*2` / `-x` / `x*x` / `x+1`). The
native `eval` mutates a `&mut [i64]` slice and is the source of truth; the reference
mirrors it through `unsafe`; the differential oracle fuzzes 3000 inputs and compares
**both** the returned count *and* the whole mutated buffer against the safe mirror.

Baselines are two constant-ish cheats: `no_op` (write nothing, return 0) and
`fill_zero` (zero everything, return `len`) — both caught on every seed because the
pinned canonical case `[3,1,4,2]` selects ≥2 positions and every expected output
element is non-zero (pinned across all 16 spec combinations).

Honest caveats:

- **Miri stays deferred**, as for `raw-ptr`: docs/04 weights the category
  behaviour .70 / constraint .30 / quality .00 and renormalises quality away until
  miri lands; correctness of pointer arithmetic here is checked by the differential
  mirror plus the grader's existing checks, not by an interpreter.
- Reference-distance is expected to sit near `raw-ptr`'s low band (pinned-pointer
  interface ≈ fixed scaffold), so view/reference capacity is reported but not gated.
- Like all unsafe-core families there is no clippy constraint layer
  (`check_clippy=false`, `max_unsafe=None`).

```
validate-family raw-ptr-mut: 8 seed(s)
  seed   0..7: OK   determinism=true reference=1.000 skeleton=0.000
                     (behavior 0.000) baselines_caught=true canary=true   [×8]
  anti-twin  view (prompt+skeleton): min=0.313 median=0.435 near-twin pairs (<0.25)=0/28
  anti-twin  reference (solution) : min=0.000 median=0.368 near-twin pairs (<0.25)=6/28
  capacity   view=8+ (asked 8, rejected 0)  reference=10
  spec-diversity: 16 distinct skills (the authoritative task-diversity measure — Q31)
  distinct-skills epoch: served 8 seed(s) covering 8 distinct skills
all gates passed
```

178 workspace tests (+6); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · `rustybench report` — the journal → deliverable command (P5)

Built the `report` command docs/08 lists (`report [--format md|json]`), the shape of P5's output: fold a
journal into a formatted deliverable rather than the terse `stats` line.

- **`bench_stats::diagnostics(records)`** (new, pure, core-only per ADR-0009): apply-rate, compile-rate
  (compiled ÷ *applied*), and the **failure-class** and **rustc-error-code histograms** — the signal
  docs/03 calls out as this benchmark's differentiator ("*which part of Rust* a model is weak at"), which
  no general-purpose benchmark produces. `FailureClass::as_str()` added to `bench-core` for the kebab
  histogram keys, and `StatReport`/`CategoryReport`/`ThroughputReport`/`DiagnosticsReport` gained
  `Serialize` for the JSON format.
- **CLI**: `report` renders **md** (headline capability + CI + throughput + pass-rate; per-category table
  with CIs, ICC and the directional-only flag; the diagnostics block; and the honest bootstrap/borrowck-
  lower-bound caveats) or **json** (`{stats, diagnostics}` with the histograms as arrays, radar-ready).
  `html` is accepted-but-declined with a clear message (deferred).

Verified on the real smoke journal (`runs/clean.jsonl`, Qwen2.5-3B): capability 0.336, throughput 53.5
tok/s / 945 units-h / 157 passes-h, and — the new part — **apply 1.000, compile 0.375**, failure classes
`none×5, logic×1, other×1, trait×1`, top codes `E0277×1, E0507×1`. That compile-rate and histogram are
exactly the per-model diagnostic the project exists to publish.

Pure code + fast unit tests (a `diagnostics` aggregate test on synthetic records; `FailureClass::as_str`),
no grading run. 172 workspace tests (+2); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · Richer `failure_class` — the diagnostic classifier reaches its dormant classes

docs/03 §L1 specifies `failure_class = classify(error_code, message_pattern, clippy_lints, category)`, but
the code only did an error-*code* lookup. Two enum variants were therefore **unreachable**: `AsyncSend`
(Rust's most characteristic async failure, `future cannot be sent between threads safely`, carries
`code: None` — 18% of realistic failures are codeless, docs/03) and `Idiom` (non-idiomatic code compiles
and passes behaviour, so it produces no code — only clippy sees it). Fixed, in pure `bench-core` logic with
fast unit tests (no grading pipeline):

- **`parse_diagnostics`** (bench-oracle) now also captures the rendered **error-level messages**, not just
  codes and the warning count.
- **`classify_compile_error(codes, messages)`** consults a message-pattern table *before* the code table:
  `… cannot be sent/shared between threads safely` → `AsyncSend`, which also disambiguates the notorious
  `E0277` (it spans four categories) — a Send/Sync bound is upgraded to `AsyncSend`, everything else falls
  through to `Trait` via the existing code table. No pattern match ⇒ identical to the old code-only path.
- **`classify_graded(behavior, clippy_clean, constraint)`** classifies a *compiled* unit: behaviour miss →
  `Logic`; else a clippy violation → `Idiom` (the idiom-refactor signal, ahead of a generic `Constraint`);
  else another constraint miss → `Constraint`; else `None`. Extracted into `bench-core` so it is pure and
  unit-tested rather than inline in the grader.

`bench-oracle::grade` calls both. **No regression**: the twelve non-clippy families never set `clippy_clean`
(→ `None`, so `classify_graded` matches the old Logic/Constraint/None), and no current family emits async
messages (→ code-table fallback unchanged). The immediate win is `idiom-loop`: a behaviourally-correct but
non-idiomatic answer now classifies as **`Idiom`** instead of a generic `Constraint`. `AsyncSend` has no
producing family yet (`async-concurrency` awaits its oracle, Q11) but the classifier is ready and tested,
so it lights up the moment async families arrive.

5 new/updated tests (codeless-async → AsyncSend; E0277 split; code fallback; the graded ordering; messages
captured by `parse_diagnostics`). 170 workspace tests (+4); clippy `-D warnings` and `cargo fmt --check`
clean. Deferred, noted: `diagnostic_completeness` (`full` | `typeck_only`, so borrow failures are published
as the lower bound they are — docs/03) is a natural next field.

---

## 2026-08-21 · The Q14 gate, automated — `clippy --fix` must not solve the instance

Promoted the previous entry's manually-measured Q14 property into a real `validate-family` gate, so it can
never silently regress if `idiom-refactor`'s transform catalogue changes. Q14 requires that a family whose
signal is clippy not be *trivially auto-solvable* by `cargo clippy --fix` — otherwise it measures
transcription, not reasoning.

- `bench-oracle` gains `clippy_fix_remaining_lints(files, allow, limits, ws)`: it materialises the
  model-visible skeleton into a fresh sandboxed workspace, runs `cargo clippy --fix --allow-dirty
  --allow-no-vcs --lib` on it (applying every machine-applicable fix), then re-lints and returns the
  clippy lints that **remain**. Empty ⇒ `--fix` produced clean code ⇒ trivially auto-solvable.
- `validate-family` runs it for clippy-graded families (`check_clippy`) and folds the result into the
  per-seed verdict: the seed fails unless lints survive `--fix`.

Live on `idiom-loop`:

```
seed 0: OK … clippy_fix_safe=true (1 lint(s) survive --fix)
```

Every seed keeps its `needless_range_loop` after `clippy --fix` (the suggestion is not machine-applicable),
so the model must genuinely reason about the iterator rewrite — now proven automatically each run rather
than by a one-off manual spike. A family that accidentally chose a *machine-applicable* lint would fail this
gate loudly (`clippy_fix_safe=false`). The check is a no-op for the twelve non-clippy families. The helper
is exercised end-to-end by `validate-family` (like `grade()` itself, it needs the sandboxed toolchain, so
it has no pure unit test). 166 workspace tests; clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · Thirteenth family: `idiom-loop` (`idiom-refactor`) — the fifth core category, and the first compositional one

The last uncovered docs/04 core category, and the only **compositional** family: instead of ablating a
reference to `todo!()`, it **de-idiomatises** it. The model is given a working but non-idiomatic function
— an explicit `for i in 0..xs.len()` index loop — and must rewrite it in idiomatic, clippy-clean iterator
style, preserving behaviour. So the ablation is *quality, not correctness*: the given code is
behaviourally correct, and **only clippy distinguishes it** (docs/03: "non-idiomatic code compiles; clippy
catches all of it"). This is the family the previous increment's L3 clippy oracle was built for.

Seed-selected on **filter** (All / Positive / Even / Nonzero) × **map** (Identity / Double / Square /
Negate) — the loop body — folding to a sum, so the idiomatic form is a
`xs.iter().copied()[.filter(…)][.map(…)].sum()` chain. 4 × 4 = **16 distinct skills**. docs/04 weights it
constraint-dominant (behavior 0.30 / **constraint 0.60** / quality 0.10): the constraint *is* clippy, and
`max_unsafe: None` keeps the L3 layer purely clippy.

`validate-family --seeds 8`, all gates green — and the middle column tells the whole story:

```
reference=1.000  skeleton=0.333 (behavior 1.000)  baselines_caught=true  canary=true
view       min=0.274 median=0.383 near-twins 0/28
reference  min=0.222 median=0.545 near-twins 1/28   capacity=38
spec-diversity: 16 distinct skills
```

`skeleton=0.333 (behavior 1.000)` is the point: the de-idiomatised loop is *behaviourally perfect* and is
caught purely by clippy dropping the constraint layer to 0 → composite `(0.30·1 + 0.60·0)/0.90 = 0.333`.
The idiomatic reference is clippy-clean → 1.000. This needed the **validate-family gates generalised** from
behaviour to composite: a degenerate answer is "caught" iff it does not achieve a full pass (composite <
1.0), which now covers the `unchanged` (copy-paste) baseline — behaviourally correct, not idiomatic — as
well as the todo!()-ablated skeletons of every other family (composite ~0, unchanged in practice).

**Q14 (the `clippy --fix` trap) — verified, not assumed.** `idiom-refactor` is only meaningful if it is not
trivially auto-solvable. Measured directly: `cargo clippy --fix` leaves `needless_range_loop` **unchanged**
(its suggestion is not machine-applicable), so the index loop survives `--fix` and the model must actually
reason about the rewrite. And all 16 idiomatic references were checked clippy-clean before authoring, so the
reference reliably scores 1.000. (An *automated* clippy-`--fix` gate in `validate-family` is a sensible
follow-up hardening; today the property is verified by measurement and holds by construction of the
transform.)

Honest consequence, recorded: for `idiom-refactor` the binary `passed()` (Q28) treats the unchanged loop as
a *pass* (it excludes clippy as "quality"), so this category's signal lives in the continuous
`capability_score` (via the 0.60 constraint weight), not in the pass rate. That is a property of Q28's
scope, not a bug — and it is why the family is authored constraint-dominant.

**All five docs/04 core categories are now covered** (`borrow-lifetimes`, `traits-generics`,
`error-handling`, `unsafe-core`, `idiom-refactor`). Registered in `FAMILY_IDS` and the `spec_diversity` pin
in the same commit. 166 workspace tests (+7); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · The clippy constraint oracle (L3) — the idiomaticity signal

Built the grading layer `idiom-refactor` needs. docs/03 puts clippy in **L3 constraint** (not L4): non-
idiomatic code compiles and is behaviourally correct, so clippy is the *only* thing that distinguishes it
(docs/03 line 84, "Clippy catches all of it"). The `ConstraintScore.clippy_clean` field already existed but
was never populated; now it is.

- `bench-oracle` `grade()` gains a clippy stage (`GradeSpec.check_clippy` / `clippy_allow`): after L1
  compiles, it runs `cargo clippy --lib --offline --message-format=json` (with `-A <lint>` for any allowed
  lints) on the answer's library only — so the hidden test targets never contribute lints — and parses the
  JSON for `clippy::*` **warning** codes. `parse_clippy_lints` counts only `clippy::` codes, so ordinary
  rustc warnings (unused vars) don't touch idiomaticity; empty ⇒ `clippy_clean = true`, else the lint names
  land in `violations`. It feeds `ConstraintScore` (already averaged into the L3 layer score, weighted by
  the per-category `constraint` weight), and — per Q28 — **not** `passed()`, since clippy is quality, not
  correctness.
- `GeneratedTask.max_unsafe` became `Option<u32>` so a family can opt the unsafe check *out* entirely
  (`None`), keeping the constraint layer clippy-only where `unsafe` is irrelevant (`idiom-refactor`) — and
  this also removes the spurious constraint credit `raw-ptr` used to get from its `u32::MAX` limit (now
  `None`: it grades on behaviour alone, no phantom 1.0 constraint term). All 12 families updated
  mechanically (`Some(0)` / `None`); every existing family's grading is unchanged (none sets `check_clippy`).

Part 1 of two. The clippy stage is dormant until a family enables it — the next entry (`idiom-refactor`)
is its first user and its end-to-end test. Direct coverage here: a `parse_clippy_lints` unit test (clippy
warnings kept, rustc warnings dropped, deduped). 159 workspace tests (+1); clippy `-D warnings` and
`cargo fmt --check` clean.

---

## 2026-08-21 · Generic construction-invariant drift-guard over the whole registry

With twelve families and more coming, added a single `cargo test` that loops over `FAMILY_IDS` and asserts
the **pure** construction invariants for every family — the ones that need no toolchain, so they run in
milliseconds rather than in the slow `validate-family` compile path:

- **determinism** — `generate(seed)` is byte-identical on a re-run (prompt, files, hidden);
- **canary** — the prompt carries its `mint_canary` string;
- **non-empty category** and **non-empty `spec_signature`** per seed, and the instance's `category`
  matches the family's;
- **spec-diversity ≥ 8** — docs/17's "comfortably above a per-epoch seed count of 8", now the public
  `bench_gen::MIN_SPEC_DIVERSITY` floor enforced generically (provisional, like
  `bench_stats::CLUSTER_FLOOR`; the real value is fixed once Phase 4 sets the per-epoch count). Every
  current family clears it with headroom (smallest is 12). This turns Q30/Q31's open "per-family floor"
  downstream note from *unspecified* into *provisionally enforced* — recorded there.

Until now these lived only in each family's own hand-written tests and in the manual CLI gate, so a new
family that simply *forgot* to add a canary or determinism test would pass CI. Now the registry itself is
the checklist: a family that skips any invariant fails `cargo test` the moment it lands in `FAMILY_IDS`.
The compile/differential gates (reference = 1.000, skeleton fails, baselines caught) stay in
`validate-family` — they need the sandboxed toolchain and are too slow for every `cargo test`. Also
refreshed the stale crate-level module doc (it still described "this first P3 increment ... one parametric
family"). 158 workspace tests (+1); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · README refreshed + corpus snapshot

Refreshed the README status block, which had drifted to "three families / 71 tests" and listed
`bench-stats` and resume as "still to come" though both have landed. It now states twelve families across
nine categories, 157 tests, the full crate list (with `bench-stats`, previously omitted), the run/run-suite/
stats/status CLI surface, and an honest "still to come" (hardware, the full corpus, `idiom-refactor`'s
compositional archetype, `unsafe-core`'s miri layer, the mined `wild` suite).

The corpus after this session's authoring pass — the run-suite epoch planner serves all twelve
(`--dry-run` confirmed):

| family | category | spec-diversity | ref-capacity | notes |
|---|---|---|---|---|
| window-op | borrow-lifetimes | 12 | 22 | in-place windows; alloc constraint |
| dual-region | borrow-lifetimes | 12 | 6 | `split_at_mut` pairs; alloc constraint |
| error-handling | error-handling | 30 | 7 | parse → validate → `Result` + `?` |
| checked-eval | error-handling | 12 | 10 | checked arithmetic; overflow/guard errors |
| trait-impl | traits-generics | 20 | 1 | implement a trait + associated type |
| generic-select | traits-generics | 16 | 4 | write a generic fn over a trait bound |
| raw-ptr | unsafe-core | 20 | 18 | forced `unsafe` raw-pointer reads (miri deferred) |
| bit-ops | bit-manipulation | 20 | 45 | mask → rotate/reverse/swap |
| str-transform | string-processing | 24 | 56 | filter → case-map → order |
| stack-machine | pattern-matching | 40 | 2 | exhaustive `match` over an enum |
| seq-transform | iterators | 48 | 43 | filter → map → terminal |
| grid-reduce | data-structures | 12 | 31 | 2-D grid reduction |

Every spec-diversity clears any plausible per-epoch seed count (the authoritative Q31 gate). The
ref-capacity column is the honest text proxy, deflated by fixed scaffolding — the pinned-interface families
(`trait-impl` 1, `stack-machine` 2, `generic-select` 4, `dual-region` 6, `error-handling` 7) sit low, the
short-body families (`str-transform` 56, `bit-ops` 45, `seq-transform` 43) high — which is exactly why the
gate is spec-diversity, not reference-distance (docs/17). **4 of 5 docs/04 core categories covered**
(`borrow-lifetimes`, `error-handling`, `traits-generics` at 2 families each; `unsafe-core` at 1); only
`idiom-refactor` is unstarted, pending the compositional/inverse-transform archetype (a framework addition,
not a per-family one — roadmap P3 R2-S6). Docs only; 157 tests, clippy/fmt clean.

---

## 2026-08-21 · Twelfth family: `checked-eval` — a *second* `error-handling` family

Brings the third core category to two families. `error-handling` (#1) is parse-then-validate over
`&[&str]` with plain arithmetic; `checked-eval` tests the sub-skill #1 doesn't — **checked arithmetic and
overflow propagation**. The model implements `fn f(xs: &[i64]) -> Result<i64, MathError>`, folding with a
seed-selected checked reduction and short-circuiting on the first error: a **guard** failure
(`MathError::OutOfRange(x)`) or an **overflow** (`MathError::Overflow`). This is the "don't panic on
overflow, propagate via `checked_*` + `.ok_or(…)?`" skill.

Seed-selected on **fold** (Sum / Product / SumSquares, all `checked_*`) × **guard** (Positive / NonZero /
AtMost(b) / InRange(lo,hi)) = **12 distinct skills**; guard bounds are per-skill constants excluded from
`spec_signature`. The `MathError` enum is pinned in the skeleton (docs/04: error-handling pins the public
error type). Correct-by-construction (ADR-0003): native `eval` and the emitted reference are mirrored
(errors compared as tag strings, as in #1).

**The differential earns its keep here.** It fuzzes values in ±3e9 — wide enough that `Product`/
`SumSquares` overflow — so a model that reaches for plain `*` instead of `checked_mul` *panics* in the
debug build, produces no test summary, and scores 0. The reference is `checked_*` throughout, so it never
panics. `validate-family --seeds 8`, all gates green:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
view       min=0.201 median=0.397 near-twins 1/28
reference  min=0.130 median=0.453 near-twins 2/28   capacity=10
spec-diversity: 12 distinct skills
```

Baselines: `const-ok` (`Ok(0)`) is caught by the canonical `[2,3,4]` (valid under every guard → `Ok(v≠0)`);
`no-guard` (folds correctly but omits the precondition) is caught by a constructed guard-failure example.
A test pins both invariants across every (fold, guard) combo. reference-capacity **10** — healthier than
the purely-pinned families because the checked-fold body varies more than a bare plumbing shape.

`error-handling` now has **2 of 40**. Three of the four covered core categories (`borrow-lifetimes`,
`traits-generics`, `error-handling`) sit at 2 families each; only `unsafe-core` (miri-gated) is still at 1,
and `idiom-refactor` (compositional archetype) remains unstarted. Second clippy `enum_variant_names` catch
of the session (the `Checked` prefix — exactly the pitfall just added to doc 17), renamed to Sum/Product/
SumSquares. 157 workspace tests (+6); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · Refreshed the authoring guide (doc 17) with the families and lessons

The roadmap calls authoring ergonomics "the highest-leverage work in the project" (an hour spent here pays
back ~270 times). [Doc 17](17-authoring-families.md) had drifted badly — it still opened with "Three
reference families" and named only `window_op`/`error_handling`/`stack_machine`, when there are now
**eleven**. Rewrote it around what the last week actually taught:

- **Archetype table.** The eleven families grouped into five shapes (seed-selected pipeline; in-place +
  alloc constraint; pinned interface; provided-enum match; signature-forced) with the canonical family to
  copy for each — so a contributor picks the nearest template by *shape*, not by reading all eleven.
- **A new "trivial baselines" section** capturing the trap I hit in four families this session: a *shaped*
  baseline (return the sum / the length / the first element) coincides with exactly one real spec and then
  passes on that seed. The fix — Vec output uses `identity`/`empty`; scalar/`Option` output uses two
  spec-independent constants with a canonical proven to avoid them, pinned by a test — is now written down.
- **"Forcing the skill through the signature"** — `raw_ptr` (unsafe unavoidable) and `trait_impl`
  (associated-type/signature match), with the honest miri caveat.
- **The reference-capacity reality**, measured across all families: pinned-interface families sit at 1–7
  (scaffolding dominates the solution text) while short-body families reach 45–56, so reference-distance is
  a diagnostic and `spec_diversity` is the gate — with the actual numbers in the doc.
- **A pitfall for clippy on generator code** (`enum_variant_names`, `needless_range_loop`,
  `unnecessary_literal_unwrap` all fired on first cuts), plus the register-in-three-places drift-guard.

Docs only — `bench-invariants` parses docs 04/07/09, not 17, so no table-arithmetic test is affected; the
workspace stays at 151 tests, clippy/fmt clean. This is the G3 lever kept current while the lessons are
fresh, rather than re-derived by the next author.

---

## 2026-08-21 · Eleventh family: `generic-select` — a *second* `traits-generics` family

Deepening the second core category. `traits-generics` had only `trait-impl` (which pins a trait + driver
and asks the model to *implement* the trait); this exercises the **sibling skill** — the model instead
**writes a generic function bounded by** a pinned trait: `fn f<T: Ranked>(items: &[T]) -> Option<i64>`.
Writing the `<T: Ranked>` bound and the generic body is the skill `trait-impl` does not test. It is also
**selection-shaped, not a fold**, so it reads differently from the reduce/pipeline families.

Seed-selected on **select** (Max / Min / First / Last — which item's `rank()`) × **project** (Identity /
Abs / Double / Square — how the chosen rank is transformed) = **16 distinct skills**; `None` iff empty.
Correct-by-construction (ADR-0003): native `eval` and the emitted reference are mirrored, and the
differential builds `Vec<R>` for a hidden `R: Ranked` and fuzzes 3000 slices against a free reference over
the raw ranks; ranks bounded so `Square` can't overflow. `validate-family --seeds 8`, all gates green:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
view       min=0.393 median=0.489 near-twins 0/28
reference  min=0.154 median=0.326 near-twins 9/28   capacity=4
spec-diversity: 16 distinct skills
```

Both trivial baselines are constants — `const-none` (always `None`) and `const-zero` (always `Some(0)`) —
caught on every seed because the canonical ranks `[3, 7, 2, 5]` yield `Some(v)` with `v` neither `None` nor
`0` under all 16 combos (pinned test). Same honest pinned-interface caveat as `trait-impl`: reference-
capacity is **4** (the trait + generic-fn scaffolding dominates the short body), while spec-diversity (16,
the authoritative gate) and view-distance (0 near-twins) both clear the bar.

`traits-generics` now has **2 of its 40** families, testing its two complementary halves (implement a
trait; consume one generically). Two core categories now sit at 2 families each (`borrow-lifetimes`,
`traits-generics`). 151 workspace tests (+6); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-21 · Tenth family: `dual-region` — a *second* `borrow-lifetimes` family

With the tractable categories covered, the next corpus goal is deepening the core categories toward their
40-family target (docs/04). `borrow-lifetimes` — the project's flagship core category — had only
`window-op`, so this is its second family, deliberately a **different borrow shape**: a
`split_at_mut`-shaped problem, which docs/04 names explicitly and `window-op` (single-pass window
mutation) does not exercise.

The model implements `fn f(v: &mut [i64]) -> usize`: split `v` at its midpoint into two disjoint halves
and, for each of the `len/2` pairs, apply a seed-selected pairwise op **in place**, returning the count of
pairs transformed. Rewriting a pair `(x, y)` from *both* values at once (e.g. `SumDiff → (x+y, x-y)`) is
the canonical case for `split_at_mut` — you need a `&mut` into both halves simultaneously. Seed-selected on
**op** (Swap / SumDiff / SortPair / AddBoth / DiffBoth / MaxBoth) × **pairing** (Aligned `b[i]` / Mirror
`b[len-1-i]`) = **12 distinct skills**.

It reuses `window-op`'s two load-bearing pieces: the **constraint-dominant weights** (behavior 0.35 /
constraint 0.55) and the **allocation instrumentation** (`alloc.rs` counting `#[global_allocator]`), so a
clone-everything answer — behaviourally correct — is caught by the alloc constraint, which is the whole
borrow-lifetimes signal. Correct-by-construction (ADR-0003): native `eval` and the emitted reference are
mirrored (the differential compares *both* the mutated array and the count over 3000 slices); values
bounded so `SumDiff`/`DiffBoth` can't overflow. `validate-family --seeds 8`, all gates green:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
view       min=0.408 median=0.511 near-twins 0/28
reference  min=0.120 median=0.296 near-twins 6/28   capacity=6
spec-diversity: 12 distinct skills
```

The two trivial baselines are `const-zero` (returns 0, no work) and `identity` (returns the correct count
`len/2` but does no work — "counts but doesn't transform"); both are caught because the canonical
`[6, 4, 3, 1]` — first half all greater than second half — is changed with a non-zero count by every one of
the 12 (op, pairing) combos (pinned as a test). Honest note: reference-capacity is **6**, on the low side —
the fixed `split_at_mut` loop scaffolding dominates the short solution text, the same pinned-scaffolding
phenomenon as `stack-machine` (2) and `trait-impl` (1). spec-diversity (12, the authoritative Q31 gate) is
what the family is authored against, and it clears the bar; the view distance (0 near-twins) keeps prompts
fresh. One clippy fix along the way: the native `eval`'s index loop tripped `needless_range_loop`, rewritten
with `enumerate` (the emitted reference keeps the readable index loop — it is a string, graded without
`-D warnings`). `borrow-lifetimes` now has **2 of its 40** families. 145 workspace tests (+8); clippy
`-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-20 · `status` — the resume readout (progress · ETA · segment history) (P4)

The docs/08 CLI lists `rustybench status … # progress, ETA, segment history`; built it, and it doubles as
the real resume readout the run protocol needs. `rustybench status --journal <j> [--epoch <e>]
[--seeds-core N] [--seeds-probe M]` reads a journal and, for the chosen epoch (the most recent one if
omitted), reports:

- **progress** — `done / planned (%)`, computed on the *same* plan/resume path `run-suite` uses
  (`plan_run` + `read_done_keys` + `remaining`), so the number is exactly what a resume would face, not a
  re-derivation that could drift;
- **ETA** — `remaining × steady-state s/unit`, where the pace comes from `bench_stats::throughput`, i.e.
  with the cache-warmth exclusion applied — so a resumed run's cold lead unit doesn't inflate the estimate;
- **segment history** — units and mean s/unit per run session (`seg 0`, `seg 1`, … or `seg —` for
  pre-segment journals);
- **next up** — the first few units `run-suite` would actually execute next (the resume readout proper).

Live on the smoke journal (`runs/clean.jsonl`, a pre-segment 4-family run) reprojected onto today's 9
families at the default 4 core / 1 probe:

```
status: epoch clean-2026-08
  plan      45 units (36 core + 9 probe over 9 families)
  done      12/45  (27%)
  pace      3.8 s/unit (steady-state over 12 timed unit(s))
  ETA       ~2.1m for the remaining 33
  segments:
    seg —: 12 unit(s), mean 3.8 s/unit
  next up (5 of 33): window-op core idx=2 …
```

And on a synthetic two-segment journal the ETA tracks the *steady-state* pace (1.0 s/unit) rather than the
warmup-inflated segment mean (34 s/unit) — the whole point of recording `segment_position`. Honest scope
note: docs/08 keys `status` on a `run_id`, but there is no persisted run-state store yet, so it is keyed on
`--journal`/`--epoch` like every other command here; the readout is identical in substance. A `fmt_duration`
unit test pins the s/m/h humanisation. 139 workspace tests (+1); clippy `-D warnings` and
`cargo fmt --check` clean. That closes the run-protocol-depth items from the overnight brief
(`segment_position`, cache-warmth exclusion, `status`, resume readout).

---

## 2026-08-20 · Run protocol depth — `segment_position` + the cache-warmth throughput exclusion (P4)

Closed the docs/08 refinement flagged as future work in the throughput entry below: *"Exclude the first
N units of each segment from timing aggregates (cache warmth); record `segment_position` on every unit so
this is auditable rather than magic."*

- **Segments recorded.** A *segment* is one `run-suite` session over an epoch (docs/09); a resume starts a
  new one, and its lead units compile against cold caches. `run-suite` now stamps every journalled unit
  with `segment` (one past the highest already recorded for the epoch — computed by `next_segment`) and
  `segment_position` (0-based within the session). The single-unit `run` writes neither (`None` — no
  segment structure), and both fields `skip_serializing_if` empty so a `run` line is byte-unchanged.
- **Cache-warmth exclusion.** `bench_stats::throughput` now drops the first `SEGMENT_WARMUP_UNITS` (= 1,
  a named, tunable constant) of each segment from the timing aggregate: a unit is warmup iff its
  `segment_position` is below the threshold. Units with no position (single `run`, older journals) are
  never excluded, and if the exclusion would empty the timed set (a one-unit segment) it falls back to
  using all — a warm-biased number beats none. It touches **timing only**; capability/pass scoring still
  uses every unit. The report carries `warmup_excluded` and `stats` prints it, so the exclusion is
  auditable rather than magic.

Verified three ways. Backward-compat: `stats` on the pre-segment `runs/clean.jsonl` is unchanged
(12 units, 53.5 tok/s, 945 units/h — no positions, nothing excluded). A synthetic segment with one cold
lead unit (1 tok/s) followed by two warm ones (100 tok/s) reports **"2 executed units … 1 segment-warmup
unit excluded", decode 100.0 tok/s** — the cold unit no longer drags the steady-state number down. A unit
test pins both the exclusion and the one-unit-segment fallback.

Honest note: as the throughput entry below already measured, cache warmth is a *smaller* effect than the
compile-success split (a compiling/passing unit runs the 3000-iter differential ≈1 s; a compile-fail
short-circuits ≈0.1 s). So this exclusion matters most for cross-segment/resumed runs; within one warm
session it moves the number little. It is the right, auditable mechanism regardless. 138 workspace tests
(+1); clippy `-D warnings` and `cargo fmt --check` clean. Still future work: `status`/`resume` readouts
(next).

---

## 2026-08-20 · Ninth family: `str-transform` (category `string-processing`)

More corpus breadth (P7): a text-processing shape. The model implements `fn f(s: &str) -> String` as a
seed-selected three-stage pipeline — **filter** (Alpha / Alnum / NonSpace / All) → **case-map** (Upper /
Lower / SwapCase) → **order** (InOrder / Reversed) = 4 × 3 × 2 = **24 distinct skills**, the widest surface
of the families authored tonight. Everything is ASCII-only by construction (the maps are the `to_ascii_*`
family, the fuzzer draws printable-ASCII bytes 0x20..0x7e), so there are no UTF-8 char-boundary hazards —
the string analogue of the arithmetic families bounding their inputs against overflow.

Solution-first / correct-by-construction (ADR-0003): native `eval` and the emitted reference are mirrored
(`s.chars().filter(…).map(…).collect()`, optional `.rev()`); the differential fuzzes 3000 random ASCII
strings. The two trivial baselines (`identity`, `empty`) are caught on every seed because every case-map is
a genuine transform (never identity) and the canonical example `"Hello, World!"` carries both cases plus
punctuation and a space, so all 24 combinations change it and leave it non-empty (pinned as a test).

Like `bit-ops`, this is a textually-healthy family — the short pipeline body changes visibly with the op,
so all three anti-memorisation numbers agree:

```
view       min=0.257 median=0.446 near-twins 0/28
reference  min=0.273 median=0.536 near-twins 0/28   capacity=56
spec-diversity: 24 distinct skills
```

Registered in `FAMILY_IDS` and the `spec_diversity` pin (== 24) in the same commit. **The corpus now spans
9 families across 9 categories** (`borrow-lifetimes`, `error-handling`, `pattern-matching`, `iterators`,
`data-structures`, `traits-generics`, `bit-manipulation`, `unsafe-core`, `string-processing`) — 4 of
docs/04's 5 core categories, only `idiom-refactor` (the compositional/inverse-transform archetype) still
uncovered. 137 workspace tests (+6); `validate-family --seeds 8` all gates green; clippy `-D warnings` and
`cargo fmt --check` clean.

(Clippy caught an `enum_variant_names` on the first cut — every `Filter` variant was prefixed `Keep`;
renamed to `Alpha`/`Alnum`/`NonSpace`/`All`. The gate earning its keep on generator code, not just on
graded answers.)

---

## 2026-08-20 · Eighth family: `raw-ptr` (category `unsafe-core`) — with an honest miri caveat

`unsafe-core` is docs/04 core #5, and this covers it — bringing core coverage to **4 of 5**
(`borrow-lifetimes`, `error-handling`, `traits-generics`, `unsafe-core`; only `idiom-refactor`, which
needs the compositional/inverse-transform archetype, remains). The model implements
`fn f(ptr: *const i64, len: usize) -> i64`, and **`unsafe` is forced by the signature** — there is no
safe way to dereference a `*const i64`, so a scoring answer must contain `unsafe { *ptr.add(i) }` (or
`from_raw_parts`). That is the genuine article, not unsafe-as-decoration.

Seed-selected on two axes — **access pattern** (Forward / EveryOther / OddIndices / FirstHalf, each a
different pointer-arithmetic walk) × **reduce** (Sum / Product / SumOfSquares / SumOfAbs / SumOfPositives)
= **20 distinct skills**. Solution-first and correct-by-construction (ADR-0003) via *three* mirrors of one
index-walk: the native `eval`, the emitted **unsafe** reference (reads through the pointer), and the
differential's **safe** reference (indexes a slice). The differential passes `xs.as_ptr()` / `xs.len()`
into the model's `f` and compares against the safe reference over 3000 bounded slices; the access-pattern
axis is what makes the patterns behaviourally distinct (each selects a different index *subset*, not just
a different order, so the reductions genuinely differ). `validate-family --seeds 8`, all gates green:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
view       min=0.288 median=0.406 near-twins 0/28
reference  min=0.176 median=0.429 near-twins 1/28   capacity=18
spec-diversity: 20 distinct skills
```

Both trivial baselines are constants (`const-zero`, `const-one`) — same reasoning as `trait-impl`: any
*shaped* degenerate (first-element, length) coincides with one real spec on the canonical input `[3,1,4,2]`,
whose answer is provably never 0 or 1 under all 20 combos (pinned as a test, which also checks every
pattern selects ≥ 2 elements).

**The honest caveat, stated loudly rather than buried.** docs/04 makes **miri mandatory** for this
category — "UB = hard behavior failure" — and the miri layer does not exist yet (roadmap P7). Behaviour +
the differential catch *wrong* answers and gross out-of-bounds (a value mismatch, or a crash the sandbox
records), but **not** subtle unsoundness that still returns the right values on the fuzzed inputs (a
provenance violation, say). So today this family grades on behaviour + differential only. It is
correct-by-construction and passes every construction gate now; when miri lands it slots into the empty
constraint slot (the family already carries docs/04's behaviour 0.70 / constraint 0.30 weights). Authoring
the family ahead of its full oracle is the deliberate call — the generator is the long pole, and a
gate-valid `unsafe-core` family in hand is worth more than a placeholder, provided the gap is on the
record. It is.

Registered in `FAMILY_IDS` and the `spec_diversity` pin (== 20) in the same commit. 131 workspace tests
(+6); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-20 · Seventh family: `bit-ops` (category `bit-manipulation`)

More corpus breadth (roadmap P7), and a deliberately *low-level* shape — no arithmetic fold anywhere. The
model implements `fn f(x: u32) -> u32` as a seed-selected two-stage bit pipeline: a **mask** (Identity /
KeepLow(n) / KeepHigh(n) / ClearLow(n) / SetLow(n)) then a **transform** (RotateLeft(k) / RotateRight(k) /
ReverseBits / SwapBytes). 5 × 4 = **20 distinct skills**; the width `n` (∈ {8,16,24}) and rotate amount `k`
(∈ 1..=31) are constant parameters of the same skill (Q31), excluded from `spec_signature` along with the
function name.

Solution-first / correct-by-construction (ADR-0003): native `eval` and the emitted reference use the same
`u32` methods (`rotate_left`, `reverse_bits`, `swap_bytes`, mask arithmetic); the differential fuzzes 3000
random `u32`. No overflow is reachable — every shift amount is < 32 and the bit ops wrap by definition,
which is the clean answer to the differential-overflow pitfall (docs/17) that the arithmetic families have
to bound their inputs against.

The trivial baselines are `identity` (returns `x`) and `const-zero` (returns `0`). They are caught on
*every* seed by a structural argument, pinned as a test: every transform is a **bijection that fixes zero**,
and no mask can zero the canonical input `0x9ABCDEF1` (it has a set bit in both the lowest and highest
positions), so the canonical result is provably never `0` and never equal to `x`.

**This family is textually healthy where the pinned-interface families are not** — a useful contrast with
yesterday's `trait-impl`/`stack-machine` entries. Because the solution is a short two-line function whose
*text* genuinely changes with the op (`rotate_left(7)` vs `swap_bytes()` share almost nothing), both text
proxies are high, not just the spec count:

```
view       min=0.392 median=0.500 near-twins 0/28
reference  min=0.400 median=0.750 near-twins 0/28   capacity=45
spec-diversity: 20 distinct skills
```

So the anti-memorisation numbers agree with each other here (view, reference *and* spec all comfortably
clear the floor), which they do **not** for a family whose fixed scaffolding dominates the text. Registered
in `FAMILY_IDS` and the `spec_diversity` pin (== 20) in the same commit. 125 workspace tests (+6);
`validate-family --seeds 8` all gates green; clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-20 · Sixth family: `trait-impl` (category `traits-generics`) — first uncovered core category

Growing the corpus (roadmap P7, the long pole). The five existing families cover `borrow-lifetimes`,
`error-handling`, `pattern-matching`, `iterators`, `data-structures`; of [docs/04](04-categories.md)'s
five **core** categories, only `borrow-lifetimes` (window-op) and `error-handling` were covered. This
adds **`traits-generics`** — core #2, the clearest uncovered gap.

The distinctive skill is deliberately *not* another fold-shaped problem: the model implements a **trait
with an associated type** for a provided unit struct, consumed by a **generic driver with a where-bound**.
The interface is pinned in the skeleton (like `error-handling` pins its enum): the trait `Aggregate {
type Item; fn keep(&self, &Self::Item)->bool; fn identity(&self)->i64; fn combine(&self, i64,
Self::Item)->i64; }`, the driver `fn drive<A: Aggregate<Item = i64>>(agg: &A, xs: &[i64]) -> i64`, and
the struct are all given; the model writes only the `impl … { type Item = i64; … }` block. Getting the
associated type or a method signature wrong makes the driver's `A: Aggregate<Item = i64>` bound
unsatisfiable, so the hidden tests fail to build and the answer scores **0.0** — which is exactly the
"L1 + signature match" oracle emphasis docs/04 assigns this category, enforced structurally rather than
by a bespoke check.

Seed-selected on two axes — **keep** (Positive / Even / NonNegative / Odd) × **reduce** (Sum / Product /
Count / SumOfSquares / SumOfAbs) = **20 distinct skills**. Solution-first and correct-by-construction
(ADR-0003): native `eval`, the emitted `impl`, and the differential's free reference are mirrored; the
differential fuzzes 3000 random slices (values ∈ -9..=9, len 0..12, so no debug overflow — the tightest
case is `Product` over eleven `9`s ≈ 3.1e10). `validate-family --seeds 8`, all five gates green on every
seed:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
spec-diversity: 20 distinct skills
distinct-skills epoch: served 8 seed(s) covering 8 distinct skills
```

**The trivial baselines are both constants — and that is deliberate, not lazy.** `const-zero` (returns 0)
and `const-one` (`identity`→1, keep→false → returns 1). Any *shaped* degenerate (sum-everything, length)
coincides with exactly one real spec and would pass on that seed, so a fixed shaped baseline can't be
universally wrong. The two constants are: the canonical worked example is the fixed input `[2, 3, 4, 5]`,
whose answer is provably never 0 or 1 under any of the 20 combos (a pinned test asserts it, and that every
keep predicate keeps ≥ 2 of its elements), so both baselines fail on every seed.

**Honest caveat, same phenomenon as `stack-machine`.** The pinned interface dominates the text, so the
*text* proxies are low: view-distance median 0.299 with 6/28 near-twin pairs, and reference-capacity **1**
(the trait+driver+impl boilerplate is ~90% of every solution; only three short method bodies vary). That
is the Q31 story again — text distance is deflated by shared scaffolding and is *not* the diversity
measure. The authoritative, ungameable number is spec-diversity = **20**, comfortably above any per-epoch
seed count, and the distinct-skills sampler serves them. Left the low text numbers reported honestly
rather than seed-varying the prompt harder to inflate a proxy (Q31 warns that lift is illusory).

Registered in `bench_gen::FAMILY_IDS` and the `spec_diversity` pin (== 20) in the same commit. 119
workspace tests (+11); clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-20 · Throughput in `stats` — the second headline number

The project's thesis is two numbers (capability + throughput), but `stats` reported only capability. Folded
throughput in, computed from the journal's `cost` fields.

- `bench_stats::Record` gained a `cost` sub-struct (prompt/completion tokens, `gen_ms`, `grade_ms`), serde-
  defaulted so older journals and synthetic records parse with zero timing.
- `throughput(records) -> Option<ThroughputReport>`: aggregate decode tok/s (completion tokens ÷ generate
  seconds — GPU-bound, so the model/hardware number), per-unit wall (gen + grade), grade share,
  **units/hour**, and **passes/hour** (docs/07's `throughput_score`, counting only scored *core* passes).
  Measured over *every executed unit* (core + probe both cost wall time), so it is computed before the
  core-only filter; `None` when no unit carried timing.
- `stats` now prints both headline numbers; wired into `StatReport.throughput`.

On the clean live run (`runs/clean.jsonl`, Qwen2.5-3B on Metal, VM already gone): **decode 53.5 tok/s,
3.8 s/unit (3.3 gen + 0.5 grade), grade 12% of wall, 945 units/hour, 157 passes/hour.** Matches the
hand-computed numbers exactly.

Test pins the arithmetic on a crafted cost-bearing journal (3 units → 50 tok/s, 1440 units/hr,
480 passes/hr) and that synthetic records yield `None`. Workspace 108; clippy/fmt clean.

Note recorded from the live data: grade time is dominated by whether tests *ran* (a compiling/passing
answer executes the 3000-iter differential ≈1 s; a compile-fail short-circuits ≈0.1 s), not primarily by
cache warmth — so the docs/08 "exclude first-N for cache warmth" refinement (needs `segment_position`) is
a smaller effect than the compile-success split, and is still future work.

---

## 2026-08-20 · Live smoke run (Qwen2.5-3B on Metal) — whole loop end-to-end, and it caught a bug

Ran the full pipeline against a real model for the first time: `llama-server` serving
`Qwen2.5-3B-Instruct-Q4_K_M` on Metal, a 12-unit epoch (`run-suite`, 4 families × 2 core + 1 probe),
then `stats` and `detect`. Resource note: a UTM/QEMU VM was at ~78% CPU, so this is a *capability/plumbing*
smoke — the 3B runs on the free GPU, but grading is CPU-bound and would contend, so throughput is not
reported as meaningful.

Everything ran end-to-end: the model answered every unit, each was graded in the sandbox, journalled with
epoch + core/probe labels, aggregated, and run through the detector. The 3B's honest capability profile:
`iterators` **1.000 / pass 1.0** (both core seeds — it writes concise correct functional code),
`borrow-lifetimes` 0.344, `error-handling` 0.222, `pattern-matching` 0.000; capability **0.392**,
pass-rate 0.25. Detector: not flagged (p=0.5), as expected for an honest run.

**The smoke caught a real correctness bug** — exactly what it is for. `stats` was scoring **12 units
including the fresh probe**, when ADR-0009 says the probe is *never* scored. `iterators` read 0.803 /
pass 0.667 (probe dragging it down) instead of the true core-only 1.000 / 1.000. Fixed: `report_with`
and `compare_models_with` now filter to `kind == "core"` — the one place that rule is enforced for
capability, pass-rate, CIs and ICC. The detector was already correct (it pairs only index-0 core+probe).
Re-ran: 8 scored units, `iterators` 1.000/1.0, capability 0.392. Pinned with a test
(`report_scores_core_only_never_probe`). Workspace 107; clippy/fmt clean. (Smoke journals live under the
git-ignored `runs/`.)

---

## 2026-08-20 · Closed the detector loop — `run-suite` → journal → `detect` verdict

The sign test was a pure statistic with no journal reader; wired it to real journal data, so the
precomputation detector now runs the full loop the run protocol emits.

- `bench_stats::Record` gained `kind` / `index` / `epoch` (serde-defaulted for older journals) so core
  and probe units can be told apart.
- `bench_stats::detect(records, α)`: per epoch, take each family's **pick-one** core bit (the index-0
  core unit — the only collapse preserving the null, Q29.1) and pair it with that family's index-0 fresh
  probe; run the one-sided `sign_test`; return a `DetectorReport` per epoch (families paired, core/probe
  wins, p, flagged). Non-index-0 core units are ignored (pick-one), epochs reported separately.
- CLI `rustybench detect --journal <j>`.

Demonstrated end-to-end on synthesised core+probe journals: a cheater (core passes / probe fails on all 8
families) is **FLAGGED** at p=0.0039 < 0.01 (core-wins 8/0); an honest run (core = probe) is clean at
p=1.0. On real data, `run-suite --seeds-probe N` emits the probe units and `detect` reads them — the
whole ADR-0009 precomputation defence now closes.

3 new tests (flag/clear, pick-one + epoch grouping, empty-when-no-probe); workspace 106; clippy/fmt
clean. The detector fires on real data as soon as a `run-suite` epoch executes against a model.

---

## 2026-08-20 · Run protocol — epoch orchestration with paired-core / fresh-probe + resume (P4)

First increment of the run protocol (docs/08, ADR-0009): serve a whole epoch across all families instead
of one task at a time, resumably.

- **Plan** (`bench_gen::epoch::plan_run`, pure/testable): per family, `n_core` paired-core seeds
  `blake3(epoch ‖ family ‖ i)` (identical for every submitter, scored) + `n_probe` fresh-probe seeds
  `blake3(probe_nonce(epoch) ‖ family ‖ i)` (the detector set, disjoint seed space). `RunUnit` carries the
  family, `UnitKind` (Core/Probe), index and derived seed, and a `key()` for dedup.
- **Resume** (`epoch::remaining` + `read_done_keys`): because units are idempotent (seed determines
  instance and grade), resuming is just "skip units whose `family|kind|index` key is already journaled for
  this epoch". Verified: seeding a journal with two done units drops them from the plan, and a different
  epoch label rotates all core seeds and shares nothing.
- **CLI** `rustybench run-suite --model … --epoch … [--seeds-core N] [--seeds-probe M] [--dry-run]`:
  plans, resume-filters, then runs each remaining unit through the single-unit grade path (extracted as
  `grade_and_line`, now shared with `run`). Journal lines gained `epoch` and `kind`. `--dry-run` prints
  the resume-filtered plan with no model calls — how this increment is demonstrated end-to-end without a
  server.
- `bench_gen::FAMILY_IDS` enumerates the registry (with a drift-guard test that every id resolves), so the
  suite serves all four families.

Demonstrated: a 12-unit epoch (4 families × (2 core + 1 probe)) plans deterministically; resume skips the
two pre-recorded units; epoch `2026-09` rotates every core seed. The only unexercised step is the live
model call, which reuses the same `grade_and_line` path `run` already exercises. 5 new tests (run-plan +
family-id drift); workspace 103; clippy/fmt clean.

Not yet: the variance/determinism probes and calibration segments of docs/08, `status`/`resume`
subcommands, and wiring the fresh-probe journal into the sign-test detector (needs this on real data).

---

## 2026-08-20 · `bench-stats` — ICC estimation (Q29.2) + CI clamping

Added the last piece of the Q29 estimation spec: intra-class correlation.

- **Estimator**: one-way random-effects ICC(1) from unbalanced ANOVA components
  (`icc_components` + `icc_from`), **clamped to [0,1]**. Returns `None` when not estimable — fewer than
  two families, or one seed per family (no within-family df to separate the two variance components).
- **Pooled + shrink**: a pooled ICC is formed by summing components across categories (families compared
  within their own category, so category effects cancel); each category's raw ICC is then
  **empirical-Bayes-shrunk** toward it by between-family df (`w = df_b/(df_b+τ)`, τ provisional). The
  shrink target falls back to the canonical `bench_invariants::ICC` (0.3) when nothing is estimable —
  single source of truth for the design assumption.
- **Diagnostic only**: `icc` and its derived `design_effect = 1+(m−1)·ICC` (floored at 1) are reported
  per category and pooled, but are **never** inputs to a CI — a bad or negative ICC cannot narrow a
  published interval, which was the Q29.2 failure mode. `validate` this via the code path: CIs come from
  the bootstrap, ICC sits beside them.
- **CI clamping**: scores/means/pass-rates live in [0,1] and the pass-rate difference in [-1,1], so a
  studentised percentile-t interval that overshoots the boundary (which it can near a boundary or with
  very few clusters) is reported as its intersection with the feasible set. A 2-family category now shows
  `[0.000, 1.000]` (maximally uncertain, and flagged directional-only) rather than a nonsensical
  `[-0.507, 1.520]`.

`stats` now prints per-category `icc`/`de` and a pooled-ICC line. Tests cover pure-between (ICC=1),
pure-within (ICC=0), and not-estimable (one seed per family → None). 19 crate tests (+4); workspace 98;
clippy/fmt clean. The stats layer's remaining gaps are both Q24-gated: shape-level resampling and
`icc_within_shape`.

---

## 2026-08-20 · `bench-stats` — paired McNemar + precomputation sign test

Added the two paired detectors from the Q29 decisions, plus the exact binomial-tail engine both need.

- **McNemar** (`mcnemar`) on the shared `passed` bits: discordant counts (A-only / B-only), the exact
  two-sided binomial p-value on the discordant split (continuity-corrected χ² reported alongside), and a
  normal-approx fallback above 1024 discordant pairs where `0.5^n` underflows.
- **`compare_models`**: aligns two models' journals by `task_id` (the paired design), runs McNemar, and
  adds a **paired, family-clustered studentised wild-bootstrap CI** on the pass-rate difference — the
  clustered interval doc 07 requires beside the discordant count. Wired as `rustybench compare
  --journal-a --journal-b`.
- **Sign test** (`sign_test`) for precomputation (Q29.1): one-sided upper-tail binomial on the
  `pick-one` core-vs-probe discordant split, flagging when the predictable core beats the fresh probe.
  Default detector α = 0.01 (an accusation wants a low false-positive rate). It is a pure statistic for
  now — it begins running on real data once the epoch protocol emits labelled fresh-probe units
  (ADR-0009).
- Exact-tail helper `binom_cdf_le_half` (stable pmf recursion, normal approx above 1024) with an
  A&S erf; unit-tested against known binomial CDF values.

Demonstrated end-to-end: `compare` over a 6-unit paired pair of journals prints discordant counts, the
exact p, and the Δ pass-rate with its paired CI (correctly *not* significant on 6 units — the tests
prove the significant path on larger synthetic data). 15 crate tests (+5); workspace 94; clippy/fmt
clean.

---

## 2026-08-20 · `bench-stats` — studentised the wild cluster bootstrap (coverage 0.90 → 0.95)

Upgraded the wild cluster bootstrap to the **studentised (percentile-t)** form. Each replicate now
computes not just the sign-flipped mean shift `Δ` but its *own* cluster-robust SE from the flipped
residuals (`e*_g = w_g e_g − n_g Δ`), forming a bootstrap t-pivot `t* = Δ / SE*`; the CI inverts those
t-quantiles: `[μ − SE·q_{1−α/2}, μ − SE·q_{α/2}]`. The t-pivot cancels the small-sample noise in the SE
estimate, which is what the raw percentile form couldn't do.

Re-measured the coverage simulation: **0.953 at G = 12**, essentially nominal 0.95, up from the raw
percentile's 0.90. Tightened the regression bound to ≥ 0.90 to lock the improvement in.

Recorded the honest limit: studentising **degenerates at 2 clusters** — the sign flips that carry the
signal also zero the bootstrap SE — so it is well-defined only above ~12 clusters. That is exactly why
sub-floor categories stay `directional_only`; the between-family-variance test now uses 6 families so it
is well-defined. Removed the now-unused raw `percentile_ci`. `bench-stats` 10 tests; workspace 89;
clippy/fmt clean. doc 07's few-clusters section updated to the studentised method.

---

## 2026-08-20 · `bench-stats` — wild cluster bootstrap (Q29.5)

Replaced the interim resample-families percentile bootstrap with the **wild cluster bootstrap**
(Cameron–Gelbach–Miller), the method the Q29.5 decision named for few-cluster coverage. It precomputes
each family's residual sum `e_g = Σ(y_i − μ)` and, per replicate, flips each `e_g` by an i.i.d.
Rademacher sign: `μ* = μ + (Σ_g w_g e_g)/N`. The replicate distribution carries the cluster-robust
variance, so it holds coverage where resampling few clusters under-covers — and it never manufactures
spread the residuals don't contain (a homogeneous category still gets a zero-width interval).

Validated by simulation, as Q29.5 requires: a new test generates clustered data with a mean-zero
population (cluster effects + within-cluster noise) and checks the 95% wild CI covers 0. **Measured
coverage 0.90 at G = 12 clusters**, up from the naive percentile's 0.84–0.92 under-coverage and close to
nominal 0.95. (Studentising the replicates would tighten it further toward 0.95 — a noted future
refinement.) Also added a test that the wild CI picks up between-family variance (a category split
1.0/0.0 gets a wide interval bracketing 0.5, not a collapsed one).

`bench-stats` now 10 tests; workspace 89; clippy/fmt clean. The `directional_only` flag and simultaneous
per-category α are unchanged — the wild bootstrap improves the interval, it does not change which
categories are rankable.

---

## 2026-08-20 · `bench-stats` — journal → capability, pass-rate, cluster-bootstrap CIs (P4)

With Q28/Q29 decided, built the crate they unblocked. `bench-stats` reads a JSONL journal and computes,
exactly per [07-statistics.md](07-statistics.md):

- **`capability_score`** = equal-weight mean of the per-category means (docs/04), not a pooled mean — a
  small probe category counts the same as a large core one.
- **pass-rate** from the structural `OracleVector::passed()` (Q28), never a threshold on the score.
- **CIs from the cluster bootstrap** (Q29): stratified family resampling (10k), the coarsest cluster
  available until shapes are labelled (Q24), flagged as a lower bound on width. Categories below a
  provisional family floor are marked **directional-only** (the honest home for the few-cluster
  under-coverage). Per-category CIs are **simultaneous** — Bonferroni `1 − α/K` (Q29.4). Deterministic
  RNG so the same journal yields the same CI.

Wired as `rustybench stats --journal <path>`. End-to-end on a synthetic 6-unit / 2-category journal:

```
capability_score = 0.610  [0.300, 0.765]  (95% overall CI, cluster bootstrap)
pass_rate        = 0.333  over 6 units in 2 categories
  borrow-lifetimes   score=0.507 [0.000, 0.760]  pass=0.333  fams=2 units=3  <-- directional-only
  error-handling     score=0.713 [0.600, 0.770]  pass=0.333  fams=2 units=3  <-- directional-only
```

The pass predicate visibly discriminates on real-shaped data: a behaviour-1.0 unit that failed the
allocation constraint, and one that used `unsafe` over budget, both correctly count as **not passed** —
the clone-everything case caught structurally, not by weighting. 8 crate tests; workspace 87; clippy/fmt
clean.

Not yet: the wild cluster bootstrap, shape-level resampling (Q24), ICC estimation, and the paired
McNemar / sign-test detectors — this increment is the load-bearing path those extend.

---

## 2026-08-20 · Q28 + Q29 decided — the pass predicate and the estimation spec

Worked through the two blocking statistical questions with the user and wrote the decisions into code and
[07-statistics.md](07-statistics.md).

**Q28 — the pass predicate is structural, not a threshold.** A task passes iff
`applied ∧ compiled ∧ behaviour == 1.0 ∧ (unsafe_ok ∧ paths_ok ∧ alloc_ok)`, where a constraint the family
did not declare is not a barrier and quality (clippy/fmt, L4) is excluded. Binary, weight-independent
(re-tuning composite weights cannot move pass rates — kills REVIEW-6's swept-cut 23.3% type-I), and
pre-registered because it defines "correct" rather than tuning a cutoff. A clone-everything answer fails
`borrow-lifetimes` by the alloc clause. Implemented as `OracleVector::passed()` in `bench-core` (+2 tests);
the continuous `capability_score` stays the headline, `passed` feeds only the six binary consumers.

**Q29 — the estimation spec, resolved around one reframe:** the published CI is *always* the cluster
bootstrap; the design-effect formula and ICC are sizing/diagnostic only (so a bad/negative ICC can't
narrow a published interval). Decisions written into doc 07:

- **Bootstrap unit = coarsest cluster** — shapes once labelled (needs Q24), family-level and flagged as
  under-covering until then; `idiom-refactor` is crossed → directional-only.
- **Few clusters → wild cluster bootstrap** (Cameron–Gelbach–Miller), coverage validated by simulation;
  categories below a cluster floor are directional-only (fixes the 92%/84% under-coverage).
- **ICC** — variance-components, clamped to [0,1], `design_effect = max(1, …)`, empirical-Bayes shrink
  toward pooled; diagnostic/sizing only.
- **Precomputation sign test → pick-one collapse** (core seed index 0): the only rule preserving the null
  (`any` 100% / `majority` 54% / `pick-one` 4.2% false-accusation).
- **Multiplicity → FWER**: simultaneous radar CIs at 1−0.05/11, Holm for pairwise model comparisons; FDR
  considered and rejected for a public leaderboard.

Both marked DECIDED in OPEN-QUESTIONS. Residual dependencies noted: the shape-level bootstrap and the
per-category cluster floor need the Q24 shape audit; the real ICC needs Phase 3.5. `bench-core` 14 tests.

---

## 2026-08-20 · Fourth family: `seq-transform` (category `iterators`)

A fourth family, authored against [17-authoring-families.md](17-authoring-families.md) with `stack-machine` as the
closest template: the model implements `transform(xs: &[i64]) -> Vec<i64>`, applying a **seed-selected
three-stage pipeline** — Filter (Positive / Even / AboveThreshold(t) / NonZero) → Map (Double / Negate /
Square / AddK(k)) → Terminal (Collect / RunningSum / DedupConsecutive). Native `eval` and the emitted
reference are mirrored one-to-one; the differential fuzzes 3000 random slices (values ∈ -9..=9, length
0..12, so no debug-build overflow); a fixed canonical case `[2, 5, 8]` is changed and rendered non-empty
by **every** combination — including every threshold `t` and constant `k` — so both trivial baselines
(`identity`, `empty`) fail on every seed. The structural surface is 4 × 4 × 3 = **48 distinct skills**,
the widest of the four families; `t`, `k` and the function name are excluded from `spec_signature` (Q31).

```
view       min=0.392 median=0.591 near-twins 0/28
reference  min=0.286 median=0.545 near-twins 0/28   capacity=43
spec-diversity: 48 distinct skills
distinct-skills epoch: served 8 covering 8 distinct skills
```

`cargo test -p bench-gen` — 40 passed (incl. `spec_diversity == 48`); `validate-family --seeds 8` — all
gates green; clippy `-D warnings` and `cargo fmt --check` clean.

---

## 2026-08-20 · Authoring guide (doc 17) + README refresh

With three families now sharing a stable pattern, distilled it into [17-authoring-families.md](17-authoring-families.md):
the `Generator` contract, the solution-first recipe (Spec → native `eval` → mirrored emitted reference →
skeleton → prompt → oracle), the worked-example rule (seed-vary + one guaranteed-non-trivial case), the
view-vs-spec-diversity distinction (Q31), the `validate-family` gate table, and the pitfalls the three
families already hit (`i64::MIN` literals, differential overflow, empty test targets). This is the G3
lever — a contributor copies the nearest of the three references and adapts it. Refreshed the README
status (it still described P0 / 14 tests) to the current 7 crates, 71 tests, three families, macOS
sandbox, and added doc 17 to the read-order table. Docs only.

---

## 2026-08-20 · Third family: `stack-machine` (category `pattern-matching`)

A third family in a new category, to keep testing whether solution-first generation generalises and to
cover a genuinely different Rust skill: an **exhaustive `match` over a provided enum** driving a
`Vec<i64>` stack. The model implements `run(program: &[Op]) -> Vec<i64>`; the `Combine` / `Map` /
`Reorder` operations have seed-selected semantics described in the prompt (5 × 4 × 2). Native `eval` and
the emitted reference are mirrored; the differential fuzzes 3000 random programs; the two baselines
(`const-empty`, `echo-pushes`) are both caught.

`validate-family --seeds 8` — all five construction gates green on every seed (reference 1.000, skeleton
fails, baselines caught, canary, determinism). Numbers:

```
view       min=0.349 median=0.453 near-twins 0/28
reference  min=0.000 median=0.207 near-twins 16/28   capacity=2
spec-diversity: 40 distinct skills
distinct-skills epoch: served 8 covering 8 distinct skills
```

**A recorded surprise that further validates Q31.** I built this family expecting *more* solution
diversity than `error-handling` (whose reference-capacity was 7). On the authoritative measure it has it
— **spec-diversity 40, the highest of the three**. But its *reference*-capacity came out **2**, *lower*
than error-handling's 7, because the fixed `match`/loop scaffolding dominates the solution text even more
than the parse-plumbing did. So the family with the *most* skills has the *lowest* reference-distance
proxy — a third, independent confirmation that reference-distance is a poor diversity measure and that
spec-diversity (the decided gate, Q31) is the right one. Left the honest number in the module doc rather
than tuning the family to make a misleading proxy look better.

Three families now span three categories (borrow-lifetimes, error-handling, pattern-matching) and three
distinct task shapes; solution-first generation holds for all three. 71 workspace tests; clippy/fmt
clean.

---

## 2026-08-20 · Epoch sampler serves distinct *skills* (Q31 follow-on)

The Q31 decision left one piece unbuilt: the epoch sampler served on view-distance, which post-finding is
near-vacuous, so within an epoch it did not guarantee distinct *skills* — it could spend several of its
`n` slots on the same skill with different constants. Built `plan_epoch_distinct_skills`: it rejects a
candidate whose `spec_signature` is already served (the point — cover different skills), and still
enforces view-distance for prompt freshness (contamination). It `Exhausted`s once the family's distinct
skills run out.

- `EpochPlan` now carries the served `specs` and a `distinct_skills()` count.
- A test cross-checks the serve-path against `spec_diversity`: asking window-op for 13 distinct skills
  exhausts at exactly **12**, its diversity. Plus determinism and unique-specs tests.
- `validate-family` demonstrates it: "distinct-skills epoch: served 6 seed(s) covering 6 distinct skills".

This surfaced a **new tension for Phase 4 / P3.5**, recorded under Q31: "one seed per skill per epoch"
maximises coverage, but the ICC / repeated-measures design may want *several* seeds of the same skill to
estimate within-skill variance. Different serving policies for different purposes; likely an epoch covers
distinct skills while a separate repeated-measures pass samples same-skill seeds. Not settled here — and
neither planner is wired into a run protocol yet, because there is no run protocol yet. 66 workspace
tests; clippy/fmt clean.

---

## 2026-08-20 · Q31 decided — spec-diversity as the authoritative task-diversity measure

Acting on the Q31 finding (previous entry), and on the "two gates" decision: added a
`spec_signature(seed)` method to the `Generator` trait — the structural choices that define the *skill*,
with numeric constants and identifiers excluded — plus `spec_diversity(gen, n)` counting distinct
signatures. That count is now the authoritative, ungameable diversity number, reported by
`validate-family` and pinned as a regression test.

Granularity: `AtMost(10)` and `AtMost(50)` share a signature (same skill, different constant); constants
still vary the prompt so they still count under the view (contamination) gate, they just don't inflate
diversity. Measured and pinned:

- `window-op`: **12** distinct skills (6 in-place ops × 2 strides)
- `error-handling`: **30** distinct skills (5 combines × 6 rule-types)

Both clear any plausible per-epoch seed count, and — unlike the text proxies — these numbers can be
neither inflated by example noise nor deflated by shared boilerplate. `validate-family` now prints, per
family: view-distance, reference-distance, view/reference capacity, and spec-diversity — the full honest
picture in one place. Q31 marked decided; the per-family diversity *floor* stays open (it depends on the
Phase-4 per-epoch seed count) and folds back into Q30. 63 workspace tests; clippy/fmt clean.

---

## 2026-08-20 · Widened `window-op` — and it exposed that the anti-twin metric is unreliable (Q31)

Set out to raise `window-op`'s tight capacity (8) with the same lever that "fixed" `error-handling`:
two new in-place ops (`Negate`, `AddConst(k)` — chosen because they change every generic window, so the
`identity` baseline stays caught; `Min`/`Max` avoided for the `i64::MIN` literal landmine) plus
seed-varied worked examples. All construction gates stayed green.

**Then measuring capacity two ways broke the earlier story.** The epoch sampler measures distance on the
prompt+skeleton — what the model sees. Seed-varying the examples makes every prompt textually distinct
*without changing the task*, so `window-op` saturated: 100 % of raw-index seeds accepted, view-capacity
effectively unbounded. Measuring on the **reference** (the solution) instead told the truth:

| family | view near-twins | view-capacity | reference near-twins | reference-capacity |
|---|---|---|---|---|
| window-op | 0/28 | saturated | 1/28 | **22** |
| error-handling | 0/28 | ~250–326 | **12/28** | **7** |

So `error-handling`'s "3 → 326" win from the previous entry was mostly illusory — its genuine
solution-diversity is **7**. View-distance over-counts (gameable by example noise); reference-distance
under-counts (deflated by shared boilerplate). Neither is task diversity, which is the structural spec
count. Written up as **[Q31](OPEN-QUESTIONS.md)** (BLOCKING) with three options; the honest correction is
recorded against Q30 and doc 04 too, rather than quietly overwriting the earlier numbers.

Changes made in response, all honest-by-construction:
- `validate-family` now prints **both** view- and reference-distance lines and **both** capacities, so
  the gap is visible in CI. Live: window-op `reference min=0.146 cap=22`; error-handling
  `reference min=0.094 near-twins 12/28 cap=7`.
- `bench_gen::epoch` gained `greedy_distinct_count` and `reference_capacity` (public, documented with the
  over/under-count caveats).
- Capacity regression tests re-pinned to **reference**-capacity: `window_op_reference_capacity_is_healthy`
  (≥18) and `error_handling_reference_capacity_is_low_by_design` (5..=12, a documented limitation, not a
  bug). New `view_distance_saturates_but_reference_distance_does_not` pins the Q31 finding itself.

`window-op` keeps the two new ops (genuine solution diversity, reference-capacity 22) and the seed-varied
examples (contamination-resistance). 62 workspace tests; clippy/fmt clean. The epoch sampler still serves
on view-distance — post-finding that means it rarely rejects, so which basis it should serve on is part
of the Q31 decision.

---

## 2026-08-20 · Widened `error-handling` (Q30 lever 2): capacity 3 → ~326

The sampler flagged `error-handling`'s distinct-at-floor capacity as **3** — unusable, below any
per-epoch seed count. Acted on it directly (Q30 lever 2: enlarge the variable surface):

- **Combine operations 3 → 5**: added `Count` (`acc + 1`) and `SumSquares` (`acc + n*n`). `Min`/`Max`
  were deliberately *not* added — their `i64::MIN` identity has no safe negative-literal form, and the
  emitted reference/tests print literals.
- **Validation rules 3 → 6**: added `AtLeast(b)`, `InRange(lo,hi)`, `Even`. Counting bound choices that
  is 12 distinct rule-instances, so 5 × 12 = 60 combine/rule logics (up from ~9).
- **Seed-varied worked examples**: the examples in the prompt and skeleton are now generated per seed
  (two varied numeric cases + a constructed rule-failure + a parse-failure + empty), where before they
  were four fixed lists. This also fixes a real gap — the old examples never illustrated a *rule*
  failure. Native `eval` and the emitted source stay mirrored, so the reference-passes and differential
  gates still hold.

Verified end to end — `validate-family --family error-handling --seeds 8`, all gates green:

```
reference=1.000  skeleton_behavior=0.000  baselines_caught=true  canary=true  determinism=true
anti-twin (prompt+skeleton): min=0.287 median=0.438 near-twin pairs (<0.25)=0/28
epoch sampler: served 8 pairwise-distant seed(s) (rejected 0 collision(s), min=0.287)
```

| `error-handling` | median | near-twins | distinct-at-floor capacity |
|---|---|---|---|
| before | 0.263 | 18/45 | 3 |
| after | **0.438** | **0/28** | **~326** |

**Honest caveat.** ~326 is *view*-capacity, and part of the lift is example-text variation that the
shingle metric rewards. That legitimately freshens prompts against exact-text recall but is not skill
diversity; the genuine distinct-logic surface is the 60 combine/rule combinations. Both clear any
per-epoch seed count, and acceptance ran ~22% (not ~100%), so it is not pure metric-gaming. The capacity
regression test was re-pinned to a stable lower bound (`error_handling_capacity_clears_a_per_epoch_count`,
seats 50) rather than the RNG-sensitive exact figure; `window_op_capacity_at_floor_is_8` stays exact
(window-op is a true structural ceiling). 61 workspace tests; clippy/fmt clean.

---

## 2026-08-20 · Distance-aware epoch sampler (Q30) — and it measured a hard capacity ceiling

Built `bench_gen::epoch`, the Q30 second-order fix: a run must not *serve* two near-twins even when a
family's average distance is healthy. `plan_epoch` draws candidate seeds in order and rejects any whose
model-view is closer than the floor to an already-accepted sibling, so the seeds it serves are
pairwise-distant by construction. It is deterministic in `(family, epoch, n, threshold)` — replay-safe —
and measures distance on the exact `view_of` string the CI gate uses, so a plan that clears the floor
here clears the gate. If a family cannot supply the requested count it returns `Exhausted` rather than
serving a twin. `MIN_INSTANCE_DISTANCE = 0.25` is now a named constant both the sampler and
`validate-family`'s near-twin gate read, so they cannot drift.

**Running it turned the soft near-twin count into a hard ceiling — the finding.** The sampler's
*distinct-at-floor capacity* (most pairwise-≥0.25 instances a family can seat) is far below what the
median implies, because pairwise-mutual distance is a much stronger constraint than average distance:

| family | median | near-twin pairs | **distinct-at-floor capacity** |
|---|---|---|---|
| window-op | 0.433 | 7/45 | **8** |
| error-handling | 0.263 | 18/45 | **3** |

`error-handling`'s capacity of **3** is below any usable per-epoch seed count: as designed it cannot
outlast repeated epochs without repeating an instance, so Q30's "enlarge the variable surface" lever is
effectively forced for it. `window-op`'s 8 is workable but tight. Both capacities are pinned as
regression tests (`window_op_capacity_at_floor_is_8`, `error_handling_capacity_at_floor_is_3`) so a
family change that moves them fails loudly rather than silently degrading anti-memorisation.

`validate-family` now closes the loop in one output: it reports median/near-twins (detection) **and**
the sampler's capacity (prevention). Live:

```
window-op:       near-twin pairs (<0.25)=7/45   · epoch sampler: capacity 8 … enlarge the variable surface (Q30)
error-handling:  near-twin pairs (<0.25)=18/45  · epoch sampler: capacity 3 … enlarge the variable surface (Q30)
```

Docs updated: Q3/Q30 (capacity table + the "capacity is the real ceiling" reframing),
[02-task-format.md](02-task-format.md) (distinct-at-floor capacity as the measure families are authored
against), [04-categories.md](04-categories.md) (`error-handling` capacity 3 → must be enlarged before it
ships). 7 new tests; workspace 59 tests; clippy/fmt clean.

---

## 2026-08-20 · Folded the Q3 variance finding into the design docs

The second-family measurement (below) produced a design-level conclusion that lived only in this build
log. Promoted it into the durable specs so the 272-family plan carries it:

- **[Q3](OPEN-QUESTIONS.md)** marked *partly resolved*. Records the settled half — solution-first
  seeding generalises in **correctness** across two different-shape families (both pass all five
  construction gates on every seed) — and the unsettled half: it does **not** generalise automatically
  in **variance**. Carries the distance table (`borrow-lifetimes` 0.433 / 7-of-45 vs `error-handling`
  0.263 / 18-of-45).
- **[Q30](OPEN-QUESTIONS.md)** opened for the mechanism the finding leaves undecided: anti-twin
  variance is per-family, not global, so a single `min_instance_distance` means different things across
  categories. Two prevention levers stated (per-category floor vs. forced variable surface), plus the
  second-order epoch-seed-collision problem and its fix (a distance-aware epoch sampler).
- **[02-task-format.md](02-task-format.md)** gained "Varying the four axes is necessary, not
  sufficient" — the rule that variance is measured per family, and that clearing the floor *on average*
  does not clear it *pairwise*.
- **[04-categories.md](04-categories.md)** notes that pinning the error enum makes `error-handling`
  correct-by-construction but naturally low-variance, cross-linking Q3/Q30.

No code changed; `bench-invariants` still green (the edits are prose, not stats-table cells).

---

## 2026-08-19 · P3 — second family (`error-handling`): seeding generalises, and two oracle bugs it caught

**The question this answers.** The docs' biggest open risk (Q3, R2-S6): does
solution-first seeding generalise beyond `window-op`'s in-place-mutation shape,
or collapse to cosmetic variation? A second family in a genuinely different
shape settles it.

**`error-handling`** (category `error-handling`): parse `&[&str]` into `i64`,
validate each against a seed-selected rule (non-negative / at-most-N / non-zero),
combine with a seed-selected operation (sum / product / sum-of-abs), returning
`Result<i64, ParseError>` with `?` propagation. Different machinery entirely —
error paths, a custom enum the model must keep, string parsing. Its constraint is
error-handling-specific: **no `.unwrap()` / `.expect()`** (propagate, don't
panic), which needed the AST forbidden-path check extended to match *method
calls*, not just type paths.

**Verdict: seeding generalises.** All five construction gates pass across every
seed — reference 1.000, skeleton fails, both trivial baselines caught (including
the `.unwrap()` one, proving the extended AST check), canary present, determinism.
The reference-passes-its-own-oracle gate confirms the generator is self-consistent
for this shape too.

**But the anti-twin numbers are a real, quantified caveat:**

| family | min | median | near-twins |
|---|---|---|---|
| window-op | 0.122 | 0.433 | 7/45 |
| **error-handling** | **0.042** | **0.263** | **18/45** |

Error-handling instances are markedly *more similar* — median 0.263 vs 0.433,
barely above the near-twin threshold. The cause is structural: the shared
boilerplate (the `ParseError` enum, the parse-propagate skeleton, the constraint
list) dominates the prompt, so the seed-varied part (op + rule) is a small
fraction of what the model sees. **Solution-first seeding generalises in
correctness but not automatically in variance** — a family with heavy fixed
scaffolding needs a larger variable surface, or a per-category anti-twin
threshold. That is a concrete design conclusion for the 272-family plan, and it
is the honest answer to Q3: *yes, but variance must be designed for, not
assumed.*

**Two oracle bugs, both found by running the second family — not by review.**

1. **Score inflation on non-conforming answers.** The live 3B rewrote the
   provided enum as *private* and dropped `#[derive(PartialEq)]`. The lib still
   compiled (a private enum is legal), so L1 passed — but the behaviour and
   differential test targets could not build against it (`E0603`, `E0369`), so
   they produced no summary, which the oracle recorded as `None`. The composite
   then renormalised over the *remaining* layers and scored the broken answer
   **1.000**. Fixed: a configured test that yields no summary is a **failure
   (0.0)**, never absent, and flags `behavior:no_summary` / `differential:no_summary`.
   The same answer now scores **0.222** — behaviour 0, with only the style
   constraint contributing. (That 0.222 for a non-functional-but-tidy answer
   reinforces the Q28 "should behaviour be a hard floor" question already on
   file; it is the documented weighting, not a new bug.)
2. **Empty test target treated as configured.** A family opts out of the alloc
   layer by leaving its target blank; the CLI wrapped `""` as `Some("")`, so the
   oracle ran `cargo test --test ""`, failed, and (after fix 1) scored it a
   constraint failure — which broke `error-handling` validation. Fixed: empty
   target → `None`.

Window-op could not have surfaced either: its answers conform to a fixed
signature with no enum, so the interface-mismatch path was never exercised. This
is precisely why a second family of a *different shape* was the right next step.

52 tests across seven crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · P3 — anti-twin distance + trivial-baseline gates

**What.** The gate that actually tests the project's central claim — that
generated instances resist memorisation — plus two more construction gates, and
a strengthened `window-op`.

`window-op` gained a fourth operation (RotateRight(k)) and a **stride** modifier
(every window / every other window), taking the distinct-logic space from 3 to
~16, so a seed now varies genuinely different logic. The reference-passes gate
confirms every one of these still grades to 1.0 — a buggy new variant would have
been caught there.

`validate-family` now runs **five gates** per seed and reports the anti-twin
distance:

```
seed 0..9: OK  determinism=true reference=1.000 skeleton_behavior=0.000 baselines_caught=true canary=true
anti-twin (prompt+skeleton): min=0.122 median=0.433 near-twin pairs (<0.25)=7/45
```

- **Trivial-baseline gate** — `const-zero` and `identity` (right signature, wrong
  body) must each fail. They do: the oracle is not fooled by a right-shaped
  answer. This is the check that a family's oracle is not trivially beatable.
- **Canary gate** — the prompt carries its per-instance canary.
- **Anti-twin distance** (normalised token-shingle Jaccard over prompt+skeleton,
  the R4-S4 metric): reported, not hard-gated, because a finite task space
  eventually collides by pigeonhole. The useful signals are the *minimum*
  distance and the near-twin count.

**The measurement earns its keep immediately.** Median 0.433 says most seed-pairs
are genuinely different tasks — the anti-memorisation property holding. But
**7 of 45 pairs are near-twins** (min 0.122): two seeds landing on the same
(operation, stride) produce prompts differing only in the function name. That is
a real, quantified limitation of a ~16-variant family, and it drives a design
conclusion the docs should absorb: **an epoch's N seeds within a family should be
chosen (or the structural axis derived from the seed index) to guarantee N
distinct variants**, rather than trusting independent hashing not to collide.
The gate turned "does generation resist memorisation?" from an assertion into a
number.

47 tests across seven crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · P3 (first increment) — bench-gen: seeded solution-first generation

**What.** The pivotal phase begins: tasks stop being frozen files and become
**seeded generators**. A family is a function from a seed to a fresh instance,
its reference, its oracle, and the skeleton — all built from the same seed,
solution-first, so the oracle is correct by construction (ADR-0003).

`bench-gen` provides seed derivation (blake3), canary minting, a SplitMix64 PRNG
(pure in the seed), the `Generator` trait, and one real family: **`window-op`**
(category `borrow-lifetimes`). Crucially the *operation itself* is seed-selected
— Reverse / RotateLeft(k) / SwapEnds — so a seed changes the logic a model must
write, not just identifiers. From the sampled op the generator derives the
reference source, the worked example outputs (computed natively so they are
correct), the differential oracle (embedded reference + generated inputs), the
alloc test, and the ablated `todo!()` skeleton.

**`rustybench validate-family` runs the construction gates** (ADR-0003), over
every seed and with no model:

```
seed   0..7: OK  determinism=true  reference=1.000  skeleton_behavior=0.000
all gates passed
```

- **Determinism** — same seed → byte-identical instance.
- **Reference passes its own oracle (1.000)** — the synthesised reference,
  graded through the *full* pipeline (compile · behaviour · differential ·
  alloc · unsafe), scores perfect. This is the load-bearing self-consistency
  check: reference, example outputs and differential all agree, so a buggy
  generator is caught here, not in the field.
- **Skeleton fails (behaviour 0.0)** — the ablation genuinely removed the answer.

**Full loop against the live 3B, on tasks it has never seen.** `rustybench run
--family window-op --seed N` generates → prompts the model → grades under the
sandbox. Two seeds, genuinely different operations:

```
seed 3  op=reverse       fn=rework_segments  score 0.751  unit 0.6  diff 0.0  logic
seed 5  op=rotate_left(3) fn=map_frames       score 0.751  unit 0.6  diff 0.0  logic
```

Different operations and names — the anti-memorisation property. The 3B wrote a
windowed reverse whose `(w..v.len()).step_by(w)` loop **misses the last full
window** — a real off-by-one, caught partially by the unit tests (3/5) and fully
by the differential oracle (which is exactly why the differential oracle exists).
The identical 0.751 is coincidental.

**Scope of this increment:** one parametric family, three construction gates. Not
yet: the compositional archetype, the anti-twin distance gate, the remaining
`validate-family` checks (trivial-baselines-fail, prompt has one canary), and the
seed→instance server issuance the integrity model needs. 43 tests across seven
crates; clippy clean under `-D warnings`; fmt clean.

---

## 2026-08-19 · bench-invariants — the doc-arithmetic CI gate (round 6's ask)

**What.** Round 6 asked for "one script that recomputes every published table
from its stated formulae and fails CI on a hand-edited cell", because the same
defect class recurred across three rounds: R1-S1 (statistics tables
arithmetically wrong), round 5's arithmetic block, and R6-S1/R6-S7 (`standard`
published two different CIs for one quantity). This is that gate.

`bench-invariants` holds the canonical parameters (corpus 5×40 + 6×12, ICC 0.3,
per-suite seeds, the timing split) and the formulae (`design_effect`, `ci_pct`,
the stratified `overall_ci`, `F/ICC` ceiling) in **one place**. Four tests parse
the actual markdown tables in `07-statistics.md` and `04-categories.md` and
assert every cell follows from the math: suite families/seeds/units, pooled
eff N, overall CI, per-category CIs, and wall-clock hours (unit-aware — smoke is
in minutes); the 272 = 248 + 24 corpus split; the 44.5-hour headline (with a
guard that a stale "39 hour" cannot reappear); and the precision ceiling.

**Proven both directions.** Passes clean on the current docs. Then I corrupted
`deep`'s overall CI to **±4.1%** — which is *exactly* the stale pre-R6 value —
and the gate failed with `deep overall CI: doc says 4.1, formula gives 4.8542`.
It catches the precise drift round 6 found, and it runs under the normal
`cargo test`, so a hand-edited cell now breaks CI.

The canonical model is now the source of truth: when Phase 3.5 measures ICC, it
changes in `bench-invariants` and every doc table must move with it or the build
fails. 36 tests across six crates; clippy clean under `-D warnings`; fmt clean.

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
