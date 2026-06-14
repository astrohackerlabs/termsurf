# Experiment 118: Wait After Command Runtime Split

## Description

`RUNTIME-010B2` still combines several process lifecycle behaviors:

- `wait-after-command`;
- `abnormal-command-exit-runtime`;
- `quit-after-last-window-closed`;
- related app lifecycle policy behavior.

Pinned Ghostty handles the first item in `vendor/ghostty/src/Surface.zig`: when
the child exits, Ghostty marks the child as exited, emits a terminal/native
message, and then either holds the surface open if `wait_after_command` is true
or closes the surface immediately when it is false. The embedded surface options
also force `wait-after-command = true` when a surface command is supplied or
when the explicit embedded `wait_after_command` option is true.

Roastty already exposes `RoasttySurfaceConfig.wait_after_command` and parses
config `wait-after-command`, but the current runtime gap does not prove that
child-exit close/hold behavior matches Ghostty. This experiment will split out
that narrow lifecycle slice and leave abnormal-exit UI and app quit policy in a
remaining gap.

The runtime tests for this experiment must avoid Ghostty's abnormal-exit branch:
pinned Ghostty checks `abnormal-command-exit-runtime` before
`wait_after_command` and uses a `<=` predicate. To keep this experiment scoped
to the normal child-exit close/hold branch, each command-exit test will set a
small explicit threshold and run a command that sleeps long enough to guarantee
the observed child exit is outside the abnormal-exit predicate.

The intended inventory result is:

- `RUNTIME-010B2A`: `Oracle complete` for `wait-after-command` child-exit
  close/hold behavior from parsed config and embedded per-surface config.
- `RUNTIME-010B2B`: `Gap` for `abnormal-command-exit-runtime`,
  `quit-after-last-window-closed`, `quit-after-last-window-closed-delay`, and
  remaining lifecycle policy behavior.

## Changes

- `roastty/src/lib.rs`
  - Add runtime state to each surface for the effective `wait-after-command`
    value.
  - Initialize that state from parsed app config and per-surface
    `RoasttySurfaceConfig.wait_after_command`.
  - Preserve Ghostty's embedded behavior that an explicit per-surface command
    holds the surface open after the child exits.
  - On terminal worker child exit:
    - mark `process_exited`;
    - keep the surface open when the effective wait-after-command state is true;
    - request surface close through `close_surface_cb` when the effective state
      is false;
    - avoid duplicate close requests if multiple terminal child-exit events are
      observed.
  - Do not treat EOF-only worker events as equivalent to Ghostty child-exit
    messages in this experiment.
  - Add focused PTY-backed tests proving:
    - default parsed config requests close after a child-exited event from a
      command whose runtime exceeds the configured
      `abnormal-command-exit-runtime` threshold;
    - parsed config `wait-after-command = true` holds the surface open after the
      child exits outside the configured abnormal-exit threshold;
    - `RoasttySurfaceConfig.wait_after_command = true` holds the surface open
      after the child exits outside the configured abnormal-exit threshold;
    - an explicit per-surface command holds the surface open even when
      `wait_after_command` is false, matching Ghostty embedded option behavior,
      after the child exits outside the configured abnormal-exit threshold;
    - `roastty_surface_process_exited` still reports true after the held child
      exit.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace `RUNTIME-010B2` with `RUNTIME-010B2A` and `RUNTIME-010B2B`.
  - Update `EXPECTED_IDS` to require the new split.
  - Mark `RUNTIME-010B2A` `Oracle complete` only with evidence from the new
    wait-after-command child-exit close/hold tests.
  - Keep `RUNTIME-010B2B` as `Gap` with explicit missing evidence for
    abnormal-exit UI, quit-after-last-window-closed, quit delay, and remaining
    lifecycle policy behavior.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that process lifecycle parity is being split into child-exit
    close/hold behavior versus abnormal-exit and app quit policy.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The focused wait-after-command runtime tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime
  ```

  These tests must configure a bounded `abnormal-command-exit-runtime`
  threshold, run child commands that outlive that threshold, and assert that the
  observed close/hold behavior is triggered by `pump.child_exited`, not an
  EOF-only event or the abnormal-exit branch.

- Existing close-surface and process-exit behavior still passes:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml close_surface
  cargo test --manifest-path roastty/Cargo.toml process_exited
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
  - old `RUNTIME-010B2` is absent;
  - `RUNTIME-010B2A` is `Oracle complete`;
  - `RUNTIME-010B2A` evidence or guard cells name `wait_after_command_runtime`;
  - `RUNTIME-010B2A` missing evidence starts with `None`;
  - `RUNTIME-010B2B` remains `Gap`;
  - `RUNTIME-010B2B` retains `abnormal-command-exit-runtime`,
    `quit-after-last-window-closed`, `quit-after-last-window-closed-delay`, and
    lifecycle policy behavior;
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

  assert "RUNTIME-010B2" not in rows, rows.get("RUNTIME-010B2")
  assert len(rows) == 27, len(rows)
  assert rows["RUNTIME-010B2A"][5] == "Oracle complete", rows["RUNTIME-010B2A"]
  assert (
      "wait_after_command_runtime" in rows["RUNTIME-010B2A"][6]
      or "wait_after_command_runtime" in rows["RUNTIME-010B2A"][9]
  ), rows["RUNTIME-010B2A"]
  assert rows["RUNTIME-010B2A"][7].startswith("None"), rows["RUNTIME-010B2A"]
  assert rows["RUNTIME-010B2B"][5] == "Gap", rows["RUNTIME-010B2B"]
  behavior = rows["RUNTIME-010B2B"][1]
  for term in (
      "abnormal-command-exit-runtime",
      "quit-after-last-window-closed",
      "quit-after-last-window-closed-delay",
      "lifecycle",
  ):
      assert term in behavior, (term, rows["RUNTIME-010B2B"])
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/118-wait-after-command-runtime-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Initial adversarial review: **Changes required**.

The reviewer found two real issues. First, the original test plan used
short-lived commands without controlling `abnormal-command-exit-runtime`, so the
tests could exercise Ghostty's abnormal-exit branch instead of the scoped normal
`wait-after-command` branch. Second, the original implementation plan treated
EOF-only worker events as equivalent to Ghostty child-exit messages without
evidence. The design now requires focused tests to run beyond the configured
abnormal-exit threshold and narrows the implementation to `pump.child_exited`
events only.

Second adversarial review: **Changes required**.

The reviewer confirmed the EOF finding was resolved, but found that using
`abnormal-command-exit-runtime = 0` was still insufficient because Ghostty uses
a `<=` predicate, so a same-millisecond child exit could remain in the abnormal
branch. That was accepted as a real finding. The design now requires commands
that run long enough to exceed the configured abnormal-exit threshold before
asserting normal `wait-after-command` close/hold behavior.

Design re-review: **Approved**.

The reviewer confirmed the threshold finding is resolved because the design now
accounts for Ghostty's `<=` predicate and requires focused tests to configure a
bounded abnormal-exit threshold and run child commands that outlive it. The
reviewer also confirmed the EOF-only finding remains resolved, the README links
Experiment 118 as `Designed`, and no plan commit had been made before approval.

## Result

**Result:** Pass

Roastty now tracks the effective wait-after-command state per surface and uses
it when terminal worker child-exit events arrive:

- parsed app config `wait-after-command = true` holds a surface open after the
  child exits;
- embedded `RoasttySurfaceConfig.wait_after_command = true` holds a surface open
  after the child exits;
- explicit embedded per-surface commands force hold behavior, matching pinned
  Ghostty's embedded option path;
- default parsed config requests surface close on normal child exit;
- EOF-only worker events do not request close and do not suppress a later
  child-exit close request;
- repeated child-exit events request close only once.

The PTY-backed command-exit tests configure `abnormal-command-exit-runtime = 1`
and use commands that sleep before exiting, so the assertions prove the normal
child-exit close/hold branch rather than Ghostty's earlier abnormal-exit branch.

The runtime inventory now splits `RUNTIME-010B2`:

- `RUNTIME-010B2A` is `Oracle complete` for normal `wait-after-command`
  child-exit close/hold behavior.
- `RUNTIME-010B2B` remains `Gap` for `abnormal-command-exit-runtime`,
  `quit-after-last-window-closed`, `quit-after-last-window-closed-delay`, and
  remaining lifecycle policy behavior.

Verification run:

```sh
cargo test --manifest-path roastty/Cargo.toml wait_after_command_runtime
cargo test --manifest-path roastty/Cargo.toml close_surface
cargo test --manifest-path roastty/Cargo.toml process_exited
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

assert "RUNTIME-010B2" not in rows, rows.get("RUNTIME-010B2")
assert len(rows) == 27, len(rows)
assert rows["RUNTIME-010B2A"][5] == "Oracle complete", rows["RUNTIME-010B2A"]
assert (
    "wait_after_command_runtime" in rows["RUNTIME-010B2A"][6]
    or "wait_after_command_runtime" in rows["RUNTIME-010B2A"][9]
), rows["RUNTIME-010B2A"]
assert rows["RUNTIME-010B2A"][7].startswith("None"), rows["RUNTIME-010B2A"]
assert rows["RUNTIME-010B2B"][5] == "Gap", rows["RUNTIME-010B2B"]
behavior = rows["RUNTIME-010B2B"][1]
for term in (
    "abnormal-command-exit-runtime",
    "quit-after-last-window-closed",
    "quit-after-last-window-closed-delay",
    "lifecycle",
):
    assert term in behavior, (term, rows["RUNTIME-010B2B"])
cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
assert "| Gap " in cfg223, cfg223
PY
prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/118-wait-after-command-runtime-split.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md
git diff --check
```

All commands passed after formatting the regenerated markdown tables.

Initial completion review: **Changes required**.

The reviewer found that the regenerated `config-matrix.md` and
`config-runtime-inventory.md` files were not Prettier-clean in the reviewed
working tree. That was accepted as a real workflow finding. The regenerated
markdown tables were formatted with Prettier again, and `prettier --check` now
passes for the README, this experiment file, `config-matrix.md`, and
`config-runtime-inventory.md`.

Completion re-review: **Approved**.

The reviewer confirmed the Prettier finding is resolved, the experiment file
records the initial completion-review finding and fix, and no result commit had
been made before the re-review approval. No required findings remain.

## Conclusion

The normal `wait-after-command` child-exit close/hold branch is no longer part
of the process lifecycle runtime gap. The remaining process lifecycle gap is
abnormal-exit presentation, quit-after-last-window-closed policy, quit delay,
and other app lifecycle behavior that needs focused runtime or GUI proof.
