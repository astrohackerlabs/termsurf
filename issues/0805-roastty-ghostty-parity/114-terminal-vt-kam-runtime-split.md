# Experiment 114: Terminal VT KAM Runtime Split

## Description

`RUNTIME-009` is currently a broad terminal behavior `Gap`. It contains one
already-proven config-driven terminal behavior, `vt-kam-allowed`, alongside
unproven terminal behaviors such as scrollback, alternate screen, shell
integration, terminfo, title reporting, and other terminal toggles.

Pinned Ghostty defines `vt-kam-allowed` as the option that allows ANSI mode 2
KAM to disable keyboard input at the application request. Roastty already has a
focused `vt_kam_allowed_*` app/runtime test family that proves both the terminal
KAM mode and the config gate, including live config update behavior and
keybinding precedence.

This experiment will split the proven VT KAM behavior out of `RUNTIME-009`
without claiming the rest of terminal runtime parity.

The intended result is:

- `RUNTIME-009A`: `Oracle complete` for `vt-kam-allowed` terminal key gating.
- `RUNTIME-009B`: `Gap` for scrollback, alternate screen, shell integration,
  terminfo, title reporting, and remaining terminal behavior toggles.

## Changes

- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace broad `RUNTIME-009` with narrower `RUNTIME-009A` and `RUNTIME-009B`
    rows.
  - Update `EXPECTED_IDS` to require the new row split.
  - Mark `RUNTIME-009A` `Oracle complete` only with evidence from the
    `vt_kam_allowed_*` runtime guard family.
  - Keep `RUNTIME-009B` as `Gap` with explicit missing evidence for the
    remaining terminal behavior surface.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that terminal runtime rows should separate protocol-mode
    toggles from broader terminal/app integration gaps.
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

- Focused VT KAM runtime tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml vt_kam_allowed
  ```

- A matrix assertion proves:
  - old `RUNTIME-009` is absent;
  - `RUNTIME-009A` is `Oracle complete`;
  - `RUNTIME-009A` evidence and guard cells name the `vt_kam_allowed` guard;
  - `RUNTIME-009B` remains `Gap`;
  - `RUNTIME-009B` retains scrollback, alternate screen, shell integration,
    terminfo, and title reporting;
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

  assert "RUNTIME-009" not in rows, rows["RUNTIME-009"]
  assert len(rows) == 23, len(rows)
  assert rows["RUNTIME-009A"][5] == "Oracle complete", rows["RUNTIME-009A"]
  assert "vt_kam_allowed" in rows["RUNTIME-009A"][6], rows["RUNTIME-009A"]
  assert "vt_kam_allowed" in rows["RUNTIME-009A"][9], rows["RUNTIME-009A"]
  assert rows["RUNTIME-009A"][7].startswith("None"), rows["RUNTIME-009A"]
  assert rows["RUNTIME-009B"][5] == "Gap", rows["RUNTIME-009B"]
  behavior = rows["RUNTIME-009B"][1]
  for term in ("scrollback", "alternate screen", "shell integration", "terminfo", "title reporting"):
      assert term in behavior, (term, rows["RUNTIME-009B"])
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/114-terminal-vt-kam-runtime-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Fresh-context Codex adversarial reviewer `Aristotle` initially returned
**CHANGES REQUIRED**:

- **Required:** verification did not prove that old broad `RUNTIME-009` was
  removed, so a bad implementation could keep stale terminal coverage while
  adding `RUNTIME-009A` and `RUNTIME-009B`.
- **Optional:** verification only checked `RUNTIME-009A` status and did not
  assert that the pass row's evidence and guard cells name the `vt_kam_allowed`
  guard.

Fix:

- The matrix assertion now requires old `RUNTIME-009` to be absent and the
  expected post-split row count to be present.
- The matrix assertion now requires `RUNTIME-009A` evidence and guard cells to
  name `vt_kam_allowed`, and its missing-evidence cell to start with `None`.

Re-review verdict: **Approved**. Fresh-context reviewer `Hubble` confirmed the
required stale-row check and optional evidence/guard assertions were resolved,
with no new required findings.

## Result

**Result:** Pass

Split the broad terminal runtime row into two rows:

- `RUNTIME-009A` is `Oracle complete` for `vt-kam-allowed` terminal key gating.
- `RUNTIME-009B` remains `Gap` for scrollback, alternate screen, shell
  integration, terminfo, title reporting, and remaining terminal behavior
  effects.

The regenerated runtime inventory now reports 23 runtime rows, 16
oracle-complete rows, 17 closed rows, and 6 gap rows. `CFG-223` remains `Gap`,
as intended.

Verification passed:

```text
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=23 oracle_complete=16 closed=17 audit_covered=0 incomplete=6 gap=6 cfg223=Gap

cargo test --manifest-path roastty/Cargo.toml vt_kam_allowed
# 6 passed
```

The matrix assertion also passed, proving old `RUNTIME-009` is absent,
`RUNTIME-009A` names the `vt_kam_allowed` guard in its evidence and guard cells,
`RUNTIME-009B` remains `Gap`, and `CFG-223` remains `Gap`.

## Conclusion

`vt-kam-allowed` now has a narrow Tier 2 runtime guard instead of being buried
inside a broad terminal gap. The remaining terminal runtime work is explicitly
tracked by `RUNTIME-009B`.

## Completion Review

Fresh-context Codex reviewer `Maxwell` returned **Approved** with no findings.

The reviewer verified:

- the README records Experiment 114 as **Pass**;
- this experiment file has `## Result` and `## Conclusion`;
- `HEAD` was still the Experiment 114 plan commit before the result commit;
- old `RUNTIME-009` is absent and `RUNTIME-009A` / `RUNTIME-009B` are present;
- `RUNTIME-009A` is `Oracle complete` and names `vt_kam_allowed` in evidence and
  guard cells;
- `RUNTIME-009B` remains `Gap` and keeps scrollback, alternate screen, shell
  integration, terminfo, title reporting, and remaining terminal behavior open;
- `CFG-223` remains `Gap`;
- generated counts match the recorded result;
- `/tmp` regeneration, Python matrix assertions, `prettier --check`,
  `git diff --check`, and
  `cargo test --manifest-path roastty/Cargo.toml vt_kam_allowed` passed.
