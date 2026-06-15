# Experiment 151: macOS glass visual runtime

## Description

`RUNTIME-008B2B2B` still groups several renderer-visible gaps together:
background blur, real compositor opacity, GUI cursor pixels, custom shader
output, broader GUI/pixel parity, and screenshot-level padding proof.

This experiment isolates the macOS glass blur/opacity slice. In pinned Ghostty,
`background-blur = macos-glass*` is implemented in the copied macOS host
`TerminalViewContainer.swift`, where the config-derived glass style, background
color, background opacity, corner radius, inactive tint overlay, and safe-area
top inset are applied to `NSGlassEffectView`. Roastty carries the same host code
with expected product/type renames.

The experiment will prove that the copied Roastty host preserves the pinned
Ghostty macOS glass runtime behavior at the source level. It will not claim
renderer cursor pixels, custom shader output, screenshot-level padding pixels,
or broad GUI pixel parity.

## Changes

- Add a focused static parity guard:
  - `issues/0805-roastty-ghostty-parity/macos_glass_visual_runtime_parity.py`
  - Compare
    `vendor/ghostty/macos/Sources/Features/Terminal/TerminalViewContainer.swift`
    with `roastty/macos/Sources/Features/Terminal/TerminalViewContainer.swift`
    after the expected Ghostty-to-Roastty renames.
  - Assert that both sources contain the glass runtime markers that matter for
    this slice: `NSGlassEffectView`, `backgroundBlur`, `macosGlassRegular`,
    `macosGlassClear`, `backgroundOpacity`, `withAlphaComponent`,
    `cornerRadius`, `updateGlassTintOverlay`, and safe-area top-inset handling.
- Update `config_runtime_inventory.py` to split `RUNTIME-008B2B2B` into:
  - an Oracle complete macOS glass blur/opacity host row owned by this
    experiment;
  - a remaining renderer-visible visual gap row for GUI cursor pixels, custom
    shader output, broader GUI/pixel parity, and screenshot-level padding proof.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`.
- Update existing runtime parity guards and `terminal_runtime_residual_audit.py`
  for the new CFG-223 row counts and remaining gap id.
- Update Issue 805 learnings with the proven macOS glass host finding after the
  result is known.

## Verification

Pass criteria:

- The static glass parity guard passes:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_glass_visual_runtime_parity.py
```

- The runtime inventory generator reports one additional Oracle complete row and
  the same total number of unresolved CFG-223 gaps unless this experiment
  discovers a real fixable discrepancy:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

- All runtime parity guards still pass:

```bash
for guard in issues/0805-roastty-ghostty-parity/*_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$guard" || exit 1
done
```

- The terminal residual audit still passes:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py
```

- Markdown and diff hygiene pass:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/151-macos-glass-visual-runtime.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Adversarial subagent `019ec9f4-6556-7893-883d-d7a4a5e75113` reviewed the design
with fresh context and returned `VERDICT: APPROVED`.

Findings: none.

The reviewer independently confirmed that the README links Experiment 151 as
`Designed`, the experiment has the required design sections, the Ghostty and
Roastty `TerminalViewContainer.swift` files normalize to no diff after expected
renames, the scope is limited to source-level macOS glass host parity, and the
verification covers the static guard, inventory regeneration, residual audit,
runtime parity guards, and diff hygiene.

## Result

**Result:** Pass

Implemented the static macOS glass visual runtime parity guard and split the
renderer-visible runtime inventory:

- `RUNTIME-008B2B2B1`: **Oracle complete** for copied macOS glass background
  blur and opacity host behavior.
- `RUNTIME-008B2B2B2`: **Gap** for the remaining renderer-visible visual work:
  non-glass compositor opacity, GUI cursor pixels, custom shader output, broader
  GUI/pixel parity, and screenshot-level padding pixel proof.

The new guard proves that pinned Ghostty's
`vendor/ghostty/macos/Sources/Features/Terminal/TerminalViewContainer.swift` and
Roastty's `roastty/macos/Sources/Features/Terminal/TerminalViewContainer.swift`
are identical after expected Ghostty-to-Roastty renames. It also asserts the
glass runtime markers that matter for this slice: `NSGlassEffectView`,
`macosGlassRegular`, `macosGlassClear`, `backgroundOpacity`,
`withAlphaComponent`, corner radius, inactive tint overlay, and safe-area
top-inset handling.

Verification passed:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_glass_visual_runtime_parity.py
```

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

Output:

```text
runtime_rows=59
oracle_complete=53
closed=55
audit_covered=0
incomplete=4
gap=4
cfg223=Gap
```

```bash
for guard in issues/0805-roastty-ghostty-parity/*_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$guard" || exit 1
done
```

The full runtime parity loop passed.

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py
```

Output:

```text
terminal_runtime_residual_audit=pass
```

```bash
python3 -m py_compile issues/0805-roastty-ghostty-parity/macos_glass_visual_runtime_parity.py
```

## Conclusion

Roastty's copied macOS `TerminalViewContainer` preserves pinned Ghostty's
`macos-glass*` blur and background-opacity host behavior at the source level.
This closes that deterministic copied-host slice without overclaiming GUI pixel
parity.

CFG-223 remains open with four unresolved runtime gaps: remaining font renderer
output effects, remaining renderer-visible visual effects, macOS app workflow/UI
effects, and notification/link/bell presentation flows.

## Completion Review

Adversarial subagent `019ec9f9-b417-76e0-9638-830214c9910b` reviewed the
completed experiment with fresh context.

Initial verdict: `CHANGES REQUIRED`.

Required finding:

- `README.md` still said macOS glass blur/opacity remained in the old
  `RUNTIME-008B2B2B` bucket even though this experiment split it into
  `RUNTIME-008B2B2B1` and the remaining `RUNTIME-008B2B2B2` gap.

Fix:

- Updated the stale Experiment 148 learning to state that, after Experiment 151,
  only non-glass compositor opacity, GUI cursor pixels, custom shader output,
  broader GUI/pixel parity, and screenshot-level padding pixel proof remain in
  `RUNTIME-008B2B2B2`.

Re-review verdict: `APPROVED`.

The reviewer independently verified the macOS glass guard, regenerated runtime
inventory counts, terminal runtime residual audit, full runtime parity guard
loop, and `git diff --check`.
