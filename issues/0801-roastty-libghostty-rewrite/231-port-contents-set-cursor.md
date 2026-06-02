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

# Experiment 231: Port `Contents::set_cursor` and `get_cursor_glyph`

## Description

Continue the `Contents` builder (Experiment 230) by porting `setCursor` and
`getCursorGlyph` from upstream `renderer/cell.zig`. These manage the cursor
glyph in the two reserved foreground lists established by `resize`: `fg_rows[0]`
(drawn **first** in the GPU buffer, for block cursors) and `fg_rows[rows + 1]`
(drawn **last**, for the other cursor styles).

This is a small, coherent slice on the existing `Contents` struct, with
predictable tests; it depends only on `CellTextVertex` and the renderer cursor
`Style` enum (Experiment 223).

### Behavior to port

Upstream:

```
pub fn setCursor(self, v: ?CellText, cursor_style: ?renderer.CursorStyle) void {
    if (self.size.rows == 0) return;
    self.fg_rows.lists[0].clearRetainingCapacity();
    self.fg_rows.lists[self.size.rows + 1].clearRetainingCapacity();
    const cell = v orelse return;
    const style = cursor_style orelse return;
    switch (style) {
        .block => self.fg_rows.lists[0].appendAssumeCapacity(cell),
        .block_hollow, .bar, .underline, .lock =>
            self.fg_rows.lists[self.size.rows + 1].appendAssumeCapacity(cell),
    }
}

pub fn getCursorGlyph(self) ?CellText {
    if (self.size.rows == 0) return null;
    if (self.fg_rows.lists[0].items.len > 0) return self.fg_rows.lists[0].items[0];
    if (self.fg_rows.lists[self.size.rows + 1].items.len > 0)
        return self.fg_rows.lists[self.size.rows + 1].items[0];
    return null;
}
```

- `set_cursor(&mut self, v: Option<CellTextVertex>, cursor_style: Option<cursor::Style>)`:
  no-op when `size.rows == 0`; always clears **both** reserved cursor lists
  first (so a previous cursor is removed even when the new value is `None`);
  returns early if either `v` or `cursor_style` is `None`; otherwise appends the
  cell to `fg_rows[0]` for `Block`, or to `fg_rows[rows + 1]` for
  `BlockHollow | Bar | Underline | Lock`.
- `get_cursor_glyph(&self) -> Option<CellTextVertex>`: `None` when
  `size.rows == 0`; otherwise the first item of `fg_rows[0]` if non-empty, else
  the first item of `fg_rows[rows + 1]` if non-empty, else `None`.

### Faithfulness and scope notes

- `cursor::Style` is `renderer::cursor::Style` (Experiment 223) — imported as
  `CursorStyle`. Its variants `Block`/`BlockHollow`/`Bar`/`Underline`/`Lock`
  match upstream `renderer.CursorStyle`, and the `match` is exhaustive (no
  wildcard), so a future variant would force a compile error rather than
  silently routing to the "last" list.
- Upstream `appendAssumeCapacity` (the cursor lists have capacity 1) becomes
  `Vec::push`; only one cursor cell is ever stored, so this never reallocates in
  practice and is behaviorally identical.
- `size.rows` is `u16`; the last-list index is `rows as usize + 1`.
- Do **not** port `add`/`clear`/`Key`/`CellType` — those are Experiment 232.
- `set_cursor`/`get_cursor_glyph` are `pub(crate)`.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. `roastty/src/renderer/cell.rs`: add
   `use super::cursor::Style as CursorStyle;` and implement
   `Contents::set_cursor` and `Contents::get_cursor_glyph` exactly as above.

2. Tests in `renderer/cell.rs` (a dummy `CellTextVertex` builder already
   exists):
   - `set_cursor_block_uses_first_list`: `Block` cursor → `fg_rows[0]` holds the
     cell, `fg_rows[rows + 1]` empty; `get_cursor_glyph` returns it.
   - `set_cursor_other_styles_use_last_list`: for each of `BlockHollow`, `Bar`,
     `Underline`, `Lock` → `fg_rows[rows + 1]` holds the cell, `fg_rows[0]`
     empty; `get_cursor_glyph` returns it.
   - `set_cursor_none_value_clears`: after setting a cursor,
     `set_cursor(None, Some(Block))` clears both lists and `get_cursor_glyph` is
     `None`.
   - `set_cursor_none_style_clears`: `set_cursor(Some(cell), None)` clears both
     lists, leaves them empty.
   - `set_cursor_replaces_previous`: setting a block then a bar cursor leaves
     only one cursor glyph (no duplication across the two lists).
   - `set_cursor_zero_rows_is_noop`: with a `0×0` `Contents`, `set_cursor` does
     not panic and `get_cursor_glyph` is `None`.
   - `get_cursor_glyph_empty_is_none`: a resized but cursor-less `Contents`
     returns `None`.

3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/cell.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `set_cursor` clears both reserved lists, routes `Block` to `fg_rows[0]` and
  the other styles to `fg_rows[rows + 1]`, and is a no-op for `rows == 0` or a
  `None` value/style (after clearing);
- `get_cursor_glyph` returns the first cursor cell from either reserved list (or
  `None`);
- the tests above pass, including the each-style routing and the clear-on-`None`
  cases;
- `add`/`clear`/`Key` are not pulled in;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if cursor routing turns out to need data beyond a
`CellTextVertex` and a `Style`.

The experiment **fails** if the list routing diverges from upstream (wrong list
for a style, not clearing on `None`, or panicking at `rows == 0`), or if any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no issues**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-080001-777276-prompt.md`
- Result: `logs/codex-review/20260602-080001-777276-last-message.md`

Codex confirmed the `set_cursor` ordering is correct (`rows == 0` returns before
touching the lists; both reserved lists are cleared before unwrapping
`v`/`style` so `None` removes a previous cursor), the routing is faithful
(`Block` → `fg_rows[0]`, the others → `fg_rows[rows + 1]`), `get_cursor_glyph`
is faithful, `Vec::push` is fine for `appendAssumeCapacity`, the exhaustive
`match` on `cursor::Style` is correct, and the seven tests are sufficient. No
changes required.

## Result

**Result:** Pass

Added `use super::cursor::Style as CursorStyle;` and implemented
`Contents::set_cursor` and `Contents::get_cursor_glyph` in
`roastty/src/renderer/cell.rs`. `set_cursor` returns early on `rows == 0`,
clears both reserved cursor lists (`fg_rows[0]` and `fg_rows[rows + 1]`), then
returns on a `None` value/style or routes `Block` → `fg_rows[0]` and
`BlockHollow | Bar | Underline | Lock` → `fg_rows[rows + 1]` via an exhaustive
`match`. `get_cursor_glyph` returns `None` on `rows == 0`, then the first
element of `fg_rows[0]`, else of `fg_rows[rows + 1]`, else `None`.

Tests added (7): `set_cursor_block_uses_first_list`,
`set_cursor_other_styles_use_last_list` (all four non-block styles),
`set_cursor_none_value_clears`, `set_cursor_none_style_clears`,
`set_cursor_replaces_previous` (no duplication across lists),
`set_cursor_zero_rows_is_noop`, `get_cursor_glyph_empty_is_none`.

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
```

Observed:

- `renderer::cell`: 34 passed (27 prior + 7 new).
- Full `roastty`: 2270 unit tests passed (2263 prior + 7 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/renderer/cell.rs` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes; `add`/`clear`/`Key` not pulled in.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
should change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-080210-604591-prompt.md`
- Result: `logs/codex-review/20260602-080210-604591-last-message.md`

Codex confirmed `set_cursor` and `get_cursor_glyph` are faithful to upstream
(rows-0 guard, clear-both-lists-before-unwrap, `Block`→first / others→last
exhaustive routing, and the cursor-glyph lookup order), that indexing is safe
under the `resize` invariant (`rows + 2` lists), and that the seven tests cover
each route plus the clear/replace/empty cases.

## Conclusion

Experiment 231 succeeds. `Contents` now manages the cursor glyph in its reserved
lists. Both Codex gates passed with zero findings.

The last `Contents` slice (Experiment 232) is `add` / `clear` plus the `Key` /
`CellType` mapping: `add` routes a foreground cell to `fg_rows[y + 1]` (the
`+ 1` skipping the reserved cursor list, with `y` from the cell's `grid_pos`),
and `clear` zeroes one row's background span and clears its `fg_rows[y + 1]`
list. That completes `renderer/cell.zig`.
