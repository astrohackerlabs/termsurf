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

# Experiment 252: CoreText `Face` glyph-measurement accessors

## Description

The remaining FFI for `Face::get_metrics`: the glyph-measurement accessors that
produce `cell_width`, `ascii_height`, and `ic_width`. `getMetrics` maps the
printable ASCII characters to glyphs, takes the **max horizontal advance** as
the cell width and the **overall bounding-box height** as the ASCII height, and
measures the `水` (or `H`) glyph for the ideographic width. This slice adds the
three `CTFont` array methods behind ergonomic `Face` wrappers, with a live Menlo
test. The full `get_metrics` assembly (the fallback chains + `FaceMetrics` +
`Metrics::calc`) is the next slice; glyph rasterization follows that.

### The objc2 API (verified, `objc2-core-text` 0.3.2 / `objc2-core-graphics`

0.3.2)

All `unsafe` on `CTFont`; `UniChar` and `CGGlyph` are `u16`, `CFIndex` is
`isize`, `CGSize`/`CGRect` come from `objc2-core-foundation` (CFCGTypes), and
`CGGlyph` from `objc2-core-graphics`:

- `glyphs_for_characters(&self, characters: NonNull<UniChar>, glyphs: NonNull<CGGlyph>, count: CFIndex) -> bool`
  — fills `glyphs` (one `CGGlyph` per input char; `0` = no glyph).
- `advances_for_glyphs(&self, orientation: CTFontOrientation, glyphs: NonNull<CGGlyph>, advances: *mut CGSize, count: CFIndex) -> c_double`
  — fills per-glyph advances (`CGSize.width` is the horizontal advance) and
  returns the total; pass `CTFontOrientation::Horizontal`.
- `bounding_rects_for_glyphs(&self, orientation: CTFontOrientation, glyphs: NonNull<CGGlyph>, bounding_rects: *mut CGRect, count: CFIndex) -> CGRect`
  — returns the **overall** bounding rect (and fills per-glyph rects when the
  pointer is non-null).

### Rust mapping (`roastty/src/font/face/coretext.rs`)

- `roastty/Cargo.toml`: add `objc2-core-graphics` (feature `CGFont` for
  `CGGlyph`, plus geometry as needed) and enable the `objc2-core-graphics`
  feature on `objc2-core-text` (which gates the glyph methods). Exact feature
  set finalized against `cargo build`.
- Add to `impl Face` (each wrapping the `CTFont` call in `unsafe` with a
  `SAFETY` note; `UniChar`/`CGGlyph` are `u16`, so the `&[u16]` slice pointers
  map directly via `NonNull::new(ptr as *mut u16)`):
  - `pub(crate) fn glyphs_for_characters(&self, chars: &[u16]) -> Vec<u16>`:
    allocate `vec![0u16; chars.len()]`, call with the two `NonNull` pointers and
    `chars.len() as isize`, return the glyph vec. (Empty input → empty vec, no
    FFI call, since `NonNull` requires non-null.)
  - `pub(crate) fn advances_for_glyphs(&self, glyphs: &[u16]) -> Vec<f64>`:
    allocate `vec![CGSize::new(0.0, 0.0); glyphs.len()]`, call with
    `CTFontOrientation::Horizontal` and the advances pointer, return each
    `CGSize.width`. (Empty input → empty vec.)
  - `pub(crate) fn bounding_rect_for_glyphs(&self, glyphs: &[u16]) -> (f64, f64)`:
    call with a null `bounding_rects` pointer, return
    `(rect.size.width, rect.size.height)` from the overall `CGRect`. (Empty
    input → `(0.0, 0.0)`.)
- `get_metrics` (next slice) will derive: `cell_width` = max of
  `advances_for_glyphs(printable-ASCII glyphs)`; `ascii_height` =
  `bounding_rect_for_glyphs(...).1`; `ic_width` =
  `advances_for_glyphs(&[water])[0]` guarded by
  `bounding_rect_for_glyphs(&[water]).0`.

### Faithfulness and scope notes

- The three wrappers mirror upstream's `getGlyphsForCharacters` /
  `getAdvancesForGlyphs` / `getBoundingRectsForGlyphs`. `advances_for_glyphs`
  returns the per-glyph widths (upstream takes their max for `cell_width`);
  `bounding_rect_for_glyphs` returns the overall rect (upstream uses its height
  for `ascii_height`). Single-element slices serve the `ic_width` path, so no
  separate single-glyph methods are needed.
- Empty-slice guards avoid constructing a `NonNull` from a null/dangling
  pointer.
- `objc2-core-graphics` is added now (needed for `CGGlyph`); it is also the
  crate glyph **rasterization** will use later (`CGContext`), so this is the
  first half of that dependency.
- No metric assembly here (no `get_metrics`), no rasterization.
- No C ABI, header, or ABI inventory changes.

## Changes

1. `roastty/Cargo.toml`: add `objc2-core-graphics` + the `objc2-core-graphics`
   feature on `objc2-core-text`.
2. `roastty/src/font/face/coretext.rs`: add `glyphs_for_characters`,
   `advances_for_glyphs`, `bounding_rect_for_glyphs` to `impl Face`.
3. Tests in `coretext.rs` (live CoreText, macOS):
   - `glyph_measurement`: `Face::new("Menlo", 12.0)`;
     `glyphs_for_characters(&[b'M' as u16, b'i' as u16])` returns two
     **non-zero** glyph IDs; `advances_for_glyphs(&glyphs)` returns two widths,
     all `> 0.0`, and (Menlo is monospaced) the `M` and `i` advances are equal;
     `bounding_rect_for_glyphs(&glyphs)` returns a height `> 0.0` and a width
     `> 0.0`.
   - `empty_glyph_inputs`: `glyphs_for_characters(&[])` is empty,
     `advances_for_glyphs(&[])` is empty, `bounding_rect_for_glyphs(&[])` is
     `(0.0, 0.0)` (no FFI call, no null-`NonNull`).

4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty face
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the three accessors call the matching `CTFont` methods with correct
  pointers/counts and return the glyph IDs, per-glyph advance widths, and the
  overall bounding rect;
- empty inputs are handled without constructing a null `NonNull`;
- the Menlo test shows non-zero monospaced advances and a positive bounding box;
- the `get_metrics` assembly and rasterization are cleanly deferred;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the objc2 glyph API needs a different
pointer/feature shape than expected.

The experiment **fails** if a measurement returns implausible values, if an
empty slice constructs a null `NonNull` (UB), or if any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-201423-511979-prompt.md`
- Result: `logs/codex-review/20260602-201423-511979-last-message.md`

Codex confirmed the design matches upstream `getMetrics` usage (max advance →
`cell_width`, overall bounding-rect height → `ascii_height`, single-glyph path →
`ic_width`), that the FFI-safety plan is sound (empty slices return **before**
any `NonNull` construction, input pointers are only cast to satisfy the objc2
API shape, output buffers are correctly sized, a null `bounding_rects` is
allowed, `count` is the slice length), that `CTFontOrientation::Horizontal` is
correct, and that adding `objc2-core-graphics` now for `CGGlyph` (and later
`CGContext` rasterization) is in scope. The Menlo + empty-input tests cover the
hazards.

## Result

**Result:** Pass

Added `objc2-core-graphics` (`CGFont`) and the `CTFontDescriptor` +
`objc2-core-graphics` features on `objc2-core-text` to `roastty/Cargo.toml` (the
glyph methods are gated on `CTFontDescriptor` + `objc2-core-graphics`;
`CTFontOrientation` lives in `CTFontDescriptor`). Added the three accessors to
`impl Face` in `coretext.rs`: `glyphs_for_characters` (UTF-16 → glyph IDs),
`advances_for_glyphs` (per-glyph `CGSize.width`), and `bounding_rect_for_glyphs`
(overall `CGRect` → `(width, height)`), each guarding empty input **before** any
`NonNull` construction and wrapping the FFI call in `unsafe` with a `SAFETY`
note.

Tests added (2): `glyph_measurement` — Menlo `M`/`i` map to non-zero glyphs,
both advances `> 0.0` and **equal** (monospaced), bounding box width/height
`> 0.0`; `empty_glyph_inputs` — all three return empty / `(0.0, 0.0)` with no
FFI call.

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty face
cargo test -p roastty
```

Observed:

- `face`: 5 passed (table spike, missing-table, scalar metrics, glyph
  measurement, empty inputs).
- Full `roastty`: 2360 unit tests passed (2358 prior + 2 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/font` (and
  `lib.rs`/header/abi).
- `git diff --check`: clean. (`Cargo.lock` gains `objc2-core-graphics`.)

No C ABI, header, or ABI inventory changes; the `get_metrics` assembly and
rasterization cleanly deferred.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
needs to change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-201737-231063-prompt.md`
- Result: `logs/codex-review/20260602-201737-231063-last-message.md`

Codex confirmed the `Cargo.toml` deps/features, that all three wrappers guard
empty inputs **before** any `NonNull` construction, size their output buffers to
`count`, use `CTFontOrientation::Horizontal`, and keep `unsafe` scoped to the
CoreText calls with accurate safety notes; that the const→mut casts are sound
(CoreText treats those inputs as read-only) while the glyph/advances output use
real mutable storage; and that `bounding_rect_for_glyphs` correctly passes a
null per-glyph buffer and returns the overall rect. Tests cover the live
measurement and the empty-input safety path; scope is clean.

## Conclusion

Experiment 252 succeeds — the **entire CoreText FFI surface `get_metrics` needs
is now proven**: table copy (250), the scalar accessors (251), and the
glyph-measurement accessors (252), each verified by live `cargo test`. Both
Codex gates passed with zero findings.

The next slice is the full **`Face::get_metrics` assembly** — now pure logic
over the proven FFI: copy the four tables (with the `head`/`bhed` fallback) via
`copy_table`, parse them, run the vertical-metrics fallback chain (OS/2-typo
when `use_typo_metrics`, else `hhea`, else OS/2-win, with the CTFont scalar
fallbacks when a table is absent), derive underline/strikethrough (with the
broken-zero guards) and cap/ex heights, measure `cell_width`/`ascii_height` over
printable ASCII and `ic_width` over `水`, build a `FaceMetrics`, and feed the
already-ported `Metrics::calc` — producing a full `Metrics` from a real macOS
font end-to-end. Glyph rasterization (`CGContext` → alpha bitmap → atlas)
follows.
