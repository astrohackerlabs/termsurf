# Experiment 155: Phase G — command-palette hosted execution

## Description

Experiment 129 added an explicit `macos/build.nu --action test --ui-tests`
command-palette UI gate and strengthened the copied UI tests so command
selection must execute an observable action. The gate remains blocked in this
environment before any test body runs:

```text
Timed out while enabling automation mode.
```

That leaves two separate facts:

1. full XCTest UI automation is still an environment/permission problem; and
2. the copied app's command-palette action path can still be tested without
   automation mode by hosting the SwiftUI command-palette components or by
   factoring a tiny command-option builder seam into ordinary hosted macOS unit
   tests.

This experiment targets the second fact only. Add a non-UI hosted test gate that
proves command-palette entries from `Roastty.Config.commandPaletteEntries`
become `CommandOption`s whose selection calls the copied app's `onAction`
closure, and that the controller-side `performAction` path reaches
`roastty_surface_binding_action` for a real `Roastty.SurfaceView`. This gives
Issue 802 an automated, default `macos/build.nu --action test` proof for command
palette action execution while keeping the full UI automation blocker visible.

## Changes

- `roastty/macos/Sources/Features/Command Palette/TerminalCommandPalette.swift`
  - Extract the pure terminal command-entry mapping into an internal helper that
    can be called from hosted unit tests without rendering the full SwiftUI
    overlay or launching XCTest UI automation.
  - Preserve copied-app behavior: filter unsupported entries, keep titles and
    descriptions from `Roastty.Command`, compute shortcut symbols through the
    existing `Roastty.Config.keyboardShortcut(for:)` lookup, and call the same
    `onAction(c.action)` closure used by the live command palette.
  - Avoid changing the visual UI layout, filtering/ranking behavior, or update /
    jump options.
- `roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift`
  - If necessary, expose the existing `performAction(_:, on:)` seam narrowly
    enough for `@testable import Roastty` hosted tests to call it, without
    changing app behavior.
- `roastty/macos/Tests/Roastty/CommandPaletteHostedTests.swift`
  - Add hosted macOS unit tests that do not require UI automation mode:
    - custom/default command-palette entries produce selectable command options;
    - unsupported entries are filtered out;
    - keyboard shortcut symbols are attached when a configured keybind maps to
      the command action;
    - invoking a command option records the exact action string through the
      copied `onAction` callback;
    - `performAction` with a real `Roastty.SurfaceView` and a benign binding
      action returns through `roastty_surface_binding_action` without crashing
      or being a dismissal-only no-op. Prefer an action with an observable
      postcondition already exposed by the Rust ABI if one is available; if not,
      use a parseable action and document the limit honestly in the result.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add an operating note distinguishing this hosted execution gate from the
    still-blocked full XCTest UI automation gate.
  - Update the Phase G command-palette checklist only to the degree proven: the
    action-entry/delegate path may be marked hosted-test covered, but full UI
    open/filter/click behavior must remain listed as pending until
    `RoasttyUITests/RoasttyCommandPaletteTests` can actually run.

Out of scope:

- Fixing the macOS `Timed out while enabling automation mode` failure.
- Rewriting `CommandPaletteView` or the copied SwiftUI UI.
- Making UI tests part of the default `macos/build.nu --action test` path.
- Native keymaps, global shortcut installation, or broader Phase G cleanup.

## Verification

- Format markdown:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/155-command-palette-hosted-execution.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Swift syntax/lint for edited Swift files:
  - `swiftlint lint roastty/macos/Sources/Features/Command\\ Palette/TerminalCommandPalette.swift roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift roastty/macos/Tests/Roastty/CommandPaletteHostedTests.swift`
    if `swiftlint` is available
- Hosted macOS tests:
  - `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/CommandPaletteHostedTests`
  - `cd roastty && macos/build.nu --action test`
- Existing focused UI gate, expected to remain blocked unless the environment
  has changed:
  - `cd roastty && macos/build.nu --action test --ui-tests --only-testing RoasttyUITests/RoasttyCommandPaletteTests`
  - Record the exact result. A continued automation-mode timeout does not fail
    this experiment, but it must remain documented as the blocker for full UI
    command-palette coverage.
- Rust checks if any Rust ABI behavior changes:
  - `cargo fmt`
  - `cargo test -p roastty command_palette -- --test-threads=1`
  - `cargo test -p roastty --test abi_harness`
- Hygiene:
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/155-command-palette-hosted-execution.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = the default hosted macOS test path proves command-palette command
entries are converted into selectable options, unsupported entries are filtered,
shortcut labels are attached, selecting an option calls the exact copied-app
`onAction` closure, and a real surface action can be dispatched through
`performAction` / `roastty_surface_binding_action`; normal hosted app tests
still pass; the README continues to state that full XCTest UI automation remains
blocked if the focused UI test still times out.

**Partial** = the command-option mapping is covered, but the real
`performAction` / `roastty_surface_binding_action` dispatch cannot be observed
without a broader app/controller harness.

**Fail** = hosted tests cannot exercise the copied command-palette action path
without changing copied app behavior or relying on XCTest UI automation.

## Design Review

**Reviewer:** Codex-native adversarial subagent `Gauss` with fresh context,
using the `adversarial-review` skill's Codex path
(`multi_agent_v1.spawn_agent`), not Claude's named `adversarial-reviewer` agent.

**Verdict:** Approved.

**Findings:** No Required, Optional, or Nit findings.

**Final verdict:** Approved.

## Result

**Result:** Pass

Added a hosted, non-UI command-palette execution gate that runs in the default
macOS test path:

- `TerminalCommandPaletteView.terminalCommandOptions(...)` now exposes the pure
  terminal command-entry mapping used by the live command palette.
- The live `terminalOptions` path still reads `appDelegate.roastty.config`,
  still filters unsupported commands, still derives keyboard shortcut symbols
  from `Roastty.Config.keyboardShortcut(for:)`, and still calls the copied
  `onAction(c.action)` callback when a command is selected.
- `CommandPaletteHostedTests` verifies:
  - command-palette config entries are read through the real
    `Roastty.Config.commandPaletteEntries` C ABI;
  - unsupported action keys are filtered out;
  - shortcut symbols are attached for a configured `keybind`;
  - selecting the option records the exact action through the copied `onAction`
    callback;
  - a real `Roastty.SurfaceView` can dispatch `clear_screen` through
    `Roastty.Surface.perform(action:)`, which wraps
    `roastty_surface_binding_action`, and rejects an invalid action.

Verification:

- `swiftlint lint roastty/macos/Sources/Features/Command\ Palette/TerminalCommandPalette.swift roastty/macos/Tests/Roastty/CommandPaletteHostedTests.swift`
  — pass, 0 violations.
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/CommandPaletteHostedTests`
  — pass, 2 tests.
- `cd roastty && macos/build.nu --action test` — pass, 213 hosted tests.
  Existing SwiftLint/Main Thread Checker/pasteboard warnings remain.
- `cd roastty && macos/build.nu --action test --ui-tests --only-testing RoasttyUITests/RoasttyCommandPaletteTests`
  — pass as a process, but executed 0 UI tests. This does not prove full UI
  behavior, so the README keeps full command-palette UI open/filter/click
  coverage listed as unproven.
- `cargo fmt --check` — pass.
- `git diff --check` — pass.
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/155-command-palette-hosted-execution.md issues/0802-libroastty-completion-and-mac-app/README.md`
  — pass.

## Conclusion

The command-palette action-entry/delegate path is now covered by ordinary hosted
macOS tests that do not require XCTest UI automation mode. This closes the
dismissal-only proof gap for configured command entries and proves the
surface-action dispatch wrapper reaches `roastty_surface_binding_action`.

Full UI behavior remains separate: the focused `RoasttyUITests` command now
returns success in this environment, but it runs zero UI tests, so Issue 802
must still treat command-palette open/filter/click UI automation as unproven
until that selector actually executes `RoasttyCommandPaletteTests`.

## Completion Review

**Reviewer:** Codex-native adversarial subagent `Hume` with fresh context, using
the `adversarial-review` skill's Codex path (`multi_agent_v1.spawn_agent`), not
Claude's named `adversarial-reviewer` agent.

**Verdict:** Approved.

**Findings:** No Required, Optional, or Nit findings.

**Final verdict:** Approved.
