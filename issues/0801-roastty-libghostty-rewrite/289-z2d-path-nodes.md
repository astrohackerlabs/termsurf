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

# Experiment 289: z2d port — the path-node representation

## Description

With the fill rasterizer complete (288), the next z2d sub-area is the **path
front-end**: turning a path (the move/line/curve/close nodes a `Canvas` builds)
into `Polygon` contours (the fill plotter) or a stroke outline (the stroke
plotter). The foundation both consume — and the representation the `Canvas` path
methods emit — is z2d's **path-node** type
(`vendor/z2d/src/internal/ path_nodes.zig`). This experiment ports that node
representation in isolation; it is small, self-contained, and ships its own
upstream test suite.

## Upstream behavior (`path_nodes.zig`)

- `PathNode` (tagged union): `move_to { point }` (start a subpath / move the
  current point), `line_to { point }` (line to a point),
  `curve_to { p1, p2, p3 }` (a cubic Bézier from the current point through the
  controls to `p3`), `close_path {}`.
- `isClosedNodeSet(nodes)`: returns whether _all_ subpaths in the node set are
  closed. Empty → `false`. It scans the nodes tracking a `closed` flag: a
  `close_path` sets `closed = true`; any other drawing node sets
  `closed = false`; a `move_to` that is **not** the first node and follows an
  unclosed subpath (`!closed`) **breaks** the scan early. The final `closed`
  value is returned. (So a path is "closed" only if every subpath ends in
  `close_path` before the next `move_to`, and the last subpath is closed.)

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

- `enum PathNode { MoveTo(Point), LineTo(Point), CurveTo { p1: Point, p2: Point, p3: Point }, ClosePath }`
  (reusing the in-module `Point`).
- `fn is_closed_node_set(nodes: &[PathNode]) -> bool` — the faithful port: empty
  → `false`; the `closed`-flag scan with the early `break` on an interior
  unclosed `move_to`.

## Scope / faithfulness notes

- **Deferred**: the `Path`/`StaticPath` builder API (`move_to`/`line_to`/
  `curve_to`/`close_path` with the current-point/transform bookkeeping), the
  `Spline` cubic-Bézier flattener, the `fill_plotter` and `stroke_plotter`, and
  `Canvas::line`/`fill`/`stroke` — later z2d slices. This experiment ports only
  the node enum + `is_closed_node_set`.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `PathNode` and `is_closed_node_set`.
2. Tests — the upstream `isClosedNodeSet` suite ported directly (six cases):
   - basic closed (`move,line,line,close,move`) → `true`;
   - multiple subpaths all closed → `true`;
   - basic not closed (`move,line,move,line,line`) → `false`;
   - closed in the middle only → `false`;
   - closed at the end but with an earlier unclosed subpath → `false`;
   - empty → `false`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty raster
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `PathNode` and `is_closed_node_set` reproduce z2d's node representation and
  the closed-subpath scan (including the interior-`move_to` early break),
  verified by the ported upstream tests;
- the `Path` builder, the flattener, the plotters, and `Canvas` path methods
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the node representation needs additional fields
to serve the (next) plotters faithfully.

The experiment **fails** if the node representation or the closed-set logic
diverges from z2d or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**. It confirmed `PathNode` maps directly to z2d's `move_to`/`line_to`/
`curve_to { p1, p2, p3 }`/`close_path`, that `is_closed_node_set` matches the
empty-case, the `closed`-flag scan, the interior-unclosed `move_to` early break,
and the final return value, and that the six upstream tests are transcribed with
the correct expectations (including the trailing-`move_to`-after-closed and the
earlier-unclosed-subpath edge cases). It judged deferring the
`Path`/`StaticPath` builder, the spline flattener, and the plotters a sound
scope.

Review artifacts:

- Prompt: `logs/codex-review/20260603-061802-439097-prompt.md`
- Result: `logs/codex-review/20260603-061802-439097-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` gained `PathNode`
(`MoveTo`/`LineTo`/`CurveTo { p1, p2, p3 }`/`ClosePath`) and
`is_closed_node_set` (the faithful empty-case + `closed`-flag scan with the
interior-`move_to` early break).

Tests — the upstream `isClosedNodeSet` suite transcribed directly:
`closed_node_set_basic_closed` (`true`), `_multiple_closed` (`true`),
`_basic_not_closed` (`false`), `_closed_in_middle` (`false`),
`_closed_at_end_not_middle` (`false`), `_empty` (`false`).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty raster` → 42 passed (6 new).
- `cargo test -p roastty` → 2543 passed, 0 failed (no regressions; +6).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The path-node representation is in place — the language the `Canvas` path
methods emit and the plotters consume. The next z2d slices, in order: the
`Spline` cubic- Bézier flattener (subdivides `CurveTo` into line segments), the
`fill_plotter` (path nodes → `Polygon` contours, closing open subpaths), and the
`stroke_plotter` (a stroked path → outline `Polygon` with the `Pen`/join/cap
machinery); then a `Canvas::fill_path`/`line`/`stroke` that builds the polygon
and calls `fill_polygon` on the (padded) `Canvas` buffer. The simplest first
consumer remains `Canvas::line` (a 2-node butt-cap path) → the three box-drawing
diagonals (`0x2571`–`0x2573`). Alongside the sprite font remain the discovery
consumer, the UCD emoji-presentation default, codepoint overrides, the shaper,
the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `PathNode` and `is_closed_node_set` match
`path_nodes.zig` exactly for this slice — including the subtle `MoveTo` behavior
(the first `MoveTo` leaves `closed` unchanged, a later `MoveTo` breaks only on
an unclosed subpath, and `ClosePath`/line/curve update the flag as upstream) —
and that the six tests are faithful transcriptions with the correct
expectations. It judged the gates clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-061951-785057-prompt.md`
- Result: `logs/codex-review/20260603-061951-785057-last-message.md`
