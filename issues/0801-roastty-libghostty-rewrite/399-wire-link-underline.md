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

# Experiment 399: wire the link underline into the pass

## Description

`link_underline` (Experiment 397) computes a cell's effective underline given
whether it is a hovered link, but `rebuild_row`'s underline step still uses the
raw `flags.underline`. This experiment wires the override live: `rebuild_row`
takes the row's hovered-link **ranges** and, per cell, computes `is_link` (the
column falls in a link range) and applies
`link_underline(is_link, flags.underline)` before drawing the underline.
`rebuild_viewport` threads a per-row `&[Vec<[u16; 2]>]` of link ranges (parallel
to `rows`, like the search highlights, Experiment 391). The ranges' origin — the
OSC 8 / hovered-link detection from the terminal + mouse state — is not modeled
in roastty, so a row with no link ranges (the common case) is unchanged; the
caller supplies the ranges.

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), the per-cell underline applies the
link override against a `links` cell set (the OSC 8 hovered-link cells), checked
with the **raw** column/row (no `x_compare` adjustment):

```zig
const underline = if (links.contains(.{ .x = @intCast(x), .y = @intCast(y) }))
    (if (style.flags.underline == .single) .double else .single)
else
    style.flags.underline;
if (underline != .none) self.addUnderline(x, y, underline, …);
```

`links` is built from the terminal's OSC 8 link state intersected with the mouse
position (`linkCells`). So a cell within the hovered link is underlined (its
single underline doubled to distinguish it), using the raw cell coordinate.

## Rust mapping (`roastty/src/renderer/cell.rs`)

`rebuild_row` gains `link_ranges: &[[u16; 2]]` (the row's hovered-link column
ranges, inclusive). In the column loop's underline step, `is_link` is whether
the column is in any range, and `link_underline` (Experiment 397) computes the
effective underline:

```rust
// In the column loop, the underline step:
let is_link = link_ranges
    .iter()
    .any(|&[start, end]| x >= start && x <= end);
let underline = link_underline(is_link, flags.underline);
if underline != Underline::None {
    let underline_color = cell
        .style
        .resolve_underline_color(palette)
        .map(|rgb| [rgb.r, rgb.g, rgb.b])
        .unwrap_or(fg);
    add_underline(contents, grid, grid_pos, underline, underline_color, rgba[3])?;
}
```

`x` is the **raw** column (`grid_pos[0]`), matching upstream's raw
`links.contains` (no spacer-tail adjustment, unlike selection/highlights).
`rebuild_viewport` gains `link_ranges: &[Vec<[u16; 2]>]` (per row) and threads
each row's slice (`.get(y).map(Vec::as_slice).unwrap_or(&[])`) to `rebuild_row`.

## Scope / faithfulness notes

- **Ported (bridged)**: the live application of the link underline override — a
  cell within a row's hovered-link ranges draws the overridden underline
  (Experiment 397's `link_underline`), wired through `rebuild_viewport`'s
  per-row link ranges.
- **Faithful**: `is_link` is the **raw** column in any inclusive `[start, end]`
  range (upstream's `links.contains({x, y})` uses the raw coordinate, **not**
  the `x_compare` of selection/highlights); the effective underline is
  `link_underline(is_link, flags.underline)`, drawn (with the explicit-color
  fallback) only when `!= None` — upstream's exact override and draw gate. A
  non-link cell is unchanged (`link_underline(false, …)` returns
  `flags.underline`), so the common no-link case is identical.
- **Faithful adaptation**: the hovered-link ranges are a separate per-row
  `&[Vec<[u16; 2]>]` parameter (not on `RunOptions`), like the search highlights
  — upstream sources them from the render/mouse state, separate from the shaper.
  The ranges' inclusive `[start, end]` model a contiguous hovered link on the
  row.
- **Deferred**: the origin of the link ranges (the OSC 8 hovered-link detection
  from the terminal + mouse state); the Metal upload. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`:
   - `rebuild_row`: add a `link_ranges: &[[u16; 2]]` param; in the underline
     step, compute `is_link` (the raw column in any range) and apply
     `link_underline(is_link, flags.underline)`. Update its doc comment.
   - `rebuild_viewport`: add a `link_ranges: &[Vec<[u16; 2]>]` param; thread
     each row's slice to `rebuild_row`. Update its doc comment.
   - Update the existing `rebuild_row`/`rebuild_viewport` test call sites
     (`&[]`).
2. Tests (in `cell.rs`):
   - `rebuild_row` with a link range over a cell with **no SGR underline** → the
     cell draws a **single** underline (the link gives it one) where without the
     range it would draw none;
   - a cell with a **single** SGR underline inside the link range → a **double**
     underline (same-grid cache identity vs the directly-rendered
     double-underline sprite, or asserting the sprite differs from the
     single-underline sprite);
   - a cell **outside** the link range keeps its SGR underline (here: none → no
     underline cell);
   - the **raw-column** check (no spacer-tail adjustment): a `SpacerTail` cell
     at **column 1** with link range `[0, 0]` is **not** linked (raw column 1 ∉
     `[0, 0]`) — an incorrect `x_compare` (column `1 - 1 = 0`) would wrongly
     mark it linked, so this protects the upstream distinction from
     selection/highlights.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty rebuild_row
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `rebuild_row` applies `link_underline` against the row's link ranges (raw
  column, inclusive) in the underline step, and `rebuild_viewport` threads the
  per-row link ranges — faithful to upstream's link override;
- the tests pass (a link gives an un-underlined cell a single underline; a
  single-underlined link cell gets a double; a non-link cell is unchanged), and
  the existing tests still pass (updated for the new signatures, passing `&[]`);
- the link-range origin and the Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the link override is mis-applied (the wrong cells,
an `x_compare` adjustment wrongly added, a non-link cell changed), or any public
C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (no Required), now addressed:

- **Low (addressed):** the test plan should add an explicit **raw-column** case
  — a normal narrow-cell range would not catch an accidental spacer-tail
  `x_compare` adjustment. The test list now includes a `SpacerTail` at column 1
  with link range `[0, 0]`: the raw-column logic must treat it as **not**
  linked, while an incorrect `x_compare` (column 0) would mark it linked —
  directly protecting the key upstream distinction from selection/highlights.

Codex confirmed the rest is faithful: the link membership uses the raw column
(not `x_compare`); `link_underline` belongs only in the underline step; no-link
rows remain unchanged; and keeping the link ranges separate from `RunOptions` is
the right boundary (hovered-link state is renderer/mouse state, not shaper
input).

Review artifacts:

- Prompt: `logs/codex-review/20260604-060111-799278-prompt.md` (design)
- Result: `logs/codex-review/20260604-060111-799278-last-message.md` (design)

## Result

**Result:** Pass

The link underline override is now live in the rebuild.

- `roastty/src/renderer/cell.rs`:
  - `rebuild_row` (new `link_ranges: &[[u16; 2]]` param, placed last for
    call-site clarity): the underline step computes
    `is_link = link_ranges.iter().any(|&[s, e]| grid_pos[0] >= s && grid_pos[0] <= e)`
    (the **raw** column `grid_pos[0]`, no `x_compare`) and draws
    `link_underline(is_link, flags.underline)` (Experiment 397) — with the
    explicit-color fallback, only when `!= None`.
  - `rebuild_viewport` (new `link_ranges: &[Vec<[u16; 2]>]` param, last): per
    row, `row_links = link_ranges.get(y).map(Vec::as_slice).unwrap_or(&[])` is
    threaded to `rebuild_row` (only — `rebuild_bg_row` is unchanged; links
    affect only the underline). Doc comments updated; all test call sites
    updated (`&[]`).

Test (in `cell.rs`): `rebuild_row_applies_link_underline` (empty runs, so only a
drawn underline appears) — a link over an un-underlined cell → a single
underline (cache identity vs `Sprite::Underline`); a link over a
single-underlined cell → a double underline (vs `Sprite::UnderlineDouble`); no
link → nothing; and a **`SpacerTail` at column 1** with link range `[0, 0]` →
**not** linked (only column 0 is underlined), proving the raw-column membership
(an `x_compare` of 0 would wrongly link it).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2858 passed, 0 failed (+1, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer) clean; `git diff --check` clean.

## Conclusion

The hovered-link underline override is now live: a cell inside a row's link
ranges draws the overridden underline (a single for an un-underlined link, a
double to distinguish a single-underlined one), using the raw column — faithful
to upstream's `links.contains({x, y})`. With `link_underline` now wired, the
only thing left for live links is the **origin** of the ranges (the OSC 8
hovered-link detection from the terminal + mouse state), which is outside the
renderer bridge.

The CPU-side renderer bridge — `renderer/cell.rs`'s `Contents` assembly — is now
a **complete, faithful** port of upstream's `rebuildCells`: cell colors
(reverse-video, full-block, faint, min-contrast flag), backgrounds + alpha,
selection/search recolor (live), the cursor (sprite + lock glyph) and its
colors/uniform inputs, the column-ordered decorations, and the link underline.
The sole remaining renderer-bridge work is the **Metal upload** of `Contents`
(the GPU buffer/uniform upload) — the GPU boundary, which depends on the GUI's
Metal/wgpu layer rather than further `rebuildCells` porting.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation matches the approved design and
addresses the prior Low: `is_link` uses `grid_pos[0]` directly (inclusive
ranges, no `x_compare`), and the added `SpacerTail` test at column 1 with range
`[0, 0]` protects that upstream distinction. It confirmed `link_underline` is
applied only in the underline step with the draw gate intact (drawn only when
`!= None`, same explicit-color fallback), that no-link rows pass `&[]` and
preserve prior behavior, that `rebuild_viewport` threads the per-row link ranges
to `rebuild_row` only (`rebuild_bg_row` correctly unchanged), and that placing
`link_ranges` last in the signatures is acceptable (an internal Rust API choice
with all call sites updated, no fidelity or public C ABI/header impact). Nothing
needed to change before the result commit.

Review artifacts:

- Result review: `logs/codex-review/20260604-060958-177060-last-message.md`
