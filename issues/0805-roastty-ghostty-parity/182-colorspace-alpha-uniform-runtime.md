# Experiment 182: Colorspace and alpha uniform runtime

## Description

After Experiment 181, `RUNTIME-008B2B2B2B2B` contains only `window-colorspace`,
`alpha-blending`, and `scroll-to-bottom.output`. The colorspace and
alpha-blending slice is narrower than scroll-to-bottom because pinned Ghostty
maps both options to the same Metal uniform bool group:

- `window-colorspace = display-p3` sets `use_display_p3`;
- `alpha-blending = linear` sets `use_linear_blending`;
- `alpha-blending = linear-corrected` sets both `use_linear_blending` and
  `use_linear_correction`.

Roastty already has equivalent Metal uniform fields and focused tests around
`MetalUniforms::from_config`, `MetalUniforms::new`, and `update_color_config`.
This experiment will split out only deterministic colorspace/alpha uniform
propagation and shader-branch parity. It will not claim
`scroll-to-bottom.output`.

## Changes

- Add `issues/0805-roastty-ghostty-parity/color_uniform_runtime_parity.py`. The
  guard will compare pinned Ghostty anchors with Roastty's implementation:
  - Ghostty `DerivedConfig` copies `window-colorspace` and `alpha-blending`;
  - Ghostty initial renderer uniforms and `changeConfig` set `use_display_p3`,
    `use_linear_blending`, and `use_linear_correction`;
  - Ghostty Metal shaders consume those bools in color conversion and blending
    branches;
  - Roastty `Config`, `MetalUniforms::from_config`, `MetalUniforms::new`,
    `update_color_config`, `MetalUniformBools`, shader source, and existing
    tests preserve the same mapping.
- Update `config_runtime_inventory.py` to add a new Oracle-complete row
  `RUNTIME-008B2B2B2B2B3` for colorspace/alpha uniform runtime behavior.
- Narrow `RUNTIME-008B2B2B2B2B` to track only `scroll-to-bottom.output`.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`.
- Update affected CFG-223 count assertions in existing guards from 79/82 to the
  new generated counts and use `scroll-to-bottom.output` as the remaining
  renderer residual sentinel.
- Update the Issue 805 README learning and experiment index with the result.

If inspection finds that Roastty only parses or formats these options but does
not source them into active Metal uniforms, fix that implementation gap inside
this experiment before promoting the row.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml update_color_config -- --test-threads=1`
  passes and proves all three relevant bool combinations.
- `cargo test --manifest-path roastty/Cargo.toml uniforms_new -- --test-threads=1`
  passes and proves constructor-time color bool initialization for both Display
  P3 / linear-corrected and sRGB / native.
- `cargo test --manifest-path roastty/Cargo.toml uniforms_from_config_sources_config_values -- --test-threads=1`
  passes and proves parsed config reaches initial uniforms.
- `cargo test --manifest-path roastty/Cargo.toml metal_uniform_layout_matches_standard_shader_struct -- --test-threads=1`
  passes and proves the bool layout matches the shader struct.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/color_uniform_runtime_parity.py`
  passes and fails if upstream or Roastty anchors for this slice disappear.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
  passes with colorspace/alpha removed from the residual and
  `scroll-to-bottom.output` still present.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  regenerates the inventory/matrix without drift.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/color_uniform_runtime_parity.py issues/0805-roastty-ghostty-parity/config_runtime_inventory.py issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
  passes.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes if any Rust
  files are edited.
- `prettier --check issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/182-colorspace-alpha-uniform-runtime.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
  passes after formatting.
- `git diff --check` passes.

Failure criteria:

- Any guard can pass while Roastty no longer sources `window-colorspace` or
  `alpha-blending` into Metal uniforms.
- The experiment promotes `scroll-to-bottom.output`.

## Design Review

Fresh-context Codex adversarial review:

- Verdict: **Approved**.
- Findings: none. The reviewer confirmed the scope stays on colorspace/alpha
  uniform behavior, leaves `scroll-to-bottom.output` unpromoted, matches pinned
  Ghostty's bool mapping, and includes concrete verification plus hygiene
  checks.

## Result

**Result:** Pass

Experiment 182 split `window-colorspace` and `alpha-blending` Metal uniform
behavior out of `RUNTIME-008B2B2B2B2B` and into the new Oracle-complete
`RUNTIME-008B2B2B2B2B3` row.

Implementation notes:

- Added `color_uniform_runtime_parity.py`, which checks pinned Ghostty's config,
  renderer, and Metal shader bool anchors against Roastty's config fields,
  uniform construction/update helpers, Metal shader source, focused tests,
  generated inventory row, and CFG-223 counts.
- Updated `config_runtime_inventory.py` so the residual renderer row now keeps
  only `scroll-to-bottom.output`.
- Regenerated `config-runtime-inventory.md` and `config-matrix.md`; CFG-223 now
  reports 87 runtime rows, 80 Oracle-complete rows, 83 closed rows, 4 incomplete
  rows, and 4 runtime gaps.
- Updated affected guards to use `scroll-to-bottom.output` as the remaining
  concrete renderer residual sentinel.

Verification:

- `cargo test --manifest-path roastty/Cargo.toml update_color_config -- --test-threads=1`
  — 1 passed.
- `cargo test --manifest-path roastty/Cargo.toml uniforms_new -- --test-threads=1`
  — 3 passed.
- `cargo test --manifest-path roastty/Cargo.toml uniforms_from_config_sources_config_values -- --test-threads=1`
  — 1 passed.
- `cargo test --manifest-path roastty/Cargo.toml metal_uniform_layout_matches_standard_shader_struct -- --test-threads=1`
  — 1 passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/color_uniform_runtime_parity.py`
  — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
  — passed.
- Additional affected guards passed: `background_image_runtime_parity.py`,
  `custom_shader_animation_runtime_parity.py`, and
  `metal_cursor_pixel_runtime_parity.py`.

## Conclusion

Colorspace and alpha-blending Metal uniform parity is no longer part of the
renderer residual. The remaining `RUNTIME-008B2B2B2B2B` work is now explicitly
limited to `scroll-to-bottom.output`.

## Completion Review

Fresh-context Codex adversarial result review:

- Initial verdict: **Changes required**.
- Required finding: previous completed rows still said colorspace/alpha behavior
  remained in `RUNTIME-008B2B2B2B2B`.
- Fix: updated those missing-evidence strings to say only
  `scroll-to-bottom.output` remains in the residual, with background image and
  color uniform behavior tracked by their new completed rows.
- Re-review verdict: **Approved**. The reviewer confirmed the stale wording was
  removed from the source and generated inventory, `__pycache__` was absent, and
  `git diff --check` passed.
