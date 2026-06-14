# Experiment 112: Platform Runtime Classification

## Description

`RUNTIME-013` remains a CFG-223 gap because platform-prefixed and otherwise
platform-specific config options are parsed/formatted, but their runtime effects
have not been classified. This row should not become a dumping ground for real
macOS app work: GTK and Linux cgroup behavior can be marked not applicable to
Roastty's macOS runtime, while macOS-specific behavior must either be proven by
existing runtime guards or routed to the existing macOS app/runtime rows that
still need walkthrough coverage.

This experiment will close only `RUNTIME-013` by creating a durable
classification manifest. It will not claim `RUNTIME-011` macOS app/window/menu
parity, `RUNTIME-012` notification/link behavior, or broader renderer/font/PTY
runtime rows.

## Changes

- Add a generated platform runtime classification artifact that accounts for
  every platform-prefixed canonical option currently present in the config
  inventory:
  - `gtk-*`;
  - `linux-*`;
  - `macos-*`.
- For each row, record:
  - option name;
  - upstream platform behavior family;
  - Roastty applicability;
  - classification using accepted runtime outcomes: `Oracle complete`, `Gap`,
    `Not applicable`, or `Intentional divergence`;
  - owner runtime row or divergence row;
  - evidence and guard.
- Add a small generator/check script so the manifest fails if a future
  platform-prefixed config option appears in the regenerated config inventory
  without a classification row.
- Classify GTK-only and Linux cgroup options as `Not applicable` to Roastty's
  macOS app/runtime, with source evidence pointing at pinned Ghostty GTK/Linux
  implementation paths and Roastty's macOS-only app surface.
- Classify macOS options as either:
  - `Oracle complete` when already covered by existing runtime rows, for example
    `macos-option-as-alt` under key/input runtime guards; or
  - still a gap owned by `RUNTIME-011` macOS app/window/menu walkthrough,
    without closing that row.
- Update `config_runtime_inventory.py` so `RUNTIME-013` becomes
  `Oracle complete` only after the generated classification manifest accounts
  for all platform-prefixed options and leaves no unowned platform row.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`, format the
  generated markdown, and update Issue 805 learnings.

## Verification

Pass criteria:

- The config inventory is regenerated from pinned Ghostty and Roastty config
  sources before the platform classification runs.
- The generated platform classification manifest includes every `gtk-*`,
  `linux-*`, and `macos-*` canonical option listed by regenerated
  `config-inventory.md`.
- GTK-only and Linux cgroup rows are explicitly `Not applicable`, with evidence
  and a Tier 0 guard.
- macOS rows are not overclaimed: options that still need GUI/app proof remain
  owned by `RUNTIME-011`, while already-covered options point to the existing
  runtime row and guard.
- `RUNTIME-013` is promoted to `Oracle complete`.
- `CFG-223` remains `Gap`.

Commands:

```bash
python3 issues/0805-roastty-ghostty-parity/config_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --output issues/0805-roastty-ghostty-parity/config-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md

prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-inventory.md \
  issues/0805-roastty-ghostty-parity/platform-runtime-classification.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

classification = Path("issues/0805-roastty-ghostty-parity/platform-runtime-classification.md").read_text()
inventory = Path("issues/0805-roastty-ghostty-parity/config-runtime-inventory.md").read_text()
matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()
cfg223 = next(row for row in matrix.splitlines() if row.startswith("| CFG-223 "))

rows = {}
for line in inventory.splitlines():
    if not line.startswith("| RUNTIME-"):
        continue
    cells = [cell.strip() for cell in line.strip("|").split("|")]
    rows[cells[0]] = cells

assert rows["RUNTIME-013"][5] == "Oracle complete", rows["RUNTIME-013"]
assert "| Gap " in cfg223
assert "| gtk-" in classification
assert "| linux-" in classification
assert "| macos-" in classification
assert "Unclassified" not in classification
PY

python3 -m py_compile issues/0805-roastty-ghostty-parity/config_inventory.py \
  issues/0805-roastty-ghostty-parity/platform_runtime_classification.py \
  issues/0805-roastty-ghostty-parity/config_runtime_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__

prettier --check issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/112-platform-runtime-classification.md \
  issues/0805-roastty-ghostty-parity/config-inventory.md \
  issues/0805-roastty-ghostty-parity/platform-runtime-classification.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md \
  issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

git diff --check
```

## Design Review

Fresh-context Codex adversarial reviewer `Descartes` initially returned
**CHANGES REQUIRED**:

- **Required:** the design used `Covered elsewhere`, which is not an accepted
  Issue 805 outcome or a `config_runtime_inventory.py` runtime status.
- **Required:** the drift guard consumed `config-inventory.md` but did not first
  regenerate it from pinned Ghostty and Roastty config sources, so source-level
  option drift could be missed.

Fix:

- The design now restricts classification statuses to accepted runtime outcomes:
  `Oracle complete`, `Gap`, `Not applicable`, and `Intentional divergence`.
- The design now requires regenerating `config-inventory.md` from
  `vendor/ghostty/src/config/Config.zig` and `roastty/src/config/mod.rs` before
  running the platform classification generator.

Re-review verdict: **Approved**. The reviewer confirmed both prior required
findings are resolved.

## Result

**Result:** Pass

Added `platform_runtime_classification.py`, a bounded generator that extracts
every `gtk-*`, `linux-*`, and `macos-*` canonical option from the regenerated
config inventory and fails if any platform-prefixed option is unclassified or
stale. The generated `platform-runtime-classification.md` accounts for 32
platform-prefixed options:

- 15 GTK/Linux rows are `Not applicable` to Roastty's macOS app/runtime.
- `macos-option-as-alt` is `Oracle complete` through existing key translation
  runtime guards.
- 16 macOS app/UI rows remain `Gap` and are explicitly owned by `RUNTIME-011`,
  so this experiment does not overclaim macOS app parity.

`RUNTIME-013` is promoted to `Oracle complete` because platform-specific runtime
effects are now classified and routed. `CFG-223` remains `Gap` because six
runtime/UI rows are still incomplete.

Verification passed:

```text
python3 issues/0805-roastty-ghostty-parity/config_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --output issues/0805-roastty-ghostty-parity/config-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# ghostty_canonical=203 ghostty_aliases=8 ghostty_internal=6 roastty=203 represented=203 missing=0 extra=0

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
# platform_options=32 gap=16 not_applicable=15 oracle_complete=1

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=21 oracle_complete=14 closed=15 audit_covered=0 incomplete=6 gap=6 cfg223=Gap
```

## Conclusion

`RUNTIME-013` now has a durable Tier 0 classification guard. The remaining
CFG-223 gaps are real runtime/UI work: font, renderer presentation, terminal
behavior toggles, PTY/process launch effects, macOS app/window/menu workflows,
and notification/link behavior.

## Completion Review

Fresh-context Codex reviewer `Tesla` returned **Approved** with no findings.

The reviewer confirmed:

- `HEAD` was still the Experiment 112 plan commit and the result commit had not
  been made yet.
- The platform inventory has 32 platform-prefixed canonical options, with 0
  unclassified rows and 0 stale rows.
- The classification counts are 16 `Gap`, 15 `Not applicable`, and 1
  `Oracle complete`.
- `RUNTIME-013` is `Oracle complete`, `RUNTIME-011` and `RUNTIME-012` remain
  `Gap`, and `CFG-223` remains `Gap`.
- Python syntax checks, `prettier --check`, `git diff --check`, and the
  `__pycache__` cleanliness check passed.
