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

# Experiment 379: the cursor cell

## Description

`Contents::set_cursor` already routes a cursor vertex (block first, the others
last), but nothing builds that vertex. This experiment ports upstream's
`addCursor`: map the cursor style to its sprite, render it through the
`SharedGrid`, build the cursor `CellTextVertex`, and call
`Contents::set_cursor`. This is the cursor half of the renderer's per-frame
work.

## Upstream behavior

`addCursor` (`renderer/generic.zig`) maps the cursor style to a sprite:

```zig
const sprite: font.Sprite = switch (cursor_style) {
    .block => .cursor_rect,
    .block_hollow => .cursor_hollow_rect,
    .bar => .cursor_bar,
    .underline => .cursor_underline,
    .lock => unreachable, // handled by a separate renderCodepoint path
};
const render = self.font_grid.renderGlyph(font.sprite_index, @intFromEnum(sprite),
    .{ .cell_width = if (wide) 2 else 1, .grid_metrics = self.grid_metrics });
self.cells.setCursor(.{
    .atlas = .grayscale,
    .grid_pos = .{ x, cursor_vp.y },
    .color = .{ cursor_color.r, cursor_color.g, cursor_color.b, alpha },
    .glyph_pos = .{ render.glyph.atlas_x, render.glyph.atlas_y },
    .glyph_size = .{ render.glyph.width, render.glyph.height },
    .bearings = .{ render.glyph.offset_x, render.glyph.offset_y },
}, cursor_style);
```

The cursor is a sprite at `cell_width = wide ? 2 : 1`, grayscale, colored by the
cursor color. The `.lock` style renders a lock **codepoint** glyph (a separate
`renderCodepoint` path), not a sprite — deferred here. roastty's `set_cursor`
already routes `Block` to the first cursor list and the others to the last.

## Rust mapping (`roastty/src/renderer/cell.rs`)

```rust
/// Render the cursor sprite for `cursor_style` through `grid` and set it as the
/// cursor cell in `contents` (via [`Contents::set_cursor`]) at `grid_pos`, with
/// `color`/`alpha`. `wide` widens the sprite to two cells. Faithful port of
/// upstream `addCursor` (the sprite cursor styles). `CursorStyle::Lock` renders a
/// codepoint glyph upstream, not a sprite, and is deferred (no-op here).
pub(crate) fn add_cursor(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    cursor_style: CursorStyle,
    wide: bool,
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    let sprite = match cursor_style {
        CursorStyle::Block => Sprite::CursorRect,
        CursorStyle::BlockHollow => Sprite::CursorHollowRect,
        CursorStyle::Bar => Sprite::CursorBar,
        CursorStyle::Underline => Sprite::CursorUnderline,
        // The lock cursor renders a codepoint glyph (deferred), not a sprite.
        // Still clear any prior cursor so a stale one does not linger.
        CursorStyle::Lock => {
            contents.set_cursor(None, Some(CursorStyle::Lock));
            return Ok(());
        }
    };

    let opts = RenderOptions {
        grid_metrics: grid.metrics,
        cell_width: Some(if wide { 2 } else { 1 }),
        constraint: Constraint::default(),
        constraint_width: 1,
        thicken: false,
        thicken_strength: 255,
    };
    let render = grid.render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)?;

    let vertex = CellTextVertex {
        glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
        glyph_size: [render.glyph.width, render.glyph.height],
        bearings: [
            i16::try_from(render.glyph.offset_x).expect("cursor x bearing fits i16"),
            i16::try_from(render.glyph.offset_y).expect("cursor y bearing fits i16"),
        ],
        grid_pos,
        color: [color[0], color[1], color[2], alpha],
        atlas: CellTextAtlas::Grayscale,
        // `is_cursor_glyph = true` — upstream marks the cursor vertex.
        flags: CellTextFlags::new(false, true),
        _padding: [0, 0],
    };
    contents.set_cursor(Some(vertex), Some(cursor_style));
    Ok(())
}
```

## Scope / faithfulness notes

- **Ported (bridged)**: upstream `addCursor`'s sprite path — map the cursor
  style to its sprite, render it at `cell_width = wide ? 2 : 1` into the
  grayscale atlas, build the cursor vertex, and `set_cursor` it.
- **Faithful**: the style → sprite mapping is upstream's exactly
  (`Block → CursorRect`, `BlockHollow → CursorHollowRect`, `Bar → CursorBar`,
  `Underline → CursorUnderline`); the atlas is grayscale, the color is the
  cursor color, the bearings are the sprite glyph's own offsets; `wide` widens
  to two cells (`cell_width = 2`); the cursor vertex sets
  `is_cursor_glyph = true` (`CellTextFlags::new(false, true)`), as upstream
  marks the cursor vertex; the routing (block first, others last) is
  `Contents::set_cursor`'s, already ported.
- **Faithful adaptation**: `add_cursor` takes the already-resolved cursor color
  (`[u8; 3]`) and the `wide` bool (the renderer derives them); the bearings use
  a checked `i16::try_from(...).expect` (upstream's `@intCast`); the `Lock`
  style actively clears the cursor (`set_cursor(None, …)`) so a prior cursor
  does not linger, even though its lock-glyph render is deferred.
- **Deferred**: the **lock** cursor's glyph (a lock **codepoint** via the
  resolver's `render_glyph`, not a sprite — `add_cursor` clears the cursor and
  no-ops on `Lock`); the under-cursor **text** glyph recolor (a separate
  concern); the renderer-level decision of _whether_/_where_ to draw the cursor
  (cursor visibility, blink, viewport position); and the Metal upload. (Consumed
  by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: add the `add_cursor` function (`CursorStyle`
   is already imported as `super::cursor::Style`).
2. Tests (in `cell.rs`):
   - **all sprite styles** (table over
     `[(Block, Sprite::CursorRect), (BlockHollow, Sprite::CursorHollowRect), (Bar, Sprite::CursorBar), (Underline, Sprite::CursorUnderline)]`):
     for each, on a fresh Menlo `SharedGrid`/`Contents`,
     `add_cursor(grid_pos [2, 1], style, wide = false, color, 255)` sets the
     cursor cell — assert it lands in the right cursor list
     (`Block → fg_rows[0]`, the others → `fg_rows[last]`), with `grid_pos`,
     grayscale atlas, the color, `flags == CellTextFlags::new(false, true)`, and
     a same-grid cache-identity match (`cell_width = 1` opts) to the expected
     cursor sprite;
   - **wide**: `add_cursor(..., Block, wide = true, ...)` matches a same-grid
     direct render with `cell_width = Some(2)` (and its `glyph_size` width
     differs from the `wide = false` render) — proving the `wide` width is
     honored;
   - **lock clears**: pre-seed a cursor (`add_cursor(..., Block, ...)`), then
     `add_cursor(..., CursorStyle::Lock, ...)` — assert **both** cursor lists
     (`fg_rows[0]` and `fg_rows[last]`) are empty (the lock no-op still clears).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty add_cursor
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `add_cursor` renders the correct sprite for each cursor style and sets the
  cursor cell (grayscale, the cursor position/color, `wide` width) via
  `Contents::set_cursor` — faithful to upstream `addCursor`'s sprite path;
- the tests pass (each style renders its sprite into the right cursor list;
  `Lock` is a no-op), and the existing tests still pass;
- the lock cursor, the under-cursor text recolor, the cursor-placement decision,
  and the Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a style maps to the wrong sprite, the cursor lands
in the wrong list, the vertex is mis-built, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with three
**Required** findings, all now addressed:

- **Required (addressed):** the cursor vertex must set `is_cursor_glyph = true`
  (upstream marks it so at `generic.zig:3301`). The vertex now uses
  `CellTextFlags::new(false, true)`, and the test asserts that flag.
- **Required (addressed):** `CursorStyle::Lock => return Ok(())` could leave a
  stale cursor if one was previously set. The `Lock` branch now actively clears
  (`contents.set_cursor(None, Some(CursorStyle::Lock))`) before returning, and
  the test pre-seeds a cursor then asserts both cursor lists are empty after
  `Lock`.
- **Required (addressed):** the tests only used `wide = false`, so the
  `cell_width = if wide { 2 } else { 1 }` behavior was unproven. A `wide = true`
  case now compares against a same-grid `cell_width = Some(2)` render (and
  asserts the wide `glyph_size` width differs from the narrow), proving the
  width is honored.

Codex confirmed the style → sprite mapping is correct (`Block`/`BlockHollow`/
`Bar`/`Underline` → the four cursor sprites, with `Contents::set_cursor` routing
block to the first cursor list and the others to the last) and that the vertex
fields otherwise match upstream (grayscale atlas, cursor color/alpha, glyph
atlas position/size, glyph-only bearings).

Review artifacts:

- Prompt: `logs/codex-review/20260603-191103-138404-prompt.md` (design)
- Result: `logs/codex-review/20260603-191103-138404-last-message.md` (design)

## Result

**Result:** Pass

The cursor cell is ported.

- `roastty/src/renderer/cell.rs`:
  `add_cursor(contents, grid, grid_pos, cursor_style, wide, color, alpha)` maps
  the cursor style to its sprite (`Block → CursorRect`,
  `BlockHollow → CursorHollowRect`, `Bar → CursorBar`,
  `Underline → CursorUnderline`), renders it at `cell_width = wide ? 2 : 1`,
  builds the cursor `CellTextVertex` (grayscale, the cursor color, glyph-only
  bearings, `is_cursor_glyph = true`), and calls `Contents::set_cursor` (which
  routes Block first, the others last). `Lock` actively clears the cursor and
  no-ops (its lock-codepoint glyph is deferred).

Tests (in `cell.rs`):

- `add_cursor_maps_styles_and_routes` — table over the four sprite styles; each
  lands in the right cursor list (`Block → fg_rows[0]`, others →
  `fg_rows[last]`) with `grid_pos`, grayscale atlas, the color,
  `flags == CellTextFlags::new(false, true)`, and a same-grid cache-identity
  match to the expected cursor sprite.
- `add_cursor_wide_uses_two_cells` — a `wide = true` Block cursor matches a
  same-grid `cell_width = Some(2)` render, and its width differs from the narrow
  render — proving the wide width is honored.
- `add_cursor_lock_clears` — pre-seeds a Block cursor, then `Lock` clears both
  cursor lists.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2833 passed, 0 failed (+3, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer) clean; `git diff --check` clean.

## Conclusion

Every renderer cell kind a frame needs is now ported: backgrounds, foreground
glyphs, the three decorations, and the cursor. From a terminal screen, the
renderer can fill `Contents` with the complete cell data the GPU draws (modulo
the deferred color adjustments and the lock-cursor glyph).

The remaining renderer-bridge work: the renderer-layer **color adjustments**
(reverse-video, selection, min-contrast, faint/dim alpha, default-bg fill,
opacity); the lock-cursor codepoint glyph and the under-cursor text recolor; the
strict column-ordered decoration merge and the link double-underline; and the
**Metal upload** of `Contents` to the GPU.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed `add_cursor` is faithful to upstream `addCursor`'s
sprite path (the four sprite styles map correctly, `cell_width` is `Some(2)` for
wide and `Some(1)` otherwise, the cursor vertex is grayscale with cursor
color/alpha and glyph-only bearings, and it sets
`CellTextFlags::new(false, true)` before calling `Contents::set_cursor`), that
all three Required fixes are applied (`Lock` actively clears, the flag is
asserted, `wide = true` is covered against a `cell_width = Some(2)` render), and
that the tests prove the list routing, sprite identity, color, and lock
clearing. Nothing needed to change before the result commit.

Review artifacts:

- Result review: `logs/codex-review/20260603-191453-118071-last-message.md`
