# Experiment 50: Non-Default Formatter Facet Audit

## Description

CFG-218 is still a broad gap: Roastty has proven default config formatter
parity, but it has not proven non-default formatter behavior and order for every
canonical Ghostty config option. The default formatter oracle is necessary but
not sufficient because many options have alternate non-default forms, repeatable
entries, optional empty output, custom `formatEntry` methods, or ordering rules
that default output does not exercise.

This experiment will build the formatter audit surface before trying to close
CFG-218. The goal is to map every pinned Ghostty canonical option to the
formatter path Roastty uses in `Config::format_config`, classify the formatter
family, attach existing evidence, and identify the smallest remaining formatter
gaps. The result should prevent accidental overclaiming: CFG-218 remains `Gap`
unless every formatter row has upstream-derived non-default formatter evidence.

## Changes

- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Add a bounded source scanner for Roastty's `Config::format_config` entries
    and Ghostty's canonical option list.
  - Emit one formatter row per canonical Ghostty option.
  - Distinguish canonical options with an explicit formatter call from
    intentional no-output formatter rows. In particular, pinned Ghostty's
    `link: RepeatableLink` has a `formatEntry` method that intentionally emits
    nothing because the option cannot currently be set; Roastty should record a
    row for `link` without requiring a nonexistent `Config::format_config`
    helper.
  - Classify each option by formatter family where possible, such as bool,
    integer, float, string, optional value, enum/custom `format_entry`,
    repeatable list/map, path, color, command, keybind, key remap, theme, window
    padding, packed flags, no-output, and custom inline formatter.
  - Derive canonical fields from `Config.zig`; derive Roastty formatter paths
    from `Config::format_config`; and audit the family classifier against
    Ghostty's `formatter_file.zig`, `formatter.zig`, and custom `formatEntry`
    implementations in `Config.zig` and related config helper files.
  - Mark rows as `Audit covered` when the formatter path and family are
    identified.
  - Mark rows as `Oracle complete` only when existing evidence is
    upstream-derived and covers non-default formatter values, repeatable/empty
    forms where applicable, and order.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Record generated formatter facet rows, counts by formatter family, covered
    rows, and gap rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Update CFG-218 to point at the formatter inventory.
  - Keep CFG-218 as `Gap` unless every formatter inventory row is
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.

## Verification

Pass criteria:

- The formatter inventory generator exits successfully and reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - one row for every canonical option, including intentional no-output rows;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`.
- Every generated formatter row names:
  - the canonical config option;
  - the Roastty formatter path/helper, or an explicit canonical no-output
    reason;
  - formatter family;
  - current coverage status;
  - evidence artifact or concrete missing evidence.
- A matrix assertion verifies that CFG-218 is internally consistent:
  - if every formatter inventory row is `Oracle complete`, CFG-218 may be
    `Pass`;
  - if any formatter inventory row is `Audit covered` or `Gap`, CFG-218 remains
    `Gap`;
  - CFG-218 points to `config-formatter-inventory.md`.
- The generator must not disturb CFG-217 or parser inventory results.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  Markdown files.
- `git diff --check` passes.

Suggested commands:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig \
  --upstream-formatter vendor/ghostty/src/config/formatter.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
python3 - <<'PY'
from pathlib import Path

matrix_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-'):
        matrix_rows.append([cell.strip() for cell in line.strip('|').split('|')])
cfg218 = next(row for row in matrix_rows if row[0] == 'CFG-218')
assert 'config-formatter-inventory.md' in cfg218[6], cfg218

formatter_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-formatter-inventory.md').read_text().splitlines():
    if line.startswith('| FORMAT-'):
        formatter_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(formatter_rows) == 203, len(formatter_rows)
incomplete = [row for row in formatter_rows if row[4] != 'Oracle complete']
assert (not incomplete and cfg218[4] == 'Pass') or (incomplete and cfg218[4] == 'Gap')
print(f'formatter_rows={len(formatter_rows)} incomplete={len(incomplete)} cfg218={cfg218[4]}')
PY
cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle
git diff --check
```

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

Required findings:

- The draft required `missing_formatter_rows=0` and every row to name a Roastty
  formatter helper, but pinned Ghostty's canonical `link` option intentionally
  emits no formatter output. Fixed by adding first-class no-output formatter
  rows and allowing each row to name either a Roastty formatter path/helper or
  an explicit canonical no-output reason.

Optional finding:

- The suggested generator command did not include Ghostty's formatter source
  files. Fixed by adding `--upstream-formatter-file` and `--upstream-formatter`,
  and by stating that the classifier is audited against `formatter_file.zig`,
  `formatter.zig`, and custom `formatEntry` implementations.

Final re-review verdict: **Approved**. The reviewer confirmed the no-output
criterion and found no new required issues.

## Result

**Result:** Pass

Added a formatter-facet inventory generator and generated
`config-formatter-inventory.md`. The audit found:

- 203 canonical Ghostty formatter rows.
- 203 Roastty formatter rows.
- 0 missing canonical formatter rows.
- 0 extra formatter rows.
- 1 intentional no-output row: canonical `link`, matching pinned Ghostty's
  `RepeatableLink.formatEntry` behavior.
- 0 `Oracle complete` rows and 203 `Audit covered` rows, so CFG-218 correctly
  remains `Gap`.

The generator updates CFG-218 to point at the formatter inventory and leaves
CFG-217 untouched. CFG-218 now has a concrete proof surface for future
formatter-family or option-specific oracle experiments.

Verification:

```text
ghostty_canonical=203
roastty_formatter_rows=203
missing_canonical_formatter_rows=0
extra_formatter_rows=0
oracle_complete=0
audit_covered=203
gap=0
no_output_rows=1
```

Matrix assertion output:

```text
formatter_rows=203 incomplete=203 cfg218=Gap
```

Additional verification:

- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  passed.
- `git diff --check` passed.

## Conclusion

CFG-218 remains open, but it is now concrete. The next formatter work can raise
formatter families from `Audit covered` to `Oracle complete` with targeted
non-default formatter oracles, starting with the cheapest broad families such as
primitive bool/integer/float/string formatting or optional-value formatting.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer independently verified:

- the formatter inventory has 203 rows, 203 `Audit covered` rows, 0
  `Oracle complete` rows, 0 `Gap` rows, and 1 no-output row;
- `link` is modeled as no-output and is supported by pinned Ghostty's
  `RepeatableLink.formatEntry`;
- CFG-217 remains `Pass`;
- CFG-218 remains `Gap` and points to `config-formatter-inventory.md`;
- the README marks Experiment 50 `Pass` and records the `link` no-output
  learning;
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  passed;
- `git diff --check` passed.

The reviewer did not run the formatter inventory generator because it writes
files, but inspected the generated output and approved the completed experiment.
