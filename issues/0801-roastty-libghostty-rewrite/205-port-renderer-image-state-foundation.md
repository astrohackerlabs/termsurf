# Experiment 205: Port Renderer Image State Foundation

## Description

Experiment 204 made Kitty render placements part of the `roastty_render_state_t`
frame snapshot. Roastty now has the renderer-facing input for Kitty image
placement, but it still does not have the renderer-side image state that
upstream uses to turn those placements into prepared image records and z-layer
buckets.

Upstream's relevant slice is `vendor/ghostty/src/renderer/image.zig`:

- `State.images`
- `State.kitty_placements`
- `State.kitty_bg_end`
- `State.kitty_text_end`
- `State.kitty_virtual`
- `State.kittyRequiresUpdate`
- `State.kittyUpdate`
- `State.prepImage`
- `State.prepKittyImage`
- `Placement`
- `Id`
- the CPU-side portions of `Image`

Do not port GPU upload or draw behavior yet. The upstream `upload` and `draw`
methods depend on the concrete renderer API and texture types; Roastty does not
have the Metal renderer slice yet. This experiment ports the
renderer-independent foundation only: image identity, CPU pending image data,
replacement/unload state transitions, Kitty placement accumulation, z ordering,
and layer split indices.

This experiment also does not port upstream `kittyRequiresUpdate` as a terminal
dirty-check function. Roastty's input here is already an update-time
render-state snapshot, so the renderer image state should prepare from the
snapshot it is given. `kitty_virtual` in this experiment means "the current
prepared placement list contains virtual render placements," not "terminal
storage has virtual placement definitions that require a future frame rescan."

This experiment should consume the render-state Kitty placement snapshots from
Experiment 204 rather than re-reading terminal state. That keeps the frame
boundary coherent: terminal state is snapshotted once by render state, and
renderer image state prepares from that snapshot.

All public names must use Roastty naming.

## Changes

1. Add a renderer module.

   Add:

   ```text
   roastty/src/renderer/mod.rs
   roastty/src/renderer/image.rs
   ```

   Wire it from `roastty/src/lib.rs` with:

   ```rust
   mod renderer;
   ```

   Keep this module internal for this experiment. Do not add C ABI until there
   is a concrete app/renderer consumption need. The goal is to port and test the
   internal renderer image-state model first.

2. Add renderer image value types in `roastty/src/renderer/image.rs`.

   Port the renderer-independent shape from upstream with Roastty names:
   - `ImageState`
   - `ImageId`
   - `RendererImage`
   - `PendingImage`
   - `PixelFormat`
   - `Placement`

   Required behavior:
   - `ImageId::Kitty(u32)` sorts before non-Kitty ids for same-z tie-breaking,
     matching upstream `Id.zLessThan`.
   - Include an `Overlay` id variant only as an inert placeholder for the
     upstream shape; do not implement overlay rendering yet.
   - `PendingImage` owns `Vec<u8>` image bytes and records width, height, and
     pixel format.
   - Supported pixel formats for this slice are the Kitty decoded formats
     already represented by Roastty's Kitty image ABI: grayscale,
     grayscale-alpha, RGB, and RGBA. Do not add PNG here because PNG decode was
     already handled before image placement snapshots reach render state.

3. Port CPU-side image state transitions.

   Implement renderer-internal methods equivalent to the CPU portions of
   upstream `Image`:
   - `is_pending`
   - `is_unloading`
   - `mark_for_unload`
   - `mark_for_replace`
   - `pending_image`

   Do not implement GPU `ready`, texture upload, texture replacement, or draw
   states yet. Represent the missing GPU states explicitly enough that the next
   Metal experiment can extend them, but this experiment's tests must not depend
   on a GPU or platform permission.

4. Add Kitty update/prep from render state.

   Add a method on `ImageState`, for example:

   ```rust
   pub(crate) fn update_kitty_from_render_placements(
       &mut self,
       placements: &[KittyGraphicsRenderPlacementSnapshot],
   )
   ```

   This method should:
   - clear and rebuild the Kitty placement list every call;
   - mark existing Kitty images for unload when their image id is absent from
     the new placement snapshot;
   - copy image bytes into owned pending image records for new images;
   - keep exactly one image record per unique Kitty image id even when multiple
     render placements reference the same image in a single frame;
   - append one renderer placement for each render placement, including multiple
     placements that share the same image id;
   - leave unchanged images alone when image id, dimensions, format, and bytes
     are unchanged;
   - avoid self-induced replacement when the same unchanged image appears more
     than once in one update;
   - mark existing images for replacement when the same image id receives
     changed dimensions, format, or bytes;
   - set `kitty_virtual = true` if any prepared placement in the input snapshot
     is virtual;
   - append one renderer `Placement` per input render placement with destination
     grid x/y, z, pixel width/height, cell offsets, and source rect;
   - sort placements by z ascending and then `ImageId::z_less_than`;
   - compute `kitty_bg_end` and `kitty_text_end` using upstream's thresholds:
     `i32::MIN / 2` for the background split and `0` for the text split.

   Use the Experiment 204 render placement info fields directly. Do not
   recompute placement geometry in this module.

5. Add tests.

   Add Rust tests for the renderer image module covering:
   - image id z tie-breaking: Kitty ids sort before overlay for equal z, and
     Kitty ids sort numerically among themselves;
   - `PendingImage` byte ownership and length accounting;
   - update from pinned Kitty render placements produces pending images and
     renderer placements with the expected geometry;
   - virtual placements set `kitty_virtual`;
   - duplicate same-frame placements for one Kitty image id create one image
     record, multiple renderer placements, and no replacement churn;
   - z sorting and `kitty_bg_end` / `kitty_text_end` split indices match
     upstream thresholds;
   - repeated update with identical image bytes does not replace the existing
     pending image;
   - repeated update with changed image bytes or dimensions marks the existing
     image for replacement;
   - update that removes a previously-present Kitty image marks it for unload;
   - update consumes already-snapshotted render-state data and does not need a
     live terminal.

   Tests may construct `KittyGraphicsRenderPlacementSnapshot` values directly if
   that keeps the renderer module focused. If direct construction requires
   making snapshot fields `pub(crate)`, do that narrowly.

6. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/lib.rs roastty/src/renderer/mod.rs roastty/src/renderer/image.rs
   cargo test -p roastty renderer::image
   cargo test -p roastty render_state
   cargo test -p roastty kitty_graphics_render_placement_c_abi
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits. Accept formatter output as-is.

## Non-Negotiable Invariants

- Do not expose `ghostty_*` symbols or comments in the Roastty public ABI.
- Do not modify vendored Ghostty source.
- Do not add Metal, texture upload, shader, draw, Swift app, or C ABI behavior
  in this experiment.
- Do not re-read terminal state from renderer image state. Consume Experiment
  204's render-state Kitty placement snapshots.
- Do not duplicate Kitty placement geometry math in the renderer image module.
- Do not make tests depend on macOS GUI permissions, Screen Recording,
  Accessibility, Metal devices, or app runtime state.

## Pass Criteria

- Roastty has an internal renderer image-state module modeled on upstream
  `renderer/image.zig`'s renderer-independent CPU state.
- Kitty render placement snapshots from render state can be transformed into
  prepared renderer image records and sorted renderer placements.
- Image replacement/unload state transitions are covered by tests.
- Kitty z-layer split indices are computed and tested.
- Existing render-state and Kitty render placement tests continue to pass.
- Full verification passes, including the public no-`ghostty` gate.

## Failure Criteria

- The implementation adds GPU upload/draw behavior or Swift/app integration.
- The renderer image module reaches back into terminal state instead of using
  render-state placement snapshots.
- The implementation recomputes Kitty geometry instead of consuming Experiment
  204's render placement info.
- Image replacement or unload state is implicit and untestable.
- Existing render-state or Kitty render placement tests regress.

## Result

Not run yet.

## Conclusion

Pending.
