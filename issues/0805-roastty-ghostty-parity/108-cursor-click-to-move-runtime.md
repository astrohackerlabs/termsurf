# Experiment 108: Cursor Click To Move Runtime

## Description

`RUNTIME-004E` remains a mouse runtime gap after Experiment 107. Pinned Ghostty
gates prompt click movement in `Surface.maybePromptClick`: the active screen
must advertise prompt click support, `cursor-click-to-move` must be enabled, the
cursor must be at a prompt input position, the click must not be a drag or
selection completion, and the click must be at or after the prompt. When those
conditions hold, Ghostty either writes a Kitty-style prompt click event or
synthetic cursor-key movement.

Roastty already ports the terminal prompt movement primitive in
`roastty/src/terminal/page_list.rs::prompt_click_move`, but `RUNTIME-004E` still
needs a surface-level runtime guard proving the config option is wired into
mouse release behavior.

This experiment will implement and test the `cursor-click-to-move` surface gate
for both pinned Ghostty prompt-click modes: Kitty-style `click_events=1` and
cursor-key line movement. It will not attempt to close
`mouse-hide-while-typing`, `right-click-action`, or `middle-click-action`.

## Changes

- Add `cursor_click_to_move` to Roastty `Surface` runtime state, initialize it
  from app config, and refresh it through `Surface.apply_config`.
- Add a focused surface mouse-release path equivalent to Ghostty's prompt click
  behavior:
  - only left-button release;
  - only when `cursor_click_to_move` is enabled;
  - only when the terminal is at a prompt and the click target is eligible;
  - for `click_events=1`, write the SGR mouse press event Ghostty emits;
  - for line-style `cl` modes, write the cursor movement sequence produced by
    the existing prompt-click terminal primitive;
  - do not handle the event when prompt-click support is absent, when the cursor
    is not at prompt input, when the click was dragged, when an active selection
    is completing, or when the click is before the prompt;
  - do not handle the event when the config is disabled.
- Prefer reusing existing terminal prompt-click primitives and mouse geometry
  helpers instead of re-implementing prompt movement math in `Surface`.
- Update `config_runtime_inventory.py` so `RUNTIME-004E` becomes
  `Oracle complete` only if the new runtime guard exists.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`, then format
  the generated markdown.
- Update Issue 805 learnings with what the experiment proves.

## Verification

Pass criteria:

- A focused runtime test proves that a semantic prompt with
  `cursor-click-to-move = true` and `click_events=1` turns an eligible
  left-click release into the expected SGR mouse press bytes.
- A focused runtime test proves that a semantic prompt with
  `cursor-click-to-move = true` and `cl=line` turns an eligible left-click
  release into the expected cursor movement bytes.
- A companion test proves `cursor-click-to-move = false` suppresses that
  behavior.
- Negative tests prove no prompt-click output is written when prompt-click
  support is absent, the cursor is not at prompt input, the click was dragged,
  an active selection exists, or the click is before the prompt.
- Existing terminal prompt movement tests still pass.
- `RUNTIME-004E` is promoted to `Oracle complete`, while `RUNTIME-004F`,
  `RUNTIME-004G`, and `RUNTIME-004H` remain `Gap`.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml cursor_click_to_move
cargo test --manifest-path roastty/Cargo.toml prompt_click_move
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

assert rows["RUNTIME-004E"][5] == "Oracle complete", rows["RUNTIME-004E"]
for row_id in ["RUNTIME-004F", "RUNTIME-004G", "RUNTIME-004H"]:
    assert rows[row_id][5] == "Gap", rows[row_id]
assert "| Gap " in cfg223
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/108-cursor-click-to-move-runtime.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Design Review

Adversarial design review by fresh-context Codex subagent `Harvey`:

- **Initial verdict:** Changes required.
- **Required findings:** The first design would have promoted `RUNTIME-004E`
  while only proving line-mode prompt movement, even though pinned Ghostty also
  handles `click_events=1`; it also did not require tests for the negative
  surface gates in `maybePromptClick`.
- **Fix:** The design now requires both `click_events=1` SGR mouse press output
  and `cl=line` cursor-key movement output, plus negative tests for absent
  prompt-click support, cursor not at prompt input, dragged click, active
  selection, click before prompt, and disabled config.
- **Re-review verdict:** Approved. The reviewer confirmed the prior findings are
  resolved and that the design still does not overclaim
  `mouse-hide-while-typing`, `right-click-action`, or `middle-click-action`.

## Result

**Result:** Pass

Implemented `cursor-click-to-move` as a surface runtime gate. Roastty now stores
the active `cursor_click_to_move` config value on each surface, refreshes it
during surface config updates, and handles eligible left-button releases before
normal selection/reporting dispatch.

The implementation reuses the existing prompt movement primitive instead of
duplicating movement math in the surface layer:

- `click_events=1` prompt support writes the Ghostty-style SGR mouse press
  sequence for the clicked viewport cell.
- `cl=line` prompt support writes synthetic cursor-key movement bytes based on
  the existing prompt-click movement calculation.
- Ineligible clicks are not handled when prompt-click support is missing,
  `cursor-click-to-move` is disabled, an active selection exists, the click was
  dragged, or the click is before the prompt.
- Eligible same-cell `cl=line` prompt clicks are consumed even when they write
  no cursor-key bytes, matching pinned Ghostty's handled no-op behavior.

During implementation, the first positive tests showed that `OSC 133;B` was
clearing prompt-click mode because Roastty updated the mode for every OSC 133
semantic prompt command. Ghostty's prompt-click options are attached to
prompt-start commands such as `OSC 133;A;click_events=1` and
`OSC 133;A;cl=line`; later input/output markers must preserve that mode. Roastty
now updates prompt-click mode only for prompt-start commands.

The runtime inventory was regenerated. `RUNTIME-004E` is now `Oracle complete`;
`RUNTIME-004F`, `RUNTIME-004G`, and `RUNTIME-004H` remain `Gap`; `CFG-223`
remains `Gap`.

Verification passed:

```text
cargo test --manifest-path roastty/Cargo.toml cursor_click_to_move
# 4 passed

cargo test --manifest-path roastty/Cargo.toml prompt_click_move
# 21 passed

cargo test --manifest-path roastty/Cargo.toml mouse_runtime
# 5 passed

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=21 oracle_complete=10 closed=11 audit_covered=0 incomplete=10 gap=10 cfg223=Gap
```

## Conclusion

`cursor-click-to-move` has a durable Tier 2 regression guard through
`cargo test --manifest-path roastty/Cargo.toml cursor_click_to_move`. The next
mouse runtime gaps are still `mouse-hide-while-typing`, `right-click-action`,
and `middle-click-action`.

## Result Review

Fresh-context Codex adversarial reviewer `Confucius` initially returned
**CHANGES REQUIRED**:

- **Required:** eligible zero-movement `cl=line` prompt clicks were not
  consumed. Roastty returned `None` when cursor movement generated no bytes,
  while pinned Ghostty still treats the release as handled.

Fix:

- `Screen::prompt_click_move_for_viewport` now returns `None` only for
  ineligible prompt clicks and `Some(PromptClickMove::ZERO)` for eligible no-op
  clicks.
- `Terminal::prompt_click_action` now returns `Some(Bytes(Vec::new()))` for
  eligible zero-movement line-mode clicks.
- `cursor_click_to_move_line_mode_same_cell_consumes_release` proves the surface
  consumes the eligible no-op release.

Re-review verdict: **Approved**. The reviewer confirmed that the zero-movement
line-mode click is now represented as a handled empty-byte action, the surface
consumes it, and the new regression test passes.
