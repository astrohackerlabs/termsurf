# Experiment 190: Port Kitty Placement Storage Foundation

## Description

Experiment 189 made Kitty graphics query/transmit execution work against
`ImageStorage`, but display remains intentionally deferred because it needs
placement state. The next coherent slice is the storage-side placement model
from:

- `vendor/ghostty/src/terminal/kitty/graphics_storage.zig`

This experiment ports the placement storage foundation only. It should add the
data model and storage behavior needed by later display execution, deletion,
renderer integration, and C ABI work, without implementing those higher layers
yet.

This experiment is still internal to Roastty. It must not expose public C ABI,
render images, execute display commands, mutate terminal cursor state, or add
Unicode virtual placement rendering.

## Changes

1. Extend `ImageStorage` with placement state.
   - Add `next_internal_placement_id`, preserving upstream's default of `0`.
   - Add a placement map keyed by `(image_id, placement_id)`.
   - Keep the image map private.
   - Keep all placement types internal to Roastty.

2. Add placement identifiers.
   - Add a `PlacementKey` with:
     - `image_id`;
     - `placement_id`.
   - Add a placement ID tag equivalent to upstream's internal/external split:
     - `Internal(u32)` for implicit placement IDs generated from
       `placement_id = 0`;
     - `External(u32)` for caller-specified non-zero placement IDs.
   - A zero caller placement ID must allocate the next internal placement ID and
     increment with wrapping behavior.
   - A non-zero caller placement ID must use the external ID unchanged.

3. Add placement metadata.
   - Add a `Placement` struct with upstream-equivalent scalar fields:
     - `x_offset`;
     - `y_offset`;
     - `source_x`;
     - `source_y`;
     - `source_width`;
     - `source_height`;
     - `columns`;
     - `rows`;
     - `z`.
   - Add a placement location enum, but keep this experiment storage-only:
     - `Virtual`;
     - a non-virtual cell/pin placeholder that can be wired to Roastty's tracked
       page pins in a later experiment.
   - Do not store raw terminal pointers or `NonNull<Pin>` in this experiment
     unless the implementation can also prove ownership and untracking
     semantics. If tracked pin ownership is needed, stop and design the next
     experiment around terminal integration instead of quietly expanding scope.

4. Add placement insertion and lookup helpers.
   - Add `ImageStorage::add_placement(image_id, placement_id, placement)`.
   - The helper must fail if `image_id` is not currently stored.
   - Adding an external placement with the same `(image_id, external_id)` must
     replace the prior placement.
   - Adding repeated zero-ID placements for the same image must create distinct
     internal placement keys.
   - Add focused lookup/count helpers needed by tests, such as:
     - `placement_len()`;
     - `placement_by_key()`;
     - `placements_for_image()`.

5. Add placement geometry helpers that do not require terminal ownership.
   - Add a small internal metrics type, such as `CellMetrics`, containing:
     - terminal columns;
     - terminal rows;
     - width in pixels;
     - height in pixels.
   - Add `Placement::pixel_size(image, metrics)` matching upstream behavior:
     - use source rectangle dimensions when set, otherwise image dimensions;
     - if neither rows nor columns are set, return native/source pixel size;
     - if both rows and columns are set, return exact grid-cell pixel size;
     - if only columns are set, preserve aspect ratio and round computed height;
     - if only rows are set, preserve aspect ratio and round computed width.
     - if the relevant metric axis is zero, return zero for the computed axis
       instead of panicking or dividing by zero.
   - Add `Placement::grid_size(image, metrics)` matching upstream behavior:
     - if both rows and columns are set, return them directly;
     - otherwise divide pixel size plus offsets by cell size with ceiling;
     - return zero for an axis if the relevant cell size is zero.

6. Update image storage cleanup and eviction behavior for placements.
   - `set_limit(0)` must clear images, loading state, placements, byte count,
     and placement-owned state while preserving image limits.
   - `set_limit(0)` must mark storage dirty, keep `total_limit = 0`, reset
     `next_internal_placement_id` to `0`, and preserve the existing
     `image_limits`, matching Experiment 188's disable behavior while adding
     placement cleanup.
   - Eviction must account for placements:
     - unused images are evicted before used images;
     - when an image is evicted, all placements for that image are removed;
     - replacement accounting from Experiment 188 must remain correct.
   - Replacing an existing image with the same image ID must subtract the old
     image bytes before capacity checks, preserve placements for that image, and
     avoid evicting unrelated images merely because the replaced image is marked
     "used" by its placements.
   - If this eviction update proves too large, do not partially port it. Record
     Experiment 190 as Partial and design Experiment 191 around eviction
     integration.

7. Add focused storage tests.
   - Port or create equivalent Rust tests for:
     - placement storage defaults;
     - zero placement ID creates distinct internal placement keys;
     - non-zero placement ID creates an external key;
     - external placement replacement does not increase placement count;
     - adding a placement for a missing image fails without mutation;
     - `set_limit(0)` clears placements and images while preserving image
       limits, marking dirty, keeping the limit disabled, and resetting
       `next_internal_placement_id` to zero;
     - same-ID image replacement with an existing placement preserves that
       placement and keeps Experiment 188 replacement accounting correct;
     - eviction removes placements for evicted images;
     - eviction prefers unused images before images with placements;
     - lowering the limit preserves Experiment 188's exact-fit behavior;
     - pixel-size native/source rectangle behavior;
     - pixel-size both-axes grid behavior;
     - pixel-size columns-only and rows-only aspect-ratio rounding;
     - pixel-size zero-metric behavior;
     - grid-size ceiling behavior with offsets;
     - zero cell-size metrics return zero instead of panicking.

## Verification

Run:

```bash
cargo fmt -- roastty/src/terminal/kitty/mod.rs roastty/src/terminal/kitty/graphics_storage.rs roastty/src/terminal/kitty/graphics_image.rs roastty/src/terminal/kitty/graphics_exec.rs
cargo test -p roastty kitty_graphics_storage
cargo test -p roastty kitty_graphics_exec
cargo test -p roastty kitty_graphics_image
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when placement storage, internal/external placement IDs,
placement geometry helpers, and placement-aware eviction are implemented and
tested without adding display execution, renderer integration, terminal cursor
movement, Unicode virtual placement rendering, or public C ABI.

## Non-Negotiable Invariants

- Do not expose any `ghostty_*` ABI names.
- Do not add public Kitty graphics C ABI in this experiment.
- Do not execute display, delete, or animation commands in `graphics_exec.rs`.
- Do not render images.
- Do not mutate terminal cursor state.
- Do not add Unicode virtual placement rendering.
- Do not store terminal pin pointers unless this experiment also proves the
  correct ownership and untracking model. Prefer a storage-only location model
  here and defer tracked terminal pins to the next experiment.
- Do not break Experiment 188 replacement accounting or exact-fit eviction.
- Do not break Experiment 189 query/transmit execution behavior.
- Do not skip Codex design review. If the design review finds a real issue, fix
  it and re-review before committing this experiment design.
- Do not skip Codex result review after implementation.

## Result

**Result:** Pass

Implemented the internal Kitty placement storage foundation in
`roastty/src/terminal/kitty/graphics_storage.rs`.

The implementation now includes:

- tagged placement IDs with `Internal(u32)` and `External(u32)` variants;
- `PlacementKey` values keyed by image ID and tagged placement ID;
- storage-only `Placement` metadata matching upstream scalar fields;
- storage-only placement locations without terminal pin ownership;
- placement insertion that fails for missing images;
- internal placement ID allocation for zero caller IDs;
- external placement replacement for repeated non-zero placement IDs;
- placement lookup and per-image enumeration helpers;
- `CellMetrics`, `PixelSize`, and `GridSize` helpers;
- upstream-style placement pixel sizing and grid sizing;
- zero-metric geometry behavior that avoids division by zero;
- `set_limit(0)` placement cleanup and internal placement ID reset;
- placement-aware eviction that prefers unused images and removes placements for
  evicted images;
- same-ID replacement behavior that preserves placements and keeps Experiment
  188's byte accounting correct.

Codex result review found no blocking issues and approved the result as pass
ready.

Verification passed:

```bash
cargo fmt -- roastty/src/terminal/kitty/mod.rs roastty/src/terminal/kitty/graphics_storage.rs roastty/src/terminal/kitty/graphics_image.rs roastty/src/terminal/kitty/graphics_exec.rs
cargo test -p roastty kitty_graphics_storage
cargo test -p roastty kitty_graphics_exec
cargo test -p roastty kitty_graphics_image
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The focused storage suite passed with 24 tests. The full Roastty suite passed
with 1,970 Rust tests plus the C harness.

## Conclusion

Roastty now has the storage-side state needed for Kitty image placements without
yet touching terminal cursor movement, tracked pins, renderer integration, or
public ABI. The next experiment should connect display execution to this storage
model for lookup, validation, placement insertion, and response behavior while
still deferring renderer and C ABI exposure.
