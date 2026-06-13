# Experiment 172: Phase F — input config surface

## Description

Remove `input` from the remaining Phase F public-config tail.

Upstream defines `input` as `RepeatableReadableIO`, a repeatable list of
`ReadableIO` values. Each entry is either:

- `raw:<string>` — write the string bytes directly;
- `path:<path>` — defer reading a finite file until terminal startup;
- any value without a recognized tag — treated as `raw:<value>` for ergonomic
  config such as `input = Hello`.

Upstream validates every entry with Zig string-literal parsing at config parse
time, but stores the original unparsed string for formatting. Empty `input =`
resets the list. Missing values report `ValueRequired`. Path existence,
readability, size limits, and actually sending the bytes to newly started
terminals are runtime work and are not part of this experiment.

## Changes

- `roastty/src/config/mod.rs`
  - Add a `ReadableIo` enum with `Raw(String)` and `Path(String)` variants.
  - Add `RepeatableReadableIo` storage with upstream repeatable semantics:
    - missing values report `ValueRequired`;
    - empty set values clear the list;
    - `raw:<value>` stores a raw entry;
    - `path:<value>` stores a path entry without checking the filesystem;
    - unrecognized tagged-looking values such as `foo:bar` store as raw values;
    - malformed Zig string escapes report `InvalidValue`.
  - Add `Config::input`, defaulting to an empty list.
  - Format `input` after `env` and before `wait-after-command`, matching
    upstream declaration order. An empty list emits `input =`; otherwise each
    item emits one `input = raw:<value>` or `input = path:<value>` line.
  - Route `Config::set("input", ...)` through the repeatable parser.
  - Update default/order tests.
  - Add a focused `input_config_parse_format_reset_load_cli_and_clone` test
    covering raw, path, unprefixed raw, unknown-tag raw fallback, malformed
    escape diagnostics, missing values, empty reset, config-file loading, CLI
    append, and clone/equality.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link Experiment 172 as `Designed`.
  - After result, update the Phase F remaining-public-options count from 2 to 1
    and leave only `keybind` in that public-config tail if this passes.
  - After result, add an operating note that `input` is parser/formatter-only
    until a runtime slice consumes `Config::input` at terminal startup.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty input_config_parse_format_reset_load_cli_and_clone`
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
- `cargo test -p roastty`
- `cargo fmt -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/172-input-config-surface.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = `input` parses, formats, resets, loads, clones, reports diagnostics,
and appears in upstream order with `RepeatableReadableIO` semantics, and the
full roastty suite passes.

**Partial** = the direct parser/formatter field lands, but ordering, reset
behavior, diagnostics, or full-suite verification remains incomplete.

**Fail** = the field cannot be added without conflicting with existing config
formatting, string-literal parsing, or future terminal-startup input handling.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Sartre`, fresh context.

**Verdict:** Approved with no findings.

The reviewer verified that the README links Experiment 172 as `Designed`, the
experiment has the required sections, the plan matches upstream
`ReadableIO`/`RepeatableReadableIO` parse and format semantics, the field order
is after `env` and before `wait-after-command`, and the verification includes
both the required `cargo fmt -p roastty` run and check-only formatter command.

## Result

**Result:** Pass

Implemented the parser/formatter/storage surface for upstream
`RepeatableReadableIO` as `Config::input`. The field now defaults to an empty
repeatable list, formats after `env` and before `wait-after-command`, parses
`raw:` and `path:` entries, falls back to raw for unknown tags, validates the
full unparsed value with Zig string-literal semantics, supports empty-list
reset, reports missing values as `ValueRequired`, and preserves the original
unparsed payload spelling for formatting.

Runtime file reading and terminal-startup delivery are still out of scope; this
experiment only removes `input` from the public parser/formatter config tail.

Verification passed:

- `cargo test -p roastty input_config_parse_format_reset_load_cli_and_clone`
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
- `cargo fmt -p roastty`
- `cargo test -p roastty` — 4,868 unit tests passed, 4 ignored; ABI harness
  passed with the existing 10 enum-conversion warnings; doc tests had 0 tests.

## Conclusion

`input` is no longer a public-config gap for parsing, formatting, diagnostics,
config-file loading, CLI appending, cloning, or field ordering. The remaining
Phase F public-config tail is now `keybind`; runtime consumption of
`Config::input` should be handled later with terminal startup behavior rather
than mixed into this config-surface slice.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Bohr`, fresh context.

**Verdict:** Approved with no findings.

The reviewer checked the uncommitted result diff against the approved plan and
upstream `vendor/ghostty/src/config/io.zig`, including raw/path tag behavior,
unknown-tag raw fallback, missing-value and reset semantics, original unparsed
payload preservation, string-literal validation, field ordering, test coverage,
README status, and that the result commit had not been made before review.
