# Experiment 113: PTY Process Runtime Split

## Description

`RUNTIME-010` is currently a single broad `Gap` covering all PTY/process launch
effects. That hides already-proven behavior together with remaining lifecycle
work.

Pinned Ghostty's process-related config surface includes command and
`initial-command`, environment variables, startup input, wait-after-command,
abnormal-command-exit-runtime, working-directory, and app quit policy. Roastty
already has focused PTY and app/surface tests for the initial-command,
environment, and working-directory slice. This experiment will split that proven
slice out of `RUNTIME-010` without claiming command, startup input, or lifecycle
parity that is not yet proven.

The intended result is:

- `RUNTIME-010A`: `Oracle complete` for initial-command, environment, and
  working-directory launch effects that are already covered by existing PTY/app
  oracles.
- `RUNTIME-010B`: `Gap` for config-level command, config-level startup input,
  wait-after-command, abnormal-command-exit-runtime,
  quit-after-last-window-closed, and related lifecycle/quit policy behavior.

This experiment is inventory/refinement plus guard verification. It should not
change process runtime semantics unless the existing evidence proves too weak
and a focused oracle is required.

## Changes

- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace broad `RUNTIME-010` with narrower `RUNTIME-010A` and `RUNTIME-010B`
    rows.
  - Update `EXPECTED_IDS` to require the new row split.
  - Mark `RUNTIME-010A` `Oracle complete` only if its evidence points at
    concrete existing tests.
  - Keep `RUNTIME-010B` as `Gap` with explicit missing evidence for config-level
    command, config-level startup input, lifecycle, and quit policy behavior.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that broad CFG-223 rows should be split when a proven runtime
    slice is otherwise hidden behind unrelated gaps.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The runtime inventory validates the new manifest and reports the expected row
  split:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  ```

- Focused PTY/app tests for the promoted slice pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml first_surface_uses_app_initial_command
  cargo test --manifest-path roastty/Cargo.toml later_surface_after_close_ignores_app_initial_command
  cargo test --manifest-path roastty/Cargo.toml surface_inherited_config
  cargo test --manifest-path roastty/Cargo.toml spawn_with_cwd
  cargo test --manifest-path roastty/Cargo.toml termio_env
  ```

- A matrix assertion proves:
  - `RUNTIME-010A` is `Oracle complete`;
  - `RUNTIME-010B` remains `Gap`;
  - `CFG-223` remains `Gap`;
  - no process lifecycle behavior is overclaimed as complete.

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

  assert rows["RUNTIME-010A"][5] == "Oracle complete", rows["RUNTIME-010A"]
  assert rows["RUNTIME-010B"][5] == "Gap", rows["RUNTIME-010B"]
  assert "command" in rows["RUNTIME-010B"][1], rows["RUNTIME-010B"]
  assert "startup input" in rows["RUNTIME-010B"][1], rows["RUNTIME-010B"]
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/113-pty-process-runtime-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Fresh-context Codex adversarial reviewer `Lorentz` initially returned **CHANGES
REQUIRED**:

- **Required:** the design overclaimed config-level `command` and startup
  `input` as already proven by the proposed `RUNTIME-010A` guard set.
- **Optional:** the matrix assertion was described as desired row states but did
  not include an executable pass/fail command.

Fix:

- `RUNTIME-010A` is now limited to initial-command, environment, and
  working-directory launch effects.
- `RUNTIME-010B` now explicitly keeps config-level command, config-level startup
  input, wait-after-command, abnormal-command-exit-runtime,
  quit-after-last-window-closed, and related lifecycle/quit policy behavior as
  `Gap`.
- Verification now includes an executable Python assertion for the expected row
  states and `CFG-223` status.

Re-review verdict: **Approved**. Fresh-context reviewer `Hooke` confirmed the
required overclaim was resolved, the assertion command was added, and no new
required findings were introduced.

## Result

**Result:** Pass

Split the broad PTY/process runtime row into two rows:

- `RUNTIME-010A` is `Oracle complete` for initial-command, environment, and
  working-directory launch behavior.
- `RUNTIME-010B` remains `Gap` for config-level command, config-level startup
  input, wait-after-command, abnormal-command-exit-runtime,
  quit-after-last-window-closed, and related lifecycle/quit policy behavior.

The regenerated runtime inventory now reports 22 runtime rows, 15
oracle-complete rows, 16 closed rows, and 6 gap rows. `CFG-223` remains `Gap`,
as intended.

Verification passed:

```text
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=22 oracle_complete=15 closed=16 audit_covered=0 incomplete=6 gap=6 cfg223=Gap

cargo test --manifest-path roastty/Cargo.toml first_surface_uses_app_initial_command
# 1 passed

cargo test --manifest-path roastty/Cargo.toml later_surface_after_close_ignores_app_initial_command
# 1 passed

cargo test --manifest-path roastty/Cargo.toml surface_inherited_config
# 6 passed

cargo test --manifest-path roastty/Cargo.toml spawn_with_cwd
# 2 passed

cargo test --manifest-path roastty/Cargo.toml termio_env
# 6 passed
```

## Conclusion

The PTY/process runtime coverage is now more honest and more useful for future
work. Existing guards prove the initial-command, environment, and
working-directory launch slice, while the remaining process row names the
unproven command/input/lifecycle behavior that still blocks `CFG-223`.

## Completion Review

Fresh-context Codex reviewer `Nietzsche` returned **Approved** with no required
findings.

The reviewer verified:

- the README records Experiment 113 as **Pass**;
- this experiment file has `## Result` and `## Conclusion`;
- `RUNTIME-010A` is limited to initial-command, environment, and
  working-directory launch behavior;
- `RUNTIME-010B` remains `Gap` for command, startup input, wait, abnormal-exit,
  and quit-policy behavior;
- `CFG-223` remains `Gap`;
- generated counts are consistent;
- `HEAD` was still the plan commit before the result commit;
- `prettier --check`, `git diff --check`, and the focused cargo tests passed.

The reviewer also noted an optional issue: running `config_runtime_inventory.py`
with a brand-new `/tmp` matrix path fails because the script updates an existing
matrix row rather than creating a matrix from scratch. I accepted this as
non-blocking because the documented workflow updates the existing Issue 805
`config-matrix.md`; no implementation change was made.
