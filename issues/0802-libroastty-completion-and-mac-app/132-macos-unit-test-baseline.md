# Experiment 132: Phase G — macOS unit-test baseline

## Description

Restore the copied Roastty macOS app's non-UI XCTest baseline after the Phase G
keybinding/config work. The normal CLI gate,
`cd roastty && macos/build.nu --action test`, now builds the app and runs unit
tests, but the partial `.xcresult` from the interrupted run shows 201 tests
started, 188 passed, 1 skipped, and 12 failing assertions in
`ConfigTests`/`MenuShortcutManagerTests`.

The failures cluster in two app-facing ABI surfaces:

- `roastty_config_get` does not yet expose several config keys that the copied
  Swift app already reads (`auto-update-channel`, `focus-follows-mouse`,
  `maximize`, `window-title-font-family`, `macos-titlebar-style`,
  `macos-window-shadow`, `resize-overlay`, and `scrollbar`).
- `roastty_config_trigger`/the keybind store do not yet model the menu shortcut
  behavior expected by the copied tests: configured keybinds must override
  built-in defaults, and `keybind = super+d=unbind` must suppress the built-in
  `new_split:right` menu shortcut.

This experiment is deliberately limited to the non-UI app-test gate. The
command-palette UI automation timeout from Experiment 129 remains a later UI
harness problem; this experiment should not broaden into XCUITest permissions or
visual automation.

## Changes

- In `roastty/src/lib.rs`, extend `roastty_config_get` with the missing scalar
  and enum getters needed by the copied Swift config tests, returning the same C
  types the Swift app already requests.
- In `roastty/src/lib.rs`, adjust config-trigger lookup semantics so app menu
  shortcut sync can distinguish three cases: explicit configured shortcut,
  explicit `unbind`, and default shortcut fallback.
- Add focused Rust tests for the new `roastty_config_get` keys and keybind
  lookup semantics, including uppercase/unicode normalization where relevant.
- If the existing Swift bridge needs no logic changes, leave it untouched; if a
  tiny bridge change is required to represent `unbind` distinctly, keep it
  mechanical and covered by the copied Swift tests.
- Update this experiment's result and Issue 802's operating notes/roadmap after
  verification.

## Verification

Pass criteria:

- `cargo fmt`
- `cargo test -p roastty -- --test-threads=1`
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/ConfigTests`
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/MenuShortcutManagerTests`
- `cd roastty && macos/build.nu --action test`
- `git diff --check`

The final full macOS unit-test gate must either pass or, if it still hangs after
all listed unit-test assertions are fixed, the result must identify the exact
remaining hanging test/process with evidence. UI tests are out of scope for this
experiment.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, `Ptolemy the 3rd`)

**Verdict:** Approved

**Findings:** None.

## Result

**Result:** Partial

Implemented the app-facing ABI pieces that explained the old finalized XCTest
failures:

- `roastty_config_get` now exposes the Swift-read config keys
  `auto-update-channel`, `focus-follows-mouse`, `maximize`,
  `window-title-font-family`, `macos-titlebar-style`, `macos-window-shadow`,
  `resize-overlay`, and `scrollbar`.
- `roastty_config_trigger` now suppresses default menu shortcut fallback when a
  configured binding shadows the same trigger, including the direct
  `keybind = super+d=unbind` case. Default action aliases such as `close_tab`
  and `copy_to_clipboard` compare against their canonical default actions, so a
  configured canonical binding on the default shortcut does not accidentally
  erase menu sync.
- `unbind` is accepted as a direct config keybind action so it can represent the
  upstream binding-set mutation semantics needed by menu sync, but it is
  non-performing at app/surface dispatch time and remains rejected for `chain=`.
- `macos-window-shadow` is now a parsed config field with the upstream default
  `true`, so the getter returns configured values instead of a hard-coded
  default.
- The implementation did not require Swift app changes.

Verification passed:

- `cargo fmt`
- `cargo fmt --check`
- `cargo build -p roastty`
- `cargo test -p roastty config_get_ -- --test-threads=1` — 36 passed, including
  the new config getter tests.
- `cargo test -p roastty config_trigger_ -- --test-threads=1` — 12 passed,
  including the new override/unbind and alias trigger tests.
- `cargo test -p roastty macos_window_shadow -- --test-threads=1` — 1 passed.
- `cargo test -p roastty -- --test-threads=1` — 4746 unit tests passed, plus the
  C ABI harness and doc tests.
- `git diff --check`

The macOS XCTest gate did not pass. A focused retry of
`cd roastty && macos/build.nu --action test --only-testing RoasttyTests/ConfigTests`
again hung before producing a finalized `.xcresult`; the spawned host was
`/Users/ryan/dev/termsurf/roastty/macos/build/Debug/Roastty.app/Contents/MacOS/roastty`.
Sampling that process (`/tmp/roastty-configtests-hang.sample.txt`) showed the
main thread in XCTest's `-[XCTestDriver _prepareTestConfigurationAndIDESession]`
/ `-[XCTFuture _waitForFulfillmentSync:withCompletion:]`, before an individual
`ConfigTests` frame. That means the remaining blocker is the Xcode test-host
session setup/exit path, not a reproduced failing config assertion.

All Xcode/Roastty processes spawned by the experiment were terminated after the
bounded runs.

## Conclusion

The old assertion cluster is addressed at the Rust embedded-ABI boundary and is
covered by deterministic tests, but the copied app's CLI-driven XCTest harness
still needs a separate experiment focused on test-host lifecycle/session setup.
The next experiment should make the macOS unit-test gate deterministic, likely
by isolating why XCTest waits during IDE-session preparation before running the
selected test suite.

## Completion Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, `Plato`)

**Verdict:** Approved

The first result review found two required issues, both fixed and re-reviewed:

- Alias reverse lookup is now canonicalized through
  `default_config_action_alias`, so app requests for `close_tab` and
  `copy_to_clipboard` match configured canonical default actions instead of
  being suppressed as shadows. The regression test
  `config_trigger_alias_action_returns_configured_canonical_binding` covers the
  case.
- `macos-window-shadow` is now a parsed config field with upstream default
  `true`, parser/formatter support, a getter that returns
  `config.parsed.macos_window_shadow`, and coverage for default true plus
  configured false.

The re-review independently ran `cargo fmt --check`,
`cargo test -p roastty config_get_ -- --test-threads=1`,
`cargo test -p roastty config_trigger_ -- --test-threads=1`, and
`cargo test -p roastty macos_window_shadow -- --test-threads=1`; all passed. No
new required findings remained.
