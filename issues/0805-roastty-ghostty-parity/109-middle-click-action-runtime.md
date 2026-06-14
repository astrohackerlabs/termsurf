# Experiment 109: Middle Click Action Runtime

## Description

`RUNTIME-004H` remains a mouse runtime gap after Experiment 108. Pinned Ghostty
handles middle-button press according to `middle-click-action`:

- `ignore` does nothing.
- `primary-paste` starts a paste clipboard request. The source clipboard follows
  `copy-on-select`: `copy-on-select = clipboard` reads the standard clipboard,
  while `true` and `false` prefer the selection clipboard and fall back to the
  standard clipboard when the runtime does not support a selection clipboard.

Roastty parses and formats `middle-click-action`, and it already has clipboard
paste request plumbing for explicit paste actions. The missing work is wiring
the middle-button press path to this config and proving the clipboard-selection
decision at runtime.

This experiment will close only `RUNTIME-004H`. It will not implement or claim
`right-click-action` (`RUNTIME-004G`) or `mouse-hide-while-typing`
(`RUNTIME-004F`).

## Changes

- Add `middle_click_action` runtime state to `Surface`, initialize it from app
  config, and refresh it through surface config updates.
- Add a middle-button press branch in `Surface::mouse_button` equivalent to
  pinned Ghostty's `Surface.zig` middle-click handling:
  - run it only after the normal mouse-reporting path declines to consume the
    event, because pinned Ghostty returns from reporting before reaching the
    middle-click paste branch;
  - `MiddleClickAction::Ignore` does not start a clipboard request and leaves
    normal mouse reporting behavior unchanged.
  - `MiddleClickAction::PrimaryPaste` starts a paste request on middle-button
    press.
  - If `copy-on-select = clipboard`, request the standard clipboard.
  - If `copy-on-select` is `true` or `false`, request the selection clipboard
    when the runtime supports it; otherwise request the standard clipboard.
- Reuse `paste_from_clipboard` so paste protection, bracketed paste encoding,
  and clipboard completion behavior stay shared with existing paste actions.
- Add focused pty/runtime tests for:
  - default `primary-paste` with selection clipboard support requests the
    selection clipboard;
  - `copy-on-select = false` with selection clipboard support also requests the
    selection clipboard;
  - default `primary-paste` without selection clipboard support falls back to
    the standard clipboard;
  - `copy-on-select = clipboard` requests the standard clipboard even when the
    selection clipboard is supported;
  - `middle-click-action = ignore` starts no clipboard request;
  - terminal mouse-reporting mode preserves mouse reporting and starts no
    middle-click clipboard request;
  - runtime config update changes an existing surface from `ignore` to
    `primary-paste`.
- Update `config_runtime_inventory.py` so `RUNTIME-004H` becomes
  `Oracle complete` only if the new runtime guards exist.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`, format the
  generated markdown, and update Issue 805 learnings.

## Verification

Pass criteria:

- Focused tests prove every `middle-click-action` variant and the
  `copy-on-select` clipboard-source decision described above.
- The tests drive `roastty_surface_mouse_button` with
  `ROASTTY_MOUSE_BUTTON_MIDDLE` so they cover the surface mouse path, not only
  parser/config helpers.
- A focused reporting-mode test proves middle-click paste does not bypass
  terminal mouse reporting.
- Existing mouse runtime tests still pass.
- `RUNTIME-004H` is promoted to `Oracle complete`, while `RUNTIME-004F` and
  `RUNTIME-004G` remain `Gap`.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml middle_click_action
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

assert rows["RUNTIME-004H"][5] == "Oracle complete", rows["RUNTIME-004H"]
for row_id in ["RUNTIME-004F", "RUNTIME-004G"]:
    assert rows[row_id][5] == "Gap", rows[row_id]
assert "| Gap " in cfg223
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/109-middle-click-action-runtime.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Design Review

Fresh-context Codex adversarial reviewer `Sagan` initially returned **CHANGES
REQUIRED**:

- **Required:** the design did not pin Ghostty's mouse-reporting interaction.
  Pinned Ghostty returns from mouse reporting before reaching the middle-click
  paste branch, so Roastty must not start a middle-click paste request when the
  terminal consumes the middle press as a mouse report.
- **Required:** the concrete tests omitted the non-obvious
  `copy-on-select = false` case, which should still prefer the selection
  clipboard when that clipboard is supported.

Fix:

- The plan now states that middle-click action handling runs only after normal
  mouse reporting declines to consume the event and requires a reporting-mode
  negative test.
- The plan now requires an explicit `copy-on-select = false` test with selection
  clipboard support.

Re-review verdict: **Approved**. The reviewer confirmed both prior findings are
resolved and found no remaining required design issues.

## Result

**Result:** Pass

Implemented `middle-click-action` as surface runtime state. Roastty now
initializes `middle_click_action` and `copy_on_select` from app config,
refreshes both on surface config update, and handles non-reporting middle-button
presses according to pinned Ghostty behavior:

- `middle-click-action = primary-paste` starts a paste request.
- `middle-click-action = ignore` starts no clipboard request.
- `copy-on-select = true` and `copy-on-select = false` request the selection
  clipboard when supported.
- unsupported selection clipboard falls back to the standard clipboard.
- `copy-on-select = clipboard` requests the standard clipboard.
- terminal mouse reporting returns before middle-click paste, so middle-click
  reporting does not start a clipboard request.

The test helper for enabling terminal mouse tracking now sends the real DEC mode
sequence instead of toggling the raw mode bit directly. This matters because the
mouse-reporting path reads the terminal runtime mouse event flag, which is
updated by the stream/mode-change path.

`RUNTIME-004H` is now `Oracle complete`. `RUNTIME-004F` and `RUNTIME-004G`
remain `Gap`, and `CFG-223` remains `Gap`.

Verification passed:

```text
cargo test --manifest-path roastty/Cargo.toml middle_click_action
# 8 passed

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=21 oracle_complete=11 closed=12 audit_covered=0 incomplete=9 gap=9 cfg223=Gap
```

## Conclusion

`middle-click-action` now has a durable Tier 2 runtime guard through
`cargo test --manifest-path roastty/Cargo.toml middle_click_action`. The
remaining explicit mouse runtime gaps are `mouse-hide-while-typing` and
`right-click-action`.

## Result Review

Fresh-context Codex adversarial reviewer `Euclid` returned **Approved** with no
findings. The reviewer independently verified the focused middle-click tests,
the existing mouse runtime tests, `cargo fmt --check`, Prettier,
`git diff --check`, and the runtime/matrix assertions for `RUNTIME-004H`,
`RUNTIME-004F/G`, and `CFG-223`.
