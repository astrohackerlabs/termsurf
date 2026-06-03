+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 269: Collection scale factor — match a face to the primary

## Description

When a fallback face has a different design size than the primary face, the
Collection scales it so a chosen metric (ideograph width, ex/cap height, or line
height) matches the primary — the `font-size-adjust` behavior. This experiment
ports `SizeAdjustment` and the `scaleFactor` metric-matching computation
(`font/Collection.zig` lines 571–664). It is pure `f64` math over two
`FaceMetrics`, fixture-testable with no FFI. Storing the factor per entry and
applying it via `setSize` (which needs `load_options`) is a later experiment.

## Upstream behavior (`font/Collection.zig`)

- `SizeAdjustment` (lines 571–582):
  `none | ic_width | ex_height | cap_height | line_height`.
- `scaleFactor(face_metrics, adjustment)` (lines 592–664):
  - `none` → `1.0`.
  - normalize both faces from px to ems: `primary_scale = 1/primary.px_per_em`,
    `face_scale = 1/face.px_per_em`.
  - choose the metric to match by a **fall-through chain** starting at
    `adjustment`: `ic_width → ex_height → cap_height → line_height`. At each
    step, if the **face** does not _validly_ define that metric (its raw field
    differs from its effective accessor — i.e. it's null or ≤ 0), fall through
    to the next; `line_height` always succeeds (no raw field).
  - the chosen `primary_metric = primary.<metric>() * primary_scale`,
    `face_metric = face.<metric>() * face_scale` (the effective accessors), and
    return `primary_metric / face_metric`.

## Rust mapping (`roastty/src/font/collection.rs`)

- `enum SizeAdjustment { None, IcWidth, ExHeight, CapHeight, LineHeight }`.
- `fn scale_factor(primary: &FaceMetrics, face: &FaceMetrics, adjustment: SizeAdjustment) -> f64`:
  - `None` → `1.0`.
  - `primary_scale = 1.0 / primary.px_per_em`,
    `face_scale = 1.0 / face.px_per_em`.
  - resolve the metric via the fall-through, where "the face validly defines X"
    is `face.<field>.is_some_and(|v| v > 0.0)` (the faithful equivalent of
    upstream's `raw != effective` — the effective accessor returns the field
    only when it's `Some(> 0)`, else an estimate):
    - `IcWidth`: use it if `face.ic_width.is_some_and(|v| v > 0.0)`, else fall
      to `ExHeight`;
    - `ExHeight`: use it if `face.ex_height.is_some_and(|v| v > 0.0)`, else
      `CapHeight`;
    - `CapHeight`: use it if `face.cap_height.is_some_and(|v| v > 0.0)`, else
      `LineHeight`;
    - `LineHeight`: always use it.
  - the matched `(primary_metric, face_metric)` use the effective accessors:
    `effective_ic_width`/`effective_ex_height`/`effective_cap_height`/`line_height`
    (already on `FaceMetrics`), each times its scale; return
    `primary_metric / face_metric`.

The fall-through is modeled as a small loop that advances `adjustment` past
invalid metrics, then a `match` selecting the effective values.

## Scope / faithfulness notes

- **Deferred**: the `Collection` integration — caching the primary face's
  metrics (`primary_face_metrics`, loaded from face index 0), storing a
  per-entry `scale_factor`, and applying it via `Face::setSize` (which needs
  `load_options`). This experiment is the pure computation, taking both metrics
  explicitly (so the primary-face loading and its `→ 1.0` fallback are the
  caller's concern later).
- The fall-through uses the **face**'s metric validity (matching upstream), and
  `line_height` is the always-valid terminus.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/collection.rs`: add `SizeAdjustment` and `scale_factor`;
   import `crate::font::metrics::FaceMetrics`.
2. Tests in `collection.rs` (pure, fixture `FaceMetrics`):
   - `scale_factor_none_is_one`: `None` → `1.0` for any metrics.
   - `scale_factor_same_metrics_is_one`: identical primary/face metrics → `1.0`
     for every adjustment.
   - `scale_factor_line_height`: two faces with known
     `px_per_em`/ascent/descent/ line_gap → the computed ratio
     `(primary.lh/primary.ppem)/(face.lh/face.ppem)`.
   - `scale_factor_falls_through`: a face with `ic_width = None` (or `≤ 0`)
     under `IcWidth` falls through to `ex_height` (assert the result equals the
     `ExHeight` computation, not the `IcWidth` one), and a face with all of
     ic/ex/cap absent falls all the way to `line_height`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty collection
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `scale_factor` returns `1.0` for `None` and identical metrics, the correct
  px→em-normalized ratio for `line_height`, and falls through invalid face
  metrics in the `ic_width → ex_height → cap_height → line_height` order;
- the chosen metric uses the effective accessors and the
  `primary_metric / face_metric` form;
- the Collection integration (primary caching, per-entry storage, `setSize`) is
  cleanly deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the fall-through validity check needs a
different shape than `is_some_and(|v| v > 0.0)` to match upstream.

The experiment **fails** if the metric-matching math or the fall-through order
diverges from upstream, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-223440-515842-prompt.md`
- Result: `logs/codex-review/20260602-223440-515842-last-message.md`

## Result

**Result:** Pass

`collection.rs` gained `SizeAdjustment` and `scale_factor`: `None` → `1.0`; else
both faces are px→em normalized and the chosen metric falls through
`ic_width → ex_height → cap_height → line_height` (using each face field's
`is_some_and(|v| v > 0.0)` validity), returning `primary_metric / face_metric`
from the effective accessors.

Tests (pure fixture `FaceMetrics`):

- `scale_factor_none_is_one` — `None` → `1.0`.
- `scale_factor_same_metrics_is_one` — identical metrics → `1.0` for every
  adjustment.
- `scale_factor_line_height` — the computed em-normalized ratio for two faces of
  different sizes.
- `scale_factor_falls_through` — `IcWidth` with an undefined `ic_width` falls
  through to `ex_height` (and differs from the ic estimate); all-of-ic/ex/cap
  absent falls to `line_height`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty collection` → 30 passed, 0 failed.
- `cargo test -p roastty` → 2416 passed, 0 failed (no regressions; +4).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The `font-size-adjust` metric-matching computation is ported and fixture-pinned.
The remaining `Collection` work is the **integration**: an `Entry`
`scale_factor` field, an `AddOptions { size_adjustment }` so `add` computes it
(caching the primary face's metrics, loaded from index 0, with the `→ 1.0`
fallback), and the application via a `Face::set_size` / `load_options` path
(which also brings in `setSize`/`updateMetrics`). Beyond the Collection: the
`DeferredFace` + `discovery` lazy-loading sub-area, the `CodepointResolver`, the
shaper, and the Nerd Font attribute table.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-223826-671253-prompt.md`
- Result: `logs/codex-review/20260602-223826-671253-last-message.md`

Codex confirmed `scale_factor` matches upstream's fall-through order using the
candidate face's explicit-metric validity with `LineHeight` as the terminus,
that the px→em normalization and `primary_metric / face_metric` formula are
correct and use the same effective-accessor semantics, that the `unreachable!()`
arms are truly unreachable (`None` returns before the loop and `adj` is never
set to `None`), and that the tests are meaningful (the fall-through test proves
`IcWidth` → `ExHeight` rather than the ic estimate, and the all-the-way fallback
to `LineHeight`).

Codex confirmed the design matches upstream: validity is based on the
**candidate face** metric (not the primary), the fall-through order is
`ic_width → ex_height → cap_height → line_height` with `line_height` as the
unconditional terminus, and the return formula compares normalized em-space
metrics
`(primary_effective / primary.px_per_em) / (face_effective / face.px_per_em)`.
Using the effective accessors for the chosen metric is faithful to upstream's
`icWidth()`/`exHeight()`/`capHeight()`/`lineHeight()`, and the
`is_some_and(|v| v > 0.0)` validity check matches the ported accessor semantics
(explicit positive value vs estimated fallback).
