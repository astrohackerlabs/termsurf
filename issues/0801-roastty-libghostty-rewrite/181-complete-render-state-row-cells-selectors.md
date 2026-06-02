# Experiment 181: Complete Render State Row Cells Selectors

## Description

Complete the remaining row-cells selector surface from upstream:

- `vendor/ghostty/src/terminal/c/render.zig`;
- `vendor/ghostty/src/terminal/render.zig`.

Experiment 180 made the row-cells lifecycle and basic selectors real, but kept
style, grapheme, UTF-8, and resolved color selectors deferred. Those selectors
all depend on the same missing data: each render-state cell snapshot needs to
own the copied raw cell plus any copied style and grapheme data needed by C
callers after the terminal, render state, or row iterator is freed.

This experiment should make all remaining row-cells selectors real:

- `STYLE`;
- `GRAPHEMES_LEN`;
- `GRAPHEMES_BUF`;
- `BG_COLOR`;
- `FG_COLOR`;
- `GRAPHEMES_UTF8`.

The goal is to finish the row-cells C ABI as a coherent subsystem slice, not to
leave another deferred row-cell selector behind.

## Changes

1. Re-read upstream and current Roastty sources:
   - `vendor/ghostty/src/terminal/c/render.zig`;
   - `vendor/ghostty/src/terminal/render.zig`;
   - `vendor/ghostty/src/lib/types.zig`;
   - `roastty/src/terminal/page.rs`;
   - `roastty/src/terminal/page_list.rs`;
   - `roastty/src/terminal/screen.rs`;
   - `roastty/src/terminal/terminal.rs`;
   - `roastty/src/terminal/style.rs`;
   - `roastty/src/terminal/color.rs`;
   - `roastty/src/lib.rs`;
   - `roastty/include/roastty.h`;
   - `roastty/tests/abi_harness.c`.

2. Add a caller-owned buffer C type.

   Upstream `GRAPHEMES_UTF8` writes into `lib.Buffer`:

   ```zig
   pub const Buffer = extern struct {
       ptr: ?[*]u8 = null,
       cap: usize = 0,
       len: usize = 0,
   };
   ```

   Add the Roastty equivalent to `roastty/include/roastty.h`:

   ```c
   typedef struct {
     uint8_t* ptr;
     size_t cap;
     size_t len;
   } roastty_buffer_s;
   ```

   Mirror it in `roastty/src/lib.rs`.

   Required macOS layout:
   - `sizeof(roastty_buffer_s) == 24`;
   - `alignof(roastty_buffer_s) == 8`;
   - `offsetof(ptr) == 0`;
   - `offsetof(cap) == 8`;
   - `offsetof(len) == 16`.

   Behavior:
   - the caller owns `ptr`;
   - Roastty never allocates or frees the buffer;
   - `GRAPHEMES_UTF8` sets `len` to the required byte count before checking
     capacity;
   - if `ptr == NULL` or `cap < len`, return `ROASTTY_OUT_OF_SPACE`;
   - if the selected cell has no text, set `len = 0` and return
     `ROASTTY_SUCCESS`, even with `ptr == NULL` and `cap == 0`;
   - invalid Unicode scalar values return `ROASTTY_INVALID_VALUE`.

3. Extend render-state cell snapshots.

   `RenderStateCellSnapshot` should own:
   - raw `roastty_cell_t`;
   - `Option<style::Style>` copied from the page for non-default style ids;
   - owned `Vec<u32>` grapheme continuation codepoints copied from the page for
     `CodepointGrapheme` cells.

   Do not store page pointers, style-set pointers, grapheme-map pointers, arena
   references, slices into page memory, or references into render-state rows.

   Update the internal snapshot path so each viewport row produces exactly
   `cols` cell snapshots with the corresponding style and grapheme data copied
   at render-state update time.

4. Add palette ownership to row-cells binding.

   Color selectors must survive render-state free. Because `BG_COLOR` and
   `FG_COLOR` can resolve palette-indexed colors, `RenderStateRowCells` must own
   a copied palette snapshot after row `CELLS` binding.

   Binding row `CELLS` must therefore copy:
   - the selected row's cell snapshots;
   - the row selection range;
   - the render state's palette snapshot.

   The row-cells handle must not borrow palette storage from the render state.
   Tests must prove palette-resolved colors remain stable after render-state
   free and after mutating the terminal palette followed by
   `roastty_render_state_update`.

5. Preserve inline background-cell color semantics.

   Upstream render snapshots store inline background-color content in the
   cell-style side array for `BG_COLOR` resolution, but upstream `STYLE` still
   returns default style when the raw cell has no non-default style id.

   Port the behavior, not the exact storage shape:
   - `STYLE` returns default for inline background-only cells;
   - `BG_COLOR` resolves `BgColorRgb` / `BgColorPalette` from the raw cell
     content;
   - regular styled cells still resolve `BG_COLOR` from their copied style.

   This matters for erased cells that carry only a background color and no text.

6. Implement `STYLE`.

   Behavior:
   - output type is `roastty_style_s*`;
   - read `out->size` before writing any field;
   - if `out->size < sizeof(roastty_style_s)`, return `ROASTTY_INVALID_VALUE`
     before writing any field;
   - on success, write `out->size = sizeof(roastty_style_s)`;
   - default cells return a default style;
   - styled cells return the copied style snapshot;
   - inline background-only cells return a default style unless they also have a
     non-default style id;
   - the function must not fetch live page style storage at getter time.

7. Implement grapheme codepoint selectors.

   `GRAPHEMES_LEN` behavior:
   - output type is `uint32_t*`;
   - if the selected cell has no text, write `0`;
   - otherwise write `1 + continuation_count`;
   - the base codepoint is the raw cell's codepoint;
   - continuation codepoints come from the owned grapheme snapshot.

   `GRAPHEMES_BUF` behavior:
   - output type is `uint32_t*`;
   - if the selected cell has no text, return `ROASTTY_SUCCESS` and write
     nothing;
   - otherwise write the base codepoint first, then all continuation codepoints;
   - this selector assumes the caller provided enough `uint32_t` slots after
     querying `GRAPHEMES_LEN`, matching upstream's unchecked buffer contract.

8. Implement `GRAPHEMES_UTF8`.

   Behavior:
   - output type is `roastty_buffer_s*`;
   - set `out->len = 0` before processing;
   - if the selected cell has no text, return `ROASTTY_SUCCESS`;
   - encode the base codepoint and continuation codepoints as UTF-8;
   - set `out->len` to the required byte count before capacity validation;
   - return `ROASTTY_OUT_OF_SPACE` when `out->ptr == NULL` or
     `out->cap < out->len`;
   - write bytes only when capacity is sufficient;
   - return `ROASTTY_INVALID_VALUE` for invalid codepoints.

9. Implement resolved colors.

   `BG_COLOR` behavior:
   - output type is `roastty_rgb_s*`;
   - if the raw cell content tag is `BgColorRgb`, return that RGB value;
   - if the raw cell content tag is `BgColorPalette`, resolve that palette index
     from the row-cells handle's owned palette snapshot;
   - otherwise resolve the selected cell's copied style background color;
   - palette colors use the row-cells handle's owned palette snapshot;
   - RGB colors return directly;
   - no background color returns `ROASTTY_INVALID_VALUE`.

   `FG_COLOR` behavior:
   - output type is `roastty_rgb_s*`;
   - no foreground color returns `ROASTTY_INVALID_VALUE`;
   - palette colors use the row-cells handle's owned palette snapshot;
   - RGB colors return directly;
   - default foreground fallback is not used for this selector, matching
     upstream's explicit `s.fg_color == .none` invalid-value behavior.

   If the current Roastty style helper's default-foreground fallback would
   obscure this behavior, add a narrow internal helper that resolves only the
   explicit style color.

10. Keep row-cells getter validation precedence from Experiment 180.

Every selector, including the newly implemented selectors, must validate:

- non-null output pointer;
- valid raw C selector value;
- selector is not `INVALID`;
- non-null row-cells handle;
- bound row-cells handle;
- selected in-range cell.

Since this experiment completes every row-cells selector, no selector should use
a deferred `ROASTTY_NO_VALUE` shortcut.

11. Add tests.

    Rust ABI tests must cover:
    - selector numeric values;
    - `roastty_buffer_s` size, alignment, and field offsets;
    - `STYLE` default, SGR style, small-size invalid-value, size rewrite on
      success, and inline background-only returns default style;
    - `GRAPHEMES_LEN` / `GRAPHEMES_BUF` for empty cells, single-codepoint cells,
      and multi-codepoint grapheme cells;
    - `GRAPHEMES_UTF8` empty, out-of-space, exact-fit, and multi-codepoint
      cases;
    - `BG_COLOR` invalid when absent, RGB style, palette style, RGB inline
      background, and palette inline background;
    - `FG_COLOR` invalid when absent, RGB foreground, and palette foreground;
    - `get_multi` success with all row-cells selectors and partial progress when
      a later color selector returns `ROASTTY_INVALID_VALUE`;
    - validation precedence for at least one newly real selector across null
      handle, unbound handle, bound-unselected handle, null output, invalid
      selector, and selected-success paths;
    - snapshot stability after terminal mutation, render-state update,
      render-state free, and row iterator free.

    The C harness must cover the new public `roastty_buffer_s` layout and at
    least one success path for `STYLE`, grapheme UTF-8, `BG_COLOR`, and
    `FG_COLOR`.

12. Keep scope narrow.

    Do not add formatter objects, Metal renderer, Swift integration, hyperlink
    expansion, highlight expansion, Kitty graphics, browser overlay behavior,
    PTY behavior, or `ghostty_*` compatibility aliases.

    Do not alter row-cells lifecycle semantics from Experiment 180 except as
    needed to carry the new owned snapshot fields.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty render_state_row_cells_c_abi
cargo test -p roastty render_state_row_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Pass criteria:

- all row-cells selectors are implemented;
- no row-cells selector returns `ROASTTY_NO_VALUE`;
- row-cells snapshots own raw cells, styles, grapheme continuation codepoints,
  and palette data needed by getters;
- getters do not borrow from terminal pages, page-list nodes, row iterators, or
  render-state rows;
- `STYLE`, grapheme codepoint selectors, `GRAPHEMES_UTF8`, `BG_COLOR`, and
  `FG_COLOR` match upstream behavior described above;
- `GRAPHEMES_UTF8` reports required length and out-of-space behavior correctly;
- resolved palette colors use the row-cells handle's owned palette snapshot;
- snapshot stability survives terminal mutation, render-state update,
  render-state free, palette mutation/update, and row iterator free;
- Rust ABI tests, C harness, full `roastty` tests, no-`ghostty` public ABI/code
  check, and `git diff --check` pass;
- Codex reviews and approves both the design and completed result.

Partial criteria:

- style and grapheme selectors pass, but resolved colors expose a missing
  palette or inline-background prerequisite that needs a focused follow-up.

Failure criteria:

- any row-cells selector remains deferred as `ROASTTY_NO_VALUE`;
- getters borrow live page/style/grapheme/palette storage instead of owned
  snapshots;
- `GRAPHEMES_UTF8` allocates memory owned by Roastty instead of using the
  caller-owned buffer;
- `GRAPHEMES_BUF` invents capacity checking that is not present in the upstream
  selector contract;
- foreground color falls back to default foreground instead of returning
  `ROASTTY_INVALID_VALUE` when no explicit foreground exists;
- the implementation expands into formatter objects, renderer backend code,
  Swift integration, browser overlay behavior, or PTY behavior;
- the API exposes `ghostty_*` symbols or compatibility aliases.

## Review

This design must be reviewed with the Codex review skill before implementation.
All real findings must be fixed, and the design must be re-reviewed until Codex
approves it.

After implementation and result recording, the completed result must also be
reviewed with Codex and approved before the result commit.

## Result

**Result:** Pass

Experiment 181 completed the row-cells selector surface:

- added public `roastty_buffer_s` for caller-owned UTF-8 output buffers;
- extended render-state cell snapshots with copied style and owned grapheme
  continuation data;
- copied the render-state palette into the row iterator and then into the
  row-cells handle during row `CELLS` binding;
- implemented `STYLE`, `GRAPHEMES_LEN`, `GRAPHEMES_BUF`, `BG_COLOR`, `FG_COLOR`,
  and `GRAPHEMES_UTF8`;
- preserved the Experiment 180 validation order for every row-cells selector;
- added Rust ABI tests for style, grapheme, UTF-8, resolved color, validation,
  and snapshot-stability behavior;
- extended the C harness for `roastty_buffer_s`, `STYLE`, UTF-8, `BG_COLOR`, and
  `FG_COLOR`.

Verification run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty render_state_row_cells_c_abi
cargo test -p roastty render_state_row_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Observed results:

- focused row-cells ABI tests: 6 passed;
- row iterator ABI tests: 4 passed;
- C harness link test: passed;
- full `roastty` suite: 1869 Rust tests passed, 1 C harness test passed, 0
  doctests;
- strict no-`ghostty` check on public ABI/code files: passed;
- `git diff --check`: passed.
- Codex completed-result review: approved with no blocking findings.

## Conclusion

The render-state row-cells C ABI no longer has deferred selectors. Row-cells
handles own the raw cell snapshots, copied styles, grapheme continuation
codepoints, and palette snapshot needed by every getter, so style, text, UTF-8,
and resolved color access remain stable after terminal mutation, render-state
update, render-state free, and row iterator free.

The next render-state slice can move beyond row-cell selector completion into
the remaining render-state row metadata surface, such as highlights, if that is
still missing from the upstream C ABI inventory.
