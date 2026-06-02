# Experiment 209: Port Image Draw Command Contract

## Description

Experiment 208 proved that prepared RGBA image payloads can become live internal
Metal textures. The next renderer parity slice is the draw-side command shape
from upstream `vendor/ghostty/src/renderer/image.zig`.

Upstream image drawing does not hand a raw placement directly to the graphics
backend. For every ready placement, it builds exactly one
`GraphicsAPI.shaders.Image` vertex-parameter item:

- `grid_pos`;
- `cell_offset`;
- `source_rect`;
- `dest_size`.

It then submits a render-pass step with:

- one vertex buffer containing that image parameter;
- the ready texture;
- primitive type `triangle_strip`;
- vertex count `4`;
- instance count `1`.

Roastty already has the higher-level image state, placement buckets, upload
contract, and Metal texture upload backend. This experiment should port the draw
command contract that sits between image state and the eventual Metal
render-pass implementation. It should remain backend-agnostic and testable
without opening a window or creating command buffers.

All public names must use Roastty naming.

## Changes

1. Add image shader/draw command value types.

   Add a renderer-internal value layer that mirrors upstream
   `renderer/metal/shaders.zig`'s `Image` parameter and the fixed draw call used
   by `renderer/image.zig`. The exact file can follow the existing module shape,
   for example:
   - `roastty/src/renderer/shader.rs`; or
   - a focused section in `roastty/src/renderer/image.rs` if that remains
     clearer.

   Required internal types:

   ```rust
   #[repr(C)]
   pub(crate) struct ImageVertex {
       pub(crate) grid_pos: [f32; 2],
       pub(crate) cell_offset: [f32; 2],
       pub(crate) source_rect: [f32; 4],
       pub(crate) dest_size: [f32; 2],
   }

   pub(crate) enum PrimitiveType {
       TriangleStrip,
   }

   pub(crate) struct ImageDrawCall {
       pub(crate) vertex: ImageVertex,
       pub(crate) primitive: PrimitiveType,
       pub(crate) vertex_count: u32,
       pub(crate) instance_count: u32,
   }
   ```

   Use exact upstream values for image draws:
   - primitive: `TriangleStrip`;
   - vertex count: `4`;
   - instance count: `1`.

   Do not add a public C ABI for these types in this experiment.

   `ImageVertex` must have a stable GPU-safe layout because later Metal buffer
   code will upload it directly. Upstream uses an `extern struct` for this
   shader parameter, so Roastty must use `#[repr(C)]` and test the layout:
   - `std::mem::size_of::<ImageVertex>() == 40`;
   - `std::mem::align_of::<ImageVertex>() == 4`.

2. Convert placements to image draw calls.

   Add a deterministic conversion from `Placement` to `ImageDrawCall`:
   - `grid_pos = [placement.x as f32, placement.y as f32]`;
   - `cell_offset = [placement.cell_offset_x as f32, placement.cell_offset_y as f32]`;
   - `source_rect = [placement.source_x as f32, placement.source_y as f32, placement.source_width as f32, placement.source_height as f32]`;
   - `dest_size = [placement.width as f32, placement.height as f32]`;
   - fixed draw values from step 1.

   Keep the conversion internal and side-effect-free so later Metal buffer code
   can reuse it directly.

3. Update the draw backend contract.

   Change `ImageDrawBackend` so backends receive the prepared draw command,
   while still receiving the ready texture:

   ```rust
   fn draw_image(
       &mut self,
       texture: &Texture,
       placement: Placement,
       call: ImageDrawCall,
   ) -> Result<(), Self::Error>;
   ```

   Or, if the implementation reads better, wrap placement plus call in a small
   struct. The important invariant is that `ImageState::draw` owns the upstream
   placement-to-draw-call conversion, not each backend.

   Preserve existing draw semantics:
   - missing images increment `skipped_missing`;
   - not-ready images increment `skipped_not_ready`;
   - backend errors increment `failed` and do not stop later placements;
   - successful calls increment `succeeded`;
   - placement bucket selection stays unchanged.

4. Update tests.

   Extend existing renderer image tests to prove:
   - `ImageVertex` has the expected upstream-compatible size and alignment;
   - `Placement` converts to the expected `ImageVertex` field values;
   - every generated image draw call uses triangle-strip, vertex count `4`, and
     instance count `1`;
   - draw bucket tests still receive the expected placements and now also
     receive matching draw calls;
   - failed backend calls still continue to later placements;
   - missing and not-ready images still skip without creating draw calls.

   The tests should not use Metal devices. Experiment 208 already proved the
   upload-time Metal boundary; this experiment is the backend-agnostic draw
   command contract.

5. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/image.rs roastty/src/renderer/mod.rs roastty/src/renderer/shader.rs
   cargo test -p roastty renderer::image
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   git diff --check
   ```

   If no `renderer/shader.rs` file is created, omit it from the `cargo fmt` file
   list. `cargo fmt` is required for Rust edits; accept formatter output as-is.

## Non-Negotiable Invariants

- Do not expose public C ABI for image draw commands in this experiment.
- Do not open windows, create Metal command buffers, create render passes, or
  touch CAMetalLayer/IOSurface.
- Do not change image upload semantics from Experiments 206 and 208.
- Do not change placement bucket semantics.
- Do not modify vendored Ghostty source.
- Do not expose `ghostty_*` symbols or comments in the Roastty public ABI.

## Pass Criteria

- Roastty has an internal image draw command type matching upstream image shader
  input shape.
- `ImageState::draw` converts ready placements into draw calls before invoking
  the backend.
- Existing draw summary behavior is preserved.
- Tests prove the exact placement-to-draw-call mapping and fixed triangle-strip
  draw values.
- Full verification passes, including the public no-`ghostty` gate.

## Failure Criteria

- Backends still receive only raw placements and must each reconstruct the
  upstream draw command themselves.
- The experiment adds real Metal render-pass or command-buffer behavior.
- Placement bucket ordering, skipped-placement counts, or failed-draw behavior
  regress.
- The experiment exposes new public ABI before the internal renderer contract is
  stable.

## Result

**Result:** Pass

Experiment 209 added the backend-agnostic image draw command contract. The
implementation:

- added an internal `renderer::shader` module with `ImageVertex`,
  `PrimitiveType`, and `ImageDrawCall`;
- made `ImageVertex` `#[repr(C)]` and tested its upstream-compatible layout
  (`size_of == 40`, `align_of == 4`);
- added `Placement::to_image_draw_call()`, which maps placement geometry to the
  upstream image shader fields;
- changed `ImageDrawBackend` so image drawing receives the ready texture, raw
  placement, and prepared `ImageDrawCall`;
- kept `ImageState::draw` responsible for constructing draw commands so future
  backends do not each reconstruct the mapping;
- preserved draw summary behavior for missing, not-ready, failed, and successful
  placements.

Verification passed:

```bash
cargo fmt -- roastty/src/renderer/image.rs roastty/src/renderer/mod.rs roastty/src/renderer/shader.rs
cargo test -p roastty renderer::image
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Observed results:

- `cargo test -p roastty renderer::image`: 30 passed.
- `cargo test -p roastty`: 2127 library tests plus 1 ABI harness test passed.
- The public no-`ghostty` gate and `git diff --check` both exited 0.

Codex result review approved the experiment as Pass. Its only note was to ensure
the new `roastty/src/renderer/shader.rs` file is included in the result commit.

## Conclusion

Roastty's image renderer now has the draw-side command shape that upstream
Ghostty feeds into GPU buffers: every ready placement becomes one stable-layout
image vertex plus the fixed triangle-strip draw metadata. This creates a clean
boundary for the next Metal rendering slice.

The next experiment can port the Metal buffer wrapper or the minimal render-pass
step data needed to bind these image draw commands to Metal, while still
avoiding public ABI expansion until the internal renderer path is ready.
