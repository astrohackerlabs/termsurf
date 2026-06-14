# Experiment 125: Renderer Control Runtime Split

## Description

`RUNTIME-008` still combines multiple renderer-visible config effects:
`window-vsync`, cursor blink/presentation timing, live renderer rebuild
behavior, opacity, blur, padding, cursor style, window padding color, custom
shaders, and other visual presentation effects. Recent inspection found that
Roastty already has focused runtime tests for a narrow renderer control slice:

- `window-vsync = false` selects the fallback present scheduler;
- `window-vsync = true` attempts the display-link path and falls back when the
  display link fails;
- active display-link present drivers receive display id updates;
- present drivers are stopped before surface drop;
- cursor blink ticks toggle focused surfaces, mark them dirty, and avoid
  toggling before the next blink deadline;
- terminal output resets cursor blink visibility with the Ghostty-style
  throttle, while non-output pump events do not reset it;
- focus loss stops cursor blink toggling, and focus gain resets the visible
  cursor and schedules the next blink;
- live renderer occlusion gates presentation and config updates request a live
  renderer rebuild only for surfaces that have a live view.

Pinned Ghostty's corresponding renderer control inputs live in `Config.zig`
(`window-vsync`, `cursor-style-blink`, and adjacent renderer/window visual
fields), `renderer/generic.zig`, where `window-vsync` feeds renderer config for
new surfaces, and `Surface.zig`, where the surface initializes cursor blinking
from config and sends renderer update work through surface runtime paths.

This experiment will split the already-proven renderer scheduler, cursor blink,
focus, occlusion, and live renderer rebuild control behavior out of
`RUNTIME-008`. It will not claim parity for visible opacity, blur, padding,
cursor shape/style rendering, window padding color, custom shader output, or a
full GUI visual renderer walkthrough; those remain in a follow-up renderer gap.

## Changes

- `issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`
  - Add a static guard that checks pinned Ghostty's `window-vsync`,
    `renderer/generic.zig` vsync renderer config consumption,
    `cursor-style-blink`, surface cursor blink initialization, and surface
    renderer update markers.
  - Check Roastty's present-driver `window_vsync` branch, cursor blink helpers,
    focus/occlusion/config-update live renderer paths, and existing runtime test
    names.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-008` into:
    - `RUNTIME-008A`: `Oracle complete` for renderer scheduler, cursor blink,
      focus, occlusion, and live renderer rebuild control runtime behavior.
    - `RUNTIME-008B`: `Gap` for visible opacity, blur, padding, cursor style
      shape/rendering, window padding color, custom shader output, and other
      renderer-visible effects.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the runtime inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate from the runtime inventory script so CFG-223 reflects the split.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after implementation with any
    durable finding.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml present_driver`
- `cargo test --manifest-path roastty/Cargo.toml live_cursor_blink`
- `cargo test --manifest-path roastty/Cargo.toml live_renderer_options`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- The matrix assertion inside
  `issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`
  verifies:
  - `RUNTIME-008A` is `Oracle complete`;
  - `RUNTIME-008A` evidence and guard command name `window-vsync`, cursor blink,
    focus, occlusion, live renderer rebuild behavior, and the static parity
    guard;
  - `RUNTIME-008B` remains `Gap`;
  - `RUNTIME-008B` still names visible opacity, blur, padding, cursor style
    shape/rendering, window padding color, custom shader output, and other
    renderer-visible effects;
  - CFG-223 remains `Gap` until all runtime/UI rows are closed.
- `prettier --check --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/125-renderer-control-runtime-split.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `cargo fmt --manifest-path roastty/Cargo.toml -- --check`
- `git diff --check`
- No generated `__pycache__` remains under the issue directory.

Fail criteria:

- The static guard cannot find the pinned Ghostty renderer control markers or
  the corresponding Roastty runtime/test markers.
- The split claims visible renderer parity for opacity, blur, padding, cursor
  shape/style rendering, window padding color, custom shader output, or other
  GUI-only visual effects.
- The test filters do not exercise Roastty's present-driver, cursor blink,
  focus/occlusion, and config-update live renderer control paths.
- CFG-223 is marked `Pass` while `RUNTIME-008B` or any other runtime/UI row
  remains a gap.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes required**.

The reviewer found three required issues:

- The planned static guard was too shallow for the `window-vsync` claim because
  pinned Ghostty consumes the option in `renderer/generic.zig`, and Ghostty's
  config notes scope runtime changes to new terminals.
- Verification omitted explicit formatter hygiene.
- The planned changes omitted `config-matrix.md` even though
  `config_runtime_inventory.py --matrix` writes it.

The design was fixed to include `vendor/ghostty/src/renderer/generic.zig` and
new-surface scoping in the upstream guard, add explicit `prettier --check` and
`cargo fmt --check` verification, list regenerated `config-matrix.md`, and name
`renderer_control_runtime_parity.py` as the matrix assertion location.

Re-review verdict: **Approved**.

The reviewer confirmed all required findings were resolved and reported no new
required findings.

## Result

**Result:** Pass

Added `renderer_control_runtime_parity.py` and split the renderer runtime
inventory so the already-proven renderer control slice is tracked separately
from the remaining visible-renderer gap. `RUNTIME-008A` is now `Oracle complete`
for `window-vsync` present scheduling, cursor blink timing, output reset
throttling, focus behavior, occlusion presentation gating, and live renderer
rebuild requests. `RUNTIME-008B` remains `Gap` for visible opacity, blur,
padding, cursor style shape/rendering, window padding color, custom shader
output, and other renderer-visible effects. CFG-223 remains `Gap`.

Implementation found one narrow runtime bug in the existing guard tests:
`set_font_size_points` requested a render even when the requested font size was
unchanged. That made ABI-only config updates dirty surfaces even when there was
no live view and no effective font-size change. The setter is now idempotent,
preserving real font-change reload behavior while keeping no-op config updates
quiet.

Verification passed:

- `cargo test --manifest-path roastty/Cargo.toml present_driver`
- `cargo test --manifest-path roastty/Cargo.toml live_cursor_blink`
- `cargo test --manifest-path roastty/Cargo.toml live_renderer_options`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_control_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- `prettier --check --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/125-renderer-control-runtime-split.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `cargo fmt --manifest-path roastty/Cargo.toml -- --check`
- `git diff --check`
- No generated `__pycache__` remained under the issue directory.

## Conclusion

The renderer parity gap is smaller than the broad `RUNTIME-008` row implied.
Roastty now has a durable guard for the non-visual renderer control layer:
present-driver scheduling from `window-vsync`, cursor blink timing and reset
behavior, focus/occlusion control, and live renderer rebuild requests. The
remaining renderer gap should focus on visible output: opacity, blur, padding,
cursor shape/style rendering, window padding color, custom shader output, and
GUI-visible renderer effects.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Verdict: **Approved**.

The reviewer reported no findings. It independently reran the `present_driver`,
`live_cursor_blink`, and `live_renderer_options` test filters, the
`renderer_control_runtime_parity.py` static guard, `prettier --check`,
`cargo fmt --check`, `git diff --check`, and the no-`__pycache__` check. It also
confirmed the result commit had not yet been made, `RUNTIME-008A` is
`Oracle complete`, `RUNTIME-008B` remains `Gap`, and CFG-223 remains `Gap` with
27 oracle-complete rows and 5 gap rows.
