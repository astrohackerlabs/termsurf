# Experiment 105: Reload Font Size

## Description

Experiment 104 left `RELOAD-013` as the only CFG-222 reload gap. Pinned
Ghostty's `Surface.updateConfig` rebuilds the font grid on config reload and
chooses the point size as follows:

- if the surface font size was manually adjusted, preserve the current font
  size;
- otherwise, adopt the newly configured `font-size`, clamped to `1.0..255.0`.

Roastty already tracks `font_size_points`, `original_font_size_points`, and
`font_size_adjusted`, and manual font-size actions already update that flag.
This experiment will add the missing reload behavior in `Surface::apply_config`
and close CFG-222 if all reload inventory rows become complete.

## Changes

- Update `roastty/src/lib.rs` so `Surface::apply_config` applies
  `parsed.font_size.clamp(1.0, 255.0)` on config update only when
  `font_size_adjusted` is false.
- Update `original_font_size_points` from the reloaded configured font size so
  later `reset-font-size` targets the new config value, matching Ghostty's
  replaced `DerivedConfig.original_font_size`.
- Preserve the current `font_size_points` when `font_size_adjusted` is true.
- Add focused unit coverage proving:
  - unadjusted surfaces adopt the reloaded configured font size;
  - configured reload font sizes are clamped to `1.0..255.0`;
  - manually adjusted surfaces preserve their current font size across config
    reload;
  - after manual adjustment plus config reload, reset-font-size resets to the
    newly reloaded configured font size and clears the manual flag;
  - after reset-font-size clears the manual flag, a later config reload can
    adopt configured font size again.
- Update `issues/0805-roastty-ghostty-parity/config_reload_inventory.py` so
  `RELOAD-013` becomes `Oracle complete`.
- Regenerate `issues/0805-roastty-ghostty-parity/config-reload-inventory.md` and
  `issues/0805-roastty-ghostty-parity/config-matrix.md`.
- Update the Issue 805 learnings with the completed reload-font-size rule.

## Verification

Pass/fail criteria:

- `Surface::apply_config` matches pinned Ghostty's reload font-size selection
  rule, including the reset-font-size target update caused by Ghostty replacing
  `DerivedConfig.original_font_size` during config update.
- The focused test fails without the implementation change and passes with it.
- `RELOAD-013` is `Oracle complete` in the generated reload inventory.
- CFG-222 becomes `Pass` with 14 closed reload rows, 0 incomplete rows, and 0
  reload gaps.

Commands:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size
cargo test --manifest-path roastty/Cargo.toml surface_key_table_uses_updated_app_table_storage

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_reload_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-reload-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

inventory = Path("issues/0805-roastty-ghostty-parity/config-reload-inventory.md").read_text()
row = next(line for line in inventory.splitlines() if line.startswith("| RELOAD-013 "))
assert "| Oracle complete |" in row

matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()
cfg222 = next(line for line in matrix.splitlines() if line.startswith("| CFG-222 "))
assert "14 rows closed" in cfg222
assert "0 rows are incomplete" in cfg222
assert "0 rows are reload gaps" in cfg222
assert "| Pass " in cfg222
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_reload_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/105-reload-font-size.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-reload-inventory.md

git diff --check
```

The result is `Pass` if `RELOAD-013` is promoted and CFG-222 becomes `Pass`. The
result is `Partial` if the runtime behavior works but the inventory or matrix
cannot be promoted. The result is `Fail` if the reload font-size rule cannot be
implemented without broader renderer/font changes.

## Design Review

Adversarial design review by fresh-context Codex subagent `Nash`:

- **Initial verdict:** Changes required.
- **Required finding:** The design covered `font_size_points` but omitted the
  reload update to `original_font_size_points`, which Ghostty gets by replacing
  `DerivedConfig.original_font_size` before reset-font-size actions.
- **Fix:** The design now requires updating `original_font_size_points` from the
  reloaded configured font size and verifying reset-font-size targets that new
  value after manual adjustment plus reload.
- **Re-review verdict:** Approved. The reviewer confirmed the fixed design now
  matches Ghostty's updated `DerivedConfig.original_font_size` behavior and
  introduced no new required findings.

## Result

**Result:** Pass

`Surface::apply_config` now matches pinned Ghostty's reload font-size behavior:

- it updates `original_font_size_points` from the reloaded configured
  `font-size`, clamped to `1.0..255.0`;
- unadjusted surfaces adopt the reloaded configured font size immediately;
- manually adjusted surfaces preserve their current font size across reload;
- reset-font-size resets to the newly reloaded configured font size and clears
  the manual adjustment flag;
- after reset-font-size clears the manual flag, later reloads adopt configured
  font size again.

`RELOAD-013` is now `Oracle complete` in the generated reload inventory. CFG-222
is now `Pass` with all 14 reload rows closed.

Verification passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size
cargo test --manifest-path roastty/Cargo.toml surface_key_table_uses_updated_app_table_storage

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_reload_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-reload-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# reload_rows=14 oracle_complete=14 closed=14 audit_covered=0 incomplete=0 gap=0 cfg222=Pass
```

## Conclusion

CFG-222 is closed. Config reload behavior now has a generated inventory, 14
Oracle-complete reload rows, Tier 1/2 guards, and a passing matrix row. The
remaining config parity work moves to CFG-223 runtime and UI effects.

## Completion Review

Adversarial completion review by fresh-context Codex subagent `Arendt`:

- **Verdict:** Approved.
- **Findings:** None.
- **Verification:** The reviewer confirmed scope is limited to `RELOAD-013`,
  pinned Ghostty replaces derived config and uses the updated
  `original_font_size` for reset-font-size, Roastty now implements the matching
  behavior in `Surface::apply_config`, the test covers the old missing behavior,
  `RELOAD-013` is `Oracle complete`, CFG-222 is `Pass`, and the required result
  docs are present.
- **Independent checks:**
  `cargo fmt --manifest-path roastty/Cargo.toml --check`,
  `cargo test --manifest-path roastty/Cargo.toml surface_reload_font_size`,
  `cargo test --manifest-path roastty/Cargo.toml surface_key_table_uses_updated_app_table_storage`,
  `prettier --check`, static inventory/matrix assertions, and `git diff --check`
  passed.
