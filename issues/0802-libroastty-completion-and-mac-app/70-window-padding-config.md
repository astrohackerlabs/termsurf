+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 70: Phase F — window padding config

## Description

Experiment 69 added the `working-directory` config surface. The next literal
upstream fields are `keybind` and `key-remap`, but those belong to the larger
input/keybinding subsystem rather than a small parser/formatter config slice.

This experiment advances the next small, already-supported window config
surface:

- `window-padding-x`
- `window-padding-y`

Upstream declares both as `WindowPadding` values with default
`.{ .top_left = 2, .bottom_right = 2 }` in
`vendor/ghostty/src/config/Config.zig`. Roastty already has a `WindowPadding`
type with upstream parser and formatter tests. This experiment wires those two
fields into the aggregating `Config` only: fields, defaults, parsing/reset
behavior, formatting, diagnostics, and focused tests.

Runtime terminal viewport geometry, live renderer padding application, and
padding-balance behavior are intentionally out of scope.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config::window_padding_x: WindowPadding = WindowPadding { 2, 2 }`.
  - Add `Config::window_padding_y: WindowPadding = WindowPadding { 2, 2 }`.
  - Add `From<WindowPaddingParseError> for ConfigSetError` if it is not already
    present.
  - Route both keys through defaults, `Config::set`, `format_config`,
    clone/equality, and diagnostics using the existing value helper.
  - Preserve local formatter order around the upstream window-padding sequence:
    - `window-padding-x`
    - `window-padding-y`
    - `window-padding-color`
  - Leave `window-padding-balance` out of scope because its enum/config surface
    is separate and not needed to wire the existing `WindowPadding` parser.

Out of scope:

- `keybind` and `key-remap`.
- `window-padding-balance`.
- Applying padding to renderer/viewport geometry.
- Runtime warnings for unreasonable padding values.

## Verification

- Run formatting:
  - `cargo fmt`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/70-window-padding-config.md`
- Run targeted tests:
  - `cargo test -p roastty window_padding_config`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - both defaults are `2` and format as single-value `2` entries;
  - single values and two comma-separated values parse and format for both
    fields;
  - empty values reset both fields to the default;
  - missing values return `ValueRequired`;
  - invalid values return `InvalidValue`;
  - `Config::load_str` records `ConfigDiagnostic` line/key/error entries for
    invalid `window-padding-x` and `window-padding-y` lines while keeping valid
    neighboring lines;
  - formatter order places `window-padding-x` before `window-padding-y`, and
    both before `window-padding-color`;
  - clone/equality preserves both values.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = `window-padding-x` and `window-padding-y` are represented faithfully
on `Config`, round-trip through config loading/formatting, match the existing
upstream padding parser behavior, and have targeted and full tests passing.

**Partial** = one field lands faithfully but the other needs a follow-up, or a
parser/diagnostic/formatter-order edge remains before runtime use.

**Fail** = the padding fields cannot be represented faithfully without first
porting renderer geometry or `window-padding-balance`.

## Design Review

Codex adversarial reviewer `019eb3f4-84f0-7630-b61e-2b35e15ce6c8` returned
**Approved** with no findings.

The reviewer verified that the README links Exp70 as `Designed`, the experiment
has the required sections, the plan matches upstream defaults and ordering for
`window-padding-x`, `window-padding-y`, deferred `window-padding-balance`, and
existing `window-padding-color`, and the verification plan includes the required
formatting, targeted tests, full `cargo test -p roastty`, `git diff --check`,
and clean-status checks.
