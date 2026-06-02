# Experiment 182: Port Grid Ref Accessors C ABI

## Description

Experiment 181 finished the render row-cells accessor surface. The next missing
piece in upstream `terminal/c/grid_ref.zig` is the standalone grid-reference
accessor ABI:

- `grid_ref_cell`
- `grid_ref_row`
- `grid_ref_graphemes`
- `grid_ref_hyperlink_uri`
- `grid_ref_style`

Roastty already has `roastty_terminal_grid_ref`, tracked grid refs, raw
`roastty_cell_t` / `roastty_row_t`, `roastty_style_s`, grapheme storage, and
hyperlink storage. This experiment wires those existing pieces into the public C
ABI so a caller can take a borrowed `roastty_grid_ref_s` and inspect the cell,
row, grapheme codepoints, hyperlink URI, and style at that location.

This is a narrow ABI completion slice. It must not alter how grid refs are
created, tracked, invalidated, or converted back to terminal coordinates.

Important lifetime boundary: upstream's grid-ref accessors are standalone
helpers that receive only the grid ref, not the terminal. Roastty should keep
that ABI shape for this experiment. Because there is no terminal receiver,
standalone accessors cannot safely prove that a ref still belongs to a live
terminal or that its page node is still present in the terminal's current
`PageList`. They operate under the existing borrowed-ref contract in
`roastty_grid_ref_s`: use the ref immediately, with the same terminal storage,
before terminal mutation. Stale-after-mutation use is caller-invalid and is not
a detectable API result for these standalone helpers. Long-lived or
mutation-safe references remain the tracked-grid-ref ABI's job.

## Changes

1. In `roastty/include/roastty.h`, add public prototypes matching the upstream
   standalone grid-ref helper shape with Roastty names:
   - `roastty_grid_ref_cell(const roastty_grid_ref_s*, roastty_cell_t*)`
   - `roastty_grid_ref_row(const roastty_grid_ref_s*, roastty_row_t*)`
   - `roastty_grid_ref_graphemes(const roastty_grid_ref_s*, uint32_t*, size_t, size_t*)`
   - `roastty_grid_ref_hyperlink_uri(const roastty_grid_ref_s*, uint8_t*, size_t, size_t*)`
   - `roastty_grid_ref_style(const roastty_grid_ref_s*, roastty_style_s*)`

2. In `roastty/src/terminal/page_list.rs`, add narrow internal read helpers for
   a borrowed `GridRef` node pointer:
   - raw cell value
   - raw row value
   - grapheme codepoints for the referenced cell
   - hyperlink URI bytes for the referenced cell
   - stored style for the referenced cell, defaulting to `style::Style::default`
     when the cell has no style id

   These helpers should treat a null node as `InvalidValue`, validate `x` and
   `y` against the referenced page node before reading the row/cell, and return
   `InvalidValue` for out-of-range coordinates. They should not attempt to
   search a terminal's `PageList` for the node, because these accessors
   intentionally do not receive a terminal handle. Document the helper as
   unsafe-in-spirit even if the public Rust function is safe: callers must
   supply a currently valid borrowed grid ref.

3. Do not add terminal-bound public wrappers for these five functions in this
   experiment. The exported ABI should match upstream's standalone helper shape.
   Any future terminal-bound safe wrapper would be an additional Roastty API,
   not the upstream C ABI port.

4. In `roastty/src/lib.rs`, add the five exported C functions.
   - Validate null input pointers before reading or writing.
   - Reuse `read_grid_ref_ptr` and `grid_ref_error_result`.
   - `roastty_grid_ref_cell` and `roastty_grid_ref_row` should allow null output
     pointers and still return `ROASTTY_SUCCESS`, matching upstream's "validate
     ref first, write only if output exists" behavior.
   - `roastty_grid_ref_style` should validate `out->size` before writing. It
     should allow a null `out` pointer and still return `ROASTTY_SUCCESS` after
     the ref is validated, matching upstream's null-output behavior. Non-null
     `out` reads `size` before any write; `size < sizeof(roastty_style_s)`
     returns `ROASTTY_INVALID_VALUE`; success rewrites the whole struct
     including `size = sizeof(roastty_style_s)`.
   - `roastty_grid_ref_graphemes` should set `out_len` to zero and return
     success for cells with no text. For text cells it should report the full
     codepoint count and return `ROASTTY_OUT_OF_SPACE` when the output buffer is
     null or too small. On success it should write the base codepoint followed
     by stored grapheme continuation codepoints.
   - `roastty_grid_ref_hyperlink_uri` should set `out_len` to zero and return
     success for cells with no hyperlink or missing hyperlink payload. For
     linked cells it should report the URI byte length and return
     `ROASTTY_OUT_OF_SPACE` when the output buffer is null or too small. On
     success it should copy the URI bytes without appending a NUL terminator.
   - `roastty_grid_ref_graphemes` and `roastty_grid_ref_hyperlink_uri` should
     return `ROASTTY_INVALID_VALUE` when `out_len` is null, before inspecting
     the optional output buffer.

5. Update `roastty/tests/abi_harness.c` to compile and exercise the new
   prototypes at least once from C.

## Verification

Run the focused and full verification set:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page_list.rs roastty/src/terminal/terminal.rs
cargo test -p roastty grid_ref_accessor_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The focused Rust tests must cover:

- cell and row access from a valid ref
- style access for default and non-default styled cells
- grapheme access for empty, one-codepoint, and multi-codepoint cells
- hyperlink URI access for no-link and linked cells
- out-of-space behavior for graphemes and hyperlink URI
- null outputs where upstream allows them
- null refs, undersized refs, null `out_len`, null node, and out-of-range
  coordinates

Do not add a stale-ref test for the standalone accessors. A stale borrowed node
pointer is caller-invalid under the existing grid-ref lifetime contract and is
not safely detectable without a terminal receiver. Existing tracked-grid-ref
tests remain responsible for mutation-safe references.

The experiment passes when all five public grid-ref accessors are available
through `roastty/include/roastty.h`, behave like the upstream C helpers under
the validation rules above, and the full `cargo test -p roastty` suite passes.

## Result

**Result:** Pass

Implemented the standalone grid-reference accessor C ABI:

- `roastty_grid_ref_cell`
- `roastty_grid_ref_row`
- `roastty_grid_ref_graphemes`
- `roastty_grid_ref_hyperlink_uri`
- `roastty_grid_ref_style`

The implementation keeps the upstream standalone helper shape and uses the
existing borrowed `roastty_grid_ref_s` lifetime contract. The internal
borrowed-node read helper validates null nodes and out-of-range coordinates, but
does not attempt terminal-backed stale-ref detection because these functions do
not receive a terminal handle.

The result also added focused Rust coverage for cell, row, default style,
non-default style, empty graphemes, one-codepoint graphemes, multi-codepoint
graphemes, hyperlink URI reads, out-of-space behavior, null `out_len`, null
outputs where upstream allows them, undersized refs, null nodes, and
out-of-range refs. The C harness now compiles and calls the new prototypes from
`roastty.h`.

Verification passed:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty grid_ref_accessor_c_abi
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Codex reviewed the result and initially found one real gap: the focused tests
covered empty and multi-codepoint grapheme cells, but not the required
one-codepoint text cell. That test case was added. Codex re-reviewed the
corrected result and approved it with no blocking findings.

## Conclusion

The upstream `terminal/c/grid_ref.zig` standalone accessor surface is now ported
to Roastty. This completes the direct grid-ref read helpers needed by C callers
without changing tracked-grid-ref semantics or terminal-bound coordinate
conversion.
