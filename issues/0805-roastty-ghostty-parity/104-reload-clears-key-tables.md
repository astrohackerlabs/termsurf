# Experiment 104: Reload Clears Key Tables

## Description

Experiment 103 identified `RELOAD-012` as a real CFG-222 gap. Pinned Ghostty's
`Surface.updateConfig` deactivates all active key tables during config reload
because the active table stack may point into config-owned key table data, and
because config reload should behave like ending a key sequence.

Roastty already has active key-table state, key-table deactivation notification,
and config update tests, but `Surface::apply_config` does not currently clear
`active_key_tables`. This experiment will add the missing reload side effect and
promote `RELOAD-012` from `Gap` to `Oracle complete`.

## Changes

- Update `roastty/src/lib.rs` so `Surface::apply_config` calls
  `deactivate_all_key_tables()` after applying new config-derived key data.
- Add or extend a focused unit test proving:
  - an active key table is present before config update;
  - `roastty_app_update_config` or `roastty_surface_update_config` clears the
    active key table stack;
  - the deactivation notification is emitted when a stack existed;
  - a config update with no active key tables remains a no-op for key-table
    deactivation.
- Update `issues/0805-roastty-ghostty-parity/config_reload_inventory.py` so
  `RELOAD-012` becomes `Oracle complete` and references the new test.
- Regenerate `issues/0805-roastty-ghostty-parity/config-reload-inventory.md` and
  `issues/0805-roastty-ghostty-parity/config-matrix.md`.
- Update the Experiment 103/104 issue learnings if the implementation teaches a
  durable detail useful to future reload work.

This experiment must not address `RELOAD-013`; font-size reload behavior stays
for a separate follow-up experiment.

## Verification

Pass/fail criteria:

- `Surface::apply_config` clears active key tables on reload while preserving
  the existing key-table deactivation notification semantics.
- The focused test fails without the code change and passes with it.
- `RELOAD-012` is `Oracle complete` in the generated reload inventory.
- CFG-222 remains `Gap` because `RELOAD-013` is still unresolved.

Commands:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml surface_key_table_uses_updated_app_table_storage
cargo test --manifest-path roastty/Cargo.toml surface_key_table_deactivate_actions_notify_and_noop_when_empty

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_reload_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-reload-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

inventory = Path("issues/0805-roastty-ghostty-parity/config-reload-inventory.md").read_text()
row = next(line for line in inventory.splitlines() if line.startswith("| RELOAD-012 "))
assert "| Oracle complete |" in row

matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()
cfg222 = next(line for line in matrix.splitlines() if line.startswith("| CFG-222 "))
assert "1 rows are incomplete" in cfg222
assert "1 rows are reload gaps" in cfg222
assert "| Gap " in cfg222
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_reload_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/104-reload-clears-key-tables.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-reload-inventory.md

git diff --check
```

The result is `Pass` if `RELOAD-012` is promoted and only `RELOAD-013` remains
as a CFG-222 reload gap. The result is `Partial` if key-table clearing works but
the inventory or matrix cannot be promoted. The result is `Fail` if the reload
side effect cannot be implemented without broader key-table changes.

## Design Review

Adversarial design review by fresh-context Codex subagent `Chandrasekhar`:

- **Verdict:** Approved.
- **Findings:** None.
- **Notes:** The reviewer confirmed the README links this experiment as
  `Designed`, scope is limited to `RELOAD-012`, `RELOAD-013` remains out of
  scope, the plan matches pinned Ghostty's `Surface.updateConfig` key-table
  clearing behavior, and verification keeps CFG-222 as `Gap`.

## Result

**Result:** Pass

`Surface::apply_config` now clears active key tables during config update by
calling the existing `deactivate_all_key_tables()` helper after replacing
config-derived key behavior. The focused stale-table regression test now proves:

- an active key table exists before config reload;
- `roastty_app_update_config` clears `active_key_tables`;
- `ROASTTY_KEY_TABLE_DEACTIVATE_ALL` is emitted when a stack existed;
- a second reload with no active key tables emits no key-table action;
- later key input uses the updated app key-table storage.

`RELOAD-012` is now `Oracle complete` in the generated reload inventory. CFG-222
still remains `Gap` because `RELOAD-013` is unresolved.

Verification passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml surface_key_table_uses_updated_app_table_storage
cargo test --manifest-path roastty/Cargo.toml surface_key_table_deactivate_actions_notify_and_noop_when_empty

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_reload_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-reload-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# reload_rows=14 oracle_complete=13 closed=13 audit_covered=0 incomplete=1 gap=1 cfg222=Gap
```

## Conclusion

`RELOAD-012` is closed with a Tier 1 unit guard. CFG-222 has one remaining
reload gap, `RELOAD-013`, covering configured font-size reload behavior while
preserving manual font-size adjustments.

## Completion Review

Adversarial completion review by fresh-context Codex subagent `Archimedes`:

- **Verdict:** Approved.
- **Findings:** None.
- **Verification:** The reviewer independently confirmed the implementation is
  scoped to `RELOAD-012`, matches pinned Ghostty's active key-table clearing in
  `Surface.updateConfig`, the test catches the old behavior, `RELOAD-012` is
  promoted to `Oracle complete`, CFG-222 remains `Gap` with only `RELOAD-013`
  unresolved, and the required result docs are present.
- **Independent checks:**
  `cargo fmt --manifest-path roastty/Cargo.toml --check`, both focused key-table
  tests, and `git diff --check` passed.
