# Experiment 120: Child Exited Fallback Policy Split

## Description

`RUNTIME-010B2B2` still combines terminal fallback child-exit text,
abnormal-exit close/hold policy, quit-after-last-window-closed, quit delay, and
remaining lifecycle policy. Experiments 118 and 119 proved the normal
wait-after-command close/hold slice and the child-exit payload/action dispatch
slice, but Roastty still does not model Ghostty's branch-specific child-exit
fallback policy.

Pinned Ghostty's `Surface.zig::childExited` has two user-visible branches after
receiving the child-exit payload:

- if `runtime_ms <= abnormal-command-exit-runtime`, Ghostty treats the process
  as abnormal on macOS, tries `.show_child_exited`, and returns if the app
  handles it; if the app declines, it writes abnormal fallback text into the
  terminal and still returns without taking the normal close path;
- otherwise, Ghostty tries `.show_child_exited`, writes normal terminal fallback
  text if the app declines, then applies the normal `wait-after-command`
  close/hold decision.

This experiment will implement and prove that branch-specific fallback and
close/hold policy in `libroastty`. It will not claim
`quit-after-last-window-closed`, `quit-after-last-window-closed-delay`, or broad
macOS app lifecycle parity.

The intended inventory result is:

- `RUNTIME-010B2B2A`: `Oracle complete` for terminal fallback child-exit text
  and abnormal-command-exit-runtime close/hold policy after handled or unhandled
  `show_child_exited` actions.
- `RUNTIME-010B2B2B`: `Gap` for `quit-after-last-window-closed`,
  `quit-after-last-window-closed-delay`, and remaining app lifecycle policy
  behavior.

## Changes

- `roastty/src/lib.rs`
  - Store parsed `abnormal-command-exit-runtime` on each `Surface` and keep it
    refreshed on config updates.
  - Store the effective launched command label chosen by `start_termio` (surface
    command, parsed `initial-command`, parsed `command`, or default shell
    fallback) so abnormal fallback text can report the command that was actually
    launched.
  - Replace the inline child-exit close handling with a focused helper matching
    Ghostty's ordering:
    - mark the child exit handled once;
    - classify abnormal exits with
      `runtime_ms <= abnormal-command-exit-runtime`;
    - dispatch `ROASTTY_ACTION_SHOW_CHILD_EXITED` before terminal fallback text;
    - for abnormal exits, return after a handled action or after writing
      abnormal fallback text, without requesting close;
    - for normal exits, write normal fallback text only when the action is not
      handled, then preserve the existing `wait-after-command` close/hold
      behavior.
  - Add terminal fallback text helpers that feed bytes into the existing
    terminal parser rather than bypassing terminal state.
  - Keep the previous child-exit payload/action tests passing while adjusting
    the false-action normal case to assert the fallback text is written before
    the default close request.
  - Add focused runtime tests proving:
    - normal-runtime, unhandled `show_child_exited` writes
      `Process exited. Press any key to close the terminal.` and still requests
      close when `wait-after-command = false`;
    - normal-runtime, handled `show_child_exited` does not write fallback text
      and still follows the normal close/hold policy;
    - abnormal-runtime, handled `show_child_exited` does not request close;
    - abnormal-runtime, unhandled `show_child_exited` writes abnormal fallback
      text including the pinned Ghostty labels
      `Ghostty failed to launch the requested command:`, the launched command,
      `Runtime:`, and `Press any key to close the window.`, and does not request
      close;
    - runtime exactly equal to the configured threshold follows the abnormal
      branch;
    - above-threshold runtime follows the normal branch.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace `RUNTIME-010B2B2` with `RUNTIME-010B2B2A` and `RUNTIME-010B2B2B`.
  - Mark `RUNTIME-010B2B2A` `Oracle complete` only with evidence from the new
    child-exit fallback policy tests.
  - Keep `RUNTIME-010B2B2B` as `Gap` for quit-after-last-window-closed, quit
    delay, and remaining lifecycle policy.
  - Update `EXPECTED_IDS` to require the new split.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that child-exit fallback policy is branch-specific: abnormal
    exits hold after GUI/fallback handling, while normal exits still use
    `wait-after-command`.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The focused child-exit fallback policy tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml child_exited_fallback_policy_runtime
  ```

- The existing child-exit payload, wait-after-command, process-exited, and
  close-surface regression guards still pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime
  cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime
  cargo test --manifest-path roastty/Cargo.toml process_exited
  cargo test --manifest-path roastty/Cargo.toml close_surface
  ```

- Rust formatting passes:

  ```sh
  cargo fmt --manifest-path roastty/Cargo.toml -- --check
  ```

- The crate still builds:

  ```sh
  cargo build --manifest-path roastty/Cargo.toml
  ```

- The runtime inventory validates the new manifest and reports the expected row
  split:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  ```

- A matrix assertion proves:
  - old `RUNTIME-010B2B2` is absent;
  - `RUNTIME-010B2B2A` is `Oracle complete`;
  - `RUNTIME-010B2B2A` evidence or guard cells name
    `child_exited_fallback_policy_runtime`;
  - `RUNTIME-010B2B2A` missing evidence starts with `None`;
  - `RUNTIME-010B2B2B` remains `Gap`;
  - `RUNTIME-010B2B2B` retains `quit-after-last-window-closed`,
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

  assert "RUNTIME-010B2B2" not in rows, rows.get("RUNTIME-010B2B2")
  assert len(rows) == 29, len(rows)
  assert rows["RUNTIME-010B2B2A"][5] == "Oracle complete", rows["RUNTIME-010B2B2A"]
  assert (
      "child_exited_fallback_policy_runtime" in rows["RUNTIME-010B2B2A"][6]
      or "child_exited_fallback_policy_runtime" in rows["RUNTIME-010B2B2A"][9]
  ), rows["RUNTIME-010B2B2A"]
  assert rows["RUNTIME-010B2B2A"][7].startswith("None"), rows["RUNTIME-010B2B2A"]
  assert rows["RUNTIME-010B2B2B"][5] == "Gap", rows["RUNTIME-010B2B2B"]
  behavior = rows["RUNTIME-010B2B2B"][1]
  for term in (
      "quit-after-last-window-closed",
      "quit-after-last-window-closed-delay",
      "lifecycle",
  ):
      assert term in behavior, (term, rows["RUNTIME-010B2B2B"])
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/120-child-exited-fallback-policy-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Initial adversarial review: **Changes required**.

The reviewer found that the first design overclaimed `RUNTIME-010B2B2A` because
the abnormal fallback test only required generic command/runtime details. That
would not force the implementation to preserve pinned Ghostty's fallback
headline or final instruction. The design now requires the abnormal fallback
test to assert `Ghostty failed to launch the requested command:`, the launched
command, `Runtime:`, and `Press any key to close the window.`.

The reviewer also suggested adding a build check. The design now includes
`cargo build --manifest-path roastty/Cargo.toml`.

Design re-review: **Approved**.

The reviewer confirmed the abnormal fallback finding is resolved by the specific
fallback text requirements and by storing the effective launched command label.
They also confirmed the build-check suggestion is resolved and found no new
required issues.

## Result

**Result:** Pass

Roastty now stores the parsed abnormal-exit runtime threshold per surface, keeps
it refreshed on config update, records the effective launched command label, and
routes child exits through a Ghostty-shaped branch helper. Normal exits dispatch
`show_child_exited`, write the normal fallback text only when the app does not
handle the action, and then follow the existing `wait-after-command` close/hold
policy. Abnormal exits use `runtime_ms <= abnormal-command-exit-runtime`,
dispatch `show_child_exited`, and then hold after either handled GUI action or
unhandled terminal fallback text.

The abnormal fallback text now includes the pinned Ghostty headline, launched
command, `Runtime:` label, and `Press any key to close the window.` instruction.
Both terminal fallback paths also hide the terminal cursor. The synthetic pump
helper now defaults to a normal-runtime child exit; exact threshold tests use a
custom child-exit payload so equality and above-threshold classification are
deterministic.

Verification passed:

```sh
cargo test --manifest-path roastty/Cargo.toml child_exited_fallback_policy_runtime
cargo test --manifest-path roastty/Cargo.toml child_exited_payload_runtime
cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime
cargo test --manifest-path roastty/Cargo.toml process_exited
cargo test --manifest-path roastty/Cargo.toml close_surface
cargo build --manifest-path roastty/Cargo.toml
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

assert "RUNTIME-010B2B2" not in rows, rows.get("RUNTIME-010B2B2")
assert len(rows) == 29, len(rows)
assert rows["RUNTIME-010B2B2A"][5] == "Oracle complete", rows["RUNTIME-010B2B2A"]
assert (
    "child_exited_fallback_policy_runtime" in rows["RUNTIME-010B2B2A"][6]
    or "child_exited_fallback_policy_runtime" in rows["RUNTIME-010B2B2A"][9]
), rows["RUNTIME-010B2B2A"]
assert rows["RUNTIME-010B2B2A"][7].startswith("None"), rows["RUNTIME-010B2B2A"]
assert rows["RUNTIME-010B2B2B"][5] == "Gap", rows["RUNTIME-010B2B2B"]
behavior = rows["RUNTIME-010B2B2B"][1]
for term in (
    "quit-after-last-window-closed",
    "quit-after-last-window-closed-delay",
    "lifecycle",
):
    assert term in behavior, (term, rows["RUNTIME-010B2B2B"])
cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
assert "| Gap " in cfg223, cfg223
PY
```

The inventory now reports:

```text
runtime_rows=29
oracle_complete=22
closed=23
audit_covered=0
incomplete=6
gap=6
cfg223=Gap
```

## Conclusion

`RUNTIME-010B2B2` is split. `RUNTIME-010B2B2A` is `Oracle complete` for
child-exit terminal fallback text and abnormal-command-exit-runtime close/hold
policy. `RUNTIME-010B2B2B` remains a `Gap` for `quit-after-last-window-closed`,
`quit-after-last-window-closed-delay`, and remaining app lifecycle policy
behavior.

## Completion Review

Fresh-context adversarial review: **Approved**.

The reviewer found no required issues. They independently verified that the
result had not been committed yet, reran the focused child-exit fallback,
child-exit payload, wait-after-command, process-exited, close-surface, build,
Rust format, Prettier, `git diff --check`, and matrix assertion gates, and
confirmed the generated inventory/matrix content with the read-only assertion.
