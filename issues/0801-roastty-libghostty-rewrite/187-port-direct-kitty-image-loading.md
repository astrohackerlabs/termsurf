# Experiment 187: Port Direct Kitty Image Loading

## Description

Experiment 186 added the Kitty graphics command parser and response encoder. The
next coherent Kitty graphics foundation is direct image loading from:

- `vendor/ghostty/src/terminal/kitty/graphics_image.zig`

This experiment ports the image-loading data model and direct-medium loading
path only. It should prove that parsed transmit commands can become validated
`Image` values without involving terminal execution, image storage, placement
tracking, file paths, shared memory, PNG decoding, rendering, or public C ABI.

The direct-medium path is the right next slice because it is used before storage
or rendering can do useful work. It also carries important safety behavior:
maximum image dimensions, maximum image byte size, zlib decompression limits,
and exact data-length validation against width, height, and bytes-per-pixel.

## Changes

1. Add an internal Kitty image module.
   - Add `roastty/src/terminal/kitty/graphics_image.rs`.
   - Add `pub(crate) mod graphics_image;` from
     `roastty/src/terminal/kitty/mod.rs`.
   - Keep all types internal to Roastty. Do not add public C ABI.

2. Add the command helpers needed by image loading.
   - Update `roastty/src/terminal/kitty/graphics_command.rs` with
     upstream-equivalent helpers:
     - `Command::transmission()`;
     - `Command::display()`;
     - a bytes-per-pixel helper for non-PNG `TransmissionFormat` values.
   - The image module must use these helpers instead of duplicating command
     variant matching.
   - The bytes-per-pixel helper must reject or avoid `Png` because PNG direct
     loading is intentionally deferred in this experiment.

3. Port the foundational image data model.
   - Add `Image`.
   - Add `LoadingImage`.
   - Add `LoadingImageLimits`.
   - Add image-loading errors matching the upstream behavior needed by this
     slice:
     - invalid data;
     - decompression failure;
     - dimensions required;
     - dimensions too large;
     - unsupported medium;
     - unsupported format;
     - out of memory.
   - Preserve upstream constants:
     - maximum dimension: `10000`;
     - maximum image size: `400 * 1024 * 1024`.
   - Preserve `Image::implicit_id` and `transmit_time` fields in a Rust shape
     suitable for later storage/eviction experiments.

4. Port direct-medium `LoadingImage` behavior.
   - Initialize image metadata from `Command::transmission()`.
   - Preserve `Command::display()` and `Command::quiet` on the loading image for
     later execution/storage experiments.
   - For direct medium, append the command payload immediately.
   - For file, temporary-file, and shared-memory media, return unsupported in
     this experiment unless the medium is rejected earlier by limits. Do not
     implement path reads, temp-file validation, `shm_open`, or `mmap`.
   - `LoadingImage::add_data` must support chunked direct image data, including
     a zero-byte first chunk.
   - `LoadingImage::complete` must:
     - decompress zlib data when requested;
     - leave uncompressed data unchanged;
     - reject zero dimensions;
     - reject dimensions above `10000`;
     - reject image data whose final byte length does not equal
       `width * height * bytes_per_pixel`;
     - transfer data ownership into `Image`;
     - set compression to `None` after successful decompression.

5. Add minimal zlib support.
   - Prefer `flate2` for zlib decompression. If a dependency is added, update
     `roastty/Cargo.toml` and `Cargo.lock` in this experiment.
   - If a different crate or local implementation is chosen, explain the reason
     in the result.
   - The decompressor must enforce the same `400MB` maximum output limit before
     allowing unbounded allocation.
   - Do not use an unbounded `read_to_end`-style path. Decompression must grow
     output through bounded/fallible allocation.
   - Do not add PNG decoding in this experiment. PNG direct loading should
     return unsupported format until a later PNG-specific experiment wires the
     image decoder.

6. Port the direct-loading tests from upstream.
   - Port or create equivalent Rust tests for:
     - image load with invalid RGB data is allowed at init time;
     - image too wide;
     - image too tall;
     - RGB zlib-compressed direct load;
     - RGB uncompressed direct load;
     - RGB zlib-compressed direct chunked load;
     - RGB zlib-compressed direct chunked load with zero initial chunk;
     - direct medium is always allowed even when limits disallow file/temp/shm.
   - Use the upstream fixture files in
     `vendor/ghostty/src/terminal/kitty/testdata/` with `include_bytes!` or an
     equivalent deterministic fixture path. Do not depend on network or runtime
     filesystem setup.
   - Add explicit tests for:
     - final byte-length mismatch returning invalid data;
     - missing dimensions returning dimensions required;
     - malformed zlib payload returning decompression failure;
     - PNG direct command returning unsupported format in this experiment;
     - non-direct media remaining unsupported/deferred.

## Verification

Run:

```bash
cargo fmt -- roastty/src/terminal/kitty/mod.rs roastty/src/terminal/kitty/graphics_command.rs roastty/src/terminal/kitty/graphics_image.rs
cargo test -p roastty kitty_graphics_image
cargo test -p roastty kitty_graphics_command
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when direct Kitty graphics image loading matches the
upstream direct-medium behavior under ported tests, the command parser tests
still pass, and no file/temp/shm/PNG/storage/execution/rendering behavior is
added by accident.

## Non-Negotiable Invariants

- Do not expose any `ghostty_*` ABI names.
- Do not add public Kitty graphics C ABI in this experiment.
- Do not mutate terminal state from image loading.
- Do not add image storage, placement tracking, rendering, Unicode virtual
  placement, or terminal APC execution.
- Do not implement file, temporary-file, or shared-memory reads.
- Do not implement PNG decoding.
- Do not silently skip zlib verification; if zlib support cannot be added
  cleanly, record the experiment as Partial and design the next experiment
  around the decompression blocker.
- Do not skip Codex design review. If the design review finds a real issue, fix
  it and re-review before committing this experiment design.
- Do not skip Codex result review after implementation.
