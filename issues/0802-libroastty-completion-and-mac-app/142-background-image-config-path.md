# Experiment 142: Phase H — background-image config path

## Description

Add the missing `background-image` config path surface.

Experiment 141 proved live Kitty image presentation, but it intentionally left
background images out. Before the renderer can load and present a background
image faithfully, `Config` must first carry the upstream `background-image`
value itself. Roastty currently has the ancillary fields
`background-image-opacity`, `background-image-position`, `background-image-fit`,
and `background-image-repeat`, plus a tested Metal `bg_image` shader. It does
not yet parse, format, expand, clone, or expose the `background-image` path.

Upstream Ghostty's pinned `Config.zig` defines:

- `background-image: ?Path = null`
- `background-image-opacity: f32 = 1.0`
- `background-image-position: BackgroundImagePosition = .center`
- `background-image-fit: BackgroundImageFit = .contain`
- `background-image-repeat: bool = false`

This experiment wires only the missing path field. Background image file
decoding, image-state replacement/unload, Metal upload, and live drawing stay
for the next renderer experiment.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config::background_image: Option<ConfigFilePath>`.
  - Parse `background-image` with the same single optional path semantics as
    upstream `?Path` and existing single-path fields such as `bell-audio-path`:
    missing value is a diagnostic, raw empty resets to `None`, `?path` marks the
    path optional, quoted `"?path"` remains a required path whose literal value
    starts with `?`, parsed-empty paths reset/ignore according to the local
    `ConfigFilePath` contract.
  - Format `background-image` in upstream order before
    `background-image-opacity`.
  - Expand relative/background image paths from file/CLI/default config bases
    exactly like other `ConfigFilePath` fields.
  - Add focused parser, formatter, reset, diagnostic, clone/equality, and path
    expansion tests.
- `roastty/src/lib.rs`
  - Add cached background-image path storage to `ConfigHandle`, matching
    `cached_bell_audio_path`.
  - Rebuild that cache when config state is loaded or updated.
  - Extend `roastty_config_get(..., "background-image", ...)` to return a
    `RoasttyConfigPath` and `false` when unset/invalid handles are supplied.
  - Add Rust C-ABI tests for default unset, CLI required/optional/reset,
    file-clone pointer stability, and invalid-handle behavior.
- `roastty/tests/abi_harness.c`
  - Add a C harness assertion that `background-image` can be read as
    `roastty_config_path_s`, preserving the header/link conformance gate.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After the result, note that background-image config path support exists but
    live load/upload/draw remains Phase H work.

Out of scope:

- Reading image files from disk.
- PNG/JPEG decoding for background images.
- Background image state on `SurfaceLiveRenderer`.
- Uploading a background image into `MetalTexture`.
- Drawing the `bg_image` shader from the live compositor.
- Swift app changes. The copied app does not currently expose background-image
  behavior directly; this is a library/config prerequisite.

## Verification

- Format markdown:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/142-background-image-config-path.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Format Rust:
  - `cargo fmt`
- Run focused config tests:
  - `cargo test -p roastty background_image`
  - `cargo test -p roastty config_get_background_image`
- Run ABI harness:
  - `cargo test -p roastty --test abi_harness`
- Run full Roastty Rust coverage:
  - `cargo test -p roastty -- --test-threads=1`
- Run hosted app coverage to confirm copied-app config linking still builds:
  - `cd roastty && macos/build.nu --action test`
- Run checks:
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/142-background-image-config-path.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = `background-image` exists as a faithful optional `ConfigFilePath`,
parses/formats/resets/diagnoses/expands like upstream-shaped path fields,
survives config cloning/loading, is exposed through `roastty_config_get` as a
`RoasttyConfigPath`, the C ABI harness compiles against that getter, and the
focused plus full Rust/macOS gates pass.

**Partial** = parser/formatter support lands, but C ABI exposure or path
expansion needs a separate follow-up.

**Fail** = the existing config path abstractions cannot model upstream
`background-image: ?Path` without a broader config loader redesign.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Russell`, fresh context.

**Verdict:** Approved.

**Findings:** No required, optional, or nit findings.
