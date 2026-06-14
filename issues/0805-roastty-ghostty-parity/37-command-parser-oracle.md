# Experiment 37: Command Parser Oracle

## Description

CFG-217 still has 26 parser rows that are only `Audit covered`. Canonical
`command` and `initial-command` are two of the remaining `custom parse_cli`
rows, and both route through pinned Ghostty's shared `config.Command.parseCLI`
semantics.

Pinned Ghostty's command parser requires a value, trims only ASCII spaces from
the value edges, rejects an empty/all-space value, recognizes exact `direct:`
and `shell:` prefixes before the first colon, trims ASCII spaces around the
payload, treats unknown prefixes such as `foo:` as ordinary shell commands, and
naively splits direct payloads on ASCII space. Direct splitting intentionally
preserves empty arguments produced by repeated or trailing spaces after the
payload edge trim. The human string and config formatter join direct arguments
with a single ASCII space; shell commands format as their payload.

Roastty already has a focused command/initial-command regression test covering
defaults, shell and direct prefixes, empty reset, diagnostics, formatter output,
string conversion, and clone behavior. This experiment will make that coverage
an explicit CFG-217 family oracle, extend it where needed for the direct-split
edge cases above, wire the parser inventory to recognize the oracle, and promote
only canonical `command` and `initial-command`.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing command parser regression as
    `command_config_parser_family_oracle` so the inventory generator can detect
    it as the CFG-217 oracle for the shared `Command::parse_cli` family.
  - Extend the oracle if needed to cover:
    - required missing values;
    - explicit empty values resetting optional config fields to default through
      `Config::set`;
    - whole missing, empty, and all-space inputs rejected before prefix
      handling;
    - prefixed empty/all-space payload behavior: `direct:` and `direct:   `
      become a direct command with one empty argument, while `shell:` and
      `shell:   ` become an empty shell command;
    - ASCII-space-only edge trimming, preserving tabs inside values;
    - exact `shell:` and `direct:` prefixes;
    - unknown colon prefixes falling back to shell mode;
    - direct payload splitting on ASCII spaces, including repeated-space empty
      arguments and trailing-space edge trim behavior;
    - formatter output and `Command::string`;
    - config-file diagnostics preserving earlier valid values;
    - clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `command_config_parser_family_oracle`.
  - Mark only canonical `command` and `initial-command` as `Oracle complete`
    when the oracle test is present.
  - Add command oracle detection to CFG-217 ownership so the generated matrix
    records `Experiment 37` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 179 `Oracle complete`, 24
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 179 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting command parser semantics after the result is
    proven.

## Verification

Pass criteria:

- Focused Roastty command-family oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml command_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=179`;
  - `audit_covered=24`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 179 rows are `Oracle complete`;
  - `command` and `initial-command` are both `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 37`;
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

Required finding:

- The original design said “all-space values rejected by the direct parser,”
  which could incorrectly require rejecting prefixed values such as
  `direct:   `. Upstream rejects only when the whole input trims to empty before
  prefix handling; after prefix handling, `direct:` / `direct:   ` become a
  direct command with one empty argument and `shell:` / `shell:   ` become an
  empty shell command.

Fix:

- Clarified the planned oracle cases to distinguish whole-input rejection from
  prefixed empty/all-space payload behavior.

Re-review verdict: **Approved**. The reviewer confirmed the corrected design now
matches upstream and introduced no new required findings.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml command_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 179
assert sum(row[4] == 'Audit covered' for row in rows) == 24
assert not [row for row in rows if row[4] == 'Gap']
for option in {'command', 'initial-command'}:
    row = next(row for row in rows if row[1] == f'`{option}`')
    assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 37', cfg217
assert '179 parser rows Oracle complete' in cfg217[12], cfg217
print('command_oracle_rows=2 oracle_complete=179 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/37-command-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```
