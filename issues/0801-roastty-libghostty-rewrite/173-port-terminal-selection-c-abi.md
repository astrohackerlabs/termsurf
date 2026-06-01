# Experiment 173: Port Terminal Selection C ABI

## Description

Port the upstream terminal selection C API into Roastty with Roastty naming.
This is the natural next ABI slice after Experiment 172 because upstream
selection values are expressed as two grid references plus a rectangle flag.

Roastty already has most of the internal selection behavior:

- word, word-between, line, all, and output selection derivation on `PageList`;
- selection ordering, containment, equality, and endpoint adjustment helpers;
- selection formatting through the existing screen and terminal formatter paths.

What is still missing is the public C ABI layer and active-selection storage.
Unlike upstream Ghostty, the current Roastty `Screen` does not store an active
selection yet; formatter tests pass explicit selections directly. This
experiment adds the active selection state required by the upstream
`terminal.set(.selection, ...)`, `terminal.get(.selection, ...)`, and selection
formatting fallback behavior.

## Changes

1. Add public selection ABI types to `roastty/include/roastty.h` and matching
   Rust `#[repr(C)]` structs/enums in `roastty/src/lib.rs`:
   - `roastty_selection_s`
     - `size_t size`
     - `roastty_grid_ref_s start`
     - `roastty_grid_ref_s end`
     - `bool rectangle`
   - select-word, select-word-between, and select-line option structs matching
     the upstream field order with Roastty names.
   - selection format options matching upstream semantics:
     - `emit`
     - `unwrap`
     - `trim`
     - optional selection pointer
   - selection order and adjustment enums using upstream discriminants, renamed
     to `ROASTTY_SELECTION_*`.
   - formatter emit constants used by selection formatting:
     - `ROASTTY_SELECTION_FORMAT_PLAIN = 0`
     - `ROASTTY_SELECTION_FORMAT_VT = 1`
     - `ROASTTY_SELECTION_FORMAT_HTML = 2`
   - order constants:
     - `ROASTTY_SELECTION_ORDER_FORWARD = 0`
     - `ROASTTY_SELECTION_ORDER_REVERSE = 1`
     - `ROASTTY_SELECTION_ORDER_MIRRORED_FORWARD = 2`
     - `ROASTTY_SELECTION_ORDER_MIRRORED_REVERSE = 3`
   - adjustment constants:
     - `ROASTTY_SELECTION_ADJUST_LEFT = 0`
     - `ROASTTY_SELECTION_ADJUST_RIGHT = 1`
     - `ROASTTY_SELECTION_ADJUST_UP = 2`
     - `ROASTTY_SELECTION_ADJUST_DOWN = 3`
     - `ROASTTY_SELECTION_ADJUST_HOME = 4`
     - `ROASTTY_SELECTION_ADJUST_END = 5`
     - `ROASTTY_SELECTION_ADJUST_PAGE_UP = 6`
     - `ROASTTY_SELECTION_ADJUST_PAGE_DOWN = 7`
     - `ROASTTY_SELECTION_ADJUST_BEGINNING_OF_LINE = 8`
     - `ROASTTY_SELECTION_ADJUST_END_OF_LINE = 9`

   Rust must receive incoming enum fields and arguments as raw `c_int` values
   and convert them with checked helpers. Do not transmute C enum values into
   Rust enums.

2. Add conversion helpers in `roastty/src/lib.rs`:
   - `read_selection(ptr)` validates `size` before reading the rest of
     `roastty_selection_s`.
   - `read_grid_ref(ptr)` reuses the Experiment 172 safety rule: read only the
     `size` field before validating that the struct is large enough.
   - both selection endpoints must resolve to valid pins in the same terminal.
   - foreign-terminal refs, forged `x`/`y`, null pointers, unknown point tags,
     undersized structs, and invalid enum values return `ROASTTY_INVALID_VALUE`.
   - `write_selection(out, selection)` writes the top-level `size` field and
     both nested `roastty_grid_ref_s.size` fields.
   - no C caller may dereference, free, retain, or mutate the opaque grid-ref
     node pointer.

3. Add active-selection state to the active `Screen`:
   - store `Option<selection::Selection>` on `Screen`;
   - store selections using tracked pins so the active selection follows page
     storage movement where the existing `PageList::track_selection` machinery
     supports that;
   - clear/untrack active selections on reset and when setting a null selection;
   - update active-selection tracking correctly when replacing an existing
     selection;
   - replacement must be atomic: setting an invalid new selection must leave the
     prior active selection and its tracked pins unchanged;
   - keep alternate and primary screen selections independent, matching the
     existing active-screen model.

4. Add narrow internal forwarding methods instead of exposing `PageList`
   internals broadly:
   - `Screen::select_word`
   - `Screen::select_word_between`
   - `Screen::select_line`
   - `Screen::select_all`
   - `Screen::select_output`
   - `Screen::selection_adjust`
   - `Screen::selection_order`
   - `Screen::selection_ordered`
   - `Screen::selection_contains`
   - `Screen::selection_equal`
   - `Screen::set_selection`
   - `Screen::clear_selection`
   - `Screen::active_selection`
   - matching `Terminal` wrappers that operate only on `screens.active`

   The implementation should avoid making `PageList` selection helpers public
   beyond what these wrappers need.

5. Implement the renamed selection C functions:

   ```c
   ROASTTY_API roastty_result_e roastty_terminal_select_word(
       roastty_terminal_t,
       const roastty_terminal_select_word_options_s*,
       roastty_selection_s*);
   ROASTTY_API roastty_result_e roastty_terminal_select_word_between(
       roastty_terminal_t,
       const roastty_terminal_select_word_between_options_s*,
       roastty_selection_s*);
   ROASTTY_API roastty_result_e roastty_terminal_select_line(
       roastty_terminal_t,
       const roastty_terminal_select_line_options_s*,
       roastty_selection_s*);
   ROASTTY_API roastty_result_e roastty_terminal_select_all(
       roastty_terminal_t,
       roastty_selection_s*);
   ROASTTY_API roastty_result_e roastty_terminal_select_output(
       roastty_terminal_t,
       const roastty_grid_ref_s*,
       roastty_selection_s*);
   ROASTTY_API roastty_result_e roastty_terminal_selection_adjust(
       roastty_terminal_t,
       roastty_selection_s*,
       int adjustment);
   ROASTTY_API roastty_result_e roastty_terminal_selection_order(
       roastty_terminal_t,
       const roastty_selection_s*,
       int* out_order);
   ROASTTY_API roastty_result_e roastty_terminal_selection_ordered(
       roastty_terminal_t,
       const roastty_selection_s*,
       int desired_order,
       roastty_selection_s*);
   ROASTTY_API roastty_result_e roastty_terminal_selection_contains(
       roastty_terminal_t,
       const roastty_selection_s*,
       roastty_point_s,
       bool*);
   ROASTTY_API roastty_result_e roastty_terminal_selection_equal(
       roastty_terminal_t,
       const roastty_selection_s*,
       const roastty_selection_s*,
       bool*);
   ROASTTY_API roastty_result_e roastty_terminal_selection_format_buf(
       roastty_terminal_t,
       const roastty_terminal_selection_format_options_s*,
       uint8_t* out,
       size_t out_len,
       size_t* out_written);
   ROASTTY_API roastty_result_e roastty_terminal_selection_format(
       roastty_terminal_t,
       const roastty_terminal_selection_format_options_s*,
       roastty_string_s*);
   ```

   Every sized input struct is passed by pointer, even where upstream passes by
   value, so Roastty can validate `size` before reading trailing fields. Null
   output pointers return `ROASTTY_INVALID_VALUE`. For
   `roastty_terminal_selection_format_buf`, `out == NULL && out_len == 0` is a
   size probe and returns `ROASTTY_OUT_OF_SPACE` with `out_written` set to the
   required byte count; `out == NULL && out_len > 0` is invalid.
   `out != NULL && out_len < required` also returns `ROASTTY_OUT_OF_SPACE` and
   sets `out_written` to the required byte count.

   `roastty_terminal_selection_format` is the Roastty-owned allocation
   equivalent of upstream `format_alloc`: it returns `roastty_string_s` and is
   freed with `roastty_string_free`, consistent with existing Roastty
   string-returning APIs. Do not introduce a separate C allocator ABI in this
   experiment.

6. Wire terminal option/data selection:
   - add `ROASTTY_TERMINAL_OPTION_SELECTION = 21`, matching upstream's
     `selection = 21`;
   - leave option IDs `15..20` unimplemented for the deferred Kitty/APC options;
   - `roastty_terminal_set(..., ROASTTY_TERMINAL_OPTION_SELECTION, ptr)` sets
     the active selection;
   - `roastty_terminal_set(..., ROASTTY_TERMINAL_OPTION_SELECTION, NULL)` clears
     the active selection;
   - `roastty_terminal_get(..., ROASTTY_TERMINAL_DATA_SELECTION, out)` returns
     the active selection or `ROASTTY_NO_VALUE` when none exists.

7. Preserve codepoint-array semantics from upstream:
   - `NULL + len 0` means "use the API default";
   - non-null + len 0 means "use an explicitly empty boundary set";
   - len > 0 requires a non-null pointer;
   - reject values that are not valid Unicode scalar values before using them in
     Rust selection logic.

8. Keep print and formatter scope narrow:
   - support `Plain`, `Vt`, and `Html` emit modes only if they already exist in
     the Roastty formatter ABI;
   - do not add styled map, point map, pin map, or allocator ABI in this
     experiment;
   - do not change selection derivation semantics.

9. `roastty_terminal_selection_contains` takes `roastty_point_s`, not a grid
   reference, matching upstream's point-based containment API. Validate the raw
   point tag before constructing an internal point, reject out-of-bounds points,
   and test invalid point tags.

## Verification

1. Run `cargo fmt` after all Rust edits and accept its output.

2. Add Rust ABI tests in `roastty/src/lib.rs` for:
   - exact selection/options struct sizes, alignment, and offsets;
   - enum discriminants;
   - active selection set/get/clear through terminal option/data;
   - setting an invalid replacement selection leaves the previous active
     selection readable and unchanged;
   - `ROASTTY_TERMINAL_DATA_SELECTION` writes the top-level `size` field and
     both nested grid-ref `size` fields;
   - format fallback to active selection;
   - format with an explicit selection overriding active selection;
   - null buffer size probing and `ROASTTY_OUT_OF_SPACE`;
   - non-null too-small format buffers return `ROASTTY_OUT_OF_SPACE` and report
     the required byte count instead of truncating;
   - invalid emit/order/adjustment values;
   - invalid `roastty_point_s` tags and out-of-bounds points for
     `selection_contains`;
   - undersized selection and option structs validated before trailing fields
     are read;
   - null pointers;
   - foreign-terminal grid refs;
   - forged grid-ref coordinates;
   - `NULL + len 0`, non-null + len 0, len > 0 + null, and invalid scalar
     codepoint arrays.

3. Add C harness coverage in `roastty/tests/abi_harness.c` for:
   - header compile/link coverage for every new exported function;
   - C-side `sizeof`, `alignof`, and `offsetof` checks;
   - selecting `"World"` from `"Hello World"` through word selection and
     formatting it;
   - `selection_format_buf` with a too-small non-null buffer reports the
     required byte count;
   - setting the active selection through `roastty_terminal_set` and reading it
     back through `roastty_terminal_get`;
   - adjusting an active or explicit selection and verifying the endpoint moves;
   - `selection_contains`, `selection_order`, `selection_ordered`, and
     `selection_equal`;
   - output selection on an OSC 133 semantic prompt fixture if the existing C
     harness can create the fixture without new helper infrastructure.

4. Port or reference upstream tests from
   `vendor/ghostty/src/terminal/c/selection.zig`:
   - format uses active selection;
   - format uses provided selection;
   - format returns no value without active selection;
   - word/line/all/output helper behavior is covered through existing Roastty
     internal tests and the new ABI smoke tests.

5. Run:

   ```bash
   cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
   cargo test -p roastty terminal_selection_c_abi
   cargo test -p roastty terminal_grid_ref
   cargo test -p roastty terminal_get_abi
   cargo test -p roastty terminal_stream
   cargo test -p roastty
   ! rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c
   git diff --check
   ```

   The no-Ghostty check must produce no matches in the edited Roastty
   source/header files. References to upstream Ghostty remain allowed in issue
   documentation and vendored paths.

## Failure Criteria

- The public API exposes `ghostty_*` names or compatibility aliases.
- Selection structs read trailing fields before validating their `size`.
- Active selection is stored as untracked pins and can become stale across page
  storage mutation.
- Setting a new active selection leaks tracked pins from the previous active
  selection.
- Setting an invalid replacement selection clears or changes the previous active
  selection.
- Selection formatting silently falls back to full-screen output when no active
  selection exists; it must return `ROASTTY_NO_VALUE`.
- The experiment changes selection derivation behavior instead of only exposing
  it.
- The experiment introduces Kitty graphics, APC, allocator, print, map, or
  platform integration work outside the selection C ABI slice.

## Review

This design must be reviewed with the Codex review skill before implementation.
All real findings must be fixed, and the design must be re-reviewed until Codex
approves it.

After implementation and result recording, the completed result must also be
reviewed with Codex and approved before the result commit.

## Result

**Result:** Pass

Implemented the terminal selection C ABI foundation with Roastty naming:

- added `roastty_selection_s`, selection option structs, format/order/adjustment
  enums, and exported selection functions to `roastty/include/roastty.h`;
- added active-selection storage on `Screen` using tracked pins;
- wired `ROASTTY_TERMINAL_OPTION_SELECTION = 21` and
  `ROASTTY_TERMINAL_DATA_SELECTION = 31`;
- added size-first C ABI readers for selection, nested grid refs, option
  structs, and codepoint arrays;
- exposed word, word-between, line, all, output, adjust, order, ordered,
  contains, equal, buffer formatting, and Roastty-owned string formatting;
- preserved the existing string ownership model with `roastty_string_free`
  instead of introducing a new allocator ABI;
- added Rust ABI tests and C harness coverage for layout, active set/get,
  formatting, relation helpers, adjustment, invalid enum values, undersized
  structs, invalid codepoint arrays, foreign/forged refs, and atomic
  replacement;
- added C harness link/smoke coverage for every new exported selection symbol.
  The C harness checks the `select_output` no-value path because the successful
  output-selection fixture depends on lower-level semantic prompt state that is
  already covered by Rust tests.

Verification run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page_list.rs roastty/src/terminal/screen.rs roastty/src/terminal/terminal.rs
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_selection_c_abi
cargo test -p roastty terminal_grid_ref
cargo test -p roastty terminal_get_abi
cargo test -p roastty terminal_stream
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

All verification passed.

## Conclusion

Roastty now has a public selection ABI layer over the internal selection
machinery. This closes the main dependency opened by Experiment 172: C callers
can construct, store, query, adjust, compare, and format selections using
grid-ref-backed values while preserving size-first ABI safety and Roastty
ownership conventions.

The next experiment can continue outward from terminal interaction state rather
than selection foundations. Candidate next slices include terminal drag
selection C ABI, selection/clipboard integration, or another remaining
libroastty API surface that depends on selection now being externally
representable.
