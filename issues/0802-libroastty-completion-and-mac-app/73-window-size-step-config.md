+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 73: Phase F — window size and step resize config

## Description

Experiment 72 wired the window scalar block through `window-title-font-family`.
The next missing upstream window fields are:

- `window-height`
- `window-width`
- `window-step-resize`

Upstream declares `window-height: u32 = 0`, `window-width: u32 = 0`, and
`window-step-resize: bool = false` in `vendor/ghostty/src/config/Config.zig`.
Its `finalize()` also clamps nonzero window sizes to a minimum of width `10` and
height `4`.

This experiment adds the config parser/formatter surface and the matching
config-level finalize clamp. Runtime window sizing and actual macOS step-resize
behavior are out of scope.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config::window_height: u32 = 0`.
  - Add `Config::window_width: u32 = 0`.
  - Add `Config::window_step_resize: bool = false`.
  - Route all three keys through defaults, `Config::set`, `format_config`,
    diagnostics, clone/equality, and formatter-order tests.
  - Add `Config::finalize` clamps:
    - if `window_width > 0`, clamp to at least `10`;
    - if `window_height > 0`, clamp to at least `4`.
  - Preserve upstream order:
    - `window-colorspace`
    - `window-height`
    - `window-width`
    - `window-position-x`
    - `window-position-y`
    - `window-save-state`
    - `window-step-resize`

Out of scope:

- Applying the initial window size to the app runtime.
- Runtime step-resize behavior.
- `window-new-tab-position` and later tab/window UI fields.
- `keybind` and `key-remap`.

## Verification

- Run formatting:
  - `cargo fmt`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/73-window-size-step-config.md`
- Run targeted tests:
  - `cargo test -p roastty window_size_step_config`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - defaults are `window-height = 0`, `window-width = 0`, and
    `window-step-resize = false`;
  - size fields parse decimal and base-prefixed unsigned integers;
  - size fields reset to defaults on empty value;
  - size fields return `ValueRequired` on missing value and `InvalidValue` for
    malformed, negative, or overflowing values;
  - `window-step-resize` parses explicit booleans, bare CLI `None` as `true`,
    resets to default on empty value, and rejects invalid booleans;
  - `Config::finalize` leaves zero sizes unchanged and clamps nonzero width to
    at least `10` and nonzero height to at least `4`;
  - `Config::load_str` records diagnostics for invalid neighboring size/bool
    lines while preserving valid values;
  - formatter order matches the upstream sequence around these fields;
  - clone/equality preserves all three values.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = the three window size/step fields are represented faithfully on
`Config`, round-trip through config loading/formatting, match upstream defaults
and finalize clamp behavior, and have targeted and full tests passing.

**Partial** = some fields land faithfully but a parser, diagnostic, order, or
finalize edge requires a follow-up.

**Fail** = these fields cannot be represented faithfully without first porting
runtime window sizing.

## Design Review

Codex adversarial reviewer `019eb41a-cdb2-7d80-b0a8-cdde47be9b8e` returned
**Approved** with no required findings.

The reviewer verified that the README links Exp73 as `Designed`, the experiment
has the required sections, the scope is limited to the config
parser/formatter/finalize surface, upstream defaults/order/clamp match
`Config.zig`, runtime sizing deferral is justified, and verification includes
targeted tests, full `cargo test -p roastty`, formatting, markdown prettier,
`cargo fmt --check`, `git diff --check`, and the intended status check.
