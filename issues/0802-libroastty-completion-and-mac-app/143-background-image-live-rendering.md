# Experiment 143: Phase H — background-image live rendering

## Description

Finish the live renderer half of `background-image`.

Experiment 142 added the missing config path surface, but it deliberately
stopped before loading or drawing the image. Upstream Ghostty's renderer keeps a
renderer-owned optional background image, reloads it when the configured path
changes, uploads pending image bytes before drawing, and draws the `bg_image`
shader before cell backgrounds/text. Roastty already has the config fields,
tested `BgImageVertex` packing, a production Metal `bg_image` shader, and live
Kitty image upload/draw plumbing. It does not yet have a background-image state
object, disk decode, a `bg_image` render-pass step, or live presentation wiring.

This experiment wires a faithful first live path:

- read the configured `background-image` path;
- decode PNG/JPEG bytes into an RGBA `PendingImage`;
- retain/reuse/replace/unload the renderer-owned image as config changes;
- upload it through the existing Metal image upload backend;
- draw the existing `bg_image` shader before cell backgrounds/text, with
  `background-image-opacity`, `background-image-position`,
  `background-image-fit`, and `background-image-repeat` packed into
  `BgImageVertex`.

The out-of-scope items are intentionally narrow: async file watching/reload,
runtime diagnostics UI, non-PNG/JPEG formats, custom shader composition, and
Swift app changes. The live app should start showing a configured background
image after this experiment, but automated UI A/B coverage for user-facing
config reload can be a later workstream-3 experiment.

## Changes

- `roastty/Cargo.toml`
  - Add a focused image decoding dependency, preferably
    `image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }`,
    unless implementation discovers a narrower crate that can faithfully decode
    both formats.
- `roastty/src/renderer/image.rs`
  - Add a renderer-owned background image state that mirrors upstream's
    pending/ready/replace/unload lifecycle using the existing
    `RendererImage<Texture>` machinery.
  - Add a small `BackgroundImageConfig` / equivalent value derived from
    `Config`: optional path, opacity, fit, position, and repeat.
  - Load required and optional paths from disk. Open/read/decode failures should
    match upstream's warning-and-skip behavior precisely: an initial failed load
    yields no drawable image, but a failed replacement after a previously ready
    image preserves the old drawable image; only a reset/no configured path
    marks the image for unload.
  - Decode PNG/JPEG to RGBA `PendingImage`; reject unsupported/unknown formats.
  - Map Roastty config enums to shader `BgImagePosition` / `BgImageFit` and
    build the packed `BgImageVertex`.
  - Add focused lifecycle tests for default none, load, unchanged reuse, changed
    path replace, failed replacement preserving the previous ready image, reset
    unload, optional missing path on an initially empty state, unsupported
    bytes, and enum/opacity packing.
- `roastty/src/renderer/metal/render_pass.rs`
  - Add a `draw_background_image` step that binds the `bg_image` pipeline, one
    `BgImageVertex` buffer, the uploaded image texture, and the uniform buffer.
  - Keep the vertex buffer alive until command encoding completes, following the
    lifetime fix used for Kitty image draw buffers.
- `roastty/src/renderer/metal/compositor.rs`
  - Extend the image-aware draw path so a ready background image replaces the
    `draw_background_color` step: draw `bg_image` after the clear and before
    cell backgrounds, and fall back to `draw_background_color` only when no
    ready background image exists. This is required because the `bg_image`
    shader composites the configured terminal background color itself; drawing
    both would double-compose opacity.
  - Preserve existing Kitty bucket order: below-background Kitty images remain
    before cell backgrounds, below-text Kitty images remain before text, and
    above-text Kitty images remain after text. Background image should be below
    cell backgrounds, consistent with upstream.
  - Add Metal readback tests proving a 1x1 decoded background image reaches the
    target, translucent terminal background / image opacity is not
    double-composited, and the configured repeat/fit/position values are passed
    through to the shader input.
- `roastty/src/renderer/frame_rebuild.rs` and
  `roastty/src/renderer/frame_renderer.rs`
  - Thread the background-image state through the prepared present path
    alongside the existing Kitty `ImageState<MetalTexture>`.
  - Ensure background-image config changes are observed each frame from the
    current `Config`.
- `roastty/src/lib.rs`
  - Add the renderer-owned background-image state to `SurfaceLiveRenderer`.
  - In `present_live`, update that state from the current app config before
    rendering, then present with both Kitty image state and background image
    state.
  - Add or extend Rust tests that drive the live frame renderer with a temporary
    image file and verify target readback changes when the config path is set
    and returns to background-only after reset/unload.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After the result, update Phase H notes to say background-image live
    load/upload/draw exists, while custom shader/link-highlight/debug overlay
    remain.

## Verification

- Format markdown:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/143-background-image-live-rendering.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Format Rust:
  - `cargo fmt`
- Run focused renderer/image tests:
  - `cargo test -p roastty background_image`
  - `cargo test -p roastty bg_image`
  - `cargo test -p roastty live_background_image`
- Run ABI harness to catch dependency/header regressions:
  - `cargo test -p roastty --test abi_harness`
- Run full Roastty Rust coverage:
  - `cargo test -p roastty -- --test-threads=1`
- Run hosted app coverage:
  - `cd roastty && macos/build.nu --action test`
- Run checks:
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/143-background-image-live-rendering.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = a configured PNG/JPEG `background-image` is decoded, retained,
uploaded, and drawn in the live Metal pass with opacity/position/fit/repeat
applied through the existing `bg_image` shader; path changes replace the image;
failed replacements preserve the previous ready image; reset/no-path unloads;
initial missing/unsupported files skip without failing presentation; the
background-image shader replaces, rather than stacks on top of, the background
color step; the focused tests, ABI harness, full Rust suite, hosted macOS suite,
and hygiene checks pass.

**Partial** = decode/state/upload works, but shader draw ordering or live
surface wiring needs a follow-up.

**Fail** = the existing image-state or Metal compositor abstractions cannot host
background images without a broader renderer redesign.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Volta`, fresh context.

**Verdict:** Approved after fixes.

**Findings and fixes:**

- **Required:** The initial design incorrectly said open/read/decode failures
  should leave no drawable background image. Volta pointed out upstream
  preserves a previously ready image on failed replacement and only unloads on
  reset/no path. Fixed by specifying that lifecycle and adding a
  failed-replacement test requirement.
- **Required:** The initial design said to draw `bg_image` after clear and
  before cell backgrounds but did not say it must replace
  `draw_background_color`. Volta pointed out upstream draws either background
  image or background color, because the `bg_image` shader composites the
  terminal background itself. Fixed by requiring background image to replace the
  background-color step when ready and adding a no-double-composition readback
  test requirement.
- **Optional:** The decoder dependency wording was too loose. Fixed by requiring
  `image` with `default-features = false` and only `png` / `jpeg`, unless a
  narrower crate is chosen.

Volta's re-review approved the corrected design with no remaining findings.

## Result

**Result:** Pass.

Implemented live background-image rendering:

- `roastty/Cargo.toml` now depends on `image` with `default-features = false`
  and only PNG/JPEG features enabled.
- `roastty/src/renderer/image.rs` now has `BackgroundImageState` and
  `BackgroundImageConfig`, reusing the existing `RendererImage` lifecycle for
  pending/ready/replace/unload. It decodes configured PNG/JPEG files to RGBA,
  preserves a previous ready image on failed replacement, skips initial
  missing/unsupported images without failing presentation, and unloads on
  reset/no-path.
- `roastty/src/renderer/metal/render_pass.rs` now has a `draw_background_image`
  step for the existing `bg_image` pipeline.
- `roastty/src/renderer/metal/compositor.rs` now uploads background images and
  draws the `bg_image` shader instead of `draw_background_color` when a ready
  background image exists, preserving the existing Kitty bucket ordering around
  cell backgrounds/text.
- `roastty/src/renderer/frame_rebuild.rs`,
  `roastty/src/renderer/frame_renderer.rs`, and `roastty/src/lib.rs` thread the
  background-image state through the live frame renderer and
  `SurfaceLiveRenderer` so each present observes the current `Config`.

Verification:

- `cargo fmt`
- `cargo test -p roastty background_image -- --test-threads=1` — 14 passed
- `cargo test -p roastty bg_image -- --test-threads=1` — 8 passed
- `cargo test -p roastty live_background_image -- --test-threads=1` — 1 passed
- `cargo test -p roastty --test abi_harness` — 1 passed; existing C harness
  enum-conversion warnings remain
- `cargo test -p roastty -- --test-threads=1` — 4775 unit tests, C ABI harness,
  and doc tests passed
- `cd roastty && macos/build.nu --action test` — 210 hosted macOS tests passed
- `cargo fmt --check`
- `git diff --check`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/143-background-image-live-rendering.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Conclusion

The live Metal path now supports configured background images end to end: config
path, PNG/JPEG decode, lifecycle, upload, and shader draw. The renderer matches
the two important upstream lifecycle/order details caught in design review:
failed replacements keep the previous ready image, and a ready background image
replaces the background-color pass rather than stacking on top of it.

Remaining Phase H renderer work is now custom-shader screen pass, link-highlight
cell mapping, and the optional debug overlay.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Noether`, fresh context.

**Verdict:** Approved.

**Findings and fixes:**

- **Optional:** Noether noted that the approved plan asked for unsupported-image
  coverage, while the initial result tests covered malformed PNG/JPEG decode
  failure and optional missing paths but not a valid unsupported format. Fixed
  by adding `background_image_initial_unsupported_format_skips_without_image`,
  which writes a tiny valid GIF and verifies an initially empty background-image
  state remains unloaded.

Noether found no required issues. After the optional fix, the focused
`background_image`, `bg_image`, and `live_background_image` filters,
`cargo fmt --check`, and `git diff --check` passed again.
