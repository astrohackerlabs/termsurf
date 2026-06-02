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

# Experiment 225: Port Renderer Size, PaddingBalance, and Coordinate

## Description

Complete the port of upstream `renderer/size.zig` by adding the pieces deferred
from Experiment 224: the `Size` aggregate, the `PaddingBalance` enum with
`Size::balance_padding`, and the `Coordinate` tagged union with `convert`. These
build directly on the `CellSize`/`ScreenSize`/`GridSize`/`Padding` value types
landed in 224 and finish the renderer sizing/coordinate model that later
geometry slices (cell quads, cursor placement, mouse hit-testing) depend on.

This is a coherent slice on one surface (the existing `renderer/size.rs`) with
predictable tests — the three upstream tests deferred from Experiment 224 port
directly here — so it fits the issue's risk-based sizing rule.

### Types and behavior to port

- `PaddingBalance` enum (upstream `false`/`true`/`equal`; Rust capitalizes to
  avoid keywords): `False` (no balancing), `True` (balance but cap top padding,
  shifting excess vertical space to the bottom), `Equal` (centre the grid by
  distributing leftover space equally).
- `Size { screen: ScreenSize, cell: CellSize, padding: Padding }` — fields are
  `pub` (crate-visible through the `pub(crate)` struct, like the Experiment 224
  value types) so sibling renderer modules can construct and read a `Size`:
  - `grid() -> GridSize` = `GridSize::init(screen.sub_padding(padding), cell)`;
  - `terminal() -> ScreenSize` = `screen.sub_padding(padding)`;
  - `balance_padding(&mut self, explicit: Padding, mode: PaddingBalance)`: set
    `padding = explicit`, then
    `padding = Padding::balanced(screen, grid(), cell)`, then per `mode`:
    - `False` → upstream `unreachable`; mirror as `unreachable!()`;
    - `Equal` → no further change;
    - `True` → cap top padding to
      `max_top = (explicit.left + explicit.right + cell.width) / 2`; compute
      `vshift = padding.top -| max_top` (saturating), then
      `padding.top -= vshift; padding.bottom += vshift`.
- `Coordinate` tagged union with `Surface { x: f64, y: f64 }`,
  `Terminal { x: f64, y: f64 }`, `Grid { x: Unit, y: Unit }`, plus a
  `CoordinateTag` (`Surface`/`Terminal`/`Grid`) used as the `convert` target:
  - `convert(self, to: CoordinateTag, size: Size) -> Coordinate`: identity
    fast-path when already in `to`; otherwise normalize to surface coordinates
    and reconvert. Surface→terminal subtracts `(padding.left, padding.top)`;
    surface→grid converts to terminal, clamps to `>= 0`, divides by cell size,
    truncates, then clamps to `grid.columns - 1` / `grid.rows - 1`.
  - private `convert_to_surface(self, size) -> (f64, f64)`: terminal adds the
    padding offset; grid multiplies cell size and adds the padding offset.

### Faithfulness notes

- Coordinate systems carry `f64`, so `Coordinate` derives `PartialEq` (not
  `Eq`); `CoordinateTag` derives `Eq`.
- `@intFromFloat` (col/row) maps to a truncating `as Unit` cast on a value
  already clamped to `>= 0`; `@min(col, grid.columns - 1)` maps to
  `col.min(grid.columns - 1)`. `grid()` always yields at least `1` column/row,
  so `grid.columns - 1` cannot underflow.
- The upstream surface→grid path recurses through `convert(.terminal, ...)`; the
  port computes the terminal coordinate directly
  (`surface - (padding.left, padding.top)`), which is identical, to avoid an
  internal re-dispatch.
- `balance_padding`'s `True` branch uses saturating `-|` for `vshift`, then
  plain `-`/`+` (safe because `vshift <= padding.top`).

### Scope limits

- Only `renderer/size.rs` changes; no C ABI, header, or ABI inventory changes;
  no new dependencies.

## Changes

1. Extend `roastty/src/renderer/size.rs`:
   - Add `pub(crate) enum PaddingBalance { False, True, Equal }`
     (`Debug, Clone, Copy, PartialEq, Eq`).
   - Add
     `pub(crate) struct Size { pub screen: ScreenSize, pub cell: CellSize, pub padding: Padding }`
     (`Debug, Clone, Copy, PartialEq, Eq`; `pub` fields, matching the Exp 224
     value types) with `grid`, `terminal`, and `balance_padding`.
   - Add `pub(crate) enum CoordinateTag { Surface, Terminal, Grid }`
     (`Debug, Clone, Copy, PartialEq, Eq`) and
     `pub(crate) enum Coordinate { Surface { x: f64, y: f64 }, Terminal { x: f64, y: f64 }, Grid { x: Unit, y: Unit } }`
     (`Debug, Clone, Copy, PartialEq`), with `tag`, `convert`, and the private
     `convert_to_surface`.
   - Keep the module `#![allow(dead_code)]` and the "upstream
     `renderer/size.zig`" attribution.

2. Port the upstream tests deferred from Experiment 224:
   - `size_balance_padding_equal_distributes_whitespace_equally` (upstream
     "Size.balancePadding equal distributes whitespace equally"): screen
     1050x850, cell 10x20, explicit 4 each side, mode `Equal` → `left == right`,
     `top == bottom`, `top > 0`.
   - `size_balance_padding_true_shifts_excess_top_to_bottom` (upstream
     "Size.balancePadding true shifts excess top to bottom"): screen 1090x1070,
     cell 20x40, explicit 0, mode `True` → `left == right`, `top < bottom`,
     `top == 10`, `bottom == 20`.
   - `coordinate_conversion` (upstream "coordinate conversion"): with screen
     100x100, cell 5x10, padding 0, convert each `surface` to `grid` and check
     against the expected grid coordinate, including the negative-clamp-to-0 and
     the large-value-clamp-to-`(columns-1, rows-1)` cases.
   - Add a couple of direct checks: `size_grid_and_terminal` (`grid()` and
     `terminal()` remove padding correctly) and a surface↔terminal round-trip
     via `convert`.

3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty renderer::size
cargo test -p roastty renderer
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/size.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Size`, `PaddingBalance`, `Size::balance_padding`, `Coordinate`,
  `CoordinateTag`, and `convert` are implemented with exact upstream semantics
  (balanced-then-capped padding for `True`, surface-normalized coordinate
  conversion with the grid clamp);
- the three deferred upstream tests pass (notably `True` yielding `top == 10`,
  `bottom == 20`, and the coordinate table), plus the added grid/terminal and
  round-trip checks;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `Coordinate::convert` exposes a needed `Size`
behavior that should be reordered into its own experiment.

The experiment **fails** if the `True` padding cap math, the coordinate clamp,
or the surface-normalization diverges from upstream, or if any public API/ABI
changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-071603-308134-prompt.md`
- Result: `logs/codex-review/20260602-071603-308134-last-message.md`

Codex confirmed the `PaddingBalance::True` math is faithful and the test
expectation holds (screen 1090x1070, cell 20x40, explicit 0 → grid 54x26,
balanced top/bottom = 15, `max_top = 10`, `vshift = 5`, final `top = 10`,
`bottom = 20`); that the `Coordinate`/`CoordinateTag` model and the direct
surface−padding terminal computation are equivalent to upstream's recursive
path; that the surface→grid clamp is correct and `grid.columns - 1` cannot
underflow (grid is min-1); that `Coordinate` deriving `PartialEq` (not `Eq`) is
correct; and that `unreachable!()` for `PaddingBalance::False` is an acceptable
port.

One real finding, fixed in the design above before this commit:

1. **(Medium)** the design did not specify field visibility for `Size`. Sibling
   renderer modules must be able to construct and read it, so `Size`'s fields
   are now `pub` (crate-visible through the `pub(crate)` struct), matching the
   Experiment 224 value types.

## Result

**Result:** Pass

Extended `roastty/src/renderer/size.rs` with the remaining `renderer/size.zig`
surface, completing the port of that file:

- `PaddingBalance { False, True, Equal }`;
- `Size { pub screen, pub cell, pub padding }` with `grid()`, `terminal()`, and
  `balance_padding(explicit, mode)` — sets explicit padding, recomputes balanced
  padding from `grid()`, then for `True` caps top padding to
  `(explicit.left + explicit.right + cell.width) / 2` and shifts the excess
  (saturating `vshift`) from top to bottom; `False` is `unreachable!()`;
- `CoordinateTag { Surface, Terminal, Grid }` and
  `Coordinate { Surface{x,y:f64}, Terminal{x,y:f64}, Grid{x,y:Unit} }` with
  `convert(to, size)` (identity fast-path, then surface-normalized reconvert
  with the surface→grid clamp) and the private `convert_to_surface`.

`Size` derives `Eq`; `Coordinate` derives `PartialEq` only (it carries `f64`).
The surface→terminal step is computed directly as
`surface − (padding.left, padding.top)`, equivalent to upstream's recursive
`convert(.terminal)`.

Tests added (5): `size_balance_padding_equal_distributes_whitespace_equally`,
`size_balance_padding_true_shifts_excess_top_to_bottom`,
`size_grid_and_terminal`, `coordinate_conversion` (the upstream surface→grid
table including the negative-clamp and large-value-clamp cases), and
`coordinate_surface_terminal_round_trip`.

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty renderer::size
cargo test -p roastty renderer
cargo test -p roastty
```

Observed:

- `renderer::size`: 17 passed (12 from Exp 224 + 5 new).
- Full `roastty`: 2232 unit tests passed (2227 prior + 5 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/renderer/size.rs` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
should change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-071944-183805-prompt.md`
- Result: `logs/codex-review/20260602-071944-183805-last-message.md`

Codex confirmed `Size::balance_padding` is faithful (explicit → balanced from
`grid()` → `True` cap with saturating `vshift` shifted top→bottom; `False`
`unreachable!()`; `Equal` no-op), that
`Coordinate::convert`/`convert_to_surface` match upstream (surface↔terminal
padding offset, grid↔surface cell scaling, and the surface→grid clamp), that the
direct surface−padding terminal computation is equivalent to upstream's
recursive path, and that the 5 tests are correct (including the upstream
coordinate table and the `top=10`/`bottom=20` `True` case). The `f64`→`u16`
caveat is the same accepted one as Experiment 224.

## Conclusion

Experiment 225 succeeds and **completes the port of `renderer/size.zig`**.
Together with Experiment 224, Roastty's `renderer::size` module now holds the
full sizing/coordinate model — `CellSize`, `ScreenSize`, `GridSize`, `Padding`,
`Size`, `PaddingBalance`, and `Coordinate` with surface/terminal/grid conversion
— with all six upstream tests ported and passing (17 tests total). Both Codex
gates passed (one design finding fixed; zero result findings).

This is the coordinate foundation later renderer geometry slices need. The next
renderer slice is likely `renderer/cell.zig` (building per-cell render data:
backgrounds, glyph quads, and cursor cells) or `renderer/State.zig` (the
renderer state struct), both of which consume this `Size`/`Coordinate` model and
the Experiment 223 cursor-style decision.
