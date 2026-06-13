# Experiment 163: Phase G — global event tap installation state

## Description

Prove the copied macOS app's live global-shortcut installation logic without
depending on host Accessibility permission.

Experiment 136 validated captured-event dispatch after a `CGEventTap` has
already delivered a key event, but it intentionally left the installation path
opaque. The copied app enables `GlobalEventTap` when
`roastty_app_has_global_keybinds` reports configured `global:` bindings, retries
when tap creation fails, and disables the tap when global bindings disappear.
That behavior is the remaining native global-shortcut gap in the Phase G notes,
but direct `CGEvent.tapCreate` success is permission-dependent and unsuitable as
a required automated test.

This experiment makes the installation state machine observable and injectable
while keeping the production app's default path faithful to upstream: production
still calls `CGEvent.tapCreate`, installs the source on the main run loop, and
uses the existing retry timer. Hosted tests use an internal factory/scheduler
seam to simulate success, failure, retry, and disable without requesting
Accessibility permission.

## Changes

- `roastty/macos/Sources/Features/Global Keybinds/GlobalEventTap.swift`
  - Introduce internal test seams for tap creation and retry scheduling while
    keeping the shared singleton's default dependencies pointed at the real
    `CGEvent.tapCreate`, `CFRunLoopAddSource`, and `Timer.scheduledTimer` path.
  - Expose internal read-only state needed by hosted tests: whether an event tap
    is installed and whether a retry is pending.
  - Keep public runtime behavior unchanged: `enable()` remains idempotent,
    failed creation schedules periodic retries, successful creation clears the
    retry, and `disable()` invalidates both the retry and any installed tap.
  - Preserve the existing disabled-tap re-enable callback behavior from
    Experiment 136.
- `roastty/macos/Tests/Roastty/GlobalEventTapTests.swift`
  - Add hosted tests for the installation state machine using fake dependencies:
    immediate success installs once with no retry, repeated `enable()` is
    idempotent, failed creation schedules retry, retry success installs and
    cancels retry, and `disable()` tears down pending/installed state.
  - Keep the existing dispatch tests from Experiment 136.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After implementation, update the Phase G native-key note and roadmap to
    distinguish the newly tested installation state machine from the remaining
    host-permission reality: automated tests prove Roastty requests/maintains
    the live tap correctly, while actually receiving global keystrokes still
    depends on macOS granting Accessibility permission.

Out of scope:

- Requiring or automating Accessibility permission.
- Changing which config entries count as `global:` bindings.
- Changing `roastty_app_has_global_keybinds`, `roastty_app_key`, or key
  translation semantics.
- Adding a live UI test that sends global keystrokes to an inactive app.

## Verification

- Lint edited Swift files:
  - `swiftlint lint 'roastty/macos/Sources/Features/Global Keybinds/GlobalEventTap.swift' roastty/macos/Tests/Roastty/GlobalEventTapTests.swift`
- Run formatting for the issue docs:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/163-global-event-tap-installation-state.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Run targeted hosted macOS tests:
  - `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/GlobalEventTapTests`
- Run broader hosted macOS tests:
  - `cd roastty && macos/build.nu --action test`
- Run Rust tests that cover the C-side global binding predicate and app-key
  dispatch:
  - `cargo test -p roastty app_has_global_keybinds`
  - `cargo test -p roastty app_key_global`
- Run full Roastty coverage:
  - `cargo test -p roastty`
- Run checks:
  - `cargo fmt --check -p roastty`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/163-global-event-tap-installation-state.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = hosted tests prove the event tap installation state machine handles
success, idempotent enable, failure retry, retry success, and disable without
Accessibility permission, while existing dispatch tests and Rust global-key
tests continue to pass.

**Partial** = the state seam works, but one of the production dependencies
cannot be isolated without changing runtime behavior.

**Fail** = live tap installation cannot be tested without requesting
Accessibility permission or materially diverging from the copied app's runtime
path.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Parfit`, fresh context.

**Verdict:** Approved after one required verification fix.

**Findings:**

- Required: the initial design omitted the explicit `swiftlint` verification
  required by `roastty/macos/AGENTS.md` for Swift edits.

**Fix:** Added a `swiftlint lint` verification step covering
`GlobalEventTap.swift` and `GlobalEventTapTests.swift`.

The reviewer re-reviewed the fix and approved the design with no remaining
required findings.

## Result

**Result:** Pass

The copied macOS app's `GlobalEventTap` now has an internal dependency seam for
event-tap creation and retry scheduling. The shared production singleton still
uses the real path: `CGEvent.tapCreate`, `CFMachPortCreateRunLoopSource`,
`CFRunLoopAddSource`, `Timer.scheduledTimer`, and `CGEvent.tapEnable` for
system-disabled tap recovery. Tests instantiate isolated `GlobalEventTap`
instances with fake dependencies, so hosted automation proves the installation
state machine without requesting Accessibility permission.

`GlobalEventTapTests` now covers ten cases total. The six new installation tests
prove:

- immediate enable success installs one tap with no retry;
- repeated `enable()` does not reinstall an already installed tap;
- failed creation schedules a retry;
- retry success installs the tap and cancels the pending retry;
- `disable()` cancels a pending retry;
- `disable()` invalidates an installed tap.

The four existing dispatch tests from Experiment 136 still prove inactive-app
configured `global:` keydown suppression and active-app/non-global/non-keydown
pass-through behavior.

Verification:

- `swiftlint lint 'roastty/macos/Sources/Features/Global Keybinds/GlobalEventTap.swift' roastty/macos/Tests/Roastty/GlobalEventTapTests.swift`
  passed with 0 violations.
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/GlobalEventTapTests`
  passed 10 Swift Testing tests in 1 suite.
- `cd roastty && macos/build.nu --action test` passed 219 hosted Swift tests in
  23 suites. Existing Swift concurrency, Main Thread Checker, App Intents, and
  pasteboard warnings/noise remained.
- `cargo test -p roastty app_has_global_keybinds` passed 2 targeted Rust tests.
- `cargo test -p roastty app_key_global` passed 11 targeted Rust tests.
- `cargo test -p roastty` passed 4,850 Rust unit tests, 0 failed, 4 ignored; the
  C ABI harness passed with existing enum-conversion warnings; doc tests passed
  with 0 tests.
- `cargo fmt --check -p roastty` passed.
- `git diff --check` passed.
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/163-global-event-tap-installation-state.md issues/0802-libroastty-completion-and-mac-app/README.md`
  passed.

## Conclusion

Roastty now proves the copied app requests and maintains the live global event
tap correctly without making Accessibility permission a test prerequisite. The
remaining global-shortcut caveat is host-level: actually receiving inactive-app
global keystrokes still depends on macOS granting Accessibility permission to
the app, but the app-side enable/retry/disable logic is no longer an untested
gap.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Carver`, fresh context.

**Verdict:** Approved.

**Findings:**

- Nit: `GlobalEventTap.swift` still described the initializer as private after
  the test seam made it internal. Fixed by updating the comment.

No required findings remained, and the reviewer approved the completed
experiment before the result commit.
