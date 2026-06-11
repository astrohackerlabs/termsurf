+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 83: Phase F — quick terminal screen and animation config

## Description

Experiment 82 wired the GTK quick-terminal layer and namespace fields. The next
unported upstream quick-terminal config fields are:

- `quick-terminal-screen`
- `quick-terminal-animation-duration`
- `quick-terminal-autohide`

Upstream declares `quick-terminal-screen` as `QuickTerminalScreen = .main`, with
enum tags `main`, `mouse`, and `macos-menu-bar`. It declares
`quick-terminal-animation-duration` as `f64 = 0.2`, and
`quick-terminal-autohide` as an OS-dependent bool default. Because this issue is
currently porting the macOS app and `roastty` defaults are macOS-biased where
upstream varies by platform, this experiment uses upstream's macOS default:
`quick-terminal-autohide = true`.

This experiment adds the Rust config parser/formatter surface for all three
fields. Runtime quick-terminal screen selection, animation timing, focus-loss
autohide behavior, and app C ABI accessors are out of scope.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config::quick_terminal_screen` with upstream macOS default `main`.
  - Add `QuickTerminalScreen::{Main, Mouse, MacosMenuBar}`.
  - Route `quick-terminal-screen` through `Config::set`, config loading
    diagnostics, clone/equality, and formatting.
  - Add `Config::quick_terminal_animation_duration` as `f64` with upstream
    default `0.2`.
  - Route `quick-terminal-animation-duration` through the existing `f64` field
    parser/formatter.
  - Add `Config::quick_terminal_autohide` as `bool` with upstream macOS default
    `true`.
  - Route `quick-terminal-autohide` through the existing bool field
    parser/formatter.
  - Preserve the current local formatter convention by inserting all three keys
    after `gtk-quick-terminal-namespace`, matching upstream declaration order.

Out of scope:

- Runtime quick-terminal screen selection, animation, focus-loss autohide, or
  toggle actions.
- C ABI `roastty_config_get` exposure for these fields; Exp 10 documented that
  the app accessor is currently inert and that remains a later
  feature-completion item.
- The following quick-terminal fields: `quick-terminal-space-behavior`,
  `quick-terminal-keyboard-interactivity`, and later quick-terminal options.
- Any broader formatter reordering of already-ported keys.

## Verification

- Run formatting:
  - `cargo fmt`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/83-quick-terminal-screen-animation-config.md`
- Run targeted tests:
  - `cargo test -p roastty quick_terminal_screen_animation`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - defaults are `QuickTerminalScreen::Main`, animation duration `0.2`, and
    autohide `true`;
  - default `format_config` emits the three keys after
    `gtk-quick-terminal-namespace` and before `font-family`;
  - all three screen keywords parse and format;
  - an empty screen value resets to `main`;
  - unknown screen keywords are `ConfigSetError::InvalidValue`;
  - missing screen values are `ConfigSetError::ValueRequired`;
  - animation duration parses and formats normal floating-point values;
  - an empty animation duration resets to `0.2`;
  - missing animation duration values are `ConfigSetError::ValueRequired`;
  - malformed animation duration values are `ConfigSetError::InvalidValue`;
  - autohide parses explicit bool values and a bare key as `true`;
  - an empty autohide value resets to `true`;
  - malformed autohide values are `ConfigSetError::InvalidValue`;
  - `Config::load_str` records diagnostics for invalid neighboring
    quick-terminal lines while preserving valid parsed values;
  - clone/equality preserves all three field values.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = all three quick-terminal fields are represented faithfully on
`Config`, round-trip through config loading/formatting, match upstream macOS
defaults and parser behavior for this slice, and have targeted and full tests
passing.

**Partial** = one or two fields land completely, but another requires a
follow-up.

**Fail** = any key cannot be represented faithfully without first implementing
runtime quick-terminal behavior or C ABI accessors.

## Design Review

Codex adversarial reviewer `019eb497-6d03-7d51-8702-62f38b173739` returned
**Approved** with no required findings.

The reviewer verified read-only that Experiment 83 is linked from the issue
README as `Designed`, upstream defaults and enum tags match the design, the
planned parser behavior matches upstream/local helpers, formatter placement
matches local order, scope is consistent with adjacent parser/formatter-only
quick-terminal experiments, the verification plan covers likely implementation
mistakes, and `git diff --check` passed for the issue docs.

## Result

**Result:** Pass

Implemented `quick-terminal-screen` in `roastty/src/config/mod.rs` as
`QuickTerminalScreen::{Main, Mouse, MacosMenuBar}` with upstream default `Main`.
The enum parses exact upstream keywords, formats through the existing enum
formatter path, resets an empty value to `main`, and reports missing or unknown
values through the expected `ConfigSetError` variants.

Implemented `quick-terminal-animation-duration` as an `f64` field with upstream
default `0.2`, routed through the existing `set_f64_field` and float formatter.
Implemented `quick-terminal-autohide` as a bool with upstream macOS default
`true`, routed through the existing bool field parser so a bare key parses as
`true` and an empty value resets to the default.

The first full-suite run caught two stale formatter-order assertions in older
quick-terminal tests: they still expected `font-family` immediately after the
Experiment 82 keys. Those assertions were updated to keep strict order coverage
across the newly inserted Experiment 83 keys, and the targeted plus full suites
were rerun successfully.

Verification passed:

- `cargo fmt`
- `cargo test -p roastty quick_terminal_screen_animation`
- `cargo test -p roastty quick_terminal`
- `cargo test -p roastty gtk_quick_terminal`
- `cargo test -p roastty config_format_config`
- `cargo test -p roastty`
  - 4521 unit tests passed
  - ABI harness passed with the existing 10 enum-conversion warnings
  - doc tests passed
- `cargo fmt --check`
- `git diff --check`

## Conclusion

The quick-terminal screen, animation-duration, and autohide config surface now
matches upstream macOS defaults, enum/float/bool parser behavior, empty-reset
behavior, formatter output, and diagnostics for this slice. Runtime
quick-terminal screen selection, animation, focus-loss autohide behavior, and
app C ABI accessors remain later work. The next upstream quick-terminal fields
are `quick-terminal-space-behavior` and `quick-terminal-keyboard-interactivity`.

## Completion Review

Codex adversarial reviewer `019eb4a5-9c62-7533-9640-9289fe4c545c` returned
**Approved** with no findings.

The reviewer verified read-only that the diff is limited to the expected three
files, upstream defaults and tags match, the implementation is
parser/formatter/config-test only, no runtime behavior or C ABI accessor work
was added, README status and operating notes were updated, and the result docs
record the stale formatter-order assertion failure and fix.

The reviewer ran `git diff --check`, `cargo fmt --check`,
`cargo test -p roastty quick_terminal_screen_animation`,
`cargo test -p roastty config_format_config`, and `cargo test -p roastty`. The
full suite passed with 4521 unit tests plus the ABI harness and doc tests.
