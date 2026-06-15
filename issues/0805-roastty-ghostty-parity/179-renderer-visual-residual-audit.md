# Experiment 179: Renderer Visual Residual Audit

## Description

`RUNTIME-008B2B2B2B2B` is now the only renderer-family CFG-223 gap, but its
remaining claim is intentionally vague: "broader GUI/pixel parity." Experiments
125, 133, 134, 144, 148, 151, 154, 163, 164, 177, and 178 already split out the
concrete renderer control, renderer option, cursor, padding, opacity, shader,
and focused live screenshot slices found in pinned Ghostty's renderer and macOS
host paths.

This experiment will audit the renderer residual bucket against pinned Ghostty
renderer, shader, config, surface, and macOS host sources and either:

- close the residual renderer row if every config-driven renderer-visible effect
  in the pinned Ghostty renderer/macOS-render-host paths is already represented
  by an oracle-complete inventory row or by a different still-open non-renderer
  row; or
- replace the vague residual row with one or more concrete follow-up rows for
  any renderer-visible config behavior still lacking proof.

The scope is renderer-visible output only. Broad font output parity remains in
`RUNTIME-007B2B2B2B2`, broader live macOS app walkthrough/titlebar/split
behavior remains in `RUNTIME-011B2B`, and native notification/link/bell GUI
effects remain in `RUNTIME-012B2B2B2B2B3`.

## Changes

- `issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
  - Add a static guard that reads pinned Ghostty renderer, shader, surface,
    config, and macOS host sources.
  - Enumerate known config-driven renderer-visible effects and map each to an
    oracle-complete row or to one of the remaining non-renderer gap rows.
  - Assert the mapping covers renderer control and rebuild scheduling,
    renderer-sourced visual knobs, background opacity/cell opacity,
    window-padding layout and padding pixels, cursor render data and live cursor
    pixels, macOS glass and non-glass opacity host behavior, and custom/cursor
    shader pixel readback.
  - Assert font-renderer output, macOS walkthrough/titlebar/split workflows, and
    notification/link/bell UI effects are not counted as closure for the
    renderer residual row.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - If the audit finds no uncovered renderer-visible config behavior, mark
    `RUNTIME-008B2B2B2B2B` as `Oracle complete` with evidence from the new guard
    and explain that remaining CFG-223 gaps are font, macOS walkthrough, and
    notification/link/bell GUI gaps.
  - If the audit finds a real uncovered renderer-visible behavior, split the
    residual row into concrete rows instead of closing it.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 counts from the inventory script.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning recording whether the broad renderer residual row was closed
    or split and why.

No Roastty source code should change in this experiment. If the audit finds a
concrete renderer-visible parity bug, record it as a concrete remaining row and
leave implementation for the next experiment.

## Verification

Pass criteria:

- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- Existing CFG-223 guard set:
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/window_padding_layout_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/cursor_renderer_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/cursor_priority_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_glass_visual_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/non_glass_opacity_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/custom_shader_output_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/metal_cursor_pixel_runtime_parity.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_window_padding_pixel_runtime.py`
  - `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_gui_cursor_pixel_runtime.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/*.py`
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/179-renderer-visual-residual-audit.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `git diff --check`

The experiment passes only if the renderer residual row is no longer vague:
either the guard proves all pinned Ghostty config-driven renderer-visible fields
are covered by completed rows or intentionally owned by a different remaining
gap, or the inventory records the exact uncovered renderer-visible behavior that
remains. CFG-223 may still remain a gap because the font, macOS walkthrough, and
notification/link/bell GUI rows are outside this experiment.

## Design Review

Fresh-context adversarial design review initially returned **Changes required**:

- the verification listed
  `issues/0805-roastty-ghostty-parity/macos_non_glass_opacity_runtime_parity.py`,
  but the actual guard is
  `issues/0805-roastty-ghostty-parity/non_glass_opacity_runtime_parity.py`.

The verification command was corrected.

Re-review returned **Approved** with no new required findings.
