# Experiment 157: Phase G — dead-key UI automation

## Description

Phase G's remaining native-key work is no longer the Rust `KeymapDarwin`
foundation or hosted preedit geometry. Experiments 137–140 covered those. The
remaining unproven part is the copied macOS app's live text-input path under
real UI automation: an AppKit `NSEvent` must pass through
`SurfaceView_AppKit.keyDown`, `interpretKeyEvents`, marked-text/preedit
callbacks, and finally `roastty_surface_key` / committed text delivery without
leaking dead-key control events to the terminal.

This experiment adds a focused UI gate for dead-key / IME-style composition. It
should prove the copied app can commit composed text through the live terminal
surface, not merely that the Swift unit-test host can call `setMarkedText(...)`.

The initial target is a deterministic macOS dead-key sequence, such as
`Option-E` followed by `E` producing `é`, with `macos-option-as-alt = false` so
AppKit gets the Option dead-key path. Because that sequence is layout-dependent,
the implementation must either assert/probe a compatible input source before
claiming `Pass`, or record `Partial` with the detected incompatible layout.

The test must verify two things:

1. An observable terminal outcome, preferably by selecting/copying the terminal
   contents through the app's existing selection/clipboard path or by reading
   the terminal accessibility value once it contains the committed text.
2. The route that produced that outcome. The implementation must prove the
   copied app path ran by synthesizing the sequence with
   `typeKey("e", modifierFlags: [.option])` followed by
   `typeKey("e", modifierFlags: [])` on the terminal element, and by adding a
   narrowly test-only observation hook or equivalent assertion showing
   `SurfaceView_AppKit.keyDown` saw the dead-key event, `interpretKeyEvents`
   produced marked or committed composition text, and no direct `sendText(...)`
   / paste shortcut was used.

If this environment cannot synthesize the native dead-key path reliably through
XCTest, the experiment should record `Partial` with the exact failure mode and
leave a smaller next step. It must not claim success from direct
`sendText(...)`, direct `setMarkedText(...)`, paste-only input, `typeText("é")`,
or other shortcuts that bypass the live copied-app key event path.

## Changes

- `roastty/macos/RoasttyUITests/RoasttyDeadKeyUITests.swift`
  - Add a focused UI test class using `RoasttyCustomConfigCase`.
  - Launch a clean Roastty app with a config that disables option-as-alt,
    disables disruptive prompts where needed, and gives the window a stable
    title.
  - Focus the `"Terminal pane"` surface and synthesize the real dead-key
    sequence with `typeKey`, not `typeText`.
  - Verify that the composed character is committed to the terminal through both
    an app-visible postcondition and a route observation proving the copied
    `keyDown` / `interpretKeyEvents` path ran.
- `roastty/macos/RoasttyUITests/AppKitExtensions.swift`
  - Add small UI-test helpers only if needed to poll pasteboard/accessibility
    text or assert composed output without duplicating brittle timing logic.
- `roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift`
  - Add a test-only observation hook only if needed to prove the route. It must
    be inert in normal app runs, enabled only by a UI-test launch environment
    flag, and record only bounded counters/strings needed by the test.
- `roastty/macos/RoasttyUITests/RoasttyCustomConfigCase.swift`
  - Reuse the existing empty-config default and launch environment. Only adjust
    the shared harness if the focused dead-key test reveals a general UI-test
    isolation issue; do not reintroduce test-suite suppression.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - If the focused UI test proves live dead-key composition, update the Phase G
    native-key checklist and operating notes.
  - If the environment cannot synthesize the dead-key path, record a `Partial`
    result and keep the dead-key UI automation gap explicit.

Out of scope:

- Permission-dependent global shortcut installation.
- Rewriting the copied app's input architecture.
- Changing `SurfaceView_AppKit.keyDown` unless the UI test exposes a concrete
  product bug.
- Broad IME matrix coverage across Japanese/Korean input sources.
- Making UI tests run by default.

## Verification

- Format markdown:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/157-dead-key-ui-automation.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Swift lint for edited Swift files:
  - `swiftlint lint roastty/macos/RoasttyUITests/RoasttyDeadKeyUITests.swift`
  - Include any optional edited helper/harness/source files in the same lint
    run, such as `AppKitExtensions.swift`, `RoasttyCustomConfigCase.swift`, or
    `SurfaceView_AppKit.swift`.
- Default hosted app tests still skip UI by default:
  - `cd roastty && macos/build.nu --action test`
- Focused dead-key UI gate:
  - `cd roastty && macos/build.nu --action test --ui-tests --only-testing RoasttyUITests/RoasttyDeadKeyUITests`
  - The result must report real `RoasttyDeadKeyUITests` execution. A process
    success with `Executed 0 tests` is not acceptable. If the experiment adds
    one test method, the class selector must report exactly 1 executed test; if
    it adds more, the expected count must be stated in the Result.
- If the class selector is ambiguous, run the individual test selector(s):
  - `cd roastty && macos/build.nu --action test --ui-tests --only-testing RoasttyUITests/RoasttyDeadKeyUITests/testDeadKeyCompositionCommitsText`
- Hygiene:
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/157-dead-key-ui-automation.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = the default hosted test path still skips UI tests and passes, while
the focused dead-key UI selector executes the expected real test count and
proves a composed character is committed through the copied app's live key event
path, including route evidence that `keyDown` / `interpretKeyEvents` handled the
composition.

**Partial** = the test target executes real bodies, but XCTest or the host
environment cannot synthesize the native dead-key path reliably, the current
input source is incompatible and cannot be switched/probed safely, the route
cannot be observed without product-risky hooks, or the test finds a concrete
product bug that needs a follow-up experiment.

**Fail** = the focused selector still executes zero tests, the verification
bypasses `SurfaceView_AppKit.keyDown` / `interpretKeyEvents`, or passing the
test requires making UI tests part of the default test action.

## Design Review

**Reviewer:** Codex-native adversarial subagents `Nietzsche` and `Planck` with
fresh context, using the `adversarial-review` skill's Codex path
(`multi_agent_v1.spawn_agent`), not Claude's named `adversarial-reviewer` agent.

**Initial verdict:** Changes required.

**Required finding:** The original design did not require implementation-level
proof that XCTest exercised `SurfaceView_AppKit.keyDown` / `interpretKeyEvents`,
instead of only proving that terminal output eventually contained composed text.

**Fix:** The design now requires `typeKey` dead-key synthesis on the terminal
element plus route evidence that `keyDown` saw the dead-key event,
`interpretKeyEvents` produced marked or committed composition text, and no
direct `sendText`, paste, or `typeText("é")` shortcut was used.

**Optional findings fixed:**

- The design now requires probing/asserting a compatible input source before
  claiming `Pass`, or recording `Partial` with the detected incompatible layout.
- The focused UI selector must report the expected real test count: exactly one
  test if this experiment adds one test method, or an explicitly stated count if
  it adds more.
- The lint command now names the required new test file separately and treats
  helper/harness/source files as conditional additions.

**Final verdict:** Approved. The re-review found all prior findings resolved and
no new Required findings.
