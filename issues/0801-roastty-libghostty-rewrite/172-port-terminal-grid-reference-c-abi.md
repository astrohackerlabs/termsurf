# Experiment 172: Port Terminal Grid Reference C ABI

## Description

Experiment 171 completed the terminal query callback slice. The next coherent
terminal ABI foundation is the public grid-reference surface that upstream uses
as the coordinate bridge for selection, selection gestures, formatter selection
options, render-row selection metadata, hyperlink lookup, style lookup, and
tracked references.

Port the non-owning grid-reference foundation first:

- public point/tag value types;
- public `roastty_grid_ref_s`;
- `roastty_terminal_grid_ref`;
- `roastty_terminal_point_from_grid_ref`;
- enough Rust terminal/page-list helpers to convert between public points and
  internal pins.

Do not port the full selection API in this experiment. Selection depends on grid
refs, but it also needs `roastty_selection_s`, format options, formatter output,
and active-selection set/get behavior. That belongs in the next selection
experiment once grid refs are public and tested.

Do not port tracked grid refs in this experiment. Upstream's tracked refs are an
owned, terminal-registered lifecycle object that survives page movement and is
detached on terminal free. Roastty has internal tracked-pin machinery, but the
public tracked-ref lifecycle needs its own experiment.

Do not port `grid_ref_cell`, `grid_ref_row`, `grid_ref_style`,
`grid_ref_graphemes`, or `grid_ref_hyperlink_uri` yet. The current public
Roastty header does not expose cell, row, style, or hyperlink ABI types. This
experiment should not invent those side surfaces while solving the grid-ref
foundation. Those extraction APIs are follow-up work after the corresponding
public value types exist.

Upstream reference points:

- `vendor/ghostty/src/terminal/c/terminal.zig`:
  - `grid_ref`;
  - `point_from_grid_ref`;
  - grid-ref tests around active, viewport, screen, and history coordinates;
- `vendor/ghostty/src/terminal/c/grid_ref.zig`:
  - `CGridRef` layout and non-owning `fromPin` / `toPin` behavior;
- `vendor/ghostty/src/terminal/c/selection.zig`:
  - `CSelection` stores `CGridRef` endpoints;
- `vendor/ghostty/src/terminal/c/selection_gesture.zig`:
  - selection gestures consume grid refs as event anchors.

## Changes

### 1. Add public point and grid-ref value types

In `roastty/include/roastty.h`, add Roastty-named equivalents of upstream's
public point/tag and grid-ref foundation:

```c
typedef enum {
  ROASTTY_POINT_ACTIVE = 0,
  ROASTTY_POINT_VIEWPORT = 1,
  ROASTTY_POINT_SCREEN = 2,
  ROASTTY_POINT_HISTORY = 3,
} roastty_point_tag_e;

typedef struct {
  uint16_t x;
  uint32_t y;
} roastty_point_coordinate_s;

typedef union {
  roastty_point_coordinate_s active;
  roastty_point_coordinate_s viewport;
  roastty_point_coordinate_s screen;
  roastty_point_coordinate_s history;
  uint64_t _padding[2];
} roastty_point_value_u;

typedef struct {
  roastty_point_tag_e tag;
  roastty_point_value_u value;
} roastty_point_s;

typedef struct {
  size_t size;
  void* node;
  uint16_t x;
  uint16_t y;
} roastty_grid_ref_s;
```

Keep the discriminants aligned with upstream point tags and current Rust
internals:

- active = `0`;
- viewport = `1`;
- screen = `2`;
- history = `3`.

`roastty_point_s` must mirror upstream's tagged-union C ABI shape, not flatten
the coordinate into a single shared field. Upstream uses `tag` plus a union
value padded to `[2]u64` for forward ABI stability. Roastty should preserve that
shape with renamed public identifiers so future point variants can be added
without changing the value storage size.

Add Rust and C tests for `sizeof`, alignment, and field offsets for
`roastty_point_coordinate_s`, `roastty_point_value_u`, and `roastty_point_s`.
The exact values should be derived from the compiled C header on this target,
but the required property is that `roastty_point_value_u` is at least 16 bytes
and contains the four coordinate arms plus `_padding[2]`.

`roastty_grid_ref_s.size` must be initialized to `sizeof(roastty_grid_ref_s)` on
successful output, matching the project's sized-struct pattern and upstream
`CGridRef`.

The `node` field is intentionally opaque. C callers must not inspect,
dereference, retain ownership of, or free it.

### 2. Add terminal grid-ref functions

In `roastty/include/roastty.h`, expose:

```c
ROASTTY_API roastty_result_e roastty_terminal_grid_ref(
    roastty_terminal_t terminal,
    roastty_point_s point,
    roastty_grid_ref_s* out_ref);

ROASTTY_API roastty_result_e roastty_terminal_point_from_grid_ref(
    roastty_terminal_t terminal,
    const roastty_grid_ref_s* ref,
    roastty_point_tag_e tag,
    roastty_point_coordinate_s* out_coordinate);
```

Function behavior:

- null terminal returns `ROASTTY_INVALID_VALUE`;
- null `out_ref` for `roastty_terminal_grid_ref` returns
  `ROASTTY_INVALID_VALUE`;
- null `ref` or null `out_coordinate` for `roastty_terminal_point_from_grid_ref`
  returns `ROASTTY_INVALID_VALUE`;
- `ref->size < sizeof(roastty_grid_ref_s)` returns `ROASTTY_INVALID_VALUE`
  before reading `ref->node`, `ref->x`, or `ref->y`;
- unknown point tag returns `ROASTTY_INVALID_VALUE`;
- point coordinates outside the selected coordinate space return
  `ROASTTY_INVALID_VALUE`;
- a valid point writes a non-null `node` plus `x` / `y` into `out_ref`;
- converting a ref back to a coordinate in a coordinate space that contains the
  referenced cell returns `ROASTTY_SUCCESS`;
- converting a ref into a coordinate space that does not contain the referenced
  cell returns `ROASTTY_NO_VALUE`, matching upstream's history-to-active case.

Intentional stricter-null divergence from upstream: upstream's
`terminal_grid_ref` and `terminal_point_from_grid_ref` allow null output
pointers after validating the input. Roastty should return
`ROASTTY_INVALID_VALUE` for null output pointers in this new C ABI, matching the
stricter error style already used by the current Roastty terminal ABI. Record
this as a deliberate safety choice in code comments or tests.

### 3. Add Rust conversion helpers

In `roastty/src/lib.rs`, add `#[repr(C)]` Rust equivalents for the public point,
coordinate, and grid-ref structs.

Represent public C enum values that cross into Rust as raw integer ABI storage,
not as Rust enums. In particular:

- the `tag` field inside the Rust `RoasttyPoint` mirror should be `c_int` or an
  equivalent integer type, not a Rust enum;
- the `tag` argument to `roastty_terminal_point_from_grid_ref` should be
  accepted as `c_int`;
- a checked helper must convert the raw integer into the internal
  `terminal::point::Tag`;
- unknown raw tag values must return `ROASTTY_INVALID_VALUE` before constructing
  any Rust enum or internal point type.

This is required because C can pass arbitrary integer values for enum-typed ABI
fields. Rust must not model those incoming fields as Rust enums until after
validation, or invalid tags could become undefined behavior before Roastty can
return `ROASTTY_INVALID_VALUE`.

In `roastty/src/terminal`, add narrowly scoped helpers rather than exposing page
storage broadly:

- convert a C point tag plus coordinate into `terminal::point::Point`;
- convert an internal `terminal::point::Point` into its coordinate component;
- create a non-owning grid ref from the active screen's `PageList::pin`;
- convert a non-owning grid ref back through `PageList::point_from_pin`.

The helper may need to make `PageList::point_from_pin` visible to the terminal
module. Keep that visibility as narrow as possible, for example `pub(super)`,
and do not expose `PageList::Node` or internal page storage outside `terminal`.

`roastty_terminal_point_from_grid_ref` must validate that `ref->node` belongs to
the same terminal's active `PageList` before any dereference of the opaque node
pointer. The safe implementation shape is: treat `node` as an identity pointer,
search the page list for matching node identity, and only construct/dereference
an internal pin after membership is proven. A grid ref from another terminal
must not be dereferenced through the receiving terminal.

After node membership is proven and before coordinate conversion, validate the
caller-provided coordinates against that node's page geometry:

- `ref->x < node.page.size_cols()`;
- `ref->y < node.page.size_rows()`.

C callers can mutate `roastty_grid_ref_s` fields, so a same-terminal `node`
pointer does not prove that `x` or `y` are still valid. Implement this either as
a checked `PageList` helper that internally uses the existing pin-validity
logic, or as an equivalent bounds check at the point where the matching node is
found. Do not pass a forged out-of-bounds pin into `PageList::point_from_pin`.

### 4. Define the snapshot lifetime contract

Document the grid-ref lifetime contract in `roastty/include/roastty.h` near
`roastty_grid_ref_s`:

- `roastty_grid_ref_s` is a borrowed snapshot reference into the terminal's
  current page storage;
- it is valid only for immediate calls back into the same terminal;
- callers must not use it after `roastty_terminal_free`;
- callers must not use it after terminal mutation, including
  `roastty_terminal_vt_write`, reset, resize, future scrollback mutation APIs,
  or future selection/gesture mutation APIs;
- long-lived references belong to the future tracked-grid-ref ABI, not this
  non-owning struct.

This contract is stricter than upstream's raw-pointer behavior on purpose:
Roastty should not imply that a borrowed raw page pointer is stable across
terminal mutation until tracked refs are ported.

### 5. Add Rust and C ABI tests

Add Rust tests in `roastty/src/lib.rs` for:

- public point tag discriminants;
- public point tagged-union ABI size/alignment/offset properties;
- `roastty_grid_ref_s.size` output;
- `roastty_terminal_point_from_grid_ref` rejects undersized `roastty_grid_ref_s`
  input before reading fields;
- active point round trip;
- viewport point round trip;
- screen point round trip after enough output creates scrollback;
- history/screen ref converting to active returns `ROASTTY_NO_VALUE`;
- a grid ref from one terminal passed to another terminal returns
  `ROASTTY_NO_VALUE` or `ROASTTY_INVALID_VALUE` without dereferencing the
  foreign node pointer;
- a same-terminal grid ref with a valid `node` but forged out-of-bounds `x`
  returns `ROASTTY_INVALID_VALUE`;
- a same-terminal grid ref with a valid `node` but forged out-of-bounds `y`
  returns `ROASTTY_INVALID_VALUE`;
- null terminal;
- null output pointers;
- unknown point tag;
- out-of-bounds x and y.

Extend `roastty/tests/abi_harness.c` so the compiled public header proves:

- the point and grid-ref structs are visible to C;
- tag constants have the expected values;
- point value union size and offsets match the intended tagged-union ABI shape;
- `roastty_terminal_grid_ref` can read a point written by C;
- `roastty_terminal_point_from_grid_ref` can convert the result back to C
  coordinates;
- undersized `roastty_grid_ref_s` input is rejected;
- forged same-terminal `x` and `y` coordinates are rejected;
- invalid/null calls return the expected `roastty_result_e`.

### 6. Keep scope out of adjacent ABI surfaces

Do not change:

- selection structs or functions;
- formatter structs or functions;
- render iteration structs or functions;
- tracked grid refs;
- cell/row/style/hyperlink public ABI;
- terminal resize/reflow;
- Kitty graphics.

This experiment only establishes the coordinate/ref bridge needed by those
future surfaces.

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/mod.rs \
  roastty/src/terminal/page_list.rs roastty/src/terminal/point.rs \
  roastty/src/terminal/terminal.rs
cargo test -p roastty terminal_grid_ref
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_get_abi
cargo test -p roastty terminal_stream
cargo test -p roastty
! rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs \
  roastty/include/roastty.h roastty/tests/abi_harness.c
git diff --check
```

The experiment passes only if:

- every command above succeeds;
- the C harness links against the public header and exercises the new API;
- no `ghostty_*` public names are introduced;
- `roastty_grid_ref_s` remains a non-owning borrowed ref, with no allocation or
  free API added for it;
- selection and tracked refs remain unimplemented and explicitly available for
  follow-up experiments.

## Codex Review Requirements

Before implementation, run the Codex review skill against this experiment
design. Fix every real design issue Codex finds and rerun review until Codex
approves the design.

After implementation, run Codex review again against the code diff, test output,
and recorded result. Fix every real issue before marking the experiment `Pass`,
`Partial`, or `Fail`.

## Result

**Result:** Pass

Implemented the non-owning terminal grid-reference C ABI:

- added public `roastty_point_tag_e`, `roastty_point_coordinate_s`,
  `roastty_point_value_u`, `roastty_point_s`, and `roastty_grid_ref_s`;
- added `roastty_terminal_grid_ref`;
- added `roastty_terminal_point_from_grid_ref`;
- documented the borrowed snapshot lifetime contract in `roastty.h`;
- preserved the upstream tagged-union point ABI shape with `[2]u64` padding;
- kept incoming C point tags as raw integer storage in Rust until validation;
- validated `roastty_grid_ref_s.size` before reading trailing fields;
- validated node membership before using the opaque node pointer;
- validated same-terminal forged `x` / `y` coordinates before constructing an
  internal pin;
- added Rust and C ABI coverage for layout, successful round trips, null
  pointers, unknown tags, undersized refs, foreign-terminal refs, and forged
  coordinates.

Verification run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/page_list.rs \
  roastty/src/terminal/point.rs roastty/src/terminal/screen.rs \
  roastty/src/terminal/terminal.rs
cargo test -p roastty c_harness_links_against_roastty_header_and_roastty_dylib
cargo test -p roastty terminal_grid_ref
cargo test -p roastty terminal_get_abi
cargo test -p roastty terminal_stream
cargo test -p roastty
if rg -n "ghostty|Ghostty|ghostty_" roastty/src/lib.rs \
  roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

All verification commands passed. The full `cargo test -p roastty` run passed
1830 Rust tests plus the C harness.

## Conclusion

Experiment 172 establishes the public coordinate/ref bridge needed by later
selection, selection gesture, formatter-selection, render-selection, and
tracked-ref work. The implementation intentionally stops at non-owning grid refs
and does not expose cell, row, style, hyperlink, selection, or tracked-ref APIs.

The next experiment can build on this by porting the selection C ABI foundation:
`roastty_selection_s`, active selection set/get, basic selection derivation
helpers, and selection formatting, while continuing to defer gesture and
tracked-reference lifecycles if they do not fit the same reviewable slice.
