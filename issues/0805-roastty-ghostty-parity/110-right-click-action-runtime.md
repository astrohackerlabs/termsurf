# Experiment 110: Right Click Action Runtime

## Description

`RUNTIME-004G` remains a mouse runtime gap. Pinned Ghostty handles right-button
press according to `right-click-action` after terminal mouse reporting has had
the first chance to consume the event:

- `ignore` does nothing and consumes the right click.
- `paste` clears any active selection and starts a standard clipboard paste
  request.
- `copy` copies the active selection to the standard clipboard, clears the
  selection, and consumes the right click.
- `copy-or-paste` copies and clears when a selection exists; otherwise it starts
  a standard clipboard paste request.
- `context-menu` may select the clicked word for menu actions, then returns
  unconsumed so the app runtime can show the context menu. If an existing
  selection contains the clicked point, it preserves that selection.
  Link-specific context-menu selection remains tracked by `RUNTIME-012`.

Roastty already parses and formats `right-click-action`, has selection,
clipboard copy, paste request, and mouse geometry helpers, but the right-button
surface path currently does not apply this config. This experiment will wire and
test the runtime behavior for the five pinned variants.

This experiment will close only `RUNTIME-004G`. It will not claim
`mouse-hide-while-typing`, link-specific context menu behavior, broader
notification/link behavior, or macOS menu UI parity beyond the surface return
value that allows the app menu to open.

## Changes

- Add `right_click_action` runtime state to `Surface`, initialize it from app
  config, and refresh it on surface config update.
- Add right-button press handling in `Surface::mouse_button` after terminal
  mouse reporting has declined to consume the event, matching pinned Ghostty's
  ordering.
- Preserve pinned mouse-reporting behavior before right-click action handling:
  reporting clears active selection, resets selection gesture state, dispatches
  the terminal mouse report, and returns consumed without
  copy/paste/context-menu side effects.
- Implement variant behavior:
  - `ignore`: consume the right-button press without clipboard or selection
    changes.
  - `paste`: clear active selection, then start a standard clipboard paste
    request.
  - `copy`: if a selection exists, copy it to the standard clipboard; clear any
    active selection either way; consume the event.
  - `copy-or-paste`: copy and clear when a selection exists; otherwise start a
    standard clipboard paste request.
  - `context-menu`: if the clicked point is inside the current selection,
    preserve it; otherwise select the clicked word. Return unconsumed so the app
    can show its context menu.
- Prefer existing helpers for copy, paste, word selection, active selection, and
  mouse geometry. Add small helpers only where needed to preserve the pinned
  right-click ordering and return semantics.
- Add focused runtime tests that drive `roastty_surface_mouse_button` with
  `ROASTTY_MOUSE_BUTTON_RIGHT`.
- Update `config_runtime_inventory.py` so `RUNTIME-004G` becomes
  `Oracle complete` only if the variant and reporting-order guards exist.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`, format the
  generated markdown, and update Issue 805 learnings.

## Verification

Pass criteria:

- Tests prove `right-click-action = ignore` consumes the press and starts no
  clipboard request.
- Tests prove `right-click-action = paste` clears selection and starts a
  standard clipboard paste request.
- Tests prove `right-click-action = copy` copies an existing selection to the
  standard clipboard, clears the selection, and does not paste.
- Tests prove `right-click-action = copy` with no active selection consumes the
  press, performs no clipboard write, starts no paste request, and leaves
  selection clear.
- Tests prove `right-click-action = copy-or-paste` copies and clears when a
  selection exists, and otherwise starts a standard clipboard paste request.
- Tests prove `right-click-action = context-menu` returns unconsumed, selects
  the clicked word when no containing selection exists, and preserves an
  existing selection that contains the clicked point.
- A reporting-mode test proves terminal mouse reporting consumes a right press
  before right-click action handling, clears a preexisting selection, resets
  nonzero selection gesture state, and has no clipboard request or context-menu
  selection side effect.
- A config-update test proves an existing surface changes behavior after
  `right-click-action` is updated.
- Existing mouse runtime tests still pass.
- `RUNTIME-004G` is promoted to `Oracle complete`, while `RUNTIME-004F` remains
  `Gap`.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml right_click_action
cargo test --manifest-path roastty/Cargo.toml mouse_runtime
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

assert rows["RUNTIME-004G"][5] == "Oracle complete", rows["RUNTIME-004G"]
assert rows["RUNTIME-004F"][5] == "Gap", rows["RUNTIME-004F"]
assert "| Gap " in cfg223
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/110-right-click-action-runtime.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Design Review

Fresh-context Codex adversarial reviewer `Helmholtz` initially returned
**CHANGES REQUIRED**:

- **Required:** the reporting-mode test did not prove pinned Ghostty's
  selection-clearing and selection-gesture reset semantics before mouse
  reporting consumes the right press.
- **Required:** the `copy` variant tests omitted the no-selection branch, where
  pinned Ghostty consumes the event, performs no copy, and still leaves
  selection clear.
- **Required:** the design planned surface config update support but did not
  require a runtime update test for an existing surface.
- **Optional:** the design mentioned clicked link selection for `context-menu`,
  but this experiment's planned tests only cover word selection and preserved
  containing selections.

Fix:

- The pass criteria now require a reporting-mode test with preexisting selection
  and nonzero gesture state, proving selection clear, gesture reset, mouse
  report consumption, and no right-click action side effects.
- The pass criteria now require the `copy` no-selection branch.
- The pass criteria now require a config-update test for an existing surface.
- The design now explicitly excludes link-specific context-menu selection from
  `RUNTIME-004G` and leaves it under `RUNTIME-012`.

Re-review verdict: **Approved**. The reviewer confirmed the prior findings are
resolved and found no remaining required design issues.

## Result

**Result:** Pass

Roastty now stores `right-click-action` as surface runtime state, initializes it
from config, refreshes it on surface config update, and applies it on
right-button press only after terminal mouse reporting has had the first chance
to consume the event.

Focused tests now prove:

- `ignore` consumes the right-button press without clipboard activity.
- `paste` clears selection and requests standard clipboard paste.
- `copy` writes an existing selection to the standard clipboard, clears the
  selection, and does not paste.
- `copy` with no selection consumes the press without copy or paste activity.
- `copy-or-paste` copies and clears when a selection exists, then pastes from
  the standard clipboard when no selection exists.
- `context-menu` returns unconsumed, selects the clicked word, and preserves an
  existing selection when the clicked point is inside it.
- reporting-mode right clicks clear selection, reset nonzero selection gesture
  state, dispatch the mouse report, and skip right-click action side effects.
- existing surfaces change behavior when `right-click-action` is reloaded.

`RUNTIME-004G` is promoted to `Oracle complete`. `RUNTIME-004F` remains `Gap`,
and `CFG-223` remains `Gap` because eight runtime/UI rows are still incomplete.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml right_click_action
cargo test --manifest-path roastty/Cargo.toml mouse_runtime
cargo fmt --manifest-path roastty/Cargo.toml

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/110-right-click-action-runtime.md

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

assert rows["RUNTIME-004G"][5] == "Oracle complete", rows["RUNTIME-004G"]
assert rows["RUNTIME-004F"][5] == "Gap", rows["RUNTIME-004F"]
assert "| Gap " in cfg223
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/110-right-click-action-runtime.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Conclusion

Right-click action parity for the non-link surface runtime path is now covered
by focused tests and the generated CFG-223 runtime inventory. The next CFG-223
mouse gap is `RUNTIME-004F` (`mouse-hide-while-typing`), while link-specific
context menu behavior remains in `RUNTIME-012`.

## Completion Review

Fresh-context Codex adversarial reviewer `Dewey` returned **Approved** with no
findings.

The reviewer independently checked:

- the result remained uncommitted before review;
- the working-tree diff was limited to the expected six files;
- `cargo test --manifest-path roastty/Cargo.toml right_click_action`;
- `cargo test --manifest-path roastty/Cargo.toml mouse_runtime`;
- `cargo fmt --manifest-path roastty/Cargo.toml -- --check`;
- Prettier check for the touched issue docs;
- `git diff --check`;
- the runtime inventory counts and assertions for `RUNTIME-004G`,
  `RUNTIME-004F`, and `CFG-223`.

The reviewer skipped normal `python3 -m py_compile` because it writes
`__pycache__`, and instead used a non-writing source compile check.
