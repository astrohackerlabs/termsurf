# Experiment 111: Mouse Hide While Typing Runtime

## Description

`RUNTIME-004F` remains the last explicit mouse runtime gap after Experiments
108-110. Pinned Ghostty stores `mouse-hide-while-typing` in surface derived
config, hides the mouse only when a pressed key event carries UTF-8 text, shows
the mouse again on mouse movement, mouse button, or scroll, and shows the mouse
immediately if config reload disables hiding while the mouse is hidden.

Roastty already parses and formats `mouse-hide-while-typing`, exposes
`ROASTTY_ACTION_MOUSE_VISIBILITY`, and the macOS app already maps that action to
`SurfaceView.setCursorVisibility`. The missing parity surface is the libroastty
runtime state and event ordering that emits those visibility actions.

This experiment will close only `RUNTIME-004F`. It will not claim broader macOS
GUI cursor visibility workflows, link hover shape behavior, renderer visual
effects, or other CFG-223 runtime rows.

## Changes

- Add `mouse_hide_while_typing` and a hidden/visible mouse runtime state to
  `Surface`, initialize it from app config, and refresh it on surface config
  update.
- Add small `hide_mouse` / `show_mouse` helpers equivalent to pinned Ghostty:
  - `hide_mouse` is idempotent and emits `ROASTTY_ACTION_MOUSE_VISIBILITY` with
    hidden for the surface target.
  - `show_mouse` is idempotent and emits `ROASTTY_ACTION_MOUSE_VISIBILITY` with
    visible for the surface target.
- In `Surface::key`, hide the mouse only when:
  - `mouse-hide-while-typing` is enabled;
  - the key action is press;
  - the mouse is currently visible;
  - the event has non-empty UTF-8 text.
- Preserve pinned Ghostty ordering around key handling: keybindings and VT KAM
  can consume input before the typing-hide branch, but unconsumed or
  performable-unperformed bindings still fall through to the hide branch before
  encoding. The implementation must put the hide decision in a shared
  post-binding/pre-encoding point instead of only the no-binding path.
- Show the mouse again before handling mouse position, mouse button, and mouse
  scroll events, matching pinned Ghostty's `mousePos`, `mouseButtonCallback`,
  and `scrollCallback` behavior.
- During surface config update, if hiding is disabled while the mouse is hidden,
  show it immediately.
- Extend the test action recorder or add a typed action helper so tests assert
  both `ROASTTY_ACTION_MOUSE_VISIBILITY` and its payload: `ROASTTY_MOUSE_HIDDEN`
  or `ROASTTY_MOUSE_VISIBLE`.
- Add focused runtime tests that use the action callback recorder to prove:
  - disabled config never emits mouse visibility for typed text;
  - enabled config hides once on a text key press and does not duplicate hidden
    actions on subsequent text presses while already hidden;
  - a text key with an unconsumed or performable-unperformed configured binding
    still emits hidden before the key is encoded;
  - key release does not hide the mouse;
  - press events without UTF-8 text do not hide the mouse;
  - mouse position, mouse button, and mouse scroll each show a hidden mouse
    again;
  - config update from enabled/hidden to disabled emits visible and changes
    subsequent text-key behavior on the existing surface.
- Update `config_runtime_inventory.py` so `RUNTIME-004F` becomes
  `Oracle complete` only if the focused runtime guard exists.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`, format the
  generated markdown, and update Issue 805 learnings.

## Verification

Pass criteria:

- Focused tests drive the public surface entry points: `roastty_surface_key`,
  `roastty_surface_mouse_pos`, `roastty_surface_mouse_button`, and
  `roastty_surface_mouse_scroll`.
- Tests prove the exact key-event gates: enabled config, key press, non-empty
  UTF-8 text, and idempotent hide behavior.
- Tests prove the Ghostty key-path placement by covering a text key that matches
  an unconsumed or performable-unperformed configured binding and still reaches
  hide-before-encode behavior.
- Tests assert the mouse visibility action payloads, not just the action tag.
- Tests prove the three mouse event families restore visibility from hidden:
  movement, button, and scroll.
- A config-update test proves disabling `mouse-hide-while-typing` on an existing
  hidden surface emits visible and prevents later text-key hides.
- Existing mouse runtime and key tests still pass.
- `RUNTIME-004F` is promoted to `Oracle complete`.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml mouse_hide_while_typing
cargo test --manifest-path roastty/Cargo.toml mouse_runtime
cargo test --manifest-path roastty/Cargo.toml surface_key_configured
cargo fmt --manifest-path roastty/Cargo.toml

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

inventory = Path("issues/0805-roastty-ghostty-parity/config-runtime-inventory.md").read_text()
matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()
cfg223 = next(row for row in matrix.splitlines() if row.startswith("| CFG-223 "))

rows = {}
for line in inventory.splitlines():
    if not line.startswith("| RUNTIME-"):
        continue
    cells = [cell.strip() for cell in line.strip("|").split("|")]
    rows[cells[0]] = cells

assert rows["RUNTIME-004F"][5] == "Oracle complete", rows["RUNTIME-004F"]
assert "| Gap " in cfg223
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/111-mouse-hide-while-typing-runtime.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Design Review

Fresh-context Codex adversarial reviewer `Epicurus` initially returned **CHANGES
REQUIRED**:

- **Required:** the design could pass while placing the hide decision only on
  the no-binding key path. Pinned Ghostty runs keybinding handling first, but
  unconsumed or performable-unperformed bindings still fall through to the
  hide-before-encode branch.
- **Optional:** the design did not require test recorder support for
  `ROASTTY_ACTION_MOUSE_VISIBILITY` payloads, so tests could assert only the
  action tag and miss hidden/visible inversions.

Fix:

- The design now requires the hide decision to live at a shared
  post-binding/pre-encoding point and requires a configured-binding fallthrough
  test.
- The design now requires extending the action recorder or adding a typed helper
  so tests assert `ROASTTY_MOUSE_HIDDEN` and `ROASTTY_MOUSE_VISIBLE` payloads.

Re-review verdict: **Approved**. The reviewer confirmed the prior required and
optional findings are resolved and found no remaining required design issues.

## Result

**Result:** Pass

Roastty now stores `mouse-hide-while-typing` as surface runtime state, tracks
whether the mouse is hidden, and emits idempotent
`ROASTTY_ACTION_MOUSE_VISIBILITY` actions through the same callback path the
macOS app consumes. The typed action union and test recorder now preserve the
mouse visibility payload, so tests assert hidden versus visible rather than only
the action tag.

The key path now uses a shared fallthrough helper: VT KAM still consumes before
typing-hide behavior, but unconsumed or performable-unperformed bindings fall
through to the Ghostty-equivalent hide-before-encode point. Mouse position,
mouse button, and mouse scroll restore visibility before continuing their normal
handling. Surface config update shows the mouse immediately when the option is
disabled while hidden.

Focused tests prove:

- disabled config emits no mouse visibility action for typed text;
- enabled config hides once for repeated text-key presses;
- key release and empty-text press events do not hide;
- mouse movement, button, and scroll each show a hidden mouse;
- disabling the option on an existing hidden surface emits visible and prevents
  later text-key hides;
- an unconsumed configured binding still performs its action, emits hidden, and
  encodes the key to the PTY.

`RUNTIME-004F` is promoted to `Oracle complete`. `CFG-223` remains `Gap` because
seven runtime/UI rows are still incomplete.

Verification passed:

```text
cargo test --manifest-path roastty/Cargo.toml mouse_hide_while_typing
# 6 passed

cargo test --manifest-path roastty/Cargo.toml mouse_runtime
# 5 passed

cargo test --manifest-path roastty/Cargo.toml surface_key_configured
# 11 passed

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=21 oracle_complete=13 closed=14 audit_covered=0 incomplete=7 gap=7 cfg223=Gap
```

## Conclusion

`mouse-hide-while-typing` now has a durable Tier 2 runtime guard through
`cargo test --manifest-path roastty/Cargo.toml mouse_hide_while_typing`. The
explicit mouse runtime subrows are now complete; the remaining CFG-223 gaps are
broader font, renderer, terminal, PTY/process, macOS app, notification/link, and
platform-classification runtime rows.

## Completion Review

Fresh-context Codex adversarial reviewer `Mill` returned **Approved** with no
required findings.

The reviewer independently verified:

- the result was still uncommitted before review;
- `cargo test --manifest-path roastty/Cargo.toml mouse_hide_while_typing`;
- `cargo test --manifest-path roastty/Cargo.toml mouse_runtime`;
- `cargo test --manifest-path roastty/Cargo.toml surface_key_configured`;
- `cargo fmt --manifest-path roastty/Cargo.toml --check`;
- Prettier check for the touched issue docs;
- `git diff --check`;
- runtime inventory assertions showing `RUNTIME-004F` is `Oracle complete` and
  `CFG-223` remains `Gap`.

The reviewer skipped normal `python3 -m py_compile` because it writes
`__pycache__`, and instead used a non-writing `ast.parse` syntax check.
