# Experiment 103: Config Reload Inventory

## Description

CFG-222 is still a gap because the previous config experiments prove parsing,
formatting, diagnostics, finalization, and load/source precedence, but they do
not prove live reload behavior. Pinned Ghostty reload has a separate behavioral
surface: conditional reload replays already-loaded configuration without
re-reading files, app and surface reload paths apply conditional state at the
right level, and surface reload preserves or resets selected live state.

This experiment will create a generated reload inventory for the pinned Ghostty
commit and classify the corresponding Roastty coverage row by row. It is an
inventory experiment, not a broad implementation pass: code changes are allowed
only for the inventory generator, generated matrix/docs, and narrow tests needed
to promote existing reload behavior to oracle-complete status if the audit finds
the behavior is already implemented.

The initial reload manifest is:

- `RELOAD-001`: irrelevant conditional changes return no new config;
- `RELOAD-002`: relevant theme conditional changes replay config and finalize a
  new config;
- `RELOAD-003`: conditional reload preserves replay entries without duplication;
- `RELOAD-004`: theme reload applies theme values first and user config values
  on top;
- `RELOAD-005`: theme reload failure reports the failure while preserving
  window-theme conditional finalization semantics;
- `RELOAD-006`: app color-scheme changes update app conditional state and
  request a soft app reload;
- `RELOAD-007`: app config update applies app conditional state before storing
  app-level parsed config and propagating to surfaces;
- `RELOAD-008`: new surfaces inherit the app conditional state while preserving
  their launch working directory;
- `RELOAD-009`: surface color-scheme changes update surface conditional state
  and request a soft surface reload;
- `RELOAD-010`: surface config update applies surface conditional state;
- `RELOAD-011`: surface config update propagates reloadable terminal/runtime
  state such as palette, selection, mouse, paste, and key remap settings;
- `RELOAD-012`: surface reload clears active key tables because table stack
  entries point into config-owned data in Ghostty;
- `RELOAD-013`: surface reload preserves a manually adjusted font size while an
  unadjusted surface follows the configured font size;
- `RELOAD-014`: hard reload action is represented distinctly from soft reload
  and asks the app layer to re-read config rather than replay only conditional
  state.

Rows may be split or renamed during implementation if the source audit proves a
cleaner upstream boundary, but the inventory must remain explicit about each
pinned Ghostty reload behavior and may not mark CFG-222 `Pass` until every row
is `Oracle complete`, `Not applicable`, or an accepted documented divergence.

## Changes

- Add `issues/0805-roastty-ghostty-parity/config_reload_inventory.py` to
  generate the reload inventory from a manifest with pinned Ghostty source
  anchors, Roastty evidence anchors, status, guard tier, guard command, and
  notes.
- Add generated `issues/0805-roastty-ghostty-parity/config-reload-inventory.md`.
- Update `issues/0805-roastty-ghostty-parity/config-matrix.md` row `CFG-222` to
  point at the reload inventory, keep it `Gap` until all reload rows are
  complete, and report row counts.
- Update `issues/0805-roastty-ghostty-parity/README.md` learnings with any
  durable reload findings from the completed inventory.
- If the audit finds already-implemented Roastty behavior without an adequate
  guard, add focused unit tests in `roastty/src/config/mod.rs` or
  `roastty/src/lib.rs` only for that specific row.

## Verification

Pass/fail criteria:

- The generated reload inventory includes every manifest row above, with pinned
  Ghostty source anchors and Roastty status for each row.
- The generator fails if a manifest row has an invalid status, missing guard
  field, missing evidence anchor, or if `CFG-222` is marked `Pass` while any
  reload row is incomplete.
- Existing CFG-217 through CFG-221 matrix rows remain unchanged.
- If source tests are added, they are focused on the promoted reload rows and
  pass under `cargo test --manifest-path roastty/Cargo.toml <test-filter>`.

Commands:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_reload_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-reload-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()
line = next(row for row in matrix.splitlines() if row.startswith("| CFG-222 "))
assert "config-reload-inventory.md" in line
assert ("| Pass " in line) == (
    "0 rows are incomplete" in line and "0 rows are reload gaps" in line
)
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_reload_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

cargo fmt --manifest-path roastty/Cargo.toml --check
cargo test --manifest-path roastty/Cargo.toml config_conditional_theme
cargo test --manifest-path roastty/Cargo.toml app_set_color_scheme
cargo test --manifest-path roastty/Cargo.toml surface_set_color_scheme
cargo test --manifest-path roastty/Cargo.toml surface_update_config
cargo test --manifest-path roastty/Cargo.toml surface_apply_config

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/103-config-reload-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-reload-inventory.md

git diff --check
```

The result is `Pass` if the inventory is generated, validated, and closes
CFG-222 with every reload row complete, not applicable, or accepted as a
documented divergence. The result is `Partial` if the inventory exists but
identifies one or more unresolved reload gaps. The result is `Fail` if the
inventory cannot be grounded in pinned Ghostty source anchors or cannot be
checked deterministically.

## Design Review

Adversarial design review by fresh-context Codex subagent `Kierkegaard`:

- **Initial verdict:** Changes required.
- **Required finding:** The pass criteria contradicted the experiment's own
  unresolved-gap outcome by allowing `Pass` when follow-up reload gaps remain.
- **Fix:** The pass criteria now require every reload row to be complete, not
  applicable, or accepted as a documented divergence before CFG-222 can close.
- **Optional finding:** Add an explicit matrix assertion after regeneration.
- **Fix:** The verification commands now assert that CFG-222 points at
  `config-reload-inventory.md` and is not marked `Pass` unless the generated row
  counts report zero incomplete rows and zero gaps.
- **Re-review verdict:** Approved. The reviewer confirmed both prior findings
  are resolved and no new required findings were introduced.

## Result

**Result:** Partial

The reload inventory was generated and wired into `config-matrix.md`. It records
14 pinned Ghostty reload rows:

- 12 rows are `Oracle complete`;
- 0 rows are `Audit covered`;
- 2 rows are `Gap`;
- CFG-222 remains `Gap`.

The two unresolved reload gaps are:

- `RELOAD-012`: Roastty surface config update does not clear active key tables
  like pinned Ghostty's `Surface.updateConfig` does.
- `RELOAD-013`: Roastty surface config update does not apply configured
  `font-size` for unadjusted surfaces or prove preservation of manually adjusted
  font size.

Verification passed:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_reload_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-reload-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# reload_rows=14 oracle_complete=12 closed=12 audit_covered=0 incomplete=2 gap=2 cfg222=Gap

PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()
line = next(row for row in matrix.splitlines() if row.startswith("| CFG-222 "))
assert "config-reload-inventory.md" in line
assert ("| Pass " in line) == (
    "0 rows are incomplete" in line and "0 rows are reload gaps" in line
)
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_reload_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

cargo fmt --manifest-path roastty/Cargo.toml --check
cargo test --manifest-path roastty/Cargo.toml config_conditional_theme
cargo test --manifest-path roastty/Cargo.toml app_set_color_scheme
cargo test --manifest-path roastty/Cargo.toml surface_set_color_scheme
cargo test --manifest-path roastty/Cargo.toml surface_update_config
cargo test --manifest-path roastty/Cargo.toml surface_apply_config

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/103-config-reload-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-reload-inventory.md

git diff --check
```

Focused Rust results:

- `config_conditional_theme`: 7 passed.
- `app_set_color_scheme`: 1 passed.
- `surface_set_color_scheme`: 1 passed.
- `surface_update_config`: 5 passed.
- `surface_apply_config`: 2 passed.

## Conclusion

CFG-222 now has a durable reload parity manifest and matrix guard, but it cannot
close yet. The next experiment should fix `RELOAD-012` first because clearing
active key tables on config reload is a narrow surface-state behavior with a
clear upstream source anchor and an existing Roastty key-table test harness.

## Completion Review

Adversarial completion review by fresh-context Codex subagent `Mendel`:

- **Initial verdict:** Changes required.
- **Required finding:** The generator's row ID validation was tautological
  because IDs were derived from row position instead of row data.
- **Fix:** `ReloadRow` now has an explicit `id` field, every manifest row stores
  its `RELOAD-00N` ID, validation compares row IDs to `EXPECTED_IDS`, duplicate
  detection uses row IDs, and generated row output emits `row.id`.
- **Re-review verdict:** Approved. The reviewer confirmed the prior finding is
  resolved and no new required findings were introduced.
