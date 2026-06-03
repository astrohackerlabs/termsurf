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

# Experiment 375: the underline decoration cell

## Description

With foreground text and backgrounds done, the next renderer subsystem is the
**decorations** (underline, strikethrough, overline). This experiment ports the
underline — the most complex, with five variants. `add_underline` maps a cell's
`Underline` style to its sprite, renders that sprite through the `SharedGrid`,
and emits a `Key::Underline` decoration cell into `Contents`. This is upstream's
`addUnderline`. Strikethrough/overline (simpler, single-sprite) and the row
integration follow.

## Upstream behavior

`addUnderline` (`renderer/generic.zig`):

```zig
const sprite: font.Sprite = switch (style) {
    .none => unreachable,
    .single => .underline,
    .double => .underline_double,
    .dotted => .underline_dotted,
    .dashed => .underline_dashed,
    .curly => .underline_curly,
};
const render = try self.font_grid.renderGlyph(
    font.sprite_index, @intFromEnum(sprite),
    .{ .cell_width = 1, .grid_metrics = self.grid_metrics });
try self.cells.add(.underline, .{
    .atlas = .grayscale,
    .grid_pos = .{ x, y },
    .color = .{ color.r, color.g, color.b, alpha },
    .glyph_pos = .{ render.glyph.atlas_x, render.glyph.atlas_y },
    .glyph_size = .{ render.glyph.width, render.glyph.height },
    .bearings = .{ render.glyph.offset_x, render.glyph.offset_y },
});
```

The decoration is a **sprite** glyph (the line), rendered at `cell_width = 1`
through the sprite font, always into the grayscale atlas. Unlike a shaped glyph,
its `bearings` are the glyph's own offsets only (there is no shaper cell, so no
`x_offset`/`y_offset`). The color is the underline color (or the foreground),
supplied by the caller. roastty has the matching pieces: `Sprite::Underline…`
codepoints, `Index::special(Special::Sprite)`, and `SharedGrid::render_glyph`.

## Rust mapping (`roastty/src/renderer/cell.rs`)

```rust
use crate::font::collection::Special;
use crate::font::sprite::draw::Sprite;
use crate::terminal::sgr::Underline;

/// Render a cell's underline as a sprite through `grid` and add it to `contents`
/// as a [`Key::Underline`] decoration cell at `grid_pos` with `color`/`alpha`.
/// `Underline::None` adds nothing. Faithful port of upstream `addUnderline`: the
/// sprite (one of five variants) is drawn at `cell_width = 1` into the grayscale
/// atlas, and the bearings are the sprite glyph's own offsets (a decoration has
/// no shaper offset).
pub(crate) fn add_underline(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    underline: Underline,
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    let sprite = match underline {
        Underline::None => return Ok(()),
        Underline::Single => Sprite::Underline,
        Underline::Double => Sprite::UnderlineDouble,
        Underline::Dotted => Sprite::UnderlineDotted,
        Underline::Dashed => Sprite::UnderlineDashed,
        Underline::Curly => Sprite::UnderlineCurly,
    };

    let opts = RenderOptions {
        grid_metrics: grid.metrics,
        cell_width: Some(1),
        constraint: Constraint::default(),
        constraint_width: 1,
        thicken: false,
        thicken_strength: 255,
    };
    let render = grid.render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)?;

    contents.add(
        Key::Underline,
        CellTextVertex {
            glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
            glyph_size: [render.glyph.width, render.glyph.height],
            // A decoration has no shaper cell, so only the glyph's own bearings.
            bearings: [
                i16::try_from(render.glyph.offset_x).expect("underline x bearing fits i16"),
                i16::try_from(render.glyph.offset_y).expect("underline y bearing fits i16"),
            ],
            grid_pos,
            color: [color[0], color[1], color[2], alpha],
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::new(false, false),
            _padding: [0, 0],
        },
    );
    Ok(())
}
```

## Scope / faithfulness notes

- **Ported (bridged)**: upstream `addUnderline` — map the `Underline` variant to
  its `Sprite`, render it through the grid at `cell_width = 1`, and add a
  `Key::Underline` cell (grayscale, the cell's grid position, the supplied
  color, the sprite's atlas placement/size, and the sprite glyph's bearings).
- **Faithful**: the variant → sprite mapping is upstream's exactly
  (`single → underline`, `double → underline_double`,
  `dotted → underline_dotted`, `dashed → underline_dashed`,
  `curly → underline_curly`); the atlas is always grayscale (the sprite is
  monochrome); the bearings are the glyph's own offsets (no shaper offset — a
  decoration is not a shaped glyph); the render options set `cell_width = 1` and
  the grid metrics (as upstream), with the remaining fields at their defaults
  (the sprite path ignores constraint/thicken). `Underline::None` adds nothing
  (upstream's `unreachable` is never reached because the caller guards
  `!= none`; roastty guards it inside).
- **Faithful adaptation**: the bearings use a checked
  `i16::try_from(...).expect` (upstream's `@intCast`);
  `is_cursor_glyph`/`no_min_contrast` are `false`; the color is supplied by the
  caller (the underline color or foreground — its resolution is the caller's, a
  later step). `add_underline` is co-located with `Contents`/`add_glyph` in
  `renderer/cell.rs`.
- **Deferred**: the strikethrough and overline decorations (the same pattern,
  single sprite each); the underline-color resolution
  (`Style::underline_color`); the row/viewport integration (calling
  `add_underline` per decorated cell); and the Metal upload. (Consumed by tests
  now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: add the `add_underline` function; import
   `font::collection::Special`, `font::sprite::draw::Sprite`, and
   `terminal::sgr::Underline`.
2. Tests (in `cell.rs`):
   - **all variants** (table-driven over
     `[(Single, Sprite::Underline), (Double, Sprite::UnderlineDouble), (Dotted, Sprite::UnderlineDotted), (Dashed, Sprite::UnderlineDashed), (Curly, Sprite::UnderlineCurly)]`):
     for each, on a fresh Menlo `SharedGrid`/`Contents`,
     `add_underline(grid_pos [0, 0], variant, [5, 6, 7], 255)` adds one cell to
     `fg_rows[1]`; then **direct-render the expected sprite on the same grid**
     with the same underline `RenderOptions` and assert the emitted vertex's
     `glyph_pos`/`glyph_size`/ `bearings` equal it. The grid's glyph cache is
     keyed by the sprite codepoint, so this is a cache **hit** (identical atlas
     region) **iff** `add_underline` rendered exactly the expected sprite — a
     wrong variant→sprite mapping is a cache miss into a different atlas region
     and fails the `glyph_pos` assert. Also assert `grid_pos == [0, 0]`,
     `atlas == Grayscale`, `color == [5, 6, 7, 255]`.
   - `add_underline(..., Underline::None, ...)` adds nothing.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty add_underline
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `add_underline` renders the correct sprite for each `Underline` variant and
  adds a `Key::Underline` grayscale decoration cell with the cell position,
  color, and sprite placement/bearings — faithful to upstream `addUnderline`;
- the tests pass (a `Single` underline adds one correct cell; `None` adds none),
  and the existing tests still pass;
- strikethrough/overline, the underline-color resolution, the integration, and
  the Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant maps to the wrong sprite, the cell is
mis-built (wrong atlas/position/color), or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Required** finding, now addressed:

- **Required (addressed):** the test only covered `Single`/`None`, so it did not
  guard the five-way `Underline` → `Sprite` mapping (a `Double → Underline` or
  `Dotted → UnderlineDashed` regression would still pass). The test is now
  table-driven over all five non-`None` variants and compares the emitted vertex
  to a **same-grid** direct render of the expected sprite — because the grid's
  glyph cache is keyed by the sprite codepoint, the direct render is a cache
  **hit** (identical atlas region) only if `add_underline` rendered exactly that
  sprite; a wrong mapping is a cache miss into a different atlas region and
  fails the `glyph_pos`/`glyph_size`/`bearings` asserts.

Codex confirmed the design is otherwise sound: the variant → sprite mapping
matches the local enums; `Underline::None → Ok(())` is a reasonable Rust guard
around upstream's caller-side `unreachable`; using
`Index::special(Special:: Sprite)`, the grayscale atlas, glyph-only bearings,
the supplied RGBA color, `Key::Underline`, and
`CellTextFlags::new(false, false)` is faithful to `addUnderline`; the
`RenderOptions` fields are acceptable for the sprite path (`grid.metrics` +
`cell_width = Some(1)` are the meaningful inputs, the rest harmless); and
routing through `Key::Underline` is correct (`Contents::add` places underline
vertices in the foreground row list at the same `y + 1` offset as text).

Review artifacts:

- Prompt: `logs/codex-review/20260603-184723-652094-prompt.md` (design)
- Result: `logs/codex-review/20260603-184723-652094-last-message.md` (design)
