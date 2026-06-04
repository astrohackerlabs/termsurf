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

# Experiment 395: the cursor's own color

## Description

The cursor itself has a color — what `add_cursor` paints the cursor glyph with.
Upstream computes it (`cursor_color`) with a precedence: an **OSC 12** override,
then the **config `cursor-color`** (a `TerminalColor`), then the default
**foreground**. This experiment ports that as `cursor_color`, the companion to
the under-cursor recolor (`cursor_text_color`, Experiment 394). Its `Color`/
`CellForeground`/`CellBackground` resolution is the same selection foreground
arm, so the configured case reuses `selection_colors(...).fg`; it differs from
`cursor_text_color` only in the **OSC 12 override** and the **default** (the
foreground, not the background). This is the CPU color; passing it to
`add_cursor` (and the uniforms/Metal upload) is deferred.

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`):

```zig
const cursor_color = cursor_color: {
    // OSC 12 explicit cursor color.
    if (state.colors.cursor) |v| break :cursor_color v;
    // Configured cursor color.
    if (self.config.cursor_color) |v| switch (v) {
        .color => |color| break :cursor_color color.toTerminalRGB(),
        inline .@"cell-foreground", .@"cell-background" => |_, tag| {
            const fg_style = cursor_style.fg(.{ .default = state.colors.foreground,
                .palette = &state.colors.palette, .bold = self.config.bold_color });
            const bg_style = cursor_style.bg(&state.cursor.cell, &state.colors.palette)
                orelse state.colors.background;
            break :cursor_color switch (tag) {
                .@"cell-foreground" => if (cursor_style.flags.inverse) bg_style else fg_style,
                .@"cell-background" => if (cursor_style.flags.inverse) fg_style else bg_style,
                .color => unreachable,
            };
        },
    };
    // Default: the foreground.
    break :cursor_color state.colors.foreground;
};
self.addCursor(&state.cursor, style, cursor_color);
```

So: the OSC 12 cursor color (`state.colors.cursor`, an explicit `?RGB`) takes
precedence; else the `cursor-color` config (`?TerminalColor`) — an explicit
`.color`, or the under-cursor cell's resolved `fg_style`/`bg_style` (defaulting
to the default background) swapped under `inverse`; else the default
**foreground**. The configured `Some(...)` arms are the same
selection-foreground resolution as `cursor_text_color` (Experiment 394); only
the OSC 12 override and the `None` default (foreground, not background) differ.

## Rust mapping (`roastty/src/renderer/cell.rs`)

```rust
/// Compute the cursor's own color — what `add_cursor` paints the cursor glyph
/// with (upstream's `cursor_color`). Precedence: the OSC 12 override
/// (`osc12_cursor`), then the `cursor-color` config (an explicit color or the
/// under-cursor cell's resolved foreground/background swapped under `inverse`),
/// then the default **foreground**. The configured `Some(...)` resolution is the
/// selection foreground arm (so it reuses [`selection_colors`] `.fg`); only the
/// OSC 12 override and the `None` default differ from [`cursor_text_color`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn cursor_color(
    osc12_cursor: Option<Rgb>,
    config: Option<SelectionColor>,
    cursor_style: TermStyle,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
) -> Rgb {
    // OSC 12 takes precedence over the config and the default.
    if let Some(rgb) = osc12_cursor {
        return rgb;
    }
    match config {
        // No configured cursor color → the default foreground.
        None => default_fg,
        // The `.color`/`.cell-foreground`/`.cell-background` resolution is the
        // selection foreground arm — `Some(...)` never reaches its `None`
        // (default-background) default, so this matches upstream's configured arm.
        Some(cfg) => {
            selection_colors(cursor_style, default_fg, default_bg, palette, bold, None, Some(cfg)).fg
        }
    }
}
```

`osc12_cursor` is the OSC 12 cursor color (upstream `state.colors.cursor`), a
parameter for now (the terminal OSC-12 color state and the wiring into
`add_cursor` are deferred). `config` is the `cursor-color` `?TerminalColor` as
`Option<SelectionColor>`.

## Scope / faithfulness notes

- **Ported (bridged)**: the cursor's own color (upstream's `cursor_color`) as
  `cursor_color` — the OSC 12 → config → default-foreground precedence.
- **Faithful**: the OSC 12 override wins; the config `None` → the default
  foreground (upstream's `state.colors.foreground`); the config `Some`
  resolution (`Color`, `CellForeground`/`CellBackground` swapped under
  `inverse`, the background defaulting to the default background) is the
  selection foreground arm, so it reuses `selection_colors(...).fg` —
  `Some(cfg)` never triggers that function's `None`→default-background default,
  so the result equals upstream's configured arm.
- **Faithful adaptation**: `cursor_color` differs from `cursor_text_color`
  (Experiment 394) only in the OSC 12 override and the `None` default
  (foreground vs background); both reuse the one `TerminalColor` foreground
  resolution. The OSC 12 color and the `cursor-color` config are parameters
  (`Option<Rgb>` / `Option<SelectionColor>`) — the terminal color state is not
  modeled here.
- **Deferred**: passing the computed color into `add_cursor` (the cursor draw
  already takes a `color` — wiring the source is deferred); the OSC 12 terminal
  color state; the block-cursor uniforms and the Metal upload; the
  column-ordered decoration merge + link double-underline. (Consumed by tests
  now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: add the `cursor_color` function.
2. Tests (in `cell.rs`): a `cursor_color_*` test over a cell with explicit SGR
   `fg = a` / `bg = b`:
   - **OSC 12 set** (`Some(osc)`) → `osc`, even when a config is also set
     (precedence);
   - **no OSC 12, `None` config** → `default_fg`;
   - **`Color(c)`** → `c`;
   - **`CellForeground`** → `a` (non-inverse) / `b` (inverse);
   - **`CellBackground`** → `b` (non-inverse) / `a` (inverse);
   - a **no-explicit-bg** cell with `CellBackground` non-inverse → `default_bg`
     (the background defaults to the default background).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty cursor_color
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `cursor_color` computes the cursor's color with the OSC 12 → config →
  default-foreground precedence, the config `Some` resolution matching the
  selection foreground arm — faithful to upstream's `cursor_color`;
- the tests pass (the OSC 12 precedence, the `None` default-foreground, the
  config matrix incl. inverse and the no-bg default), and the existing tests
  still pass;
- the OSC 12 state, the `add_cursor` wiring, and the Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the precedence is wrong (OSC 12 not winning, the
`None` default being the background instead of the foreground), an arm is wrong
(the inverse swap inverted), or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the precedence is faithful (OSC 12 cursor color wins,
then the configured `cursor-color`, then the default foreground) and that
handling `None => default_fg` separately is necessary and correctly
distinguishes this from `cursor_text_color` (where `None => default_bg`). It
confirmed that reusing `selection_colors(..., None, Some(cfg)).fg` for the
configured arm is valid — `Some(cfg)` never reaches the selection foreground
arm's `None => default_bg` fallback, and the
`Color`/`CellForeground`/`CellBackground` behavior matches upstream (including
`final_bg = bg_style.unwrap_or(default_bg)` and the inverse swaps). It judged
the parameter shape right for this slice (`Option<Rgb>` for the OSC 12 state,
`Option<SelectionColor>` for the config), the deferral of the OSC 12 state
plumbing / `add_cursor` wiring / Metal upload reasonable, and the tests
sufficient (the precedence and the color-resolution matrix).

Review artifacts:

- Prompt: `logs/codex-review/20260603-210547-733551-prompt.md` (design)
- Result: `logs/codex-review/20260603-210547-733551-last-message.md` (design)

## Result

**Result:** Pass

The cursor's own color is now computable.

- `roastty/src/renderer/cell.rs`:
  `cursor_color(osc12_cursor, config, cursor_style, default_fg, default_bg, palette, bold) -> Rgb`
  — the OSC 12 override (`Some(rgb)`) wins; else a `None` config → the default
  **foreground**; else the configured `Some(cfg)` reuses
  `selection_colors(cursor_style, default_fg, default_bg, palette, bold, None, Some(cfg)).fg`
  (the selection foreground arm; `Some` never hits its `None`→default-background
  default). `pub(crate)` and not yet called in production (the OSC 12 state, the
  `add_cursor` wiring, and the Metal upload are deferred), but reachable in the
  library crate, so no dead-code warning.

Test (in `cell.rs`): `cursor_color_resolves_with_precedence` — over a cell with
explicit SGR `fg = a` / `bg = b`: OSC 12 `Some(osc)` wins even with a config
set; no OSC 12 + `None` config → `default_fg` (foreground, distinct from
cursor-text's `default_bg`); `Color(c1)` → `c1`; `CellForeground` → `a`
(non-inverse) / `b` (inverse); `CellBackground` → `b` (non-inverse) / `a`
(inverse); a no-explicit-bg `CellBackground` non-inverse → `default_bg`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2854 passed, 0 failed (+1, no regressions; the
  `selection_colors` tests guard the shared resolution).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer) clean; `git diff --check` clean.

## Conclusion

The cursor's own color is now ported faithfully as `cursor_color`, the companion
to the under-cursor recolor (`cursor_text_color`): the OSC 12 override → the
`cursor-color` config (the shared selection-foreground resolution) → the default
foreground. Both cursor colors reuse the one `TerminalColor` foreground
resolution, differing only in the OSC 12 override and the `None` default. Like
the other color computations, this is CPU-side; passing it into `add_cursor` and
the uniforms is part of the deferred Metal upload.

The remaining renderer-bridge work: the block-cursor uniforms (`cursor_pos`/
`cursor_wide`/`cursor_color`) and wiring the cursor colors into `add_cursor`
(with the Metal upload); the column-ordered decoration merge + link
double-underline; and the **Metal upload** of `Contents`.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation matches the approved design:
OSC 12 wins first, a `None` config falls back to `default_fg`, and the
configured `SelectionColor` values reuse
`selection_colors(..., None, Some(cfg)).fg`, which preserves upstream's
`Color`/`CellForeground`/`CellBackground` behavior including the inverse swaps
and `final_bg`. It confirmed the test covers the required precedence and
resolution matrix — including the important distinction from `cursor_text_color`
(`None → default_fg` here, not `default_bg`) and the no-explicit-background
fallback — with the change internal Rust only and no public C ABI/header change.
Nothing needed to change before the result commit.

Review artifacts:

- Result review: `logs/codex-review/20260603-210743-073925-last-message.md`
