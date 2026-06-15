# Experiment 181: Background image renderer runtime

## Description

`RUNTIME-008B2B2B2B2B` still groups several renderer-visible effects: background
image rendering/options, `window-colorspace`, `alpha-blending`, and
`scroll-to-bottom.output`. The background image slice is concrete and already
has identifiable upstream anchors in pinned Ghostty's renderer:

- `DerivedConfig` copies `background-image`;
- the derived renderer config stores `background-image-opacity`,
  `background-image-position`, `background-image-fit`, and
  `background-image-repeat`;
- the renderer prepares a background image buffer from those options;
- the Metal draw path renders the background image pass before cells;
- config changes reload or repack the image state.

This experiment will split out only the deterministic background image renderer
runtime slice. It will not claim `window-colorspace`, `alpha-blending`, or
`scroll-to-bottom.output`.

## Changes

- Add `issues/0805-roastty-ghostty-parity/background_image_runtime_parity.py`.
  The guard will statically compare pinned Ghostty background-image anchors with
  Roastty's config-to-renderer image path:
  - Ghostty derived config fields and option copies in
    `vendor/ghostty/src/renderer/generic.zig`;
  - Ghostty image preparation, config-change reload/repack, and draw-pass
    markers in `generic.zig`;
  - Roastty `BackgroundImageConfig::from_config`,
    `BackgroundImageState::update_from_config`, `BackgroundImageConfig::vertex`,
    Metal background-image render-pass draw, live frame renderer wiring, and
    existing focused tests.
- Update `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py` to
  split a new Oracle-complete row for background image renderer runtime:
  `RUNTIME-008B2B2B2B2B2`.
- Narrow the existing `RUNTIME-008B2B2B2B2B` residual row so it continues to
  track only `window-colorspace`, `alpha-blending`, and
  `scroll-to-bottom.output`.
- Regenerate `config-runtime-inventory.md` and the CFG-223 line in
  `config-matrix.md`.
- Update the Issue 805 README learnings and experiment index with the result.

If inspection shows a real implementation gap in the background image path, fix
that gap inside this same narrow slice before promoting the row. Do not promote
background image behavior on static evidence alone if the renderer/runtime tests
do not prove path loading, option packing, draw pass output, and reset/unload
behavior.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml background_image -- --test-threads=1`
  passes and covers parser/formatter helpers, image load/upload/replace/reset,
  vertex option packing, render-pass output, and live frame rendering/unload.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/background_image_runtime_parity.py`
  passes and fails if the upstream or Roastty anchors for this slice disappear.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
  passes with background-image removed from the residual and colorspace,
  alpha-blending, and scroll-to-bottom still present.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  regenerates the inventory/matrix without drift.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/background_image_runtime_parity.py issues/0805-roastty-ghostty-parity/config_runtime_inventory.py issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
  passes.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes if any Rust
  files are edited.
- `prettier --check issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/181-background-image-renderer-runtime.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
  passes after formatting.
- `git diff --check` passes.

Failure criteria:

- Any guard can pass while Roastty no longer sources the background image path
  or image options from config, no longer draws the Metal background-image pass,
  or no longer resets/unloads the background image when config removes it.
- The experiment promotes `window-colorspace`, `alpha-blending`, or
  `scroll-to-bottom.output`.

## Design Review

Fresh-context Codex adversarial review:

- Initial verdict: **Changes required**.
- Required finding: the verification allowed Rust implementation fixes but did
  not require a Rust formatting check.
- Fix: added `cargo fmt --manifest-path roastty/Cargo.toml --check` as a pass
  criterion when Rust files are edited.
- Re-review verdict: **Approved**. The reviewer confirmed the formatting check
  and review record resolved the finding with no new Required findings.
