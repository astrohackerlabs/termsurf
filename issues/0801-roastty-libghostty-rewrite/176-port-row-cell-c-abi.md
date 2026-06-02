# Experiment 176: Port Row and Cell C ABI

## Description

Port the packed row/cell value getter ABI from upstream:

- `vendor/ghostty/src/terminal/c/cell.zig`;
- `vendor/ghostty/src/terminal/c/row.zig`.

Experiment 175 completed owned grid refs. The next foundation needed by render
and formatter work is the public ability to inspect packed row and cell values
that later iterators will hand to C callers. This experiment ports the value
types, selector enums, `get`, and `get_multi` functions for rows and cells. It
does not add render iterators or expose live page storage yet.

Public names must use Roastty naming only:

- `roastty_cell_t`;
- `roastty_cell_*`;
- `roastty_row_t`;
- `roastty_row_*`.

Upstream names may appear only in this issue document as source citations.

## Changes

1. Re-read upstream source:
   - `vendor/ghostty/src/terminal/c/cell.zig`;
   - `vendor/ghostty/src/terminal/c/row.zig`;
   - Roastty's current packed `Cell`, `Row`, color, style-id, wide-cell, and
     semantic prompt representations.

2. Add C ABI value types and enums in `roastty/include/roastty.h` and
   `roastty/src/lib.rs`.
   - `typedef uint64_t roastty_cell_t;`
   - `typedef uint64_t roastty_row_t;`
   - Rust mirrors these as `type RoasttyCell = u64;` and
     `type RoasttyRow = u64;`.
   - On supported macOS targets:
     - `sizeof(roastty_cell_t) == 8`;
     - `_Alignof(roastty_cell_t) == 8`;
     - `sizeof(roastty_row_t) == 8`;
     - `_Alignof(roastty_row_t) == 8`.
   - Add cell content tag values:
     - `ROASTTY_CELL_CONTENT_CODEPOINT = 0`;
     - `ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME = 1`;
     - `ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE = 2`;
     - `ROASTTY_CELL_CONTENT_BG_COLOR_RGB = 3`.
   - Add cell wide values:
     - `ROASTTY_CELL_WIDE_NARROW = 0`;
     - `ROASTTY_CELL_WIDE_WIDE = 1`;
     - `ROASTTY_CELL_WIDE_SPACER_TAIL = 2`;
     - `ROASTTY_CELL_WIDE_SPACER_HEAD = 3`.
   - Add cell semantic content values:
     - `ROASTTY_CELL_SEMANTIC_OUTPUT = 0`;
     - `ROASTTY_CELL_SEMANTIC_INPUT = 1`;
     - `ROASTTY_CELL_SEMANTIC_PROMPT = 2`.
   - Add row semantic prompt values:
     - `ROASTTY_ROW_SEMANTIC_NONE = 0`;
     - `ROASTTY_ROW_SEMANTIC_PROMPT = 1`;
     - `ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION = 2`.
   - Add selector enums matching upstream numeric order exactly.

   Cell selectors:

   | Name                              | Value | Output type                        |
   | --------------------------------- | ----- | ---------------------------------- |
   | `ROASTTY_CELL_DATA_INVALID`       | 0     | none; always invalid               |
   | `ROASTTY_CELL_DATA_CODEPOINT`     | 1     | `uint32_t*`                        |
   | `ROASTTY_CELL_DATA_CONTENT_TAG`   | 2     | `roastty_cell_content_tag_e*`      |
   | `ROASTTY_CELL_DATA_WIDE`          | 3     | `roastty_cell_wide_e*`             |
   | `ROASTTY_CELL_DATA_HAS_TEXT`      | 4     | `bool*`                            |
   | `ROASTTY_CELL_DATA_HAS_STYLING`   | 5     | `bool*`                            |
   | `ROASTTY_CELL_DATA_STYLE_ID`      | 6     | `uint16_t*`                        |
   | `ROASTTY_CELL_DATA_HAS_HYPERLINK` | 7     | `bool*`                            |
   | `ROASTTY_CELL_DATA_PROTECTED`     | 8     | `bool*`                            |
   | `ROASTTY_CELL_DATA_SEMANTIC`      | 9     | `roastty_cell_semantic_content_e*` |
   | `ROASTTY_CELL_DATA_COLOR_PALETTE` | 10    | `uint8_t*`                         |
   | `ROASTTY_CELL_DATA_COLOR_RGB`     | 11    | `roastty_rgb_s*`                   |

   Row selectors:

   | Name                                         | Value | Output type                      |
   | -------------------------------------------- | ----- | -------------------------------- |
   | `ROASTTY_ROW_DATA_INVALID`                   | 0     | none; always invalid             |
   | `ROASTTY_ROW_DATA_WRAP`                      | 1     | `bool*`                          |
   | `ROASTTY_ROW_DATA_WRAP_CONTINUATION`         | 2     | `bool*`                          |
   | `ROASTTY_ROW_DATA_GRAPHEME`                  | 3     | `bool*`                          |
   | `ROASTTY_ROW_DATA_STYLED`                    | 4     | `bool*`                          |
   | `ROASTTY_ROW_DATA_HYPERLINK`                 | 5     | `bool*`                          |
   | `ROASTTY_ROW_DATA_SEMANTIC_PROMPT`           | 6     | `roastty_row_semantic_prompt_e*` |
   | `ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER` | 7     | `bool*`                          |
   | `ROASTTY_ROW_DATA_DIRTY`                     | 8     | `bool*`                          |

3. Add public functions:

   ```c
   ROASTTY_API roastty_result_e roastty_cell_get(
       roastty_cell_t,
       roastty_cell_data_e,
       void*);

   ROASTTY_API roastty_result_e roastty_cell_get_multi(
       roastty_cell_t,
       size_t,
       const roastty_cell_data_e*,
       void**,
       size_t*);

   ROASTTY_API roastty_result_e roastty_row_get(
       roastty_row_t,
       roastty_row_data_e,
       void*);

   ROASTTY_API roastty_result_e roastty_row_get_multi(
       roastty_row_t,
       size_t,
       const roastty_row_data_e*,
       void**,
       size_t*);
   ```

   Incoming enum values must be accepted as raw `int`/`c_int` in Rust and
   validated before conversion.

4. Define getter behavior:
   - invalid selector returns `ROASTTY_INVALID_VALUE`;
   - null output returns `ROASTTY_INVALID_VALUE`;
   - `get_multi` null keys or values returns `ROASTTY_INVALID_VALUE`;
   - `get_multi` with `count == 0` succeeds and writes `0` to `out_written` if
     provided;
   - `get_multi` writes the number of fully completed entries to `out_written`
     on success or first failure;
   - selector output types match the tables above;
   - getters expose upstream's raw packed-field view:
     - `CODEPOINT` returns the codepoint for codepoint/grapheme cells and `0`
       for background-color cells;
     - `HAS_TEXT` is true only for codepoint/grapheme cells with nonzero
       codepoint;
     - `COLOR_PALETTE` reads the low palette byte from the packed content field
       regardless of the current content tag;
     - `COLOR_RGB` decodes the packed RGB bits regardless of the current content
       tag;
     - color selectors do not return `ROASTTY_NO_VALUE` for non-color cells;
     - style, hyperlink, protected, semantic, and wide selectors read their
       packed fields directly;
     - row selectors read their packed fields directly.

5. Keep scope narrow:
   - Do not add render state, row iterators, or row-cell iterators.
   - Do not add formatter objects.
   - Do not add style lookup, hyperlink lookup, grapheme extraction, or Kitty
     image extraction.
   - Do not expose raw page pointers or node pointers through this API.
   - Do not add `ghostty_*` symbols or compatibility aliases.

6. Add Rust tests in `roastty/src/lib.rs` for:
   - ABI numeric discriminants for every content, wide, semantic, cell-data, and
     row-data enum value;
   - `RoasttyCell` and `RoasttyRow` size/alignment;
   - cell getter coverage for text, empty, styled, hyperlink flag, protected,
     semantic content, palette background, RGB background, and wide/spacer
     values;
   - row getter coverage for wrap, wrap continuation, grapheme, styled,
     hyperlink, semantic prompt, virtual placeholder, and dirty;
   - invalid selector and null output behavior;
   - `get_multi` success, count-zero success, null keys/values, partial failure
     `out_written`, and no `out_written` behavior.

7. Add C harness coverage in `roastty/tests/abi_harness.c` for:
   - compile/link coverage for every new export;
   - enum numeric values for every content, wide, semantic, cell-data, and
     row-data enum value;
   - `sizeof` and `_Alignof` for `roastty_cell_t` and `roastty_row_t`;
   - basic cell/row getter calls;
   - `get_multi` success and partial failure.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs
cargo test -p roastty cell_c_abi
cargo test -p roastty row_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Pass criteria:

- row and cell packed ABI types match the current Roastty packed value sizes;
- all upstream selector numeric values are represented with Roastty names;
- all getter output types match the upstream C ABI shape;
- invalid raw enum values are rejected before conversion;
- `get_multi` reports partial progress correctly;
- C header, Rust ABI tests, C harness, full `roastty` tests, no-Ghostty check,
  and `git diff --check` all pass;
- Codex reviews and approves both the design and completed result.

Partial criteria:

- the simple scalar row/cell selectors pass, but one packed field cannot yet be
  faithfully extracted without a small internal accessor added in a follow-up.

Failure criteria:

- the API exposes page/node pointers or live storage ownership;
- raw C enum values are represented as Rust enums before validation;
- output pointer nullability differs from this experiment;
- selector numeric values drift from upstream;
- the implementation expands into render iterators, formatter objects, style
  lookup, hyperlink lookup, grapheme extraction, or Kitty graphics.

## Review

This design must be reviewed with the Codex review skill before implementation.
All real findings must be fixed, and the design must be re-reviewed until Codex
approves it.

After implementation and result recording, the completed result must also be
reviewed with Codex and approved before the result commit.
