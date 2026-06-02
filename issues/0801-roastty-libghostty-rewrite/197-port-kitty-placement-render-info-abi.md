# Experiment 197: Port Kitty Placement Render Info ABI

## Description

Experiment 196 exposed Kitty graphics image snapshots and placement iteration
through the public C ABI, but it deliberately left renderer-facing geometry
helpers out of scope. A renderer can now discover that an image and placement
exist, but it still cannot ask the ABI:

- how large the placement should be in pixels;
- how many terminal grid cells it occupies;
- where the placement is relative to the viewport;
- what source rectangle should be sampled from the image;
- whether the placement is currently visible;
- what rectangle of terminal cells the placement covers.

This experiment ports the coherent geometry/render-info slice from upstream's
Kitty graphics C ABI using Roastty names:

- `placement_rect`;
- `placement_pixel_size`;
- `placement_grid_size`;
- `placement_viewport_pos`;
- `placement_source_rect`;
- `placement_render_info`.

This experiment is still ABI and math only. It does not render images, decode
PNG, add Metal, add non-direct media, or draw Unicode virtual placeholders.

Use upstream source as the behavior reference:

- `vendor/ghostty/src/terminal/c/kitty_graphics.zig`
- `vendor/ghostty/src/terminal/kitty/graphics_storage.zig`
- `vendor/ghostty/src/terminal/Screen.zig`
- `vendor/ghostty/src/terminal/Terminal.zig`

All public names must be Roastty names.

## Changes

1. Add the public render-info struct and function declarations in
   `roastty/include/roastty.h`.

   Add:

   ```c
   typedef struct roastty_kitty_graphics_placement_render_info_s {
     size_t size;
     uint32_t pixel_width;
     uint32_t pixel_height;
     uint32_t grid_cols;
     uint32_t grid_rows;
     int32_t viewport_col;
     int32_t viewport_row;
     bool viewport_visible;
     uint32_t source_x;
     uint32_t source_y;
     uint32_t source_width;
     uint32_t source_height;
   } roastty_kitty_graphics_placement_render_info_s;
   ```

   Add functions:

   ```c
   roastty_result_e roastty_kitty_graphics_placement_rect(
       roastty_kitty_graphics_placement_iterator_t,
       roastty_kitty_graphics_image_t,
       roastty_terminal_t,
       roastty_selection_s*);

   roastty_result_e roastty_kitty_graphics_placement_pixel_size(
       roastty_kitty_graphics_placement_iterator_t,
       roastty_kitty_graphics_image_t,
       roastty_terminal_t,
       uint32_t*,
       uint32_t*);

   roastty_result_e roastty_kitty_graphics_placement_grid_size(
       roastty_kitty_graphics_placement_iterator_t,
       roastty_kitty_graphics_image_t,
       roastty_terminal_t,
       uint32_t*,
       uint32_t*);

   roastty_result_e roastty_kitty_graphics_placement_viewport_pos(
       roastty_kitty_graphics_placement_iterator_t,
       roastty_kitty_graphics_image_t,
       roastty_terminal_t,
       int32_t*,
       int32_t*);

   roastty_result_e roastty_kitty_graphics_placement_source_rect(
       roastty_kitty_graphics_placement_iterator_t,
       roastty_kitty_graphics_image_t,
       uint32_t*,
       uint32_t*,
       uint32_t*,
       uint32_t*);

   roastty_result_e roastty_kitty_graphics_placement_render_info(
       roastty_kitty_graphics_placement_iterator_t,
       roastty_kitty_graphics_image_t,
       roastty_terminal_t,
       roastty_kitty_graphics_placement_render_info_s*);
   ```

2. Add internal geometry helpers in `roastty/src/lib.rs`.

   Add helper accessors for the currently selected placement iterator entry and
   selected placement key.

   Important safety rule: geometry functions must not blindly dereference the
   `PlacementLocation::Pin` pointer copied into the Experiment 196 iterator
   snapshot. That pointer can become stale if the terminal mutates after the
   iterator snapshot is captured. For every geometry function that needs live
   placement state, use the selected placement key to re-lookup the placement in
   the terminal's current active-screen Kitty graphics storage. If the selected
   key no longer exists, return `ROASTTY_NO_VALUE`.

   `placement_source_rect` may use the selected placement snapshot because it
   only needs placement source fields and the owned image snapshot; it does not
   dereference tracked pins.

3. Expose terminal-side helpers in `roastty/src/terminal/terminal.rs` and
   `roastty/src/terminal/screen.rs`.

   Add narrowly scoped `pub(crate)` helpers instead of exposing page-list
   internals through the C ABI:
   - current Kitty cell metrics from the terminal's Kitty graphics APC state and
     terminal size;
   - live placement lookup by `PlacementKey`;
   - placement rectangle as `TerminalSelection` or equivalent grid refs;
   - viewport-relative placement position and visibility.

   The viewport position must mirror upstream's logic:
   - virtual placements are not visible and report no viewport position;
   - convert the placement pin and viewport top-left to screen coordinates;
   - `viewport_row = placement_screen_y - viewport_screen_y`;
   - `viewport_col = placement_screen_x`;
   - visible iff `viewport_row + grid_rows > 0 && viewport_row < terminal_rows`.

4. Implement the public C functions in `roastty/src/lib.rs`.

   Behavior:
   - null iterator/image/terminal/output pointers return
     `ROASTTY_INVALID_VALUE`;
   - `placement_rect` validates `out->size >= sizeof(roastty_selection_s)`
     before writing and does not partially mutate undersized selection outputs;
   - iterator with no selected entry returns `ROASTTY_NO_VALUE` for geometry
     functions that need a selected placement;
   - selected placement missing from live terminal storage returns
     `ROASTTY_NO_VALUE`;
   - virtual placements return `ROASTTY_NO_VALUE` from `placement_rect` and
     `placement_viewport_pos`, and `viewport_visible = false` from
     `placement_render_info`;
   - zero grid size returns `ROASTTY_NO_VALUE` from `placement_rect`;
   - `placement_source_rect` clamps to image bounds and applies upstream's "0
     means full image dimension" convention;
   - `placement_render_info` validates `out.size >= sizeof(struct)` before
     writing and fills all fields in one call.

5. Add Rust tests in `roastty/src/lib.rs`.

   Add `kitty_graphics_render_info_c_abi` tests covering:
   - pixel size with native source size, explicit columns+rows, columns-only
     aspect-ratio, rows-only aspect-ratio, and zero cell metrics;
   - grid size ceilings with offsets;
   - source rectangle clamping and `0 = full dimension`;
   - placement rect for a tracked placement, including undersized
     `roastty_selection_s` validation without partial mutation;
   - viewport position cases matching upstream coverage: fully visible,
     top-clipped visible, bottom-clipped visible, spanning top-and-bottom
     visible, fully above invisible, below viewport invisible, and virtual
     invisible placements;
   - render-info aggregate output matches the individual helper outputs;
   - render-info rejects undersized structs without partial mutation;
   - stale iterator selection after placement deletion returns
     `ROASTTY_NO_VALUE` rather than dereferencing stale tracked-pin state;
   - stale iterator selection after same-key placement replacement uses the live
     replacement placement or returns `ROASTTY_NO_VALUE`, and never reports the
     old snapshot's tracked-pin position;
   - null handles and null outputs return `ROASTTY_INVALID_VALUE`.

6. Extend the C ABI harness in `roastty/tests/abi_harness.c`.

   Add a smoke test that:
   - transmits and displays one direct image;
   - obtains image and placement iterator handles;
   - calls `placement_rect`;
   - calls `placement_pixel_size`, `placement_grid_size`,
     `placement_source_rect`, `placement_viewport_pos`, and
     `placement_render_info`;
   - checks struct layout with `sizeof`, `_Alignof`, and `offsetof`;
   - confirms `placement_render_info.size` validation rejects undersized
     structs.

7. Preserve formatting and review rules.

   Run:

   ```bash
   cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/screen.rs
   prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/197-port-kitty-placement-render-info-abi.md
   ```

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/screen.rs
cargo test -p roastty kitty_graphics_render_info_c_abi
cargo test -p roastty kitty_graphics_c_abi
cargo test -p roastty terminal_stream_kitty_graphics
cargo test -p roastty --test abi_harness
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when:

- every new geometry/render-info C function exists in the public header and
  links from the C ABI harness;
- pixel size, grid size, source rect, viewport position, placement rect, and
  aggregate render info match upstream semantics;
- geometry functions revalidate selected placement keys against live terminal
  storage before dereferencing tracked placement state;
- stale iterator selections after terminal mutation return `ROASTTY_NO_VALUE`
  instead of reading stale pointers;
- same-key placement replacement after iterator selection does not reuse stale
  snapshot tracked-pin state;
- virtual placements report invisible/no-position behavior without breaking
  source/pixel/grid helpers;
- all existing Kitty graphics execution and ABI tests still pass;
- Codex approves the experiment design before implementation and approves the
  result before the experiment is recorded.

## Non-Negotiable Invariants

- Do not render images.
- Do not decode PNG.
- Do not add Metal or any platform renderer.
- Do not add non-direct image media support.
- Do not add animation execution.
- Do not add Unicode virtual placement rendering.
- Do not expose any `ghostty_*` ABI names.
- Do not weaken Experiment 196's image snapshot ownership model.
- Do not expose live Rust map iterators or borrowed placement references across
  the C ABI.
- Do not dereference a tracked-pin pointer from an old iterator snapshot without
  first revalidating that the selected placement key still exists in the live
  terminal storage.
- Existence revalidation is not enough when the same placement key has been
  replaced. Geometry must use live placement state for that key or return
  `ROASTTY_NO_VALUE`; it must never compute from stale snapshot pin state.
- Do not skip Codex design review or Codex result review.

## Result

**Result:** Pass

Implemented the Kitty placement geometry/render-info C ABI slice with Roastty
public names:

- `roastty_kitty_graphics_placement_rect`
- `roastty_kitty_graphics_placement_pixel_size`
- `roastty_kitty_graphics_placement_grid_size`
- `roastty_kitty_graphics_placement_viewport_pos`
- `roastty_kitty_graphics_placement_source_rect`
- `roastty_kitty_graphics_placement_render_info`

The implementation adds the public render-info struct, C declarations, live
terminal geometry helpers, and C ABI functions. Geometry that depends on live
tracked placement state revalidates the selected iterator key against current
terminal storage before computing. If the selected placement was deleted, the
ABI returns `ROASTTY_NO_VALUE`. If the same key was replaced, geometry uses the
live replacement placement rather than stale tracked-pin state.

The standalone `placement_source_rect` helper remains snapshot-based because it
does not dereference live tracked state. `placement_render_info` uses the same
snapshot source fields so its aggregate source output matches the individual
source helper, while live geometry fields still use the live placement.

Rust tests cover:

- struct layout stability;
- pixel size, grid size, source rect, placement rect, viewport position, and
  aggregate render-info output;
- native source size, explicit grid size, columns-only aspect ratio, rows-only
  aspect ratio, source clamping, and zero cell metrics;
- undersized struct/selection validation without partial mutation;
- stale iterator selection after deletion;
- same-key replacement with changed geometry and changed source fields;
- fully visible, top-clipped, bottom-clipped, spanning, fully-above invisible,
  below-viewport invisible, and virtual invisible viewport cases;
- invalid handles and null outputs.

The C ABI harness now links and calls the new functions through
`roastty/include/roastty.h`, including render-info layout checks and undersized
struct validation.

Verification passed:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/screen.rs
cargo test -p roastty kitty_graphics_render_info_c_abi
cargo test -p roastty kitty_graphics_c_abi
cargo test -p roastty terminal_stream_kitty_graphics
cargo test -p roastty --test abi_harness
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Codex result review initially found two real blockers:

- `placement_render_info` source fields could disagree with
  `placement_source_rect` after same-key replacement;
- viewport tests did not yet cover all required clipped/invisible cases.

Both were fixed. Codex re-reviewed the corrected diff and found no remaining
correctness blockers.

## Conclusion

Experiment 197 successfully ports the renderer-facing Kitty placement geometry
ABI slice. The renderer boundary can now query placement cell rectangles,
pixel/grid dimensions, source sampling rectangles, viewport-relative position,
visibility, and aggregate render info without exposing Rust map iterators or
borrowing live placement references across the C ABI.

The next experiment can move to the next coherent Kitty graphics subsystem
slice. The likely next step is direct image-data/renderer handoff preparation,
while still deferring actual image rendering, PNG decoding, Metal integration,
non-direct media, animation, and Unicode virtual placement drawing until their
own experiments.
