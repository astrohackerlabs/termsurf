# Experiment 208: Port Metal Texture Upload Backend

## Description

Experiment 207 added the Metal image texture value layer: pixel formats,
resource option packing, texture usage, image texture option mapping, and the
RGBA-only bridge from renderer image upload payloads. The next coherent slice is
to port the real Metal texture object boundary from upstream
`vendor/ghostty/src/renderer/metal/Texture.zig` and connect it to Experiment
206's internal image upload backend contract.

Upstream's `Texture.zig` does four things:

- builds an `MTLTextureDescriptor`;
- applies pixel format, width, height, resource options, and usage;
- asks an `MTLDevice` for a new texture;
- writes optional initial data with
  `replaceRegion:mipmapLevel:withBytes:bytesPerRow:`.

Roastty should use the project's existing Rust Objective-C direction:
`objc2`/`objc2-metal`, not the older `rust-objc` crate and not untyped raw
message sends unless a specific API is missing from `objc2-metal`.

This experiment should add the smallest real Metal texture wrapper needed for
image uploads. It should not add render passes, command buffers, shaders,
IOSurface targets, Swift app integration, or public C ABI.

All public names must use Roastty naming.

## Changes

1. Add the Metal Objective-C dependency.

   Update `roastty/Cargo.toml` to add:

   ```toml
   objc2 = "0.6"
   objc2-metal = { version = "0.3", default-features = false, features = [
       "MTLDevice",
       "MTLPixelFormat",
       "MTLResource",
       "MTLTexture",
       "MTLTypes",
   ] }
   ```

   If implementation proves `MTLTextureDescriptor` is gated behind a more
   specific feature, add only that exact feature. Do not enable `objc2-metal`'s
   broad default feature set.

   `objc2_metal::MTLCreateSystemDefaultDevice()` also requires linking
   CoreGraphics. Use the narrowest solution that works:
   - prefer a direct, private link block in the Metal module:

     ```rust
     #[link(name = "CoreGraphics", kind = "framework")]
     unsafe extern "C" {}
     ```

   - only add `objc2-core-graphics` as a dependency if the direct link block
     fails or a real CoreGraphics API becomes necessary.

   The live smoke test must prove this link path works.

2. Extend the Metal value bridge only where needed.

   In `roastty/src/renderer/metal/api.rs` and
   `roastty/src/renderer/metal/texture.rs`, add conversion helpers from
   Roastty's internal value types to the corresponding `objc2-metal` types:
   - `MetalPixelFormat` -> `objc2_metal::MTLPixelFormat`;
   - `MetalResourceOptions` -> `objc2_metal::MTLResourceOptions`;
   - `MetalTextureUsage` -> `objc2_metal::MTLTextureUsage`.

   These conversions must be mechanical from the raw integer values already
   tested in Experiment 207. Add tests that prove the conversion raw values
   match the internal raw values.

3. Add a real Metal texture wrapper.

   In `roastty/src/renderer/metal/texture.rs`, add an internal wrapper, for
   example:

   ```rust
   pub(crate) struct MetalTexture {
       texture: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLTexture>>,
       width: usize,
       height: usize,
       bytes_per_pixel: usize,
   }
   ```

   Add `MetalTexture::new(device, options, width, height, data)` equivalent to
   upstream `Texture.init`:
   - create `MTLTextureDescriptor::new()`;
   - set pixel format, width, height, resource options, and usage;
   - ask the supplied `MTLDevice` for a texture;
   - compute and store bytes-per-pixel from `options.pixel_format`;
   - if `data` is present, require `data.len() == width * height * bpp`;
   - write initial data via `replaceRegion:mipmapLevel:withBytes:bytesPerRow:`;
   - return a testable error on invalid pixel format, byte-length mismatch, or
     Metal texture creation failure.

   Do not implement texture replacement/in-place region updates beyond the
   initial-data path unless the initial write naturally uses a small private
   helper.

4. Add a Metal image upload backend.

   Add an internal backend, for example:

   ```rust
   pub(crate) struct MetalImageUploadBackend<'a> {
       device: &'a objc2::runtime::ProtocolObject<dyn objc2_metal::MTLDevice>,
       storage_mode: MetalStorageMode,
       srgb: bool,
   }
   ```

   Implement Experiment 206's `ImageUploadBackend` for it:
   - accept only `PendingImage` values whose pixel format maps through
     `image_texture_format_for_upload_pixel_format`;
   - build image texture options with Experiment 207's helper;
   - create a `MetalTexture` with initial data;
   - return the texture as the backend texture type.

   Do not change `ImageState::upload` semantics. It already prepares upload
   payloads as RGBA before calling the backend.

5. Add automated tests.

   Add unit tests that do not require GUI permissions:
   - conversion helpers preserve raw values from internal Metal value types;
   - invalid pixel format fails before trying to create a texture;
   - byte-length mismatch fails before trying to create a texture;
   - a headless live Metal smoke test:
     - call `objc2_metal::MTLCreateSystemDefaultDevice()`;
     - if no device is returned, fail the test because Roastty is macOS-only and
       the real texture wrapper cannot be considered implemented without a Metal
       device;
     - create a 1x1 RGBA texture with four bytes of data;
     - assert width, height, and bytes-per-pixel are recorded correctly;
     - read the texture back with `getBytes:bytesPerRow:fromRegion:mipmapLevel:`
       and assert the four bytes match the original input, proving the initial
       `replaceRegion:mipmapLevel:withBytes:bytesPerRow:` write used the correct
       region, pointer, and bytes-per-row;
   - `MetalImageUploadBackend` can upload a prepared RGBA `PendingImage`;
   - `ImageState<MetalTexture>::upload` can move a pending image to ready using
     the real Metal backend.

   Keep the live test small and headless. Do not open a window, create a
   CAMetalLayer, touch IOSurface, use Screen Recording, or require
   Accessibility.

6. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/metal/api.rs roastty/src/renderer/metal/texture.rs
   cargo test -p roastty renderer::metal
   cargo test -p roastty renderer::image
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits. Accept formatter output as-is.

## Non-Negotiable Invariants

- Do not expose `ghostty_*` symbols or comments in the Roastty public ABI.
- Do not modify vendored Ghostty source.
- Use `objc2`/`objc2-metal`, matching the project's current Objective-C
  direction. Do not use `rust-objc`.
- Do not add render passes, command buffers, shader pipelines, IOSurface,
  CAMetalLayer, Swift app, or C ABI behavior.
- Do not add broad `objc2-metal` default features unless a reviewed build
  blocker proves that narrower features are insufficient.
- Do not change Experiment 206's image upload/draw state-machine semantics.
- Keep all new Metal code internal to Roastty.

## Pass Criteria

- Roastty has a real internal Metal texture wrapper for image upload textures.
- The wrapper creates an `MTLTextureDescriptor`, creates an `MTLTexture`, and
  writes initial RGBA data.
- Metal value conversions to `objc2-metal` preserve the raw values tested in
  Experiment 207.
- `MetalImageUploadBackend` implements the Experiment 206 upload backend
  contract for prepared RGBA images.
- A headless live Metal test proves a 1x1 texture can be created and initialized
  without GUI permissions.
- Existing renderer image tests and the full Roastty suite continue to pass.
- Full verification passes, including the public no-`ghostty` gate.

## Failure Criteria

- The experiment only adds dependency declarations or compile-only wrappers
  without proving live texture creation.
- The experiment opens a window, creates a render target, or touches
  CAMetalLayer/IOSurface.
- The wrapper accepts malformed byte lengths or invalid pixel formats.
- The implementation changes `ImageState::upload` semantics to fit Metal instead
  of implementing the backend contract.
- Existing renderer image or Metal value tests regress.

## Result

Not run yet.

## Conclusion

Pending.
