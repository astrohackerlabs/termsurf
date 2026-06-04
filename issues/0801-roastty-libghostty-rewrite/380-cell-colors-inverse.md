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

# Experiment 380: per-cell colors with reverse-video

## Description

`rebuild_row`/`rebuild_bg_row` currently color cells directly from `resolve_fg`/
`resolve_bg`, ignoring the **reverse-video** (`inverse`) flag that swaps a
cell's foreground and background. This experiment ports the base per-cell color
computation, `cell_colors`: resolve the cell's foreground and background, then —
when `inverse` is set — swap them. This is the core of upstream's per-cell
`fg`/`bg` derivation in `rebuildCells`; the selection/search/min-contrast/
full-block nuances are deferred. A follow-up wires `cell_colors` into the row
passes.

## Upstream behavior

`rebuildCells` (`renderer/generic.zig`) computes each cell's colors from the
resolved styles `fg_style = style.fg(...)` and `bg_style = style.bg(palette)`,
then (for the common, non-selected case):

```zig
// background:
.false => if (style.flags.inverse != isCovering(cell.codepoint()))
    fg_style                                    // inverse → the fg becomes the bg
else
    bg_style,
// foreground:
const final_bg = bg_style orelse state.colors.background;
.false => if (style.flags.inverse) final_bg else fg_style,  // inverse → the bg becomes the fg
```

So with `inverse` (and ignoring the `isCovering` full-block twist): the cell's
**foreground** becomes its resolved **background** (or the default background
when none), and the cell's **background** becomes its resolved **foreground**.
Without `inverse`, the colors are the resolved styles unchanged.

## Rust mapping (`roastty/src/renderer/cell.rs`)

```rust
/// A cell's final foreground and background colors. `bg = None` means the default
/// background (transparent slot — the screen background shows through).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellColors {
    pub fg: Rgb,
    pub bg: Option<Rgb>,
}

/// Compute a cell's final colors from its `style`, applying reverse-video
/// (`inverse`): the foreground and background swap. Faithful port of the base
/// (non-selection) per-cell color computation in upstream `rebuildCells`. The
/// `isCovering` full-block twist, selection/search colors, and minimum-contrast
/// are deferred.
pub(crate) fn cell_colors(
    style: Style,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
) -> CellColors {
    let fg_style = style.resolve_fg(default_fg, palette, bold);
    let bg_style = style.resolve_bg(palette);

    if style.flags.inverse {
        // The background becomes the foreground (default bg when the cell has no
        // explicit bg), and the foreground becomes the background.
        let final_bg = bg_style.unwrap_or(default_bg);
        CellColors {
            fg: final_bg,
            bg: Some(fg_style),
        }
    } else {
        CellColors {
            fg: fg_style,
            bg: bg_style,
        }
    }
}
```

## Scope / faithfulness notes

- **Ported (bridged)**: the base per-cell color computation — resolve the
  foreground (`resolve_fg`) and background (`resolve_bg`), and apply the
  reverse-video swap when `inverse` is set, with the default background filling
  a `None` background under inverse.
- **Faithful**: without `inverse`, the colors are the resolved styles unchanged
  (`fg = fg_style`, `bg = bg_style`); with `inverse`,
  `fg = bg_style.unwrap_or (default_bg)` and `bg = Some(fg_style)` — upstream's
  `fg = if inverse final_bg else fg_style` and
  `bg = if inverse fg_style else bg_style` (sans the `isCovering` term).
  `bg = None` (no explicit background) stays the default in the non-inverse
  case.
- **Faithful adaptation**: `cell_colors` returns a small `CellColors { fg, bg }`
  the row passes will consume (replacing their direct
  `resolve_fg`/`resolve_bg`). It takes the renderer's
  `default_fg`/`default_bg`/`palette`/`bold` config.
- **Deferred**: the `isCovering` (U+2588 full-block) twist on the background
  swap; the selection/search background and foreground colors; the
  minimum-contrast adjustment; the faint/dim alpha; and the integration into
  `rebuild_row`/`rebuild_bg_row` (a follow-up). (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: add the `CellColors` struct and the
   `cell_colors` function.
2. Tests (in `cell.rs`):
   - a **non-inverse** cell (`fg = Color::Rgb(a)`, `bg = Color::Rgb(b)`):
     `cell_colors` returns `{ fg: a, bg: Some(b) }`;
   - an **inverse** cell with the same `fg`/`bg`: returns
     `{ fg: b, bg: Some(a) }` (swapped);
   - an **inverse** cell with **no** background (`bg = Color::None`): returns
     `{ fg: default_bg, bg: Some(a) }` (the default background fills the
     foreground);
   - a **non-inverse** cell with no background: returns `{ fg: a, bg: None }`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty cell_colors
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `cell_colors` returns the resolved foreground/background, swapped under
  `inverse` (with the default background filling a `None` background under
  inverse) — faithful to the base per-cell color computation in upstream
  `rebuildCells`;
- the tests pass (non-inverse unchanged; inverse swaps; inverse with no bg uses
  the default), and the existing tests still pass;
- the `isCovering` twist, selection/search/min-contrast, and the integration
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the inverse swap is wrong (wrong direction, or the
default background mishandled), the non-inverse case changes the colors, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the base inverse swap is faithful to upstream's
non-selected, non-`isCovering` case (non-inverse returns the resolved `fg`/`bg`
unchanged; inverse uses `fg = bg_style.unwrap_or(default_bg)` and
`bg = Some(fg_style)`, matching the upstream swap direction and correctly
turning a default background into an explicit foreground under reverse-video);
that deferring `isCovering` is acceptable here because `cell_colors` is not yet
wired into the row passes and the scope explicitly limits this to the base
computation; that `bg: Option<Rgb>` is the right shape (`None` is the
default/transparent slot for non-inverse, while inverse necessarily returns
`Some(fg_style)`); and that the tests cover the important cases (unchanged
non-inverse, swapped inverse, inverse with no bg using `default_bg`, non-inverse
with no bg staying `None`).

Note for the follow-up: the `isCovering` (U+2588) twist on the background must
be handled when `cell_colors` is wired into `rebuild_row`/`rebuild_bg_row`,
since full-block cells would otherwise color wrongly.

Review artifacts:

- Prompt: `logs/codex-review/20260603-191809-716430-prompt.md` (design)
- Result: `logs/codex-review/20260603-191809-716430-last-message.md` (design)
