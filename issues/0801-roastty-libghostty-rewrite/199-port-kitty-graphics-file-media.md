# Experiment 199: Port Kitty Graphics File Media

## Description

Experiment 198 made Kitty graphics media permissions configurable at the
terminal boundary, but Roastty still rejects every non-direct medium in
`LoadingImage::init(...)` with `ImageLoadError::UnsupportedMedium`.

This experiment ports the next coherent upstream slice: regular-file and
temporary-file Kitty image loading. These two media types share the same
filesystem path validation and read path in upstream Ghostty, while shared
memory uses a distinct `shm_open`/`mmap` implementation. Keep shared-memory
loading for a later experiment.

Use upstream source as the behavior reference:

- `vendor/ghostty/src/terminal/kitty/graphics_image.zig`
- `vendor/ghostty/src/terminal/kitty/graphics_command.zig`
- `vendor/ghostty/src/os/TempDir.zig`
- `vendor/ghostty/src/os/file.zig`

## Changes

1. Extend Kitty image-load errors in
   `roastty/src/terminal/kitty/graphics_image.rs`.

   Add explicit variants for temporary-file validation failures if they are
   useful for unit tests:
   - temporary file not in a temp directory;
   - temporary file not named with `tty-graphics-protocol`.

   The public Kitty response mapping in `graphics_exec.rs` may still encode
   these as `EINVAL: invalid data` unless upstream exposes a more specific
   protocol-facing string. Preserve the existing response shape for unrelated
   errors.

2. Implement regular-file loading for `TransmissionMedium::File`.

   Behavior:
   - reject the medium with `UnsupportedMedium` unless `limits.file` is enabled;
   - reject non-direct PNG with `UnsupportedMedium` before path resolution or
     file reading, because Roastty does not yet provide a PNG decode hook and
     upstream short-circuits this case before loading external bytes;
   - reject paths containing an interior NUL byte before calling path APIs;
   - canonicalize the path before opening it;
   - reject paths that appear unsafe, matching upstream's rough filter:
     `/proc/`, `/sys/`, and `/dev/` except `/dev/shm/`;
   - open the path read-only;
   - reject non-regular files;
   - apply `transmission.offset` with a seek before reading;
   - read at most `transmission.size` bytes when `size > 0`, otherwise at most
     `MAX_IMAGE_SIZE`;
   - return `InvalidData` on open/stat/seek/read/path errors;
   - preserve the regular file after loading.

   Keep all byte buffers owned by Roastty after loading; the caller may delete
   or mutate the source file without changing the stored image data.

3. Implement temporary-file loading for `TransmissionMedium::TemporaryFile`.

   Behavior:
   - reject the medium with `UnsupportedMedium` unless `limits.temporary_file`
     is enabled;
   - reject non-direct PNG with `UnsupportedMedium` before path resolution,
     temp-file validation, file reading, or deletion;
   - perform the same NUL, canonicalization, unsafe-path, regular-file, offset,
     and size handling as regular-file loading;
   - require the canonical path to be inside a temp directory accepted by
     upstream's `isPathInTempDir` logic:
     - `/tmp`;
     - `/dev/shm`;
     - the process temp directory from `std::env::temp_dir()`;
     - the canonical form of that temp directory, to handle macOS symlinks such
       as `/tmp -> /private/tmp`;
   - require the path to contain `tty-graphics-protocol`;
   - install the cleanup point immediately after the temp-dir and name checks
     pass, matching upstream's `defer unlink` placement;
   - delete the temporary file after it passes temp-dir/name validation, even if
     a later open/stat/seek/read or completion check fails;
   - do not delete a blocked-by-limits file, a non-direct PNG rejected before
     path handling, or a temporary file rejected before temp-dir/name
     validation.

4. Keep shared-memory and PNG out of scope.

   `TransmissionMedium::SharedMemory` must continue to return
   `UnsupportedMedium` even when `limits.shared_memory` is true. PNG decoding is
   still deferred. Direct PNG behavior remains unchanged: bytes may be accepted
   initially and completion fails with `UnsupportedFormat`. Non-direct PNG must
   be rejected up front with `UnsupportedMedium` before path access or temp-file
   deletion, matching upstream when the PNG decoder hook is absent.

5. Wire the terminal option flags from Experiment 198 into execution tests.

   The loader already receives active-screen `image_limits`. Add terminal-stream
   tests proving that enabling/disabling
   `ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_FILE` and
   `ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_TEMP_FILE` gates actual
   file-backed Kitty image transmission.

6. Add tests.

   In `roastty/src/terminal/kitty/graphics_image.rs`, port or adapt upstream
   tests for:
   - regular file blocked by limits;
   - regular file allowed by limits;
   - regular file remains after load;
   - temporary file blocked by limits and remains on disk;
   - temporary file allowed by limits;
   - temporary file is deleted after successful load;
   - temporary file with wrong name is rejected and remains on disk;
   - temporary file outside an accepted temp directory is rejected;
   - valid temporary path/name with invalid image bytes is deleted after the
     loader reaches the cleanup point, even though completion fails;
   - non-direct PNG file/temp-file input is rejected before reading and does not
     delete a temporary file;
   - offset and size read only the requested byte range;
   - non-regular paths are rejected;
   - paths containing interior NUL bytes are rejected;
   - unsafe paths are rejected without reading.

   In `roastty/src/terminal/terminal.rs` or `roastty/src/lib.rs`, add
   integration coverage that goes through the terminal parser and storage path:
   - file medium is rejected by default;
   - enabling the file medium through the terminal option allows a file-backed
     image to be stored;
   - enabling temporary-file medium allows a temp-file-backed image to be stored
     and deletes the source file;
   - disabling either flag rejects that medium again without affecting direct
     image loading.

7. Preserve formatting and review rules.

   Run:

   ```bash
   cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/kitty/graphics_image.rs roastty/src/terminal/kitty/graphics_exec.rs
   prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/199-port-kitty-graphics-file-media.md
   ```

## Verification

Run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/terminal/terminal.rs roastty/src/terminal/kitty/graphics_image.rs roastty/src/terminal/kitty/graphics_exec.rs
cargo test -p roastty kitty_graphics_image
cargo test -p roastty terminal_stream_kitty_graphics
cargo test -p roastty kitty_graphics_terminal_options_c_abi
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

The experiment passes when:

- regular-file Kitty image loading works when `limits.file` is enabled;
- temporary-file Kitty image loading works when `limits.temporary_file` is
  enabled;
- both media remain rejected when their limits are disabled;
- temporary files are deleted only after passing temporary-file validation and a
  cleanup point is installed;
- blocked temporary files, non-direct PNG temp files, and temp files rejected
  before temp-dir/name validation remain on disk;
- valid temporary files that pass temp-dir/name validation are deleted even when
  later loading or completion fails;
- offset/size handling matches upstream behavior;
- non-regular, unsafe, malformed, or NUL-containing paths are rejected;
- shared-memory loading remains unsupported;
- PNG decoding remains unsupported;
- direct Kitty graphics behavior is unchanged;
- Codex approves the experiment design before implementation and approves the
  result before the experiment is recorded.

## Non-Negotiable Invariants

- Do not implement shared-memory loading.
- Do not decode PNG.
- Do not render images.
- Do not add Metal or any platform renderer.
- Do not weaken Experiment 198's terminal option persistence or reset behavior.
- Do not allow file/temp-file media unless the corresponding terminal option is
  enabled.
- Do not delete temporary files that were rejected before the cleanup point.
- Do not expose any `ghostty_*` ABI names.
- Do not skip Codex design review or Codex result review.
