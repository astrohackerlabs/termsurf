# Experiment 148: Window Padding Layout Runtime

## Description

`RUNTIME-008B2B2` still owns several renderer-visible gaps: background blur,
real compositor opacity, window padding layout pixels, GUI cursor pixels, custom
shader output, and broader GUI/pixel parity. A narrow deterministic slice inside
that row is `window-padding-x`, `window-padding-y`, and `window-padding-balance`
after config parsing:

- pinned Ghostty stores the parsed padding fields in `Surface.DerivedConfig`;
- `DerivedConfig.scaledPadding` converts configured point padding to physical
  pixels with `floor(configured * dpi / 72)`;
- surface init applies explicit scaled padding directly when
  `window-padding-balance = false`;
- surface init and resize apply `renderer.size.Size.balancePadding` when balance
  is `true` or `equal`;
- content-scale changes update unbalanced padding because explicit point padding
  depends on DPI;
- renderer grid size is computed from `screen - padding`, not the full screen.
- PTY rows/columns come from that padded grid size, so shell programs and the
  renderer agree about terminal dimensions.

Roastty already has the parsed config fields and a faithful
`roastty/src/renderer/size.rs` value-level port for `Size`, `Padding`,
`GridSize`, `ScreenSize`, and coordinate conversion. The active live renderer
path, however, still uses `Padding::default()` when calling
`FrameRenderer::update_screen`, and derives live columns/rows from the full
surface width/height. This experiment will wire config-derived scaled padding
and balance mode into the live renderer size/grid calculation.

This experiment will split `RUNTIME-008B2B2`:

- `RUNTIME-008B2B2A`: **Oracle complete** for deterministic
  `window-padding-x`/`window-padding-y` scaling, `window-padding-balance` layout
  math, content-scale dependent unbalanced padding updates, and feeding the
  padded `Size`/grid into the active live renderer.
- `RUNTIME-008B2B2B`: **Gap** for remaining renderer-visible GUI/pixel effects:
  background blur, real compositor opacity, GUI cursor pixels, custom shader
  output, and broader GUI/pixel parity. Screenshot-level padding pixel parity
  also remains here until a GUI walkthrough proves it.

This experiment will not claim screenshot parity, native window/chrome padding
behavior, background blur, real compositor opacity, GUI cursor pixels, custom
shader output, or broad renderer/GUI pixel parity.

## Changes

- `roastty/src/renderer/size.rs`
  - Add a config-aware helper, or equivalent small API, that converts
    `Config.window_padding_x`, `Config.window_padding_y`, and
    `Config.window_padding_balance` plus independent X/Y content scale into a
    renderer `Size`.
  - Preserve Ghostty's point-to-pixel conversion:
    `floor(configured_padding * dpi / 72)`, with macOS default DPI of `72`.
    Top/bottom must use Y DPI/scale, and left/right must use X DPI/scale.
  - Preserve Ghostty's balance behavior: `false` keeps explicit scaled padding;
    `true` and `equal` call `Size::balance_padding` with the converted explicit
    padding.
  - Add focused tests for:
    - explicit unbalanced padding at scale 1;
    - explicit unbalanced padding at symmetric scale 2;
    - asymmetric scale proving X padding and Y padding scale independently;
    - `true` balance top-cap/bottom-shift behavior;
    - `equal` balance behavior;
    - grid computation from `screen - padding`;
    - content-scale changes affecting unbalanced padding.
- `roastty/src/lib.rs`
  - Add a surface helper, or equivalent small API, that recomputes
    config-derived renderer size and live row/column state from the active app
    config, surface physical size, current independent X/Y content scale, and
    current cell size.
  - Call that helper before any `pty_size()` use in startup/resize paths that
    can know a cell size, so initial spawn and later `resize_pty` receive padded
    rows/columns.
  - In `present_live`, recompute the same padded renderer `Size` after the live
    font grid is available, use the padded size's `grid()` for live surface
    columns/rows, and keep PTY size state in sync with the renderer grid.
  - Pass the padded `Size` to `FrameRenderer::update_screen` instead of
    `Padding::default()`.
  - Preserve existing live-render behavior when padding is zero or config is
    unavailable.
- `issues/0805-roastty-ghostty-parity/window_padding_layout_runtime_parity.py`
  - Add a static guard checking pinned Ghostty's `DerivedConfig` padding fields,
    `scaledPadding`, init/resize/content-scale balance markers, Roastty's
    config-aware size helper, active live renderer wiring, focused tests, and
    inventory split.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-008B2B2` into `RUNTIME-008B2B2A` and `RUNTIME-008B2B2B`.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223/static runtime guards
  - Update current runtime row counts from 55/49/51/4/4 to 56/50/52/4/4.
  - Update references from `RUNTIME-008B2B2` to `RUNTIME-008B2B2B` where they
    mean the remaining renderer GUI/pixel gap.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- Pinned Ghostty evidence shows config-derived `window-padding-*` values are
  stored in derived config, scaled by DPI, and applied to renderer size on init,
  resize, and content-scale changes.
- Roastty converts parsed padding config to physical pixels with the same
  `floor(points * dpi / 72)` rule.
- Roastty applies X scale/DPI only to left/right padding and Y scale/DPI only to
  top/bottom padding.
- Roastty preserves Ghostty's `window-padding-balance = false`, `true`, and
  `equal` behavior.
- Roastty active live renderer uses padded renderer `Size` and `Size::grid()`
  for `FrameRenderer::update_screen` and live surface row/column state.
- Roastty initial PTY spawn and later PTY resize use the padded rows/columns
  whenever a cell size is available, instead of full-surface rows/columns.
- Existing zero-padding behavior is unchanged.
- `RUNTIME-008B2B2A` is Oracle complete and cites focused tests plus the new
  static guard.
- `RUNTIME-008B2B2B` remains `Gap` for background blur, real compositor opacity,
  GUI cursor pixels, custom shader output, broader GUI/pixel parity, and
  screenshot-level padding pixel proof.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml window_padding_layout_runtime
cargo test --manifest-path roastty/Cargo.toml size_balance_padding
cargo test --manifest-path roastty/Cargo.toml size_grid_and_terminal
cargo test --manifest-path roastty/Cargo.toml coordinate_conversion
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/window_padding_layout_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
for guard in issues/0805-roastty-ghostty-parity/*_runtime_parity.py; do PYTHONDONTWRITEBYTECODE=1 python3 "$guard" || exit 1; done
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/148-window-padding-layout-runtime.md
git diff --check
```

Fail criteria:

- The active live renderer still calls `update_screen` with `Padding::default()`
  for configured padding.
- Live rows/columns are still derived from full surface dimensions when
  configured padding should reduce the terminal area.
- Initial PTY spawn or later PTY resize can receive full-surface rows/columns
  after configured padding should reduce the terminal grid.
- The implementation uses a different scaling rule than
  `floor(points * dpi / 72)`.
- The implementation applies the same scale to both axes instead of preserving
  independent X/Y scaling.
- Balanced padding does not use the Ghostty `Size::balancePadding` behavior.
- The experiment promotes background blur, real compositor opacity, GUI cursor
  pixels, custom shader output, screenshot-level padding pixels, or broad
  GUI/pixel parity from the remaining gap.
- CFG-223 is marked complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Initial verdict:** Changes required.

The reviewer found two required issues:

- the design did not require independent X/Y content-scale conversion even
  though pinned Ghostty scales top/bottom from Y DPI and left/right from X DPI;
- the design claimed PTY size state but did not explicitly design or verify the
  startup/resize paths that call `pty_size()` before `present_live`.

**Fix:** Updated the design to require separate X/Y scale or DPI values,
asymmetric-scale test coverage, active live renderer wiring with both
`scale_factor_x` and `scale_factor_y`, and explicit startup/resize PTY sizing
proof whenever a cell size is available.

**Final verdict:** Approved.

The reviewer confirmed both prior findings were resolved and no new required
findings were introduced.
