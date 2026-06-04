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

# Experiment 404: the under-preedit background skip

## Description

Experiment 403 skipped the **foreground** of cells under the IME preedit;
upstream's single `continue` skips the **whole** cell — neither background nor
foreground is drawn — so the preedit (which draws its own cells, with no
background) shows through on the screen background. This experiment ports the
**background** half: `rebuild_bg_row` writes a **transparent** background for
cells in the row's preedit range, instead of the cell's normal background. The
preedit range is the same per-row input as Experiment 403, already threaded
through `rebuild_viewport` (which now also passes it to `rebuild_bg_row`). This
completes the under-preedit skip.

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), the per-cell `continue` (the same
one ported in Experiment 403 for the foreground) is placed **before** the
background cell is written:

```zig
// If this cell falls within our preedit range then we skip this because
// preedits are setup separately.
if (preedit_range) |range| {
    if (range.y != y) break :preedit;
    if (x < range.x[0]) break :preedit;
    if (x <= range.x[1]) continue;   // skip the cell — no bg, no fg
    …
}
// (then the bg cell is written, then the foreground)
```

So a cell on the preedit row inside `[range.x[0], range.x[1]]` has **no
background written** — it keeps the cleared (transparent) value, so the screen
background shows through and the preedit glyph (which `addPreeditCell` draws
with no background) appears over it.

## Rust mapping (`roastty/src/renderer/cell.rs`)

roastty's `rebuild_bg_row` writes **every** cell's background unconditionally
(Experiment 384, to avoid stale backgrounds). To match upstream's net effect (no
background under the preedit) within that model, an under-preedit cell is
written **transparent** (`CellBg([0, 0, 0, 0])`) instead of skipped:

```rust
pub(crate) fn rebuild_bg_row(
    …,
    alpha: u8,
    preedit_range: Option<[u16; 2]>,
) {
    let row = usize::from(y);
    for (col, cell) in row_cells.iter().enumerate() {
        let x = u16::try_from(col).expect("viewport column fits u16");
        // A cell under the preedit draws no background (the preedit shows through
        // on the screen background). Raw column (no `x_compare`), like links.
        if preedit_range.is_some_and(|[start, end]| x >= start && x <= end) {
            *contents.bg_cell_mut(row, col) = CellBg([0, 0, 0, 0]);
            continue;
        }
        // …the normal per-cell background (selection/search opaque, cell_colors,
        //   bg_alpha) as before…
    }
}
```

`rebuild_viewport` already computes the per-row preedit range (`row_preedit`,
Experiment 403); it now also passes it to `rebuild_bg_row`.

## Scope / faithfulness notes

- **Ported (bridged)**: the under-preedit **background** skip — a cell in the
  row's preedit range gets a transparent background, completing the
  under-preedit cell skip begun in Experiment 403 (foreground).
- **Faithful**: the column test is the raw-column inclusive range (upstream's
  `range.x[0] <= x <= range.x[1]` on the preedit row, raw `x` like links); an
  under-preedit cell's background is transparent — upstream's net effect (the
  `continue` leaves the bg cell at its cleared/transparent value), so the screen
  background shows through and the preedit glyph draws over it. A non-preedit
  cell's background is unchanged.
- **Faithful adaptation**: roastty writes `CellBg([0, 0, 0, 0])` explicitly for
  the under-preedit cells, because `rebuild_bg_row` writes every cell
  unconditionally (Experiment 384) rather than relying on a prior clear — the
  visible result (a transparent background under the preedit) matches upstream's
  skip.
- **Deferred**: the `rebuild_viewport` cursor/preedit assembly (calling
  `add_preedit`, computing the preedit range from the cursor viewport) — depends
  on the live render `State`/`Mouse`; the Metal upload. (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`:
   - `rebuild_bg_row`: add a `preedit_range: Option<[u16; 2]>` param (last); for
     a cell in the range, write `CellBg([0, 0, 0, 0])` and skip the normal
     background computation. Update its doc comment.
   - `rebuild_viewport`: pass the per-row `row_preedit` to `rebuild_bg_row` (it
     is already computed for `rebuild_row`).
   - Update the existing `rebuild_bg_row` test call sites (`None`).
2. Tests (in `cell.rs`):
   - `rebuild_bg_row` with `preedit_range = Some([1, 2])` over a 4-cell row
     whose cells have explicit backgrounds → columns 1 and 2 are transparent
     (`[0, 0, 0, 0]`), columns 0 and 3 keep their (opaque) backgrounds;
   - the **raw-column** check: a `SpacerTail` at column 1 with
     `preedit_range = Some([0, 0])` is **not** skipped (its background is drawn,
     not transparent);
   - a `rebuild_viewport` end-to-end test combining the fg (Experiment 403) and
     bg skips: a preedit on row 0 over a column → that column has a transparent
     background **and** no foreground, while a neighbor is drawn normally.
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

- `rebuild_bg_row` writes a transparent background for cells in the row's
  preedit range (raw column, inclusive) and the normal background otherwise, and
  `rebuild_viewport` threads the per-row range — faithful to upstream's
  under-preedit `continue` (background half);
- the tests pass (the under-preedit columns transparent, neighbors unchanged;
  the raw-column SpacerTail not skipped; the end-to-end bg+fg skip), and the
  existing tests still pass (updated for the new signature, passing `None`);
- the `rebuild_viewport` cursor/preedit assembly and the Metal upload stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if an under-preedit cell's background is not
transparent, a non-preedit cell's background changes, the raw-column test is
mis-applied, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed that writing `CellBg([0, 0, 0, 0])` for under-preedit
cells is the right adaptation — upstream's `continue` leaves those background
cells at the cleared transparent value, and roastty's background pass
intentionally writes every cell, so explicitly writing transparent preserves the
same visible result and avoids stale backgrounds. It confirmed the raw-column
inclusive range test is correct (matching upstream's
`range.x[0] <= x <= range.x[1]` without `x_compare`), and that applying it
**before** the normal background color/alpha computation preserves upstream's
ordering — the preedit skip wins over selection, explicit background, inverse,
and the default-background behavior. It agreed this cleanly completes the
foreground skip from Experiment 403 while leaving the cursor/preedit assembly
and the Metal upload deferred, and judged the tests sufficient (transparent
under-preedit cells, unchanged neighbors, the raw-column `SpacerTail`, and the
end-to-end bg+fg skip).

Review artifacts:

- Prompt: `logs/codex-review/20260604-064538-080494-prompt.md` (design)
- Result: `logs/codex-review/20260604-064538-080494-last-message.md` (design)
