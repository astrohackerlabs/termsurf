# Experiment 17: Float Parser Oracle

## Description

CFG-217 still has 154 parser rows that are only `Audit covered`. The next
bounded family is the 9-row float scalar family. Pinned Ghostty's generic config
parser uses `std.fmt.parseFloat(f32|f64, value)` for these rows. That is close
to Rust's built-in float parser, but pre-design probes found concrete syntax
that Rust rejects and Zig accepts, including decimal digit separators and
hexadecimal float literals such as `0x1p4`.

This experiment will make Roastty's direct float scalar helpers match the pinned
Ghostty parse shape for representative `f64` and `f32` fields, then promote all
9 float scalar rows to `Oracle complete`. CFG-217 must remain `Gap` because
other parser families are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a shared Zig-compatible float parser helper used by both `set_f64_field`
    and `set_f32_field`.
  - Preserve existing missing value and set-but-empty reset semantics:
    - missing value returns `ValueRequired`;
    - set-but-empty value resets to the field default before parsing.
  - Cover and, where needed, implement Ghostty/Zig float parsing behavior for:
    - decimal integers and fractions;
    - signed values;
    - decimal exponent syntax;
    - case-insensitive `nan`, `inf`, and `infinity`, including mixed-case and
      signed special literals such as `nAn`, `inF`, `+Inf`, and `-iNf`;
    - overflow to positive/negative infinity;
    - interior single underscores in decimal digits;
    - hexadecimal float syntax with `0x`/`0X`, optional fractional part, and
      optional `p`/`P` binary exponent, including exponentless values such as
      `0x0`, `-0x0`, and `0x1`;
    - accepted hex separators between hex digits and rejected leading, trailing,
      consecutive, prefix-adjacent, dot-adjacent, and exponent-adjacent
      underscores;
    - invalid empty values, bare signs, malformed underscores, malformed
      hexadecimal exponents, and non-numeric text.
  - Add a focused float parser family test covering representative rows:
    - `bell-audio-volume` (`f64`);
    - `background-image-opacity` (`f32`);
    - at least one default-reset check on a non-default value.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark float scalar parser rows as `Oracle complete` when the float family
    oracle test is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 58 `Oracle complete`, 145
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 58 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting the float scalar parser oracle and the Rust/Zig
    syntax difference found by this experiment.

## Verification

Pass criteria:

- Focused Roastty tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml float_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=58`;
  - `audit_covered=145`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 58 rows are `Oracle complete`;
  - every float scalar row is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 17`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `git diff --check` passes.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml float_config_parser_family_oracle
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
assert cfg217[11] == 'Experiment 17', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
float_rows = [row for row in parser_rows if row[3] == 'float scalar']
assert len(float_rows) == 9, len(float_rows)
assert all(row[4] == 'Oracle complete' for row in float_rows)
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 58
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} float_oracle={len(float_rows)} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/17-float-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Fresh-context adversarial design review found three required coverage gaps:

- The plan needed case-insensitive and signed special literal coverage for
  `nan`, `inf`, and `infinity`, not only lowercase/uppercase examples.
- The plan needed exponentless hexadecimal floats such as `0x0`, `-0x0`, and
  `0x1`, not only `p`/`P` exponent forms.
- The plan needed explicit accepted and rejected underscore cases for
  hexadecimal float digits and boundaries.

The design was updated to require those cases. Focused re-review approved the
fixed design with no remaining findings.

## Result

**Result:** Pass

Roastty now uses a shared Zig-compatible float parser helper for direct `f64`
and `f32` config fields. The helper preserves the existing missing-value and
set-but-empty reset behavior, validates Zig-style digit separators, and then
delegates normalized values to C99 `strtod`/`strtof` for correctly rounded
decimal and hexadecimal float parsing. This adds the Zig float syntax that
Rust's built-in parser does not cover: digit separators, hexadecimal floats,
exponentless hex floats, case-insensitive signed special literals, and overflow
to infinities. The helper also rejects C payload-NaN syntax such as
`nan(payload)`, which Zig rejects.

Focused Roastty verification passed:

```bash
cargo test --manifest-path roastty/Cargo.toml float_config_parser_family_oracle
```

Output summary:

```text
running 1 test
test config::tests::float_config_parser_family_oracle ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4907 filtered out; finished in 0.01s
```

The parser inventory generator passed and moved the 9 float scalar rows to
`Oracle complete`:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

Output:

```text
ghostty_canonical=203
roastty_parser_rows=203
missing_canonical_parser_rows=0
missing_dispatch_rows=0
extra_parser_rows=0
compatibility_only_parser_arms=5
noncanonical_noncompat_parser_arms=0
oracle_complete=58
audit_covered=145
gap=0
```

CFG-217 remains `Gap` because 145 parser rows are still audit-only, but there
are still no parser dispatch gaps.

## Conclusion

Float scalar parser semantics are now proven for the pinned Ghostty target. The
experiment also found and fixed a real parser parity issue: Rust's built-in
float parser was too narrow for Ghostty's Zig `std.fmt.parseFloat` syntax, and a
manual hexadecimal parser was not sufficiently precise for f64 rounding-boundary
values.

## Completion Review

Fresh-context adversarial completion review initially found two required issues:

- The first implementation used a manual hexadecimal float accumulator that was
  not correctly rounded for f64 boundary values such as `0x1.000000000000081p0`.
- The result text overclaimed proven semantics while that rounding divergence
  existed.

The implementation was changed to validate Zig-style digit separators and
delegate normalized values to C99 `strtod`/`strtof`, with f64 and f32
rounding-boundary regressions. Re-review then found one more required issue:
libc accepts C payload-NaN syntax such as `nan(payload)`, which Zig rejects.

The helper now rejects payload-NaN syntax before libc delegation, the oracle
covers both f64 and f32 payload-NaN rejects, and the result text documents the
guard. Final re-review approved the completed result with no remaining findings
and independently verified the focused test, Rust fmt check, and
`git diff --check`.
