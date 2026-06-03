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

# Experiment 270: Collection scale-factor integration — add with size adjustment

## Description

Experiment 269 ported the `scaleFactor` computation; this experiment integrates
it into the `Collection`. When a fallback face is added with a size adjustment,
the Collection computes the factor against the primary face, **resizes** the
face, and records the factor on its `Entry` — the `add` half of upstream's
size-adjustment flow (`font/Collection.zig` lines 112–150). The `Face::setSize`
primitive is ported alongside.

### Upstream behavior (`font/Collection.zig` / `face/coretext.zig`)

- `Entry.scale_factor` holds the resolved scale (a
  `union { adjustment, scale }`; in the eager path it's the computed `scale`).
- `add` (lines 125–149):
  `scale_factor = self.scaleFactor(face.getMetrics(), opts.size_adjustment)`; if
  load options exist,
  `new_opts.size.points *= scale_factor; face.setSize(new_opts)`; append the
  entry with `.scale_factor = .{ .scale = scale_factor }`.
- `scaleFactor` (Experiment 269) caches `primary_face_metrics` from face index 0
  and returns `1.0` if the primary can't be loaded.
- `Face.setSize` (`coretext.zig`): create a copy at the new size and replace
  `self` (`copyWithAttributes(size, null, null)`).

### Rust mapping

1. **`Face::set_size(&mut self, points: f64)`** (`face/coretext.rs`): replace
   `self.font` with `copy_with_attributes(points, null, None)` (a copy at the
   new size), re-running color detection and **preserving** `synthetic_bold`.
   (Faithful to upstream's `setSize`, which passes a null matrix; synthetic
   faces are never resized in the realistic flow.)
2. **`Entry.scale_factor: f64`** (`collection.rs`) with a `scale_factor()`
   accessor; the existing `add` sets it to `1.0` (no adjustment).
3. **`Collection.primary_face_metrics: Option<FaceMetrics>`** cache; a
   `compute_scale_factor(&mut self, face: &FaceMetrics, adjustment) -> f64`:
   `None` → `1.0`; lazily load the primary face's metrics from
   `get_face(Index::default())` (face index 0) — if that errors, return `1.0` —
   cache them, then call the pure `scale_factor(primary, face, adjustment)`.
4. **`add_with_adjustment(&mut self, face: Face, style: Style, fallback: bool, adjustment: SizeAdjustment) -> Result<Index, AddError>`**:
   compute the factor from `face.get_metrics()` and push
   `Entry { face, fallback, scale_factor: factor }` (with the same
   `CollectionFull` guard as `add`). It does **not** physically resize the face
   — upstream resizes to the **collection load size × factor**, and this eager
   slice has no collection size (`load_options`), so the resize is deferred to
   the future `load_options`/`setSize` path. (The factor is size-independent and
   is correctly recorded now.)

The existing `add(face, style, fallback)` is **unchanged** (it stores
`scale_factor = 1.0`), so the many existing callers and tests are untouched.

### Scope / faithfulness notes

- **Deferred**: the **physical resize** of the added face (upstream resizes to
  the collection load size × factor; this slice has no collection size, so it
  records the factor and the resize lands with `load_options`/`setSize`), the
  deferred-face lazy scale resolution (`getFaceFromEntry` recomputing the factor
  on load — the `DeferredFace` sub-area), and
  `updateMetrics`/`metric_modifiers`. This is the **eager** add-with-adjustment
  path: compute and record the factor.
- `set_size` is ported here as the resize primitive (tested standalone) so the
  future `load_options` path can apply the recorded factor.
- `set_size` mirrors upstream's null-matrix `setSize`; preserving
  `synthetic_bold` keeps a resized synthetic-bold face faithful.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/face/coretext.rs`: add `set_size`.
2. `roastty/src/font/collection.rs`: add `Entry.scale_factor` + accessor, the
   `primary_face_metrics` field, `compute_scale_factor`, and
   `add_with_adjustment`; `Collection::new` initializes the cache to `None` and
   `add` sets `scale_factor = 1.0`.
3. Tests:
   - `add_with_adjustment_none_is_unscaled` (live): add Menlo `Regular` (index
     0), then `add_with_adjustment` a second Menlo with `SizeAdjustment::None`;
     its `Entry::scale_factor()` is `1.0`.
   - `add_with_adjustment_same_font_is_one` (live): with Menlo as primary, add
     another Menlo via `LineHeight`; the factor is `≈ 1.0` (same metrics) — the
     integration computed against the primary without error.
   - `add_with_adjustment_distinct_font_scales` (live): with Menlo as primary,
     `add_with_adjustment` a different family (`Helvetica`) with `LineHeight`;
     assert the factor is finite, positive, and **differs from `1.0`** (the
     proportional face has a different em-normalized line height than monospace
     Menlo), proving the primary was loaded and used.
   - `plain_add_scale_factor_is_one` (live): a face added via `add` has
     `Entry::scale_factor() == 1.0`.
   - `set_size_resizes` (live): `Face::new("Menlo", 32.0)`, `set_size(20.0)`;
     `size()` ≈ `20.0` and `'M'` still resolves/renders.
   - `set_size_preserves_synthetic_bold` (live):
     `Face::new_synthetic_bold("Menlo", 32.0)`, then `set_size(24.0)`;
     `synthetic_bold_width()` is still `Some` and `size()` ≈ `24.0`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty collection
cargo test -p roastty face
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `add_with_adjustment` computes the scale factor against the primary face (with
  the `→ 1.0` no-primary fallback) and records it on the `Entry` (the physical
  resize is deferred with the collection size); `add` stores `1.0`;
- `set_size` replaces the face's `CTFont` at the new size, re-detecting color
  and preserving `synthetic_bold`;
- the existing `add` callers/tests are unchanged;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `compute_scale_factor`'s primary-load borrow or
the `set_size` matrix handling needs a different shape.

The experiment **fails** if the scale computation, resize, or per-entry
recording diverges from upstream, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Medium**
finding: resizing the added face to `face.size() * factor` bakes in
construction-time size, whereas upstream resizes to the **collection load size ×
factor** (the size-independent factor times the collection's size). The design
was revised so `add_with_adjustment` only **computes and records** the factor on
the `Entry` and explicitly **defers the physical resize** to the future
`load_options`/collection-size path; `set_size` is still ported as the resize
primitive (tested standalone), and the integration tests verify factor recording
(None→1.0, same-font→≈1.0, a distinct family→finite/positive/≠1.0 proving the
primary was loaded, plain `add`→1.0). Codex's re-review confirmed the finding is
resolved and approved the design (the `compute_scale_factor` borrow is sound,
`Index::default()` is the right primary index, and the `→ 1.0` no-primary
fallback matches upstream).

Review artifacts:

- Prompts: `logs/codex-review/20260602-224149-302529-prompt.md`,
  `…-224343-021541-prompt.md`
- Results: `logs/codex-review/20260602-224149-302529-last-message.md`,
  `…-224343-021541-last-message.md`

## Result

**Result:** Pass

`Face::set_size` (resizing copy, re-detect color, preserve `synthetic_bold`)
landed. `Entry` gained `scale_factor` (+ accessor); `Collection` gained the
`primary_face_metrics` cache, `compute_scale_factor` (lazy primary load from
index 0, `→ 1.0` on no primary), and `add_with_adjustment` (computes + records
the factor, resize deferred). `add` records `1.0`.

Tests (live CoreText):

- `set_size_resizes` / `set_size_preserves_synthetic_bold` — resize works and
  the bold marker survives with its width **recomputed** for the new size (the
  result-review **Low** fix below).
- `plain_add_scale_factor_is_one` — `add` records `1.0`.
- `add_with_adjustment_none_is_unscaled` — `None` → `1.0`.
- `add_with_adjustment_same_font_is_one` — same-font `LineHeight` → ≈ `1.0`.
- `add_with_adjustment_distinct_font_scales` — Menlo primary + Helvetica
  fallback `LineHeight` → a finite, positive factor `≠ 1.0`, proving the primary
  was loaded and the computation ran against it.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2422 passed, 0 failed (no regressions; +6).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The `Collection`'s size-adjustment factor is computed and recorded per entry,
and `Face::set_size` is ready to apply it. The `Collection`'s remaining work is
the **collection load size** (so the recorded factor can physically resize a
face via `set_size`) plus `setSize`/`updateMetrics`/`metric_modifiers`. That,
together with the **`DeferredFace` + `discovery`** lazy-loading sub-area
(CoreText font matching), is the last of the Collection. Above it: the
**`CodepointResolver`** (sprite/box routing over `Collection.getIndex`), the
**shaper**, the **Nerd Font attribute table**, and **SVG color detection**.

## Completion Review

Codex reviewed the completed implementation. It raised one **Low** finding:
`Face::set_size` carried the stale 32pt-derived `synthetic_bold` stroke width
forward after a resize. The fix recomputes the width from the new size
(`Some((points / 14).max(1))`) when the face was synthetic-bold — a documented
improvement over upstream's `setSize` (which drops `synthetic_bold` but never
resizes synthetic faces) — and the `set_size_preserves_synthetic_bold` test now
asserts the exact recomputed width. A follow-up review confirmed the finding is
**fully resolved** with no new issues (the borrow soundness of
`compute_scale_factor`'s lazy primary load, the no-resize factor recording, and
the distinct-font test all stand).

Review artifacts:

- Prompts: `logs/codex-review/20260602-224641-571205-prompt.md`,
  `…-224812-081261-prompt.md`
- Results: `logs/codex-review/20260602-224641-571205-last-message.md`,
  `…-224812-081261-last-message.md`
