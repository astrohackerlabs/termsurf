# Experiment 40: Repeatable Path Parser Oracle

## Description

CFG-217 still has 22 parser rows that are only `Audit covered`. Two of the
remaining `custom parse_cli` rows share pinned Ghostty's `RepeatablePath`
semantics:

- `custom-shader`;
- `gtk-custom-css`.

Pinned Ghostty's `RepeatablePath.parseCLI` delegates to `Path.parse`. A missing
value is required, a raw empty value clears the repeatable list, required paths
append as required entries, leading `?` marks optional entries, surrounding
quotes protect a literal leading `?`, and parsed-empty paths such as `?`, `""`,
and `?""` are ignored rather than clearing the list. Formatting emits a blank
entry for an empty list and one path entry per stored item, prefixing optional
items with `?`. File and CLI loading later expand relative paths against their
respective bases.

Roastty already has lower-level repeatable path coverage and option-specific
coverage for both `custom-shader` and `gtk-custom-css`, including formatter
output, reset behavior, diagnostics, clone behavior, and file/CLI base
expansion. This experiment will make that coverage an explicit CFG-217 oracle,
extend it where needed for the shared direct parser boundary, wire the parser
inventory to recognize the oracle, and promote only `custom-shader` and
`gtk-custom-css`.

`config-file` also uses `RepeatablePath`, but it is already Oracle complete from
the earlier shared path oracle. This experiment does not change `config-file`.
The remaining `config-default-files` row is a separate boolean/load-order facet
and remains outside this scope.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing repeatable path and option-specific coverage as
    `repeatable_path_config_parser_family_oracle` so the inventory generator can
    detect it as the CFG-217 oracle for non-`config-file` repeatable path rows.
  - Extend the oracle if needed to cover:
    - missing direct parser values;
    - raw empty values clearing the repeatable list;
    - required path appending;
    - leading `?` optional path appending;
    - quoted literal leading `?`;
    - `?`, `""`, and `?""` parsed-empty values ignored rather than clearing;
    - formatter output for empty, required, optional, and quoted-literal cases;
    - config-file diagnostics preserving earlier valid values;
    - file-base and CLI-base path expansion for both `custom-shader` and
      `gtk-custom-css`;
    - clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `repeatable_path_config_parser_family_oracle`.
  - Mark only canonical `custom-shader` and `gtk-custom-css` as
    `Oracle complete` when the oracle test is present.
  - Leave canonical `config-file` already Oracle complete and unchanged.
  - Add repeatable path oracle detection to CFG-217 ownership so the generated
    matrix records `Experiment 40` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 183 `Oracle complete`, 20
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 183 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting non-`config-file` repeatable path parser
    semantics after the result is proven.

## Verification

Pass criteria:

- Focused Roastty repeatable-path oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml repeatable_path_config_parser_family_oracle
```

- Existing option-level and expansion tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml custom_shader_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml custom_shader_expands_from_file_and_cli_bases
cargo test --manifest-path roastty/Cargo.toml gtk_css_notifications_progress_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml gtk_custom_css_expands_from_file_and_cli_bases
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=183`;
  - `audit_covered=20`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 183 rows are `Oracle complete`;
  - `custom-shader` and `gtk-custom-css` are `Oracle complete`;
  - `config-file` remains `Oracle complete` from the earlier shared path oracle;
  - `config-default-files` remains not `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 40`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  passes.
- No `__pycache__` or other `py_compile` artifacts remain in the issue folder.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Changes required**, then fixed.

Required findings:

- The original design incorrectly said `config-file` should remain audit-only
  and still expected 183 `Oracle complete` rows. In the current inventory,
  `config-file` is already Oracle complete from the earlier shared path oracle,
  so promoting only `custom-shader` and `gtk-custom-css` from the current 181/22
  counts still produces 183/20.
- The verification assertions repeated the same `config-file` exclusion mistake.

Fix:

- Clarified that `config-file` is already Oracle complete and unchanged, while
  the separate `config-default-files` row remains outside this scope.
- Updated the matrix assertion to require `config-file` to stay Oracle complete
  and `config-default-files` to remain not Oracle complete.

Re-review verdict: **Approved**. The reviewer confirmed the corrected counts and
`config-file` / `config-default-files` assertions now match the current
inventory state and the experiment scope.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml repeatable_path_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml custom_shader_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml custom_shader_expands_from_file_and_cli_bases
cargo test --manifest-path roastty/Cargo.toml gtk_css_notifications_progress_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml gtk_custom_css_expands_from_file_and_cli_bases
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
python3 - <<'PY'
from pathlib import Path

rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        rows.append([cell.strip() for cell in line.strip('|').split('|')])

assert len(rows) == 203, len(rows)
assert sum(row[4] == 'Oracle complete' for row in rows) == 183
assert sum(row[4] == 'Audit covered' for row in rows) == 20
assert not [row for row in rows if row[4] == 'Gap']
for option in {'custom-shader', 'gtk-custom-css'}:
    row = next(row for row in rows if row[1] == f'`{option}`')
    assert row[4] == 'Oracle complete', row
config_file = next(row for row in rows if row[1] == '`config-file`')
assert config_file[4] == 'Oracle complete', config_file
config_default_files = next(row for row in rows if row[1] == '`config-default-files`')
assert config_default_files[4] != 'Oracle complete', config_default_files

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 40', cfg217
assert '183 parser rows Oracle complete' in cfg217[12], cfg217
print('repeatable_path_oracle_rows=2 oracle_complete=183 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/40-repeatable-path-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Result

**Result:** Pass

Roastty now has a focused repeatable path parser family oracle for the two
canonical non-`config-file` rows:

- `custom-shader`;
- `gtk-custom-css`.

Implementation notes:

- Renamed and extended the lower-level repeatable path test as
  `repeatable_path_config_parser_family_oracle`.
- Added formatter checks for empty, required, optional, and quoted-literal
  repeatable paths.
- Taught `config_parser_inventory.py` to detect the repeatable path oracle,
  promote only `custom-shader` and `gtk-custom-css`, and make CFG-217's owner
  `Experiment 40` when this oracle is present.
- Left `config-file` already Oracle complete and unchanged; the separate
  `config-default-files` row remains outside this scope.

Verification commands run:

```bash
cargo test --manifest-path roastty/Cargo.toml repeatable_path_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml custom_shader_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml custom_shader_expands_from_file_and_cli_bases
cargo test --manifest-path roastty/Cargo.toml gtk_css_notifications_progress_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml gtk_custom_css_expands_from_file_and_cli_bases
```

Results:

```text
test config::tests::repeatable_path_config_parser_family_oracle ... ok
test config::tests::custom_shader_config_parse_format_reset_and_diagnose ... ok
test config::tests::custom_shader_expands_from_file_and_cli_bases ... ok
test config::tests::gtk_css_notifications_progress_config_parse_format_reset_and_diagnose ... ok
test config::tests::gtk_custom_css_expands_from_file_and_cli_bases ... ok
```

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

Result:

```text
ghostty_canonical=203
roastty_parser_rows=203
missing_canonical_parser_rows=0
missing_dispatch_rows=0
extra_parser_rows=0
compatibility_only_parser_arms=5
noncanonical_noncompat_parser_arms=0
oracle_complete=183
audit_covered=20
gap=0
```

The matrix assertion passed and printed:

```text
repeatable_path_oracle_rows=2 oracle_complete=183 cfg217=Gap
```

Additional hygiene checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
```

## Conclusion

The non-`config-file` repeatable path parser rows are now oracle-complete for
CFG-217. The important upstream boundary is that raw empty values clear the
list, while parsed-empty paths after optional-marker/quote handling are no-ops.
CFG-217 remains `Gap` because 20 parser rows are still only audit-covered.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Changes required**, then fixed.

Required finding:

- The verification list still included the old
  `config_file_repeatable_path_parse_cli_matches_upstream` test filter after the
  test was renamed. That stale command exits successfully while running zero
  tests.

Fix:

- Removed the stale test-filter command from both the pass criteria and the
  suggested command list. The renamed
  `repeatable_path_config_parser_family_oracle` test remains the direct shared
  repeatable-path guard.

The reviewer independently verified the focused repeatable-path and option-level
tests, Rust format check, `git diff --check`, generated inventory counts, the
two promoted target rows, `config-file` remaining Oracle complete,
`config-default-files` remaining not Oracle complete, CFG-217's `Gap` status and
Experiment 40 ownership, and that the result commit had not yet been made.

Re-review verdict: **Approved**. The reviewer confirmed the stale zero-test
command is absent from runnable verification lists and appears only in this
review narrative.
