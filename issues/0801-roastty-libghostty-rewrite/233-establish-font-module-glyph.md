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

# Experiment 233: Establish the `font` Module and Port `Glyph`

## Description

Begin the font subsystem by establishing a `roastty/src/font/` module and
porting upstream `font/Glyph.zig` — the value type describing a single
rasterized glyph: its pixel size, bearings, and top-left position in the glyph
atlas. `Glyph` is a leaf type the atlas/face/shaper produce and the
cell-rendering path consumes, so it is the natural foundation for the rest of
the font stack.

This is intentionally a **small foundation slice** (allowed by the issue's
granularity policy for correctness-critical foundations): it stands up the new
module tree and a pure value type with no behavior, so later font experiments
(metrics, atlas, faces, shaping) have a place to attach.

### Type to port

Upstream `font/Glyph.zig`:

```
width: u32,      // glyph width in pixels
height: u32,     // glyph height in pixels
offset_x: i32,   // left bearing
offset_y: i32,   // top bearing
atlas_x: u32,    // top-left corner x in the atlas (normalize to 0..1 before shader use)
atlas_y: u32,    // top-left corner y in the atlas
```

`offset_x`/`offset_y` are **signed** (`i32`) bearings (a glyph can sit left of
or above its pen origin); `width`/`height`/`atlas_x`/`atlas_y` are `u32`. The
atlas coordinates are raw pixel positions, normalized to `0..1` only at
shader-upload time (a later slice).

### Scope and faithfulness notes

- Field names, types, and order mirror upstream exactly.
- `Glyph` derives `Debug, Clone, Copy, PartialEq, Eq`; fields are `pub`
  (crate-visible through the `pub(crate)` struct) since the atlas and renderer
  will read them.
- `roastty/src/font/` is a new module tree; `mod font;` joins the existing
  `input`/`renderer`/`terminal` declarations in `lib.rs`. The module is internal
  (no C ABI surface).
- No font rasterization, atlas, face, metrics, or CoreText code — those are
  later experiments. This slice is the module + the `Glyph` data type only.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. Create `roastty/src/font/mod.rs`:
   - Module-level `#![allow(dead_code)]` with a "consumed by later font/renderer
     slices" comment, and an "upstream `font/`" attribution (no literal
     `ghostty` token).
   - `pub(crate) mod glyph;`.

2. Create `roastty/src/font/glyph.rs`:
   - `pub(crate) struct Glyph { pub width: u32, pub height: u32, pub offset_x: i32, pub offset_y: i32, pub atlas_x: u32, pub atlas_y: u32 }`
     (`Debug, Clone, Copy, PartialEq, Eq`) with the upstream field doc comments.

3. Wire `mod font;` into `roastty/src/lib.rs` alongside
   `mod input; mod renderer; mod terminal;`.

4. Tests in `roastty/src/font/glyph.rs`:
   - `glyph_holds_fields`: construct a `Glyph` and read every field back.
   - `glyph_offsets_are_signed`: a `Glyph` with negative `offset_x`/`offset_y`
     round-trips (confirms the signed `i32` bearings).

5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty font
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the `font` module exists and is wired from `lib.rs`;
- `Glyph` mirrors upstream field names, types (including the signed `i32`
  bearings), and order;
- the construction and signed-offset tests pass;
- no rasterization/atlas/face/metrics scope is pulled in;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `Glyph` turns out to need an additional field
once the atlas/face slice lands.

The experiment **fails** if a field type diverges from upstream (e.g. an
unsigned bearing), if font behavior leaks into this slice, or if any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no issues**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-081131-530226-prompt.md`
- Result: `logs/codex-review/20260602-081131-530226-last-message.md`

Codex confirmed the field names, order, and types match upstream exactly (signed
`i32` bearings, `u32` dimensions/atlas coordinates), that the `font/mod.rs` +
`font/glyph.rs` tree with internal `mod font;` wiring is appropriate, that the
slice — though thin — is acceptable as a foundation slice (module namespace +
leaf value type, no rasterization/atlas/face/shaping), that the raw-pixel
atlas-coordinate note matches upstream, and that the two tests are adequate for
a pure value type. No changes required.
