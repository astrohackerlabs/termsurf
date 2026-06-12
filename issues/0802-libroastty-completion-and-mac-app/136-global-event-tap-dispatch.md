# Experiment 136: Phase G — global event tap dispatch

## Description

Validate the native global-key event path that is already present in the copied
macOS app.

Earlier Phase G work wired configured `global:` bindings into Roastty's app-key
dispatcher, and the copied app already has a `GlobalEventTap` that enables a
session event tap when `roastty_app_has_global_keybinds` is true. The remaining
gap is that the captured-event callback is private and untested: no hosted test
proves that a `CGEvent` captured while the app is inactive is converted to an
`NSEvent`, sent through `roastty_app_key`, and suppressed when a configured
global binding handles it.

This experiment adds a narrow testable dispatch seam around the existing event
tap callback. It does not attempt to install a live `CGEventTap` in tests, since
that depends on Accessibility permissions and can be flaky in CI/local
automation.

## Changes

- `roastty/macos/Sources/Features/Global Keybinds/GlobalEventTap.swift`
  - Extract the keydown dispatch body into an internal helper that accepts the
    event type, `CGEvent`, app-active state, and optional `roastty_app_t`.
  - Keep `cgEventFlagsChangedHandler` behavior unchanged: disabled taps are
    re-enabled, non-keydown events pass through, active-app events pass through,
    missing app/delegate/NSEvent pass through, and handled global bindings
    suppress the event by returning `nil`.
  - Preserve the existing event-tap creation, retry timer, and permission
    behavior.
- `roastty/macos/Tests/Roastty/GlobalEventTapTests.swift`
  - Add hosted tests that create temporary Roastty configs and raw
    `roastty_app_t` values without installing a real event tap.
  - Construct `CGEvent` keyboard events for macOS virtual keycode `0` (`KeyA`).
  - Prove an inactive app with `keybind = global:a=ignore` is handled and would
    be suppressed by the event tap callback.
  - Prove the same global binding is not handled while the app is active.
  - Prove a non-global `keybind = a=ignore` is not handled through the global
    tap path while inactive.
  - Prove non-keydown event types pass through.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After implementation, narrow the Phase G global-shortcut notes to say the
    callback dispatch path is hosted-test validated, while permission-dependent
    live tap installation remains outside automated tests unless a later
    experiment adds a stable harness for it.

Out of scope:

- Installing a real `CGEventTap` during tests or requiring Accessibility
  permission.
- Changing when `AppDelegate` enables or disables the shared event tap.
- Changing `roastty_app_key` semantics.
- Supporting `global:` trigger sequences, which the parser still rejects.
- Full Rust-side `KeymapDarwin` text translation or dead-key/preedit handling.

## Verification

- Run formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/136-global-event-tap-dispatch.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Run targeted Rust tests that cover app-key global dispatch:
  - `cargo test -p roastty app_key_global`
  - `cargo test -p roastty app_has_global_keybinds`
- Run the targeted macOS hosted test:
  - `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/GlobalEventTapTests`
- Run broader macOS coverage:
  - `cd roastty && macos/build.nu --action test`
- Run full Roastty tests:
  - `cargo test -p roastty -- --test-threads=1`
- Run checks:
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/136-global-event-tap-dispatch.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = the hosted test proves the event tap dispatch helper consumes an
inactive-app `CGEvent` only when a configured `global:` binding handles it, does
not consume active-app/non-global/non-keydown events, and the existing Rust
global app-key tests still pass.

**Partial** = the dispatch seam works but hosted macOS tests cannot construct a
stable `CGEvent`/`NSEvent` pair for the keybinding path.

**Fail** = the global event tap callback cannot be tested without installing a
real event tap or changing runtime semantics.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Sartre`, fresh context.

**Verdict:** Approved.

**Findings:** None.

The reviewer confirmed the README links Experiment 136 as `Designed`, the
experiment has the required sections, the scope is narrow and does not overclaim
live Accessibility-permission or event-tap installation validation, and the
formatting/diff checks passed.

## Result

**Result:** Pass

The copied macOS `GlobalEventTap` callback now routes keydown dispatch through
an internal helper that accepts the captured `CGEvent`, current app-active
state, and optional `roastty_app_t`. The callback still preserves the existing
disabled-tap re-enable path and pass-through behavior, but the key dispatch body
is now directly testable without installing a real event tap.

`GlobalEventTapTests` creates temporary Roastty configs and raw app handles,
then uses synthetic macOS keycode-0 `CGEvent` values to validate the interesting
cases:

- inactive `keybind = global:a=ignore` keydown events are handled and would be
  suppressed by the event tap;
- active-app keydown events pass through even when the same global binding is
  configured;
- inactive non-global `keybind = a=ignore` events pass through the global tap
  path;
- non-keydown event types pass through.

The first targeted hosted-test run failed to compile because the new test file
was missing `@testable import Roastty` for the internal helper and
`TemporaryConfig.config` access. Adding the testable import fixed the issue, and
the rerun passed.

Verification:

- `cargo test -p roastty app_key_global` passed 11 targeted unit tests.
- `cargo test -p roastty app_has_global_keybinds` passed 2 targeted unit tests.
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/GlobalEventTapTests`
  passed 4 hosted Swift tests in 1 suite.
- `cd roastty && macos/build.nu --action test` passed 206 hosted Swift tests in
  20 suites.
- `cargo test -p roastty -- --test-threads=1` passed 4,751 Rust unit tests, the
  C ABI harness, and doc tests.
- `cargo fmt --check` passed.
- `git diff --check` passed.
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/136-global-event-tap-dispatch.md issues/0802-libroastty-completion-and-mac-app/README.md`
  passed.

## Conclusion

Global event-tap captured-event dispatch is now covered by hosted automation:
the callback's key path converts a captured `CGEvent` into Roastty app-key input
and suppresses only inactive-app configured `global:` captures that the app
handles. The remaining native global-shortcut work is narrower: permission-
dependent live `CGEventTap` installation/accessibility validation, plus the
larger `KeymapDarwin` text translation and dead-key/preedit work that this
experiment intentionally left untouched.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Avicenna`, fresh
context.

**Verdict:** Approved.

**Findings:** None.

The reviewer inspected the uncommitted result diff from plan commit
`638052449cd1e`, verified the result commit had not been made before review, and
approved the implementation, tests, README status, and result documentation
without required changes.
