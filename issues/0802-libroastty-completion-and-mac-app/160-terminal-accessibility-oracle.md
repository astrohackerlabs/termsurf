# Experiment 160: Phase G — terminal accessibility oracle

## Description

Experiment 159 restored the copied-app custom-config title gate. The focused
terminal-output UI selector now proves that `ROASTTY_CONFIG_PATH` reaches the
visible first window, but it still skips after failing to observe
`TERMSURF_READY_158` through the current terminal accessibility helper.

This experiment targets that remaining layer directly. The goal is to make a
focused copied-app UI test observe deterministic terminal output through the
actual terminal surface accessibility path, then decide whether the dead-key UI
test can require app-visible committed text.

The diagnostic must distinguish these layers:

1. The configured `initial-command` does not reach or execute in the first
   surface.
2. The command writes output, but the terminal screen/read-text path does not
   contain it.
3. `SurfaceView_AppKit.accessibilityValue()` has the output, but XCTest is
   querying the wrong element or role.
4. The copied-app accessibility tree exposes a text area, but its value is stale
   or incomplete because of cache invalidation or timing.

If the actual `SurfaceView` accessibility value already contains the marker, the
fix should be in the UI-test helper/query. If the value is stale or empty while
the terminal screen has output, the fix should be in the narrow
accessibility/read-text bridge. If the terminal screen itself lacks the marker,
record that product layer explicitly and do not claim dead-key visibility.

## Changes

- `roastty/macos/RoasttyUITests/RoasttyTerminalText.swift`
  - Query terminal text from the most precise accessible surface element
    available, not only the wrapper group and app-wide text view list.
  - Include role, label, value, and useful descendants in failure snapshots so a
    skip identifies whether XCTest found the terminal surface or a surrounding
    wrapper.
- `roastty/macos/RoasttyUITests/RoasttyTerminalOutputUITests.swift`
  - Keep the configured-title gate as a hard prerequisite.
  - Require the deterministic startup marker if the accessibility surface proves
    reliable.
  - If the marker still cannot be observed, skip only after recording which
    layer failed with a concrete snapshot.
- `roastty/macos/RoasttyUITests/RoasttyDeadKeyUITests.swift`
  - If the output oracle becomes reliable, replace the final app-visible `é`
    skip with a required assertion.
  - If the oracle remains unreliable, leave the route trace as the gate and
    document the remaining app-visible blocker.
- `roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift`
  - Touch only if the focused diagnostic proves a product/testability bug in the
    copied app's terminal accessibility value, selected text, or string range
    implementation.
  - Any product change must remain upstream-shaped and useful for real
    accessibility, not a test-only hidden hook.
- `roastty/src/lib.rs`
  - Touch only if the diagnostic proves `roastty_surface_read_text`,
    selection/range conversion, or first-surface command execution is wrong in
    the embedded ABI path.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add the experiment index line and update the Phase G note with the result.

Out of scope:

- Broad shell startup or command configuration rewrites.
- Global shortcut or native keymap work.
- UI-test-only environment hooks that bypass the copied app's real terminal
  accessibility tree.
- Claiming dead-key success from native route trace alone.
- Replacing the copied-app UI oracle with hosted unit-test snapshots.

## Verification

- Format markdown:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/160-terminal-accessibility-oracle.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Swift lint for edited Swift files:
  - `swiftlint lint roastty/macos/RoasttyUITests/RoasttyTerminalText.swift roastty/macos/RoasttyUITests/RoasttyTerminalOutputUITests.swift`
  - Include `RoasttyDeadKeyUITests.swift`, `SurfaceView_AppKit.swift`, or any
    other edited Swift source in the same lint run.
- Rust formatting if Rust changes:
  - `cargo fmt`
- Full Rust library tests if Rust library code changes:
  - `cargo test -p roastty`
- Default hosted app tests still skip UI by default:
  - `cd roastty && macos/build.nu --action test`
- Focused terminal-output UI gate:
  - `cd roastty && macos/build.nu --action test --ui-tests --only-testing RoasttyUITests/RoasttyTerminalOutputUITests`
  - Must report exactly 1 executed test. `Executed 0 tests` is a failure.
  - If this experiment claims `Pass`, the selector must have 0 skips and prove
    `TERMSURF_READY_158` through the copied app's terminal accessibility path.
  - If this experiment remains `Partial`, the selector may skip only after the
    configured-title gate and after the result records the exact failing layer.
- Focused dead-key UI gate if its final assertion changes:
  - `cd roastty && macos/build.nu --action test --ui-tests --only-testing RoasttyUITests/RoasttyDeadKeyUITests`
  - If this experiment claims dead-key `Pass`, the selector must execute exactly
    1 test with 0 skips and prove visible `é` plus the full native route trace.
  - If dead-key remains `Partial`, the selector may still skip, but only after
    proving `setMarkedText`, `insertText accumulated=é`, and
    `committedPreeditText text=é`.
- Hygiene:
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/160-terminal-accessibility-oracle.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = a focused copied-app UI test proves deterministic terminal output is
observable through the real terminal accessibility path with exactly 1 executed
test, 0 skips, and 0 failures; if dead-key assertions are strengthened, the
dead-key focused selector also passes with visible committed `é`.

**Partial** = the focused copied-app UI test identifies a narrower terminal
output/accessibility layer that still blocks app-visible output, while
preserving a real one-test selector and concrete failure evidence.

**Fail** = the selector executes zero tests, weakens the configured-title gate,
bypasses the copied app's terminal surface, relies only on hosted/unit
snapshots, or claims dead-key success without app-visible terminal output.

## Design Review

**Reviewer:** Codex-native adversarial subagent `Dalton` with fresh context,
using the `adversarial-review` skill's Codex path
(`multi_agent_v1.spawn_agent`), not Claude's named `adversarial-reviewer` agent.

**Verdict:** Approved.

The reviewer found no Required findings. It checked the README status link, the
experiment design structure, the scope against Experiments 158 and 159, the
terminal-accessibility focus, the no-hidden-test-hook constraint, concrete
verification gates, and the dead-key non-overclaim rule. It also ran
`git status --short`, the requested `git diff` commands, `git diff --check`, and
`prettier --check --prose-wrap always --print-width 80` for the README and
Experiment 160 design.
