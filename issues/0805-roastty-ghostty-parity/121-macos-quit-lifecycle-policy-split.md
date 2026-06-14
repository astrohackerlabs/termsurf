# Experiment 121: macOS Quit Lifecycle Policy Split

## Description

`RUNTIME-010B2B2B` still tracks `quit-after-last-window-closed`,
`quit-after-last-window-closed-delay`, and remaining lifecycle policy behavior.
After Experiments 118 through 120, the only process-lifecycle items left in that
row are the app quit-after-last-window bridge and the configured quit delay.

Pinned Ghostty's source shows a platform split:

- on macOS, `quit-after-last-window-closed` is read through the embedded config
  getter and returned by
  `AppDelegate.applicationShouldTerminateAfterLastWindowClosed`;
- `quit-after-last-window-closed-delay` is documented in `Config.zig` as only
  implemented on Linux, and Ghostty's macOS Swift app does not consume it;
- GTK/Linux has a real quit timer implementation, but Roastty's Issue 805 target
  is the copied macOS app, not a GTK/Linux app surface.

This experiment will add a durable static guard for the copied macOS bridge and
a Rust config-getter guard for the embedded C ABI value. It will not claim broad
macOS app/window/menu lifecycle parity; those workflows remain owned by
`RUNTIME-011`.

The intended inventory result is:

- `RUNTIME-010B2B2B1`: `Oracle complete` for macOS
  `quit-after-last-window-closed` config bridging from `roastty_config_get` to
  the copied `AppDelegate.applicationShouldTerminateAfterLastWindowClosed`
  behavior.
- `RUNTIME-010B2B2B2`: `Not applicable` for
  `quit-after-last-window-closed-delay` in Roastty's copied macOS app, with
  upstream source evidence that the delay is Linux-only and the macOS app does
  not consume it.

## Changes

- `roastty/src/lib.rs`
  - Add focused `roastty_config_get` tests proving
    `quit-after-last-window-closed` returns the default `false`, parsed `true`,
    parsed reset/default `false`, and rejects invalid handles/outputs through
    the existing config-get validation pattern.
- `issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py`
  - Add a static parity checker that:
    - compares Ghostty and Roastty
      `applicationShouldTerminateAfterLastWindowClosed` after expected
      `Ghostty`/`ghostty` to `Roastty`/`roastty` renaming;
    - compares the `DerivedConfig.shouldQuitAfterLastWindowClosed` flow in
      `AppDelegate.swift`;
    - compares the Swift config getter for `shouldQuitAfterLastWindowClosed`;
    - verifies Roastty's embedded C ABI exposes `quit-after-last-window-closed`
      through `roastty_config_get`;
    - verifies pinned Ghostty documents `quit-after-last-window-closed-delay` as
      only implemented on Linux;
    - verifies neither Ghostty nor Roastty macOS Swift sources consume
      `quit-after-last-window-closed-delay`.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace `RUNTIME-010B2B2B` with `RUNTIME-010B2B2B1` and `RUNTIME-010B2B2B2`.
  - Mark `RUNTIME-010B2B2B1` `Oracle complete` only with evidence from the new
    Rust and static macOS bridge guards.
  - Mark `RUNTIME-010B2B2B2` `Not applicable` only for the copied macOS app quit
    delay, with explicit source evidence that the delay is Linux-only.
  - Update `EXPECTED_IDS` to require the new split.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that macOS quit-after-last-window parity is an app delegate
    config bridge, while the quit delay is Linux-only upstream.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The focused Rust config-getter tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml config_get_quit_after_last_window_closed_runtime
  ```

- The static macOS quit lifecycle checker passes:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py
  ```

- The checker compiles:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py
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
  - old `RUNTIME-010B2B2B` is absent;
  - `RUNTIME-010B2B2B1` is `Oracle complete`;
  - `RUNTIME-010B2B2B1` evidence or guard cells name
    `config_get_quit_after_last_window_closed_runtime` and
    `macos_quit_lifecycle_parity.py`;
  - `RUNTIME-010B2B2B1` missing evidence starts with `None`;
  - `RUNTIME-010B2B2B2` is `Not applicable`;
  - `RUNTIME-010B2B2B2` behavior or evidence names
    `quit-after-last-window-closed-delay`;
  - `CFG-223` remains `Gap` because unrelated runtime rows still remain open.

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

  assert "RUNTIME-010B2B2B" not in rows, rows.get("RUNTIME-010B2B2B")
  assert len(rows) == 30, len(rows)
  assert rows["RUNTIME-010B2B2B1"][5] == "Oracle complete", rows["RUNTIME-010B2B2B1"]
  assert "config_get_quit_after_last_window_closed_runtime" in (
      rows["RUNTIME-010B2B2B1"][6] + rows["RUNTIME-010B2B2B1"][9]
  ), rows["RUNTIME-010B2B2B1"]
  assert "macos_quit_lifecycle_parity.py" in (
      rows["RUNTIME-010B2B2B1"][6] + rows["RUNTIME-010B2B2B1"][9]
  ), rows["RUNTIME-010B2B2B1"]
  assert rows["RUNTIME-010B2B2B1"][7].startswith("None"), rows["RUNTIME-010B2B2B1"]
  assert rows["RUNTIME-010B2B2B2"][5] == "Not applicable", rows["RUNTIME-010B2B2B2"]
  assert "quit-after-last-window-closed-delay" in (
      rows["RUNTIME-010B2B2B2"][1] + rows["RUNTIME-010B2B2B2"][6]
  ), rows["RUNTIME-010B2B2B2"]
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown, Python, and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/121-macos-quit-lifecycle-policy-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Adversarial Codex subagent, fresh context, read-only review of the experiment
design and linked README entry.

**Verdict:** Approved.

Findings: none.

## Result

**Result:** Pass.

Implemented the split:

- added `config_get_quit_after_last_window_closed_runtime` in
  `roastty/src/lib.rs` to prove `roastty_config_get` exposes
  `quit-after-last-window-closed` with the macOS default `false`, parsed `true`,
  reset/default `false`, and invalid null input rejection;
- added `macos_quit_lifecycle_parity.py` to compare the pinned Ghostty and
  Roastty macOS app delegate/config getter blocks after expected app-name
  renaming, verify the embedded C ABI key exposure, and verify
  `quit-after-last-window-closed-delay` is Linux-only upstream and unused by
  Ghostty/Roastty macOS Swift sources;
- replaced the old combined `RUNTIME-010B2B2B` gap with `RUNTIME-010B2B2B1` as
  `Oracle complete` and `RUNTIME-010B2B2B2` as `Not applicable`;
- regenerated `config-runtime-inventory.md` and `config-matrix.md`, leaving
  `CFG-223` as `Gap` because unrelated runtime rows remain open;
- added the README learning that macOS quit-after-last-window parity is a narrow
  config bridge, while the quit delay is Linux/GTK-only upstream.

Verification passed:

```sh
cargo test --manifest-path roastty/Cargo.toml config_get_quit_after_last_window_closed_runtime
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/macos_quit_lifecycle_parity.py
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

assert "RUNTIME-010B2B2B" not in rows, rows.get("RUNTIME-010B2B2B")
assert len(rows) == 30, len(rows)
assert rows["RUNTIME-010B2B2B1"][5] == "Oracle complete", rows["RUNTIME-010B2B2B1"]
assert "config_get_quit_after_last_window_closed_runtime" in (
    rows["RUNTIME-010B2B2B1"][6] + rows["RUNTIME-010B2B2B1"][9]
), rows["RUNTIME-010B2B2B1"]
assert "macos_quit_lifecycle_parity.py" in (
    rows["RUNTIME-010B2B2B1"][6] + rows["RUNTIME-010B2B2B1"][9]
), rows["RUNTIME-010B2B2B1"]
assert rows["RUNTIME-010B2B2B1"][7].startswith("None"), rows["RUNTIME-010B2B2B1"]
assert rows["RUNTIME-010B2B2B2"][5] == "Not applicable", rows["RUNTIME-010B2B2B2"]
assert "quit-after-last-window-closed-delay" in (
    rows["RUNTIME-010B2B2B2"][1] + rows["RUNTIME-010B2B2B2"][6]
), rows["RUNTIME-010B2B2B2"]
cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
assert "| Gap " in cfg223, cfg223
PY
prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/121-macos-quit-lifecycle-policy-split.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md
git diff --check
```

The inventory generator reported:

```text
runtime_rows=30
oracle_complete=23
closed=25
audit_covered=0
incomplete=5
gap=5
cfg223=Gap
```

## Conclusion

The macOS `quit-after-last-window-closed` behavior is now guarded at the copied
Swift app bridge and the embedded C ABI boundary. The
`quit-after-last-window-closed-delay` runtime effect is not applicable to
Roastty's copied macOS app because pinned Ghostty documents and implements it as
Linux/GTK-only. `RUNTIME-011` still owns broader macOS app/window/menu lifecycle
walkthrough work.

## Completion Review

Adversarial Codex subagent, fresh context, read-only review of the completed
experiment, implementation diff, recorded result, and issue README status.

**Verdict:** Approved.

Findings: none.
