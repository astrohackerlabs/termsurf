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

# Experiment 259: Synthetic bold — rect growth + fill-stroke draw

## Description

The last monochrome `renderGlyph` branch: **synthetic bold**, the faux-bold
effect for fonts without a real bold variant. Upstream stores a per-face
`synthetic_bold: ?f64` line width and, when set, (1) grows the glyph's bounding
rect by the line width before constraining, and (2) draws the glyph with a
**fill-stroke** text drawing mode at that line width, so the outline is both
filled and stroked — making it heavier.

This experiment adds the `synthetic_bold` line width to `Face`, a constructor to
build a synthetic-bold face, and the two `renderGlyph` pieces. With color/sbix
still deferred (synthetic bold is suppressed for sbix bitmap fonts upstream),
the effect applies whenever `synthetic_bold` is set.

### Upstream behavior (`font/face/coretext.zig`)

- `synthetic_bold` is set by `syntheticBold` (lines 183–198):
  `line_width = max(size.points / 14, 1)` — a heuristic scaling the stroke with
  the point size.
- In `renderGlyph`, **after** the bounding rect and **before** the `< 0.25`
  guard (lines 315–320), for non-sbix fonts:
  `rect.size.width += line_width; rect.size.height += line_width; rect.origin.x -= line_width / 2; rect.origin.y -= line_width / 2;`.
  The grown rect then flows through the guard, `constrain`, the `scaleCTM`
  denominator, and the draw position — i.e. everything downstream sees the grown
  rect.
- In the draw block, the non-color branch sets **both** gray fill and gray
  **stroke** color (lines 505–508), and when `synthetic_bold` is set (lines
  512–515): `setTextDrawingMode(.fill_stroke); setLineWidth(line_width);`
  (before the CTM translate/scale).

### Rust mapping (`roastty/src/font/face/coretext.rs`)

1. **`Face`** gains `synthetic_bold: Option<f64>`. `Face::new` sets it `None`.
   Add `Face::new_synthetic_bold(name, size)` — builds the `CTFont` as in `new`
   and sets `synthetic_bold = Some((size / 14.0).max(1.0))` (faithful to the
   upstream heuristic; `size` is the point size passed to `CTFont::with_name`).
2. **`draw_coverage`** gains `stroke_width: Option<f64>` and:
   - always `set_gray_stroke_color(Some(&ctx), fill_gray, 1.0)` (matching the
     upstream non-color branch, which sets the stroke color unconditionally);
   - when `Some(lw)`:
     `set_text_drawing_mode(Some(&ctx), CGTextDrawingMode::FillStroke)` and
     `set_line_width(Some(&ctx), lw)` — placed after the fill/stroke color and
     before `translate_ctm` (upstream order).
   - `rasterize_glyph` passes `stroke_width = None` (output unchanged).
3. **`render_glyph`**: read the grown rect into locals once and use them
   everywhere downstream:
   - `let (mut rw, mut rh, mut ox, mut oy) = (rect.size.width, rect.size.height, rect.origin.x, rect.origin.y);`
   - if
     `let Some(lw) = self.synthetic_bold { rw += lw; rh += lw; ox -= lw / 2.0; oy -= lw / 2.0; }`
   - the `< 0.25` guard, the `constrain` input
     (`width: rw, height: rh, x: ox, y: oy + cell_baseline`), the draw position
     (`-ox, -oy`), and the scale denominators (`width / rw`, `height / rh`) all
     use the grown locals.
   - pass `self.synthetic_bold` as `draw_coverage`'s `stroke_width`.

### Scope / faithfulness notes

- **Deferred** (later experiments): color/sbix (the depth-4 RGBA path and the
  sbix synthetic-bold suppression — here there's no sbix, so the growth always
  applies when `synthetic_bold` is set) and the broader `Face` font-options /
  discovery plumbing (`new_synthetic_bold` is a focused constructor mirroring
  upstream's `syntheticBold` heuristic).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/face/coretext.rs`:
   - Add `synthetic_bold: Option<f64>` to `Face`; set `None` in `new`; add
     `new_synthetic_bold`.
   - Add the rect growth + the grown-rect locals to `render_glyph`, and pass
     `self.synthetic_bold` to `draw_coverage`.
   - Extend `draw_coverage` with `stroke_width` (gray stroke color always;
     fill-stroke mode + line width when `Some`).
   - Update `rasterize_glyph`'s call (`stroke_width = None`).
   - Import `CGTextDrawingMode`.
2. New tests (live CoreText, macOS):
   - `synthetic_bold_is_heavier`: render `'M'` with `Face::new("Menlo", 32.0)`
     and with `Face::new_synthetic_bold("Menlo", 32.0)`, both `.none`. Assert
     the bold glyph's canvas is at least as large (`bold.width >= plain.width`,
     `bold.height >= plain.height`) and its **total ink** (sum of pixel values
     in the region) strictly exceeds the plain glyph's — the fill-stroke makes
     it heavier.
   - `new_face_has_no_synthetic_bold` / `new_synthetic_bold_sets_width`: a plain
     face has `synthetic_bold == None`; a synthetic-bold face has
     `Some((32.0 / 14.0).max(1.0))`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty face
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Face` carries `synthetic_bold`, `new_synthetic_bold` sets the
  `max(size / 14, 1)` line width, and `render_glyph` grows the rect by the line
  width (used consistently downstream) and draws fill-stroke at that width;
- `draw_coverage` sets the gray stroke color unconditionally and the fill-stroke
  mode + line width when stroking, in upstream order;
- a synthetic-bold `'M'` is at least as large and strictly heavier (more ink)
  than the plain `'M'`, and `synthetic_bold` is `None`/`Some(width)` as
  expected;
- `rasterize_glyph`'s existing tests still pass (`stroke_width = None`, output
  unchanged);
- color/sbix stay cleanly deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the objc2 stroke API needs a different shape
than expected.

The experiment **fails** if the grown rect isn't applied consistently
downstream, the fill-stroke draw is mis-ordered, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-211640-544834-prompt.md`
- Result: `logs/codex-review/20260602-211640-544834-last-message.md`

Codex confirmed the design is faithful **provided** the grown-rect locals
(`rw/rh/ox/oy`) replace **every** downstream use of the original `rect` (guard,
constrain input, draw origin, scale denominators) — an implementation note to
verify. The `max(size / 14, 1)` line-width adaptation is reasonable because
`Face::new` passes that same size to CoreText, the fill-stroke setup order /
unconditional stroke color / pre-CTM `set_line_width` (with constraint-scaled
stroke width) all match upstream, and the heavier/larger test catches the
meaningful failure modes without brittle pixel pinning.

## Result

**Result:** Pass

`Face` gained `synthetic_bold: Option<f64>` (`None` in `new`; set by
`new_synthetic_bold` to `(size / 14).max(1.0)`). `render_glyph` grows the rect
by the line width into `rw/rh/ox/oy` locals **before** the `< 0.25` guard, and
every downstream use — the guard, the `constrain` input, the draw position
(`-ox`, `-oy`), and the scale denominators (`width / rw`, `height / rh`) — uses
the grown locals. `draw_coverage` gained `stroke_width: Option<f64>`: it sets
the gray stroke color unconditionally and, when `Some(lw)`, the `FillStroke`
drawing mode

- line width (before the CTM transforms, upstream order). `rasterize_glyph`
  passes `None`.

Tests (live CoreText):

- `new_face_has_no_synthetic_bold` / `new_synthetic_bold_sets_width` — the field
  is `None` for a plain face and `Some((32 / 14).max(1))` for a bold one.
- `synthetic_bold_is_heavier` — the synthetic-bold `'M'` has a canvas at least
  as large (`bold_w >= plain_w`, `bold_h >= plain_h`) and strictly more total
  ink than the plain `'M'`, confirming both the rect growth and the fill-stroke
  draw.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty face` → 26 passed, 0 failed.
- `cargo test -p roastty` → 2377 passed, 0 failed (no regressions; +3).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

With synthetic bold landed, **every monochrome branch of upstream `renderGlyph`
is now ported** — constraints, sub-pixel positioning, thicken/font-smoothing,
and synthetic bold all feed the rasterization and atlas write. The one remaining
`renderGlyph` branch is the **color/sbix** path: `isColorGlyph`/`ColorState`
detection (font color traits + the sbix table presence and SVG-table
`hasGlyph`), the depth-4 P3 RGBA bitmap context
(`byte_order_32_little | premultiplied_first`), an RGBA atlas write, and the
sbix whole-pixel quantization. That is the next sub-area (it needs font-traits
FFI and an RGBA atlas write, so it likely spans two experiments: detection, then
the colored render). Beyond `renderGlyph`: the Collection/CodepointResolver, the
shaper, and the Nerd Font attribute table.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-212140-675654-prompt.md`
- Result: `logs/codex-review/20260602-212140-675654-last-message.md`

Codex confirmed the grown `rw/rh/ox/oy` locals are used consistently in the
guard, constrain input, draw position, and scale denominators with no accidental
downstream use of the ungrown `rect`, that the line-width heuristic and
rect-growth signs match upstream, that the stroke color / `FillStroke` /
line-width setup is ordered before the CTM transforms, that `rasterize_glyph`
stays on the non-stroked path, and that the tests are appropriate and non-flaky.
