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

# Experiment 384: the background-cell alpha

## Description

`rebuild_bg_row` (Experiment 382) writes a cell's background from
`cell_colors(...).bg` at a uniform `alpha` for every `Some` background and a
hard transparent `[0,0,0,0]` for `None`. Upstream **always** writes the
background cell — `rgb = bg orelse default_bg`, then a separately-computed
**`bg_alpha`**. That alpha is opaque (255) for inverse and explicit-background
cells but **transparent (0)** otherwise — so a covering-derived background (a
full block with no explicit background) draws transparent (the already-drawn
screen background shows through), while an inverse cell draws **opaque** even
when its resolved background is `None` (it falls back to the default
background). This experiment ports that `bg_alpha` logic.

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), the background cell is written
**unconditionally** (there is no `bg != null` guard) as
`{ rgb.r, rgb.g, rgb.b, bg_alpha }`, where the RGB falls back to the default
background:

```zig
const rgb = bg orelse state.colors.background;
const bg_alpha: u8 = bg_alpha: {
    const default: u8 = 255;
    if (selected != .false) break :bg_alpha default;          // selection → opaque
    if (style.flags.inverse) break :bg_alpha default;         // inverse → opaque
    if (config.background_opacity_cells and bg_style != null) // opacity config
        break :bg_alpha @intFromFloat(255 * config.background_opacity);
    if (bg_style != null) break :bg_alpha default;            // explicit bg → opaque
    break :bg_alpha 0;                                        // else → transparent
};
self.cells.bgCell(y, x).* = .{ rgb.r, rgb.g, rgb.b, bg_alpha };
```

`bg_style` is the cell's **original** resolved background (`style.bg(palette)`,
`Some` only for an explicit `Palette`/`Rgb` background) — distinct from the
final `bg` (which can come from the foreground via the inverse/covering twist).
So: a cell with an explicit background, or an inverse cell, draws its background
opaque; a covering-derived background (a full block whose `bg` came from the
foreground via the covering twist, with no explicit `bg_style`) draws
transparent. Crucially the `bg_alpha` branches are evaluated **independently of
whether `bg` is `null`**: an inverse cell with no explicit background still
draws an opaque default background (its final `bg` is `None`, so
`rgb = default_bg`, but `bg_alpha = 255`).

## Rust mapping (`roastty/src/renderer/cell.rs`)

`rebuild_bg_row` writes every cell's background **unconditionally**: the RGB is
`cell_colors(...).bg` falling back to `default_bg`, and `bg_alpha` is computed
independently (the selection and `background_opacity_cells` branches are
deferred — never-selected and no opacity config):

```rust
for (col, cell) in row_cells.iter().enumerate() {
    let colors = cell_colors(cell.style, cell.codepoint, default_fg, default_bg, palette, bold);
    // Opaque (the base alpha) for an inverse cell or one with an explicit
    // background; a covering-derived or default background is transparent. This
    // is evaluated independently of whether the final bg is `Some` (upstream's
    // bg_alpha branches run regardless of `bg == null`).
    let has_explicit_bg = !matches!(cell.style.bg_color, Color::None);
    let bg_alpha = if cell.style.flags.inverse || has_explicit_bg { alpha } else { 0 };
    // The RGB falls back to the default background (upstream `bg orelse default`).
    let rgb = colors.bg.unwrap_or(default_bg);
    *contents.bg_cell_mut(row, col) = CellBg([rgb.r, rgb.g, rgb.b, bg_alpha]);
}
```

Note the `None`-background path is no longer a hard `[0,0,0,0]`: it becomes
`[default_bg.r, default_bg.g, default_bg.b, bg_alpha]`. For the common
non-inverse default cell `bg_alpha = 0`, so it is still transparent (and
identical to `[0,0,0,0]` when `default_bg` is black); for an inverse cell with
no explicit background it is the **opaque** default background.

## Scope / faithfulness notes

- **Ported (bridged)**: the per-cell background alpha and the unconditional
  background-cell write — opaque (the base `alpha`) for an inverse cell or one
  with an explicit background, transparent (0) otherwise, with the RGB falling
  back to `default_bg` — matching upstream's `rgb = bg orelse default` plus
  `bg_alpha` (sans the deferred branches).
- **Faithful**: `has_explicit_bg` is `style.bg_color != Color::None`, equivalent
  to upstream's `bg_style != null` (the original resolved background is `Some`
  only for an explicit color); the `bg_alpha` decision is
  `inverse || has_explicit_bg`, evaluated independently of the final `bg`,
  exactly as upstream evaluates its branches regardless of `bg == null`. This
  corrects two cases: the **non-inverse full-block-without-bg** cell (previously
  opaque, now transparent) and the **inverse-without-explicit-bg** cell (its
  final `bg` is `None`, so it now draws an opaque default background instead of
  being clamped to `[0,0,0,0]`).
- **Faithful adaptation**: the opaque alpha is the renderer's base `alpha`
  (upstream's `default = 255`); roastty checks `style.bg_color` directly (the
  same information as `bg_style != null`) rather than re-resolving; the RGB
  falls back to `default_bg` (upstream's `bg orelse state.colors.background`).
- **Deferred**: the selection/search → opaque branch (no selection state yet)
  and the `background_opacity_cells` opacity scaling (a transparency config);
  the Metal upload. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: compute `bg_alpha` in `rebuild_bg_row`
   (opaque for inverse/explicit-bg, transparent otherwise) and write the
   background unconditionally with `rgb = colors.bg.unwrap_or(default_bg)`;
   import `terminal::style::Color`.
2. Tests (in `cell.rs`):
   - an **explicit-bg** cell (`bg = Palette/Rgb`, non-inverse) →
     `CellBg([bg, alpha])` (opaque — unchanged behavior; covered by the existing
     tests);
   - an **inverse** cell with an explicit bg → its swapped background opaque
     (covered by the existing `rebuild_viewport_applies_inverse`);
   - a **full block** (`U+2588`) with **no** explicit background, non-inverse →
     `CellBg([fg.r, fg.g, fg.b, 0])` (transparent — the new bg_alpha fix);
   - an **inverse full block** (`U+2588`) with **no** explicit background, drawn
     with a **non-black `default_bg`** →
     `CellBg([default_bg.r, default_bg.g, default_bg.b, alpha])` (opaque default
     background — proves the inverse branch fires even though the final `bg` is
     `None`, and that the RGB falls back to `default_bg`);
   - a default-background non-inverse cell → transparent (`[0,0,0,0]` when
     `default_bg` is black — covered by the existing tests).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty rebuild_bg_row
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `rebuild_bg_row` writes every background unconditionally
  (`rgb = bg orelse default_bg`) and draws it opaque for inverse/explicit-bg
  cells and transparent for a covering-derived (full-block, no-bg) cell —
  faithful to upstream's `bg_alpha`;
- the tests pass (explicit-bg opaque, full-block-no-bg transparent,
  inverse-full-block-no-bg opaque default background, default transparent), and
  the existing tests still pass;
- the selection/opacity branches and the Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the alpha is wrong (a covering bg drawn opaque, an
explicit bg drawn transparent, an inverse-no-bg cell drawn transparent), or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it (after a
revision) with one **Required** finding, now addressed:

- **Required (addressed):** the first draft early-mapped a `None` final
  background to a hard transparent `[0,0,0,0]` _before_ considering `bg_alpha`.
  That is not faithful: upstream writes the background cell **unconditionally**
  (`rgb = bg orelse default_bg`) and computes `bg_alpha` independently of
  whether `bg` is `null`. So an **inverse** cell with no explicit background
  (final `bg` is `None`) must draw an **opaque default background**, not
  transparent. The mapping now uses `rgb = colors.bg.unwrap_or(default_bg)` and
  `bg_alpha = (inverse || has_explicit_bg) ? alpha : 0`, and a new test covers
  the inverse full-block / no-explicit-bg case (with a non-black `default_bg`,
  asserting an opaque default-background `CellBg`).

Codex confirmed the rest is faithful: (1) `style.bg_color != Color::None` is
equivalent to upstream's `bg_style != null` (`Palette`/`Rgb` resolve to `Some`,
`None` to `None`); (2) the non-inverse full-block / no-explicit-bg case
correctly carries the foreground RGB at alpha `0`; (3) deferring selection and
`background_opacity_cells` leaves no hole for the non-selected,
no-opacity-config common case as long as the inverse branch is honored
independently of the final `bg` (now the case); (4) the test set is sufficient
with the added inverse full-block case, and the existing `rebuild_bg_row` tests
are preserved (they pass a black `default_bg`, so the transparent path still
yields `[0,0,0,0]`).

Review artifacts:

- Prompt: `logs/codex-review/20260603-194839-346613-prompt.md` (design)
- Result: `logs/codex-review/20260603-194839-346613-last-message.md` (design)
