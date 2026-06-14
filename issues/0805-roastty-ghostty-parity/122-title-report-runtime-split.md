# Experiment 122: Title Report Runtime Split

## Description

`RUNTIME-009B2` still bundles exact nonzero scrollback byte quota, shell
integration, terminfo, title reporting, and remaining terminal behavior effects.
Pinned Ghostty exposes `title-report` as a security-sensitive config option:
`Config.zig` defaults it to `false`, and `Surface.zig` drops report-title
requests unless `self.config.title_report` is enabled.

Roastty already parses and formats `title-report`, and its terminal stores OSC
0/2 window titles. The focused runtime gap for this experiment is that CSI `21t`
report handling for OSC-driven terminal titles must be gated by parsed config
and refresh when the app/surface config updates. It will not claim configured
static surface-title reporting parity; that remains in the remaining terminal/UI
gap because pinned Ghostty reports the runtime surface title and static
configured titles have additional app-surface behavior.

This experiment will split the title-report slice out of `RUNTIME-009B2`, fix
Roastty's runtime gate if needed, and leave the rest of terminal behavior in a
remaining `RUNTIME-009B2B` gap.

The intended inventory result is:

- `RUNTIME-009B2A`: `Oracle complete` for the `title-report` CSI `21t` runtime
  gate for OSC-driven terminal titles.
- `RUNTIME-009B2B`: `Gap` for exact nonzero scrollback byte quota, shell
  integration, terminfo, configured/static title-report surface-title behavior,
  and remaining terminal behavior effects.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add terminal state for whether CSI `21t` title reports are allowed.
  - Initialize that state from `TerminalInitOptions`.
  - Add a setter so app/surface config updates can refresh the gate on existing
    PTY-backed surfaces.
  - Change the `Csi21T` size-report branch to write `ESC ] l <title> ESC \` only
    when title reporting is enabled; when disabled, it should write no PTY
    response.
  - Add terminal-core tests proving disabled-by-default CSI `21t` produces no
    response, enabled CSI `21t` reports the current title, and toggling the gate
    at runtime enables and disables future reports without losing the stored
    title.
- `roastty/src/termio.rs`
  - Add `title_report` to `TermioSpawnOptions`.
  - Thread that value into `TerminalInitOptions` when PTY-backed terminals are
    created, so startup parsed config reaches the actual terminal instance.
- `roastty/src/lib.rs`
  - Pass parsed `title_report` through `TermioSpawnOptions` at surface startup.
  - Refresh the terminal title-report gate from parsed config in
    `Surface::apply_config`.
  - Add a PTY-backed parsed-config runtime test proving:
    - default `title-report = false` suppresses CSI `21t`;
    - parsed `title-report = true` reports the current OSC title;
    - `roastty_app_update_config` can disable and re-enable title reporting on
      an existing surface.
- `issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py`
  - Add a static checker that verifies pinned Ghostty's config default,
    `Surface.zig` title-report gate, Roastty's parser field, Roastty terminal
    gate, `TermioSpawnOptions` startup wiring, and Roastty app/surface config
    update wiring.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace `RUNTIME-009B2` with `RUNTIME-009B2A` and `RUNTIME-009B2B`.
  - Mark `RUNTIME-009B2A` `Oracle complete` only for the CSI `21t` gate for
    OSC-driven terminal titles, with evidence from the new terminal-core,
    PTY-backed runtime, and static parity guards.
  - Leave `RUNTIME-009B2B` as `Gap` for the configured/static surface-title
    reporting behavior and the remaining terminal behaviors.
  - Update `EXPECTED_IDS` to require the split.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that `title-report` is gated at Ghostty's surface config
    layer and must remain off by default because it can expose the terminal
    title.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The focused terminal-core tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml terminal_stream_title_report
  ```

- The PTY-backed parsed-config runtime tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml config_title_report_runtime
  ```

- The static title-report checker passes:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py
  ```

- The checker compiles:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py
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
  - old `RUNTIME-009B2` is absent;
  - `RUNTIME-009B2A` is `Oracle complete`;
  - `RUNTIME-009B2A` evidence or guard cells name
    `terminal_stream_title_report`, `config_title_report_runtime`, and
    `title_report_runtime_parity.py`;
  - `RUNTIME-009B2A` missing evidence starts with `None`;
  - `RUNTIME-009B2B` is `Gap`;
  - `RUNTIME-009B2B` behavior mentions `shell integration`, `terminfo`, and
    `scrollback`;
  - `RUNTIME-009B2B` behavior or missing-evidence text mentions configured
    static title-report behavior;
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

  assert "RUNTIME-009B2" not in rows, rows.get("RUNTIME-009B2")
  assert rows["RUNTIME-009B2A"][5] == "Oracle complete", rows["RUNTIME-009B2A"]
  assert "terminal_stream_title_report" in (
      rows["RUNTIME-009B2A"][6] + rows["RUNTIME-009B2A"][9]
  ), rows["RUNTIME-009B2A"]
  assert "config_title_report_runtime" in (
      rows["RUNTIME-009B2A"][6] + rows["RUNTIME-009B2A"][9]
  ), rows["RUNTIME-009B2A"]
  assert "title_report_runtime_parity.py" in (
      rows["RUNTIME-009B2A"][6] + rows["RUNTIME-009B2A"][9]
  ), rows["RUNTIME-009B2A"]
  assert rows["RUNTIME-009B2A"][7].startswith("None"), rows["RUNTIME-009B2A"]
  assert rows["RUNTIME-009B2B"][5] == "Gap", rows["RUNTIME-009B2B"]
  remaining = rows["RUNTIME-009B2B"][1]
  assert "shell integration" in remaining, remaining
  assert "terminfo" in remaining, remaining
  assert "scrollback" in remaining, remaining
  assert "configured/static" in (
      rows["RUNTIME-009B2B"][1] + rows["RUNTIME-009B2B"][7]
  ), rows["RUNTIME-009B2B"]
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown, Python, and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/122-title-report-runtime-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Adversarial Codex subagent, fresh context, read-only review of the experiment
design and linked README entry.

Initial verdict: changes required.

- Required: The design overclaimed `Oracle complete` for title-report runtime
  effects while only testing OSC-driven title reporting. Fixed by narrowing
  `RUNTIME-009B2A` to the CSI `21t` gate for OSC-driven terminal titles and
  leaving configured/static surface-title reporting in `RUNTIME-009B2B`.
- Required: The design omitted the PTY startup path through
  `TermioSpawnOptions`. Fixed by adding `roastty/src/termio.rs` to the planned
  changes and requiring startup wiring through `TermioSpawnOptions` into
  `TerminalInitOptions`.

Re-review verdict: approved.

Findings after fixes: none.

## Result

**Result:** Pass.

Implemented the split and runtime gate:

- added `Terminal.title_report`, initialization through `TerminalInitOptions`,
  and a `set_title_report` live-update setter;
- changed CSI `21t` handling so it writes no PTY response unless title reporting
  is enabled;
- threaded `title_report` through `TermioSpawnOptions` into PTY-backed terminal
  startup;
- refreshed the terminal title-report gate from parsed config in
  `Surface::apply_config`;
- added terminal-core guards for disabled-by-default, enabled response, and
  runtime toggling behavior;
- added a PTY-backed parsed-config guard proving startup config and
  `roastty_app_update_config` both control future CSI `21t` reports;
- added `title_report_runtime_parity.py` to pin the Ghostty default/gate and
  Roastty parser, terminal, startup, and live-update wiring;
- split `RUNTIME-009B2` into `RUNTIME-009B2A` as `Oracle complete` for the
  OSC-driven CSI `21t` gate and `RUNTIME-009B2B` as the remaining terminal gap.

Verification passed:

```sh
cargo test --manifest-path roastty/Cargo.toml terminal_stream_title_report
cargo test --manifest-path roastty/Cargo.toml config_title_report_runtime
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/title_report_runtime_parity.py
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

assert "RUNTIME-009B2" not in rows, rows.get("RUNTIME-009B2")
assert rows["RUNTIME-009B2A"][5] == "Oracle complete", rows["RUNTIME-009B2A"]
assert "terminal_stream_title_report" in (
    rows["RUNTIME-009B2A"][6] + rows["RUNTIME-009B2A"][9]
), rows["RUNTIME-009B2A"]
assert "config_title_report_runtime" in (
    rows["RUNTIME-009B2A"][6] + rows["RUNTIME-009B2A"][9]
), rows["RUNTIME-009B2A"]
assert "title_report_runtime_parity.py" in (
    rows["RUNTIME-009B2A"][6] + rows["RUNTIME-009B2A"][9]
), rows["RUNTIME-009B2A"]
assert rows["RUNTIME-009B2A"][7].startswith("None"), rows["RUNTIME-009B2A"]
assert rows["RUNTIME-009B2B"][5] == "Gap", rows["RUNTIME-009B2B"]
remaining = rows["RUNTIME-009B2B"][1]
assert "shell integration" in remaining, remaining
assert "terminfo" in remaining, remaining
assert "scrollback" in remaining, remaining
assert "configured/static" in (
    rows["RUNTIME-009B2B"][1] + rows["RUNTIME-009B2B"][7]
), rows["RUNTIME-009B2B"]
cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
assert "| Gap " in cfg223, cfg223
PY
prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/122-title-report-runtime-split.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md
git diff --check
```

The inventory generator reported:

```text
runtime_rows=31
oracle_complete=24
closed=26
audit_covered=0
incomplete=5
gap=5
cfg223=Gap
```

## Conclusion

Roastty now matches pinned Ghostty's default-off `title-report` behavior for the
OSC-driven CSI `21t` title-report path and keeps the gate live-updatable for
existing PTY-backed surfaces. Configured/static surface-title reporting remains
in `RUNTIME-009B2B` because pinned Ghostty reports the runtime surface title,
not merely the terminal's OSC title state.

## Completion Review

Adversarial Codex subagent, fresh context, read-only review of the completed
experiment, implementation diff, recorded result, and issue README status.

**Verdict:** Approved.

- Optional: The result verification log listed `prettier --write` and omitted
  `git diff --check`. Fixed by recording the hygiene commands as
  `prettier --check` plus `git diff --check`.
