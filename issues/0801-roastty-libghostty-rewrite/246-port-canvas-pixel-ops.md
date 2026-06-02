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

# Experiment 246: Port the Canvas exact-pixel operations (draw + export to atlas)

## Description

Port the `Canvas` from upstream `font/sprite/canvas.zig` — but only the
**exact-pixel half**, which is independent of `z2d`. Upstream backs the canvas
with a `z2d` alpha8 surface, yet every non-path method reaches **past** z2d and
manipulates the raw alpha buffer directly
(`@ptrCast(self.sfc.image_surface_alpha8.buf)` / `sliceAsBytes`) for
performance. So the faithful Rust representation of the surface is a plain
`Vec<u8>` alpha buffer, and all of these methods port with no 2D-graphics
dependency:

- `init` (allocate the padded alpha buffer),
- `pixel` / `rect` / `box` (exact-fill drawing),
- `trim` + `write_atlas` (export the drawn glyph into the already-ported
  `Atlas`),
- `clear_clipping_regions` (zero the clip margins — a test helper),
- `invert` / `flip_horizontal` / `flip_vertical` (whole-buffer transforms).

The remaining `z2d`-backed **path-rendering** methods (`transformation`,
`get_context`, `quad`, `triangle`, `line`, `stroke_path`, `inner_stroke_path`,
`fill_path`) are **deferred** to the experiment that selects a Rust
path-rasterization backend. This is the natural seam: anti-aliased vector
rendering is the one part that genuinely needs `z2d` (or a replacement), and it
is the architectural decision worth isolating. Everything here is exact integer
pixel work with deterministic tests, and it builds on the Exp 245 primitives and
the Exp 243–244 `Atlas`.

### z2d facts that fix the port (verified in z2d 0.10.0)

- `Surface.initPixel(.{ .alpha8 = .{ .a = 0 } }, …)` allocates a
  `width × height` alpha8 buffer (one byte/pixel) initialized to `0`.
- `putPixel(x, y)` is a **silent no-op** when
  `x < 0 || y < 0 || x >= width || y >= height`; otherwise it writes
  `buf[width*y + x]`.
- The alpha8 buffer is row-major, one byte per pixel — exactly what `trim`,
  `clear_clipping_regions`, `write_atlas`, `invert`, and the flips already treat
  it as.

### Upstream behavior (`canvas.zig`)

- **`init(width, height, padding_x, padding_y)`** (94–117): surface size is
  `(width + 2*padding_x) × (height + 2*padding_y)`, all alpha `0`; clips start
  `0`.
- **`pixel(x, y, color)`** (266–272): write at `(x + padding_x, y + padding_y)`
  via `putPixel` (so out-of-surface writes are dropped).
- **`rect(Rect(i32), color)`** (276–288): fill `[x, x+width) × [y, y+height)`
  with `pixel`.
- **`box(x0, y0, x1, y1, color)`** (291–303): `rect(Box{p0,p1}.rect(), color)`.
- **`trim`** (165–210): advance each clip edge inward while the current edge
  row/column (within the other clips) is fully transparent. Uses the raw buffer.
- **`write_atlas(atlas)`** (125–161): `assert(atlas.format == .grayscale)`,
  `trim`, compute `region = (sfc_w - clip_left - clip_right) × (sfc_h - clip_top
  - clip_bottom)`(saturating),`atlas.reserve(region_w,
    region_h)`, and if non-empty `atlas.setFromLarger(region, buf, sfc_w,
    clip_left, clip_top)`. The clip margins are excluded from what is written.
- **`clear_clipping_regions`** (214–242): zero the left/right/top/bottom clip
  margins of the buffer.
- **`invert`** (477–481): `v = 255 - v` for every byte.
- **`flip_horizontal`** (484–496): mirror columns —
  `buf[y*w + x] = clone[y*w + (w - x - 1)]` — then swap
  `clip_left`/`clip_right`.
- **`flip_vertical`** (499–511): mirror rows —
  `buf[y*w + x] = clone[(h - y - 1)*w + x]` — then swap
  `clip_top`/`clip_bottom`.

### Rust mapping (in `roastty/src/font/sprite/canvas.rs`, alongside the

primitives)

- `pub(crate) struct Canvas { buf: Vec<u8>, width: u32, height: u32, padding_x: u32, padding_y: u32, clip_top: u32, clip_left: u32, clip_right: u32, clip_bottom: u32 }`.
  `width`/`height` are the **surface** (padded) dimensions (the analog of
  `sfc.getWidth()/getHeight()`); `buf` is the row-major alpha8 buffer
  (`width * height` bytes). No `z2d`, no separate surface type, no allocator
  field (Rust owns the `Vec`).
- `pub(crate) fn new(width: u32, height: u32, padding_x: u32, padding_y: u32) -> Canvas`:
  `let w = width + 2*padding_x; let h = height + 2*padding_y;`,
  `buf = vec![0u8; (w*h) as usize]`, clips `0`. (Infallible; no `z2d` init error
  to thread.)
- `pub(crate) fn pixel(&mut self, x: i32, y: i32, color: Color)` (public like
  upstream — the deferred `draw/` glyph tables call `pixel` directly):
  `let px = x + padding_x as i32; let py = y + padding_y as i32;` bounds-check
  `px < 0 || py < 0 || px >= width as i32 || py >= height as i32` → return; else
  `buf[(width as i32 * py + px) as usize] = color.0`.
- `pub(crate) fn rect(&mut self, v: Rect<i32>, color: Color)`: the two nested
  loops calling `pixel`.
- `pub(crate) fn box(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color)`:
  `self.rect(Box { p0: Point { x: x0, y: y0 }, p1: Point { x: x1, y: y1 } }.rect(), color)`.
- `fn trim(&mut self)`: the four `while` loops, faithfully, over `buf` with the
  same edge/range arithmetic (saturating where upstream uses `-|`).
- `pub(crate) fn write_atlas(&mut self, atlas: &mut Atlas) -> Result<Region, AtlasError>`:
  `assert!(atlas.format() == Format::Grayscale)` — an always-on assert
  **before** `trim`/`reserve` (matching upstream's up-front `assert`, so a
  non-grayscale atlas is rejected before any mutation rather than mis-copying at
  the wrong depth) — then `self.trim()`, compute the saturating
  `region_width`/`region_height`, `atlas.reserve(...)?`, and if
  `region.width > 0 && region.height > 0` (with the two
  `debug_assert!(region.width == region_width)` / height checks)
  `atlas.set_from_larger(region, &self.buf, self.width, self.clip_left, self.clip_top)`;
  return the region. Requires a new `pub(crate) fn format(&self) -> Format`
  accessor on `Atlas` (one line, added in `atlas.rs`).
- `pub(crate) fn clear_clipping_regions(&mut self)`: the four margin-zeroing
  loops.
- `pub(crate) fn invert(&mut self)`:
  `for v in &mut self.buf { *v = 255 - *v; }`.
- `pub(crate) fn flip_horizontal(&mut self)` / `flip_vertical(&mut self)`: clone
  the buffer (`self.buf.clone()`), write the mirror, then swap the matching clip
  pair (`std::mem::swap(&mut self.clip_left, &mut self.clip_right)` /
  top/bottom). (Infallible; the upstream `Allocator.Error` came only from the
  clone allocation.)

### Faithfulness and scope notes

- The surface is modeled as a bare `Vec<u8>` alpha8 buffer because that is
  literally what upstream's exact-pixel methods operate on (they bypass z2d). A
  later experiment that adds path rendering fills into this **same** buffer
  (whether hand-rolled or by compositing a backend's output), so this
  representation does not pre-commit the backend choice.
- `pixel` reproduces z2d `putPixel`'s out-of-bounds no-op exactly, so off-canvas
  draws are dropped as upstream.
- The `z2d` path-rendering methods are deferred, not reimplemented; no
  approximate rasterizer is introduced here.
- `write_atlas` needs to read `Atlas::format`; a `pub(crate) fn format()`
  accessor is added to `atlas.rs` (no behavior change).
- Upstream `canvas.zig` has no unit tests (the sprite tests live in `draw/` with
  PNG fixtures), so this slice adds Rust-side exact-pixel tests per the Test
  Parity rule.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. `roastty/src/font/atlas.rs`: add `pub(crate) fn format(&self) -> Format`.

2. `roastty/src/font/sprite/canvas.rs`: add the `Canvas` struct and `new`,
   `pixel`, `rect`, `box`, `trim`, `write_atlas`, `clear_clipping_regions`,
   `invert`, `flip_horizontal`, `flip_vertical`; update the module doc to note
   the path methods remain deferred.

3. Tests in `roastty/src/font/sprite/canvas.rs`:
   - `pixel_padding_and_bounds`: `new(2, 2, 1, 1)` (surface 4×4);
     `pixel(0, 0, ON)` sets `buf[5]` (`= 1*4 + 1`, the padded `(1,1)`);
     `pixel(-2, 0, ON)` (padded `(-1, 1)`, `px < 0`) and `pixel(3, 0, ON)`
     (padded `(4, 1)`, `px >= width`) are no-ops — assert exactly one byte
     (`buf[5]`) is non-zero.
   - `rect_and_box_fill`: `new(4, 4, 0, 0)`;
     `rect(Rect { x: 0, y: 0, width: 2, height: 2 }, ON)` sets exactly
     `(0,0),(1,0),(0,1),(1,1)`; a fresh canvas with `box(2, 2, 0, 0, ON)`
     (corners reversed) fills the same four pixels.
   - `trim_clips_transparent_margins`: `new(2, 2, 1, 1)`; `pixel(0, 0, ON)`;
     `trim()` →
     `clip_top == 1, clip_bottom == 2, clip_left == 1, clip_right == 2` (the
     single lit pixel at surface `(1,1)`).
   - `write_atlas_exports_trimmed`: the same single-pixel canvas; `write_atlas`
     into a `grayscale` `Atlas::new(8, …)`; the returned region is `1 × 1` and
     the atlas holds the `255` byte at the region's offset.
   - `clear_clipping_regions_zeros_margins`: fill the buffer to `255`, set
     clips, `clear_clipping_regions`, assert the margins are `0` and the
     interior stays `255`.
   - `invert_and_flips`: `invert` maps `0 → 255`; a known asymmetric pattern is
     mirrored correctly by `flip_horizontal`/`flip_vertical`, and the clip pairs
     are swapped.

4. Format and test (`cargo fmt`, accept output).

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

- the `Canvas` surface is the padded alpha8 `Vec<u8>` with the clip fields, and
  `new` sizes/zeros it correctly;
- `pixel` applies padding and drops out-of-bounds writes; `rect`/`box` fill the
  exact pixels (with `box` normalizing reversed corners);
- `trim` advances the clips past transparent margins, `write_atlas` reserves and
  writes the trimmed region into the atlas via `set_from_larger`, and
  `clear_clipping_regions` zeroes the margins;
- `invert`/`flip_horizontal`/`flip_vertical` transform the buffer and swap the
  clip pairs correctly;
- the path-rendering methods are cleanly deferred (no approximate rasterizer
  added);
- the Rust-side tests pass;
- no C ABI, header, or ABI inventory changes (the `Atlas::format` accessor is
  internal `pub(crate)`);
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `write_atlas`/`trim` need a representation the
deferred path rendering forces to change.

The experiment **fails** if `pixel` mis-handles padding/bounds, if `trim` or
`write_atlas` clips the wrong region, if a flip/invert corrupts the buffer or
forgets the clip swap, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-093915-433313-prompt.md`
- Result: `logs/codex-review/20260602-093915-433313-last-message.md`

Codex confirmed the `Vec<u8>` alpha surface is faithful for the exact-pixel
half, that the `trim` math and the single-pixel clip expectations are correct,
that `write_atlas_exports_trimmed` should produce a `1×1` region with byte
`255`, and that deferring the z2d-backed path methods is cleanly scoped.

Three findings, all fixed in the design above before this commit:

1. **High — `write_atlas` grayscale check must be an always-on `assert!`.** The
   mapping used `debug_assert!`; in release a non-grayscale atlas could be
   reserved/mutated before `set_from_larger` mis-copies at the wrong depth.
   Upstream asserts up-front, so this is now
   `assert!(atlas.format() == Format::Grayscale)` before `trim`/`reserve`.
2. **Medium — `pixel` must be `pub(crate)`.** Upstream `pixel` is public and the
   deferred `draw/` glyph tables call `canvas.pixel` directly; keeping it
   private would force churn later. Changed to `pub(crate) fn pixel`.
3. **Low — `flip_vertical` prose typo.** The formula read
   `clone[(h - y - 1)*w - x]`; upstream uses `+ x`. Corrected (the
   implementation ports the `+ x` form).

## Result

**Result:** Pass

Added the `Atlas::format()` accessor (and, additionally, a one-line
`Atlas::data(&self) -> &[u8]` read accessor — needed by the designed
`write_atlas` test, which checks the exported byte in the atlas buffer; both are
internal `pub(crate)` and the renderer will read `data` to upload to the GPU) to
`roastty/src/font/atlas.rs`, and the `Canvas` to
`roastty/src/font/sprite/canvas.rs`: the padded alpha8 `Vec<u8>` surface with
the clip fields, and `new`, `pixel` (padding + the z2d-`putPixel` out-of-bounds
no-op), `rect`, `r#box` (`box` is a Rust keyword, so the method is `r#box`,
normalizing reversed corners via `Box::rect`), `trim` (the four clip-advance
loops), `write_atlas` (the up-front grayscale `assert!`, `trim`, saturating
region size, `reserve`, `set_from_larger`), `clear_clipping_regions`, `invert`,
`flip_horizontal`, and `flip_vertical` (clone + mirror + clip-pair swap). The
z2d-backed path methods are left deferred. The module doc was updated.

Tests added (6): `pixel_padding_and_bounds`, `rect_and_box_fill`,
`trim_clips_transparent_margins` (`clip_top 1`, `clip_bottom 2`, `clip_left 1`,
`clip_right 2` for the single lit pixel), `write_atlas_exports_trimmed` (`1×1`
region, `255` byte in the atlas), `clear_clipping_regions_zeros_margins`, and
`invert_and_flips`.

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty canvas
cargo test -p roastty
```

Observed:

- `canvas`: 11 passed (5 prior + 6 new).
- Full `roastty`: 2339 unit tests passed (2333 prior + 6 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/font` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes (the `Atlas` accessors are internal
`pub(crate)`); the z2d-backed path-rendering methods cleanly deferred.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
needs to change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-094504-937905-prompt.md`
- Result: `logs/codex-review/20260602-094504-937905-last-message.md`

Codex confirmed the buffer shape, padding/clip fields, `pixel` bounds behavior,
`rect`/`r#box`, `write_atlas`, `clear_clipping_regions`, `invert`, and both
flips match the intended z2d-free behavior; that the three design-gate fixes are
present (`pixel` is `pub(crate)`, `write_atlas` uses the up-front always-on
`assert!`, `flip_vertical` uses `+ x`); that the `trim` loop bounds/termination
match upstream (single-lit-pixel → `top 1`/`bottom 2`/`left 1`/`right 2`) with
no added underflow hazard for the internally-maintained clip state; and that
`Atlas::format()` is the designed accessor and the extra read-only
`Atlas::data()` is narrowly justified (cross-module export test + the renderer
upload path), not a scope problem.

## Conclusion

Experiment 246 succeeds. The `Canvas`'s entire exact-pixel surface — drawing
(`pixel`/`rect`/`r#box`), the trim-and-export path into the `Atlas`
(`trim`/`write_atlas`), and the buffer transforms
(`clear_clipping_regions`/`invert`/`flip_horizontal`/`flip_vertical`) — is
ported with a plain `Vec<u8>` alpha surface, no 2D-graphics dependency. Both
Codex gates passed (three design findings fixed; zero result findings).

What remains of `canvas.zig` is exactly the `z2d`-backed path rendering
(`transformation`/`get_context`/`line`/`quad`/`triangle`/`stroke_path`/
`inner_stroke_path`/`fill_path`) — the anti-aliased vector layer. That is the
isolated architectural decision: choose a Rust path rasterizer (a hand-rolled
alpha scanline/coverage filler, or a crate such as `tiny-skia`) that fills into
the same alpha8 buffer and reproduces z2d's anti-aliasing closely enough that
the `draw/` glyph fixtures match. The next experiment will scope that backend
choice (likely starting with the affine `transformation` value type and a single
straight-line/`fill_path` primitive against a small fixture) before the `draw/`
glyph tables are ported on top.
