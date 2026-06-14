# Experiment 33: Selection Word Chars Parser Oracle

## Description

CFG-217 still has 30 parser rows that are only `Audit covered`. Canonical
`selection-word-chars` is one of the remaining `custom parse_cli` rows.

Pinned Ghostty defines `selection-word-chars` as `SelectionWordChars = .{}`.
`SelectionWordChars.parseCLI` requires a present value, always starts the parsed
codepoint list with null (`U+0000`), parses the input through Ghostty's string
codepoint iterator with escape support, returns `InvalidValue` on iterator
failure, and treats an explicit empty string as valid with only the null
boundary. `formatEntry` skips the leading null, UTF-8 encodes each remaining
codepoint, skips unencodable codepoints, and stops before exceeding its
4096-byte buffer.

Roastty already has lower-level parser and formatter tests plus a selection
config routing test. This experiment will add a focused CFG-217 oracle named for
inventory promotion, keep the existing lower-level/routing tests in the
verification set, and promote only canonical `selection-word-chars`.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `selection_word_chars_config_parser_family_oracle` test
    covering:
    - default word-boundary codepoints;
    - default formatting;
    - literal spaces/tabs/punctuation;
    - `\t`, `\\`, and `\u{2502}` escape parsing;
    - raw empty value parsing to only the null boundary;
    - direct missing values are `ValueRequired`;
    - bad escapes are `InvalidValue` and preserve the prior valid value;
    - config-file diagnostics preserve earlier valid values;
    - CLI argument parsing reaches the same helper;
    - formatter re-encodes multibyte codepoints, skips invalid Unicode
      codepoints, and honors the 4096-byte cap;
    - cloned configs retain parsed selection word chars.
  - Keep the existing `selection_word_chars_parse_cli_parses_codepoints`,
    `selection_word_chars_format_entry_reencodes_codepoints`, and
    `selection_behavior_config_routes_and_formats` tests in the verification
    set.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark only canonical `selection-word-chars` as `Oracle complete` when the
    selection word chars oracle test is present.
  - Add selection word chars oracle detection to CFG-217 ownership so the
    generated matrix records `Experiment 33` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 174 `Oracle complete`, 29
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 174 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting `selection-word-chars` parser semantics after the
    result is proven.

## Verification

Pass criteria:

- Focused Roastty test passes:

```bash
cargo test --manifest-path roastty/Cargo.toml selection_word_chars_config_parser_family_oracle
```

- Existing lower-level and routing tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml selection_word_chars_parse_cli_parses_codepoints
cargo test --manifest-path roastty/Cargo.toml selection_word_chars_format_entry_reencodes_codepoints
cargo test --manifest-path roastty/Cargo.toml selection_behavior_config_routes_and_formats
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=174`;
  - `audit_covered=29`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 174 rows are `Oracle complete`;
  - the `selection-word-chars` row is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 33`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  passes.
- No `__pycache__` or other `py_compile` artifacts remain in the issue folder.
- `git diff --check` passes.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml selection_word_chars_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml selection_word_chars_parse_cli_parses_codepoints
cargo test --manifest-path roastty/Cargo.toml selection_word_chars_format_entry_reencodes_codepoints
cargo test --manifest-path roastty/Cargo.toml selection_behavior_config_routes_and_formats
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
python3 - <<'PY'
from pathlib import Path

matrix_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-'):
        matrix_rows.append([cell.strip() for cell in line.strip('|').split('|')])
cfg217 = next(row for row in matrix_rows if row[0] == 'CFG-217')
assert cfg217[4] == 'Gap', cfg217
assert 'config-parser-inventory.md' in cfg217[6], cfg217
assert cfg217[11] == 'Experiment 33', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
selection = [row for row in parser_rows if row[1] == '`selection-word-chars`']
assert len(selection) == 1, selection
assert selection[0][4] == 'Oracle complete', selection[0]
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 174
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} selection_word_chars={selection[0][4]} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/33-selection-word-chars-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Fresh-context adversarial subagent review completed before implementation.

**Verdict:** Approved.

No findings.
