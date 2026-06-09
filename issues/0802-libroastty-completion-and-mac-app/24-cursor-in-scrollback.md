+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 24: Phase C — suppress the cursor when scrolled into scrollback

## Description

The Exp-23 result review found (and the live `e23-scrolled_up.png` shows) a
**stray cursor block rendered in scrollback history**: when the viewport is
scrolled up, the cursor block still draws on a history row.

**The fix site (corrected by the design review):** the visible cursor block is
**not** `RunOptions.cursor_x` (that is only a run-segmentation/shaping hint —
`font/run.rs:392`, no draw effect). The block comes from `cursor_viewport(...)`:

- `renderer/frame_rebuild.rs:85-86` —
  `(cursor_x, cursor_y) = terminal.cursor_position(); cursor_viewport(cursor_x, cursor_y, terminal_grid)`
  → `FrameCursorOverlay`/`FrameBlockCursorUniform`;
- `lib.rs:7132-7138` — the duplicate, → `RenderStateCursorViewport`.

Both gate **only** on `cursor_x < cols && cursor_y < rows`
(`frame_rebuild.rs:1530`), and `cursor_position()` returns **active-area**
coords (`screen.rs:937`) that don't move with scroll — so when scrolled up,
`cursor_y < rows` is still true and the block draws at viewport row `cursor_y`,
now a history row. Upstream gates the cursor on `cursor.viewport` (null when
off- viewport, `renderer/generic.zig:2387/2457`).

## Approach

Compute the cursor's **viewport** position (pin-based) and feed the cursor-draw
path that, instead of active coords + a bounds check.

1. **New
   `Terminal::cursor_viewport_position(&self) -> Option<(CellCountInt, CellCountInt)>`**
   (→ `screen` → `page_list`): resolve the cursor's active pin once
   (`pin(Point::active(0, cursor.y))`); scan viewport rows `0..rows`, and return
   `Some((cursor.x, vy))` for the viewport row `vy` whose pin is the **same
   physical row** (`pin.node == cursor_pin.node && pin.y == cursor_pin.y`);
   `None` if the cursor's active row is not in the current viewport. (`Pin.node`
   is `NonNull<Node>` — canonical + stable within one `&self` call, so
   `(node, y)` identifies one physical row; confirmed by the review.) Keep the
   column-bounds intent (`cursor.x < cols`).
2. **`frame_rebuild.rs:85-86`** — replace `cursor_position()` +
   `cursor_viewport(...)` with
   `terminal.cursor_viewport_position().map(|(x, y)| Coordinate::new(x, u32::from(y)))`.
3. **`lib.rs:7132-7138`** — replace `cursor_position()` + the inline bounds
   check with
   `cursor_viewport_position().map(|(x, y)| RenderStateCursorViewport { x, y, wide_tail: false })`.
4. The now-obsolete `cursor_viewport()` helper (`frame_rebuild.rs:1529`) + its
   unit tests (`:1740-1741`) are removed/replaced by a terminal-level test.

- **Unscrolled** (viewport == active):
  `pin(viewport(0,cy)) == pin(active(0,cy))` (the review confirmed
  `get_top_left(Viewport)` delegates to `Active` when not scrolled), so the
  cursor maps to row `cy` exactly as before — no behavior change.
- **Scrolled**: the block draws only if the cursor's active row is visible in
  the viewport; scrolled past it (the common case) → `None` → no cursor,
  matching upstream.

**Only `libroastty`** (`terminal.rs`/`screen.rs`/`page_list.rs` for the
accessor; `frame_rebuild.rs` and `lib.rs` for the two sites). No app changes.

## Verification

1. **Headless regression test** (extends the Exp-23 setup): fill past the
   screen, then assert on the **cursor-draw output**
   (`Terminal::cursor_viewport_position()` — the value feeding
   `FrameCursorOverlay`/`RenderStateCursorViewport`, NOT `RunOptions.cursor_x`):
   **unscrolled** → `Some((x, active_cursor_row))`; **after `mouse_scroll` up
   into history** → `None`. Fails pre-fix (returns `Some` at a history row),
   passes after.
2. **No regression:** `cargo test -p roastty` (full) green — the existing
   render/cursor tests still pass (unscrolled maps to the same row); the removed
   `cursor_viewport()` unit tests are replaced by the terminal-level test.
3. **Live confirmation** (screen unlocked — check `CGSSessionScreenIsLocked`
   first): re-run the Exp-23 `seq 1 200` + scroll-up probe; the
   scrolled-into-history capture shows **no stray cursor block** on a history
   line (cf. `e23-scrolled_up.png` which did). App + descendant tree killed (0
   dangling); shots out-of-repo.
4. Faithful to upstream cursor-viewport gating (cite `generic.zig`).

**Pass** = `cursor_viewport_position()` is pin-based, the headless test
(unscrolled→Some / scrolled→None) passes, the suite is green, and the live
scrolled capture shows no stray cursor.

**Partial** = the unscrolled case is correct + tested, but the scrolled-hiding
needs a larger change (documented).

**Fail** = pin comparison can't distinguish the cases (documented).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed (re-review below).** It
**corrected the fix site**: the visible cursor block is drawn from
`cursor_viewport()` (`frame_rebuild.rs:85-86` + `lib.rs:7132`), which uses
scroll-invariant `cursor_position()` + a `< cols/rows` bounds check —
`RunOptions.cursor_x` is only a run-shaping hint (`font/run.rs:392`, no draw
effect), so the original plan would have fixed nothing. It **confirmed the pin
primitive is sound** (`Pin.node` is `NonNull<Node>`, canonical + stable within a
`&self` call; two rows can't share `(node, y)`) and that **unscrolled is
unchanged** (`get_top_left(Viewport)` delegates to `Active`, so
`pin(viewport(0,cy)) == pin(active(0,cy))`), and that `render_rows_snapshot`
emits no cursor. Findings, folded in:

- **Required — wrong fix site** (`shape_run_options.cursor_x` vs the
  `cursor_viewport` draw path). **Fixed:** the fix is now a pin-based
  `Terminal::cursor_viewport_position()` feeding the two `cursor_viewport`
  sites.
- **Required — the test asserted on the shaping hint.** **Fixed:** the test
  asserts on `cursor_viewport_position()` (the draw-path value): unscrolled →
  `Some`, scrolled → `None`.
- **Optional — `Pin.node` typed `NonNull<Node>`, not `*const ()`.** **Fixed**
  wording.

**Re-review: APPROVED.** A focused second pass confirmed the corrected site is
the only cursor-block-draw input (the other `cursor_position()` uses are IME
placement + data queries, not the block), the pin-scan accessor mirrors the
existing `render_rows_snapshot`/`shape_run_options` scans (`Pin: Eq`, O(rows) —
negligible), edge cases hold (row 0, fresh/alt screen), and removing
`cursor_viewport()` is safe (consumer coverage survives via the many tests that
set `cursor_viewport` directly). Notes folded in: **keep the explicit
`cursor.x < cols`** check (the `pin(active(0,cy))` x=0 won't enforce it); the
snapshot test `terminal_snapshot_captures_cursor_only_inside_terminal_grid`
stays (its `Some(Coordinate::new(2,1))` assertion passes under the pin path —
only the direct `cursor_viewport(..)` helper calls go).

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
