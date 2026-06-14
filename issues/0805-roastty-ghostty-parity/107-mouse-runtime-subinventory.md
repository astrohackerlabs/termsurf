# Experiment 107: Mouse Runtime Subinventory

## Description

`RUNTIME-004` is still too broad to close honestly. It mixes mouse reporting,
shift capture, scroll scaling, click-repeat timing, prompt cursor movement,
mouse hiding, and right/middle click actions into one row. Some of those
behaviors already have Roastty runtime hooks and tests; others are parser-only
or app/UI behavior that should remain gaps until proven.

This experiment will split `RUNTIME-004` into explicit mouse runtime subrows and
add focused libroastty runtime guards for the subrows that are already wired
through `Surface`:

- mouse reporting enable/disable and `toggle_mouse_reporting`;
- `mouse-shift-capture` interaction with terminal `XTSHIFTESCAPE` state;
- `mouse-scroll-multiplier` precision/discrete scroll step behavior;
- `click-repeat-interval` propagation into selection click timing.

It will not claim parity for `cursor-click-to-move`, `mouse-hide-while-typing`,
`right-click-action`, or `middle-click-action` unless the implementation and
tests prove those runtime effects in this experiment. If they remain unproven,
the generated runtime inventory must keep them as mouse gaps with dedicated
follow-up rows.

## Changes

- Update `roastty/src/lib.rs` tests with focused runtime oracles for the
  `Surface` mouse settings that are already represented in runtime state:
  `mouse_reporting`, `mouse_shift_capture`, `mouse_scroll_multiplier`, and
  `click_repeat_interval_ns`.
- Reuse existing test helpers such as `new_test_config_with_mouse_behavior`,
  `set_surface_worker_mouse_mode`, and deterministic selection-click timing
  rather than adding GUI automation.
- Update `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py` to
  split the current `RUNTIME-004` row into smaller mouse subrows. Proven subrows
  may be marked `Oracle complete`; unproven click/cursor/UI subrows must remain
  `Gap`.
- Regenerate `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  and update the `CFG-223` summary in
  `issues/0805-roastty-ghostty-parity/config-matrix.md`.
- Update the Issue 805 learnings with any durable conclusions about mouse
  runtime coverage and remaining gaps.

## Verification

Pass criteria:

- The new mouse runtime tests fail before the runtime behavior they guard is
  present and pass after the implementation.
- The generated runtime inventory no longer has one ambiguous `RUNTIME-004` row;
  it has explicit mouse subrows with honest statuses.
- `cursor-click-to-move`, `mouse-hide-while-typing`, `right-click-action`, and
  `middle-click-action` remain gaps unless direct runtime/UI evidence is added.
- `CFG-223` remains `Gap` until every runtime/UI row is complete, not
  applicable, or accepted as a documented divergence.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml mouse_runtime
cargo test --manifest-path roastty/Cargo.toml mouse_shift_capture
cargo test --manifest-path roastty/Cargo.toml mouse_scroll
cargo test --manifest-path roastty/Cargo.toml selection
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

assert "RUNTIME-004" not in rows

expected_status = {
    "RUNTIME-004A": "Oracle complete",  # mouse-reporting + toggle
    "RUNTIME-004B": "Oracle complete",  # mouse-shift-capture
    "RUNTIME-004C": "Oracle complete",  # mouse-scroll-multiplier
    "RUNTIME-004D": "Oracle complete",  # click-repeat-interval
    "RUNTIME-004E": "Gap",              # cursor-click-to-move
    "RUNTIME-004F": "Gap",              # mouse-hide-while-typing
    "RUNTIME-004G": "Gap",              # right-click-action
    "RUNTIME-004H": "Gap",              # middle-click-action
}
for row_id, status in expected_status.items():
    assert rows[row_id][4] == "mouse", rows[row_id]
    assert rows[row_id][5] == status, rows[row_id]

expected_options = {
    "RUNTIME-004E": "cursor-click-to-move",
    "RUNTIME-004F": "mouse-hide-while-typing",
    "RUNTIME-004G": "right-click-action",
    "RUNTIME-004H": "middle-click-action",
}
for row_id, option in expected_options.items():
    assert option in rows[row_id][1] or option in rows[row_id][6] or option in rows[row_id][7], rows[row_id]

assert "| CFG-223 |" in cfg223
assert "| Gap " in cfg223
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/107-mouse-runtime-subinventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Design Review

Adversarial design review by fresh-context Codex subagent `Curie`:

- **Initial verdict:** Changes required.
- **Required finding:** The first inventory assertion could pass while leaving
  the unsuffixed ambiguous `RUNTIME-004` row in place, and while mentioning
  unproven mouse options somewhere in the inventory without proving they had
  dedicated `Gap` rows.
- **Fix:** The verification snippet now parses runtime inventory table rows,
  rejects unsuffixed `RUNTIME-004`, asserts exact mouse subrow statuses, and
  requires the unproven cursor/mouse/right-click/middle-click options to appear
  on dedicated gap rows.
- **Re-review verdict:** Approved. The reviewer confirmed the strengthened
  assertion resolves the finding and introduces no new required findings.

## Result

**Result:** Partial

`RUNTIME-004` was split into eight explicit mouse runtime subrows:

- `RUNTIME-004A`: `mouse-reporting` config and `toggle_mouse_reporting` runtime
  effects — `Oracle complete`;
- `RUNTIME-004B`: `mouse-shift-capture` config and terminal `XTSHIFTESCAPE`
  effects — `Oracle complete`;
- `RUNTIME-004C`: `mouse-scroll-multiplier` runtime scroll-step effects —
  `Oracle complete`;
- `RUNTIME-004D`: `click-repeat-interval` selection timing effects —
  `Oracle complete`;
- `RUNTIME-004E`: `cursor-click-to-move` prompt movement effects — `Gap`;
- `RUNTIME-004F`: `mouse-hide-while-typing` cursor visibility effects — `Gap`;
- `RUNTIME-004G`: `right-click-action` effects — `Gap`;
- `RUNTIME-004H`: `middle-click-action` effects — `Gap`.

The generated runtime inventory now has 21 rows: 9 rows are `Oracle complete`,
10 rows are closed, 11 rows are incomplete, and 11 rows remain runtime gaps.
CFG-223 remains `Gap`.

Verification passed:

```bash
cargo test --manifest-path roastty/Cargo.toml mouse_runtime
# 5 passed; 0 failed

cargo test --manifest-path roastty/Cargo.toml mouse_shift_capture
# 6 passed; 0 failed

cargo test --manifest-path roastty/Cargo.toml mouse_scroll
# 14 passed; 0 failed

cargo test --manifest-path roastty/Cargo.toml selection
# 171 passed; 0 failed

cargo fmt --manifest-path roastty/Cargo.toml

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=21 oracle_complete=9 closed=10 audit_covered=0 incomplete=11 gap=11 cfg223=Gap

prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md
```

## Result Review

Adversarial result review by fresh-context Codex subagent `Hume`:

- **Initial verdict:** Changes required.
- **Required finding:** The documented verification was not reproducible as
  written because `config_runtime_inventory.py` emits compact markdown tables,
  while the checked-in inventory and matrix are Prettier-formatted. Running the
  generator immediately before `prettier --check` would fail unless the
  generated files were formatted first.
- **Fix:** The verification and result command blocks now include
  `prettier --write --prose-wrap always --print-width 80` for the generated
  matrix and runtime inventory immediately after generator execution.
- **Re-review verdict:** Approved. The reviewer confirmed the experiment now
  documents a reproducible generator -> formatter -> check workflow and found no
  new required findings.

## Conclusion

Mouse runtime parity is no longer hidden behind one ambiguous row. The
libroastty-level mouse reporting, shift-capture, scroll multiplier, and
click-repeat timing effects now have durable guards. The remaining mouse gaps
are the UI/action behaviors that need more specific experiments:
`cursor-click-to-move`, `mouse-hide-while-typing`, `right-click-action`, and
`middle-click-action`.
