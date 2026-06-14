# Experiment 116: Process Command Input Runtime Split

## Description

`RUNTIME-010B` currently keeps config-level command, startup input,
wait-after-command, abnormal-exit, and quit policy behavior in one broad process
gap. Experiment 113 split out the already-proven `initial-command`, environment,
and working-directory launch slice, but the remaining process row still mixes
two different kinds of behavior:

- new-terminal launch setup from parsed config (`command` and `input`);
- process lifecycle and app quit policy behavior (`wait-after-command`,
  `abnormal-command-exit-runtime`, and `quit-after-last-window-closed`).

A rejected first design for this experiment tried to close `command` and `input`
using existing `surface_start_*` tests. That was not sufficient: those tests set
`RoasttySurfaceConfig.command` and `RoasttySurfaceConfig.initial_input`
directly, while `CFG-223` requires parsed Ghostty config options to have
equivalent runtime effects.

This experiment will therefore fix and prove parsed-config runtime behavior for
`command` and `input` on new terminal surfaces. The expected behavior is:

- an explicit per-surface `RoasttySurfaceConfig.command` remains highest
  priority;
- the initial surface uses config `initial-command` when present;
- otherwise a parsed config `command` launches the new terminal surface command;
- with no explicit surface command, no applicable `initial-command`, and no
  config `command`, Roastty keeps the existing default-shell fallback;
- explicit per-surface `initial_input` remains highest priority;
- otherwise parsed config `input` writes its startup bytes to the PTY.

The intended inventory result is:

- `RUNTIME-010B1`: `Oracle complete` for parsed config `command` and parsed
  config `input` runtime effects, plus the existing default-shell/no-command
  guards.
- `RUNTIME-010B2`: `Gap` for wait-after-command, abnormal-command-exit-runtime,
  quit-after-last-window-closed, and related process lifecycle/quit policy
  behavior.

## Changes

- `roastty/src/lib.rs`
  - Add runtime wiring so parsed app config `command` is used when a surface has
    no explicit command and no applicable `initial-command`.
  - Preserve upstream-like precedence: surface command, first-surface
    `initial-command`, config `command`, then default shell.
  - Add runtime wiring so parsed config `input` is converted into startup bytes
    when a surface has no explicit `initial_input`.
  - Preserve explicit surface `initial_input` precedence over config `input`.
  - Add focused tests for:
    - first surface uses config `command` when `initial-command` is absent;
    - first surface with both `initial-command` and `command` uses
      `initial-command`;
    - later surface uses config `command` after the initial surface is consumed;
    - config `input` raw entries are Zig-string-decoded before delivery to the
      child PTY, including newline and non-newline escapes;
    - config `input` path entries are read and delivered to the child PTY;
    - explicit surface command/input override parsed config command/input;
    - no-command/default-shell and idempotent start behavior still pass.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace broad `RUNTIME-010B` with narrower `RUNTIME-010B1` and
    `RUNTIME-010B2` rows.
  - Update `EXPECTED_IDS` to require the new row split.
  - Mark `RUNTIME-010B1` `Oracle complete` only with evidence from the new
    parsed-config `command`/`input` runtime tests and existing no-command
    fallback guards.
  - Keep `RUNTIME-010B2` as `Gap` with explicit missing evidence for wait,
    abnormal-exit, and quit policy behavior.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that surface-level launch tests do not prove parsed-config
    runtime parity unless they exercise config loading or parsed config state.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The focused runtime tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml config_command_input_runtime
  cargo test --manifest-path roastty/Cargo.toml surface_start_without_command
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
  - old `RUNTIME-010B` is absent;
  - `RUNTIME-010B1` is `Oracle complete`;
  - `RUNTIME-010B1` evidence and guard cells name parsed-config
    `command`/`input` tests, including decoded raw `input` escape delivery, not
    only surface-level `RoasttySurfaceConfig` tests;
  - `RUNTIME-010B2` remains `Gap`;
  - `RUNTIME-010B2` retains wait-after-command, abnormal-command-exit-runtime,
    quit-after-last-window-closed, and lifecycle behavior;
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

  assert "RUNTIME-010B" not in rows, rows.get("RUNTIME-010B")
  assert len(rows) == 25, len(rows)
  assert rows["RUNTIME-010B1"][5] == "Oracle complete", rows["RUNTIME-010B1"]
  for term in (
      "config_command_input_runtime",
      "config command",
      "config input",
      "decoded raw input",
      "surface_start_without_command",
  ):
      assert term in rows["RUNTIME-010B1"][6] or term in rows["RUNTIME-010B1"][9], (
          term,
          rows["RUNTIME-010B1"],
      )
  assert "RoasttySurfaceConfig-only" not in rows["RUNTIME-010B1"][6], rows["RUNTIME-010B1"]
  assert rows["RUNTIME-010B1"][7].startswith("None"), rows["RUNTIME-010B1"]
  assert rows["RUNTIME-010B2"][5] == "Gap", rows["RUNTIME-010B2"]
  behavior = rows["RUNTIME-010B2"][1]
  for term in ("wait-after-command", "abnormal-command-exit-runtime", "quit-after-last-window-closed", "lifecycle"):
      assert term in behavior, (term, rows["RUNTIME-010B2"])
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/116-process-command-input-runtime-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Initial adversarial review: **Changes required**.

The reviewer found that the first draft overclaimed config-level runtime parity
from tests that set `RoasttySurfaceConfig.command` and
`RoasttySurfaceConfig.initial_input` directly. That was accepted as a real
finding. This revised design fixes the scope by requiring implementation and
tests for parsed app config `command` and `input`, while leaving lifecycle and
quit policy behavior in the remaining gap.

Second adversarial review: **Changes required**.

The reviewer found that the revised design still allowed a superficial raw
`input` runtime test to pass while sending literal escape spellings instead of
decoded bytes. That was accepted as a real finding. The design now requires the
config `input` raw-entry test to prove Zig-string decoded startup bytes,
including newline and non-newline escapes, before the row can become
`Oracle complete`.

Design re-review: **Approved**.

The reviewer confirmed the raw `input` finding is resolved because the design
now requires decoded startup bytes, including newline and non-newline escapes,
and the matrix assertion requires `RUNTIME-010B1` evidence or guard cells to
name decoded raw `input` escape delivery.

## Result

**Result:** Pass

Roastty now applies parsed app config `command` and `input` when starting new
terminal surfaces:

- per-surface `RoasttySurfaceConfig.command` remains highest priority;
- first-surface `initial-command` wins over config `command`;
- later surfaces use config `command`;
- config `input` raw entries are Zig-string-decoded before delivery, including
  newline and `\xNN` escapes;
- config `input` path entries are read and delivered;
- explicit per-surface `initial_input` overrides config `input`;
- no-command/default-shell startup and idempotent surface start still pass.

The runtime inventory now splits the old `RUNTIME-010B` row:

- `RUNTIME-010B1` is `Oracle complete` for parsed config command/input and
  default-shell launch effects.
- `RUNTIME-010B2` remains `Gap` for `wait-after-command`,
  `abnormal-command-exit-runtime`, `quit-after-last-window-closed`, and related
  process lifecycle policy behavior.

Verification run:

```sh
cargo test --manifest-path roastty/Cargo.toml config_command_input_runtime
cargo test --manifest-path roastty/Cargo.toml surface_start_without_command
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

assert "RUNTIME-010B" not in rows, rows.get("RUNTIME-010B")
assert len(rows) == 25, len(rows)
assert rows["RUNTIME-010B1"][5] == "Oracle complete", rows["RUNTIME-010B1"]
for term in (
    "config_command_input_runtime",
    "config command",
    "config input",
    "decoded raw input",
    "surface_start_without_command",
):
    assert term in rows["RUNTIME-010B1"][6] or term in rows["RUNTIME-010B1"][9], (
        term,
        rows["RUNTIME-010B1"],
    )
assert "RoasttySurfaceConfig-only" not in rows["RUNTIME-010B1"][6], rows["RUNTIME-010B1"]
assert rows["RUNTIME-010B1"][7].startswith("None"), rows["RUNTIME-010B1"]
assert rows["RUNTIME-010B2"][5] == "Gap", rows["RUNTIME-010B2"]
behavior = rows["RUNTIME-010B2"][1]
for term in ("wait-after-command", "abnormal-command-exit-runtime", "quit-after-last-window-closed", "lifecycle"):
    assert term in behavior, (term, rows["RUNTIME-010B2"])
cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
assert "| Gap " in cfg223, cfg223
PY
prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/116-process-command-input-runtime-split.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md
git diff --check
```

All commands passed after formatting the regenerated markdown tables.

Completion review: **Approved**.

The reviewer reran the focused command/input tests, no-command fallback tests,
Rust fmt check, matrix assertion, markdown check, and diff hygiene check. It
confirmed the generated outputs are internally consistent: 25 runtime rows, 18
`Oracle complete`, one intentional divergence, six gaps, old `RUNTIME-010B`
absent, `RUNTIME-010B1` `Oracle complete`, `RUNTIME-010B2` `Gap`, and `CFG-223`
still `Gap`.

## Conclusion

Parsed config `command` and `input` are no longer part of the process runtime
gap. The remaining process gap is specifically lifecycle policy: wait after
command, abnormal-exit handling, quit-after-last-window-closed behavior, and
related process-exit UI/app effects.
