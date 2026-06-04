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

# Experiment 415: the screen-size uniform update (ortho2d + update_screen_size)

## Description

`FrameState::sync` (Experiment 413) uploads a `MetalUniforms` to the GPU, but
roastty only has `#[cfg(test)]` constructors for it (with an **identity**
projection). The production per-frame uniforms must carry the real orthographic
projection and the grid padding derived from the screen/grid/cell sizes.
Upstream computes these in `updateScreenSizeUniforms` (called whenever the
screen size changes). This experiment ports that: a production `ortho2d` (the 2D
orthographic projection matrix) and `MetalUniforms::update_screen_size`, which
sets the three screen-derived uniform fields — `projection_matrix`,
`grid_padding`, and `screen_size`. The other uniform groups (cell/grid size,
min-contrast, colors, cursor, bools) are set by their own updates and stay out
of scope here, exactly as upstream splits them.

## Upstream behavior

`math.ortho2d` (`src/math.zig`) is the standard 2D orthographic projection:

```zig
pub fn ortho2d(left: f32, right: f32, bottom: f32, top: f32) Mat {
    const w = right - left;
    const h = top - bottom;
    return .{
        .{ 2 / w, 0, 0, 0 },
        .{ 0, 2 / h, 0, 0 },
        .{ 0.0, 0.0, -1.0, 0.0 },
        .{ -(right + left) / w, -(top + bottom) / h, 0.0, 1.0 },
    };
}
```

`updateScreenSizeUniforms` (`renderer/generic.zig`) sets the screen-derived
fields:

```zig
const terminal_size = self.size.terminal();
const blank = self.size.screen.blankPadding(self.size.padding,
    .{ .columns = …, .rows = … }, .{ .width = cell_width, .height = cell_height })
    .add(self.size.padding);
self.uniforms.projection_matrix = math.ortho2d(
    -1 * padding.left,
    terminal_size.width + padding.right,
    terminal_size.height + padding.bottom,
    -1 * padding.top,
);
self.uniforms.grid_padding = .{ blank.top, blank.right, blank.bottom, blank.left };
self.uniforms.screen_size = .{ screen.width, screen.height };
```

So it touches exactly three uniform fields, derived from `size` (screen,
padding, cell) and the grid (columns/rows).

## Rust mapping (`roastty/src/renderer/metal/shaders.rs`)

roastty's `Size` (screen/cell/padding), `Size::terminal`,
`ScreenSize::blank_padding`, and `Padding::add` are all ported. `ortho2d` is a
free function and `update_screen_size` a method on `MetalUniforms`:

```rust
pub(crate) fn ortho2d(left: f32, right: f32, bottom: f32, top: f32) -> [[f32; 4]; 4] {
    let w = right - left;
    let h = top - bottom;
    [
        [2.0 / w, 0.0, 0.0, 0.0],
        [0.0, 2.0 / h, 0.0, 0.0],
        [0.0, 0.0, -1.0, 0.0],
        [-(right + left) / w, -(top + bottom) / h, 0.0, 1.0],
    ]
}

impl MetalUniforms {
    /// Update the screen-size-derived uniform fields (upstream
    /// `updateScreenSizeUniforms`): the orthographic `projection_matrix`, the
    /// `grid_padding` (the blank space around the grid), and the `screen_size`.
    pub(crate) fn update_screen_size(&mut self, size: Size, grid: GridSize) {
        let terminal = size.terminal();
        let blank = size
            .screen
            .blank_padding(size.padding, grid, size.cell)
            .add(size.padding);
        self.projection_matrix = ortho2d(
            -(size.padding.left as f32),
            (terminal.width + size.padding.right) as f32,
            (terminal.height + size.padding.bottom) as f32,
            -(size.padding.top as f32),
        );
        self.grid_padding = [
            blank.top as f32,
            blank.right as f32,
            blank.bottom as f32,
            blank.left as f32,
        ];
        self.screen_size = [size.screen.width as f32, size.screen.height as f32];
    }
}
```

`ortho2d` matches upstream exactly (it is also what the render-pass tests'
identical local helper computes). `update_screen_size` sets only the three
screen-derived fields, in upstream's order, using the same `terminal` / blank
padding derivation; the grid (columns/rows) is a parameter (upstream's
`self.cells.size`).

## Scope / faithfulness notes

- **Ported (bridged)**: a production `ortho2d` (the 2D orthographic projection
  matrix) and `MetalUniforms::update_screen_size` (the screen-size-derived
  uniform fields: `projection_matrix`, `grid_padding`, `screen_size`) —
  upstream's `math.ortho2d` + `updateScreenSizeUniforms`.
- **Faithful**: `ortho2d` is the exact upstream matrix; `update_screen_size`
  computes `terminal = size.terminal()`, the blank padding
  (`screen.blank_padding(padding, grid, cell).add(padding)`), the projection
  from
  `(-padding.left, terminal.width + padding.right, terminal.height + padding.bottom, -padding.top)`,
  the `grid_padding` as `[top, right, bottom, left]`, and the `screen_size` —
  matching upstream field-for-field.
- **Faithful adaptation**: `update_screen_size` mutates an existing
  `MetalUniforms` (upstream mutates `self.uniforms`), and takes the grid as a
  parameter (upstream reads `self.cells.size`). Only the screen-size group is
  touched; the other uniform groups have their own updates (deferred).
- **Deferred**: the other uniform updates (cell/grid size, min-contrast, colors,
  the cursor group, the color-space bools), a full production `MetalUniforms`
  constructor, and the live call site that runs `update_screen_size` on a
  resize. (Consumed by a later slice; this experiment lands and tests the
  screen-size update.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/metal/shaders.rs`:
   - add a `pub(crate) fn ortho2d(left, right, bottom, top) -> [[f32; 4]; 4]`;
   - add
     `MetalUniforms::update_screen_size(&mut self, size: Size, grid: GridSize)`
     setting `projection_matrix`/`grid_padding`/`screen_size`. Import `Size` and
     `GridSize` from `crate::renderer::size`.
2. Tests (in `shaders.rs`):
   - `ortho2d` matches a hand-computed matrix for a sample rectangle (and the
     existing render-pass `ortho2d` helper's formula);
   - `update_screen_size` over a `Size` with explicit padding and a `GridSize`
     sets the three fields to the upstream-derived values (the projection from
     `ortho2d(-left, terminal.width + right, terminal.height + bottom, -top)`,
     the `grid_padding` from the blank padding `[top, right, bottom, left]`, and
     the `screen_size`), and leaves the other fields (e.g. `cell_size`,
     `grid_size`, `bg_color`) untouched.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty update_screen_size
cargo test -p roastty ortho2d
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `ortho2d` is the upstream orthographic matrix, and `update_screen_size` sets
  `projection_matrix`/`grid_padding`/`screen_size` from the size + grid exactly
  as `updateScreenSizeUniforms` does (and touches nothing else) — faithful to
  upstream;
- the tests pass (the `ortho2d` matrix; the three updated fields and the
  untouched others), and the existing tests still pass;
- the other uniform updates and the live resize call site stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the projection or grid-padding derivation differs
from upstream, an unrelated uniform field is changed, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the design is faithful to upstream: `ortho2d` matches
the Zig matrix exactly (including the `bottom`/`top` convention that produces
the negative-Y scale for terminal coordinates), and `update_screen_size` updates
only `projection_matrix`, `grid_padding`, and `screen_size` using the same
inputs and order as `updateScreenSizeUniforms` (`terminal()`,
`screen.blank_padding(...).add(padding)`, projection bounds
`[-left, terminal.width + right, terminal.height + bottom, -top]`, and grid
padding `[top, right, bottom, left]`). It confirmed the `grid` parameter is the
right adaptation for upstream's `self.cells.size`, and that the integer-to-float
casts are faithful (upstream computes integer sums then converts; the sum before
`as f32` matches, and `terminal.width + padding.right` / the height equivalent
are bounded by the screen/padding derivation so there is no practical overflow).
It judged the test plan sufficient (a direct `ortho2d` matrix check, plus an
`update_screen_size` test validating the three updated fields and proving the
unrelated uniform fields are unchanged).

Review artifacts:

- Prompt: `logs/codex-review/20260604-081256-d415-prompt.md` (design)
- Result: `logs/codex-review/20260604-081256-d415-last-message.md` (design)

## Result

**Result:** Pass

The screen-size uniform update is now live.

- `roastty/src/renderer/metal/shaders.rs`: a production
  `ortho2d(left, right, bottom, top) -> [[f32; 4]; 4]` (the upstream 2D
  orthographic projection matrix), and
  `MetalUniforms::update_screen_size(&mut self, size: Size, grid: GridSize)`
  setting `projection_matrix` (from
  `ortho2d(-left, terminal.width + right, terminal.height + bottom, -top)`),
  `grid_padding` (the blank padding `[top, right, bottom, left]`), and
  `screen_size` — upstream's `updateScreenSizeUniforms`, touching only the three
  screen-size fields. Added `use crate::renderer::size::{GridSize, Size};`.

Tests (in `shaders.rs`):

- `ortho2d_matches_upstream_matrix` — `ortho2d(0, 4, 2, 0)` equals the
  hand-computed matrix `[[0.5,0,0,0], [0,-1,0,0], [0,0,-1,0], [-1,1,0,1]]`.
- `update_screen_size_sets_screen_derived_fields_only` — a `Size` (screen
  100×80, cell 10×20, padding {top 2, bottom 3, right 5, left 4}) + grid {cols
  8, rows 3} → `projection_matrix == ortho2d(-4, 96, 78, -2)` (terminal 91×75),
  `grid_padding == [2, 16, 18, 4]` (blank leftover 11×15 → {top 0, bottom 15,
  right 11, left 0}, `.add(padding)` → {top 2, bottom 18, right 16, left 4}),
  `screen_size == [100, 80]`; and `cell_size` / `grid_size` / `bg_color` are
  unchanged.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2891 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + `lib.rs`/header/`abi_harness.c`)
  clean; `git diff --check` clean.

## Conclusion

The per-frame uniforms now carry the real orthographic projection and grid
padding derived from the screen/grid/cell sizes — no longer the test-only
identity. `ortho2d` is a reusable production primitive, and `update_screen_size`
is the first of upstream's uniform-update functions ported. The remaining
uniform groups — the cell/grid size (the resize path in `rebuildCells`), the
min-contrast and color-space settings, the background color, and the cursor
group — and a full production `MetalUniforms` constructor are the next
renderer-bridge slices, along with the live call sites that run these updates on
resize/config-change and the `FrameState` per-frame loop.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation matches the approved design and
upstream behavior: `ortho2d` is the same matrix as `math.ortho2d` (including the
negative-Y scale), and `update_screen_size` derives `terminal`, `blank`, the
projection bounds, `grid_padding`, and `screen_size` exactly as specified, only
mutating the three screen-size fields. It verified the test expectations are
correctly derived (terminal 91×75; projection bounds `(-4, 96, 78, -2)`; grid
80×60, padded 89×65, leftover 11×15; blank+padding
`{top 2, right 16, bottom 18, left 4}` → `[2, 16, 18, 4]`) and that the
untouched-field assertions cover the scope boundary. No public C ABI/header
impact; nothing needed to change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-081635-r415-prompt.md` (result)
- Result: `logs/codex-review/20260604-081635-r415-last-message.md` (result)
