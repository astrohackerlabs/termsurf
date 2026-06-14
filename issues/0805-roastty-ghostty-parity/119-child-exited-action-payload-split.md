# Experiment 119: Child Exited Action Payload Split

## Description

`RUNTIME-010B2B` still combines several remaining lifecycle behaviors after
Experiment 118:

- child-exit payload and `show_child_exited` action dispatch;
- `abnormal-command-exit-runtime` threshold visibility;
- terminal fallback text when the app does not handle the child-exited action;
- `quit-after-last-window-closed` and `quit-after-last-window-closed-delay`;
- remaining app lifecycle policy behavior.

Pinned Ghostty's terminal process layer reports child exits with an exit code
and runtime in milliseconds. `Surface.zig::childExited` forwards that payload to
the app through the `.show_child_exited` action before deciding whether to show
terminal fallback text, hold the surface open, or close it. Roastty already has
the copied C ABI shape and macOS Swift handler:

- `ROASTTY_ACTION_SHOW_CHILD_EXITED`;
- `roastty_surface_message_childexited_s`;
- `Roastty.App.showChildExited`;
- `Roastty.ChildExitedMessage`.

The missing `libroastty` slice is the payload path from PTY child exit to the
typed action callback. This experiment will add that path and prove it without
claiming full abnormal-exit UI or app quit policy parity.

The intended inventory result is:

- `RUNTIME-010B2B1`: `Oracle complete` for child-exit exit-code/runtime payload
  capture and `show_child_exited` action dispatch, including representative
  normal-runtime and abnormal-threshold cases.
- `RUNTIME-010B2B2`: `Gap` for terminal fallback child-exit text, abnormal-exit
  close/hold policy after handled/unhandled actions,
  `quit-after-last-window-closed`, `quit-after-last-window-closed-delay`, and
  remaining lifecycle policy behavior.

## Changes

- `roastty/src/termio.rs`
  - Add a child-exit info payload carrying exit code and runtime milliseconds.
  - Record a child start timestamp when spawning the PTY child.
  - Convert `PtyChild::try_wait()` results into the child-exit info payload.
  - Preserve the existing boolean child-exited behavior for callers that only
    need that state.
  - Add focused termio tests proving:
    - successful child exit reports exit code `0`;
    - failing child exit reports the nonzero code;
    - runtime milliseconds are populated and nonzero for a command that sleeps
      beyond a small threshold.
- `roastty/src/lib.rs`
  - Add the `roastty_surface_message_childexited_s` equivalent to
    `RoasttyActionU` conversion and its test inverse.
  - On `pump.child_exited`, dispatch `ROASTTY_ACTION_SHOW_CHILD_EXITED` to the
    app action callback with the captured exit code and runtime.
  - Dispatch the action before the wait-after-command close/hold decision,
    matching pinned Ghostty ordering.
  - Keep Experiment 118's close/hold behavior intact; do not claim terminal
    fallback message parity or app quit policy in this experiment.
  - Add focused surface tests proving:
    - child-exit action records contain the expected exit code for success and
      failure;
    - runtime milliseconds cross the configured threshold for a sleeping
      command;
    - a child exit whose runtime is at or below the configured
      `abnormal-command-exit-runtime` threshold still dispatches
      `ROASTTY_ACTION_SHOW_CHILD_EXITED` with the captured payload;
    - the action dispatch happens before a default non-wait surface requests
      close;
    - wait-after-command surfaces still hold after dispatch;
    - an action callback returning false does not prevent the existing
      close/hold behavior from Experiment 118.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace `RUNTIME-010B2B` with `RUNTIME-010B2B1` and `RUNTIME-010B2B2`.
  - Update `EXPECTED_IDS` to require the new split.
  - Mark `RUNTIME-010B2B1` `Oracle complete` only with evidence from the new
    child-exit payload and action-dispatch tests.
  - Keep `RUNTIME-010B2B2` as `Gap` with explicit missing evidence for terminal
    fallback text, abnormal-exit close/hold policy after handled/unhandled
    actions, quit-after-last-window-closed, quit delay, and remaining lifecycle
    behavior.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that child-exit parity has a payload/action layer distinct
    from terminal fallback text and app quit policy.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The focused child-exit payload/action tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime
  ```

  These tests must include both a normal child exit above the configured
  `abnormal-command-exit-runtime` threshold and an abnormal-threshold child exit
  at or below that threshold. Both cases must prove the typed
  `ROASTTY_ACTION_SHOW_CHILD_EXITED` payload reaches the app action callback.

- The Experiment 118 close/hold regression guard still passes:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime
  ```

- Existing process-exit and close-surface filters still pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml process_exited
  cargo test --manifest-path roastty/Cargo.toml close_surface
  ```

- Rust formatting passes:

  ```sh
  cargo fmt --manifest-path roastty/Cargo.toml -- --check
  ```

- The runtime inventory validates the new manifest and reports the expected row
  split:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  ```

- A matrix assertion proves:
  - old `RUNTIME-010B2B` is absent;
  - `RUNTIME-010B2B1` is `Oracle complete`;
  - `RUNTIME-010B2B1` evidence or guard cells name
    `child_exited_payload_runtime`;
  - `RUNTIME-010B2B1` missing evidence starts with `None`;
  - `RUNTIME-010B2B2` remains `Gap`;
  - `RUNTIME-010B2B2` retains terminal fallback text, abnormal-exit close/hold
    policy, `quit-after-last-window-closed`,
    `quit-after-last-window-closed-delay`, and lifecycle policy behavior;
  - `CFG-223` remains `Gap`.

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
  from pathlib import Path

  inventory = Path("issues/0805-roastty-ghostty-parity/config-runtime-inventory.md").read_text()
  matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()

  rows = {}
  for line in inventory.splitlines():
      if not line.startswith("| RUNTIME-"):
          continue
      cells = [cell.strip() for cell in line.strip("|").split("|")]
      rows[cells[0]] = cells

  assert "RUNTIME-010B2B" not in rows, rows.get("RUNTIME-010B2B")
  assert len(rows) == 28, len(rows)
  assert rows["RUNTIME-010B2B1"][5] == "Oracle complete", rows["RUNTIME-010B2B1"]
  assert (
      "child_exited_payload_runtime" in rows["RUNTIME-010B2B1"][6]
      or "child_exited_payload_runtime" in rows["RUNTIME-010B2B1"][9]
  ), rows["RUNTIME-010B2B1"]
  assert rows["RUNTIME-010B2B1"][7].startswith("None"), rows["RUNTIME-010B2B1"]
  assert rows["RUNTIME-010B2B2"][5] == "Gap", rows["RUNTIME-010B2B2"]
  behavior = rows["RUNTIME-010B2B2"][1]
  for term in (
      "terminal fallback",
      "abnormal-exit close/hold",
      "quit-after-last-window-closed",
      "quit-after-last-window-closed-delay",
      "lifecycle",
  ):
      assert term in behavior, (term, rows["RUNTIME-010B2B2"])
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/119-child-exited-action-payload-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Initial adversarial review: **Changes required**.

The reviewer found that the first design overclaimed representative
abnormal-threshold coverage because it mentioned normal-runtime and
abnormal-threshold action cases but only required success/failure exit-code
tests plus a sleeping above-threshold runtime case. That was accepted as a real
finding. The design now requires an explicit child-exit case at or below the
configured `abnormal-command-exit-runtime` threshold and requires that case to
prove `ROASTTY_ACTION_SHOW_CHILD_EXITED` receives the captured payload.

Design re-review: **Approved**.

The reviewer confirmed the abnormal-threshold finding is resolved because the
design now requires both above-threshold and at-or-below-threshold
`show_child_exited` payload dispatch cases. The reviewer also confirmed the
README links Experiment 119 as `Designed` and no plan commit had been made
before approval.

## Result

**Result:** Pass

Roastty now captures PTY child-exit status as an exit code plus runtime
milliseconds and forwards that payload through the typed
`ROASTTY_ACTION_SHOW_CHILD_EXITED` app action before the existing close/hold
decision. The worker loop now treats child exit, not EOF alone, as the terminal
condition so an EOF-only final PTY read cannot suppress the later child-exit
payload.

The new guards prove successful and failing exit codes, nonzero runtime for a
sleeping command, above-threshold dispatch, at-or-below-threshold dispatch,
dispatch before default close, wait-after-command hold after dispatch, and that
a false action result does not change the existing close/hold behavior.

Verification passed:

```sh
cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime
cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime
cargo test --manifest-path roastty/Cargo.toml process_exited
cargo test --manifest-path roastty/Cargo.toml close_surface
cargo fmt --manifest-path roastty/Cargo.toml -- --check
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

inventory = Path("issues/0805-roastty-ghostty-parity/config-runtime-inventory.md").read_text()
matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()

rows = {}
for line in inventory.splitlines():
    if not line.startswith("| RUNTIME-"):
        continue
    cells = [cell.strip() for cell in line.strip("|").split("|")]
    rows[cells[0]] = cells

assert "RUNTIME-010B2B" not in rows, rows.get("RUNTIME-010B2B")
assert len(rows) == 28, len(rows)
assert rows["RUNTIME-010B2B1"][5] == "Oracle complete", rows["RUNTIME-010B2B1"]
assert (
    "child_exited_payload_runtime" in rows["RUNTIME-010B2B1"][6]
    or "child_exited_payload_runtime" in rows["RUNTIME-010B2B1"][9]
), rows["RUNTIME-010B2B1"]
assert rows["RUNTIME-010B2B1"][7].startswith("None"), rows["RUNTIME-010B2B1"]
assert rows["RUNTIME-010B2B2"][5] == "Gap", rows["RUNTIME-010B2B2"]
behavior = rows["RUNTIME-010B2B2"][1]
for term in (
    "terminal fallback",
    "abnormal-exit close/hold",
    "quit-after-last-window-closed",
    "quit-after-last-window-closed-delay",
    "lifecycle",
):
    assert term in behavior, (term, rows["RUNTIME-010B2B2"])
cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
assert "| Gap " in cfg223, cfg223
PY
```

The inventory now reports:

```text
runtime_rows=28
oracle_complete=21
closed=22
audit_covered=0
incomplete=6
gap=6
cfg223=Gap
```

## Conclusion

`RUNTIME-010B2B` is split. `RUNTIME-010B2B1` is `Oracle complete` for the
child-exit exit-code/runtime payload and `show_child_exited` action dispatch.
`RUNTIME-010B2B2` remains a `Gap` for terminal fallback child-exit text,
abnormal-exit close/hold policy after handled or unhandled actions,
`quit-after-last-window-closed`, `quit-after-last-window-closed-delay`, and
remaining lifecycle policy behavior.

## Completion Review

Fresh-context adversarial review: **Approved**.

The reviewer found no required issues. They independently verified that the
result had not been committed yet, reran the focused child-exit payload,
wait-after-command, process-exited, close-surface, Rust format, Prettier,
`git diff --check`, and matrix assertion gates, and confirmed the inventory
regeneration was represented by the checked-in generated markdown.
