# Experiment 3: Fix Browser Drag Forwarding

## Description

Experiment 2 added the `browser-input-granularity` scenario and proved ordinary
browser text input, special keys, caret/focus state, click counts, and
modifier-click. It failed only at browser drag selection. The logs showed
Roamium received the drag down/up and mouse move events, but the final drag move
arrived with `modifiers=0`; Chromium therefore did not see an active left-button
drag and the page reported an empty browser selection.

This experiment will make the smallest Ghostboard app fix for that scoped
failure: TermSurf mouse moves generated from AppKit drag events must preserve
the active mouse-button modifier before forwarding to Roamium.

## Changes

Planned source changes:

- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - In `forwardTermSurfMouseMove`, preserve explicit button-state bits for
    AppKit drag events:
    - `.leftMouseDragged` keeps the left-button modifier bit;
    - `.rightMouseDragged` keeps the right-button modifier bit;
    - `.otherMouseDragged` keeps the middle/other-button modifier bit.
  - Keep ordinary hover/move forwarding behavior unchanged.
  - If existing AppKit geometry logs are not enough to prove terminal-selection
    suppression, add a narrow TermSurf/AppKit trace log for forwarded overlay
    mouse events and drag moves so the harness can assert that browser-drag
    down/move/up events were consumed by the overlay path and did not fall
    through to terminal selection handling.

Planned issue-document changes:

- Record the result in this experiment file.
- Update the Issue 817 README status for Experiment 3 after verification.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0817-ghostboard-input-focus-regression-matrix/README.md issues/0817-ghostboard-input-focus-regression-matrix/03-fix-browser-drag-forwarding.md`.

Static checks:

1. `git diff --check`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. `swiftc -typecheck scripts/ghostty-app/inject.swift`.

Build checks:

1. From `ghostboard/macos`, run
   `./build.nu --configuration Debug --action build`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh browser-input-granularity`.

Pass criteria:

- The app build succeeds.
- `browser-input-granularity` passes.
- The passing Roamium trace shows the browser drag move carries the active
  left-button modifier instead of `modifiers=0`.
- The page reports non-empty browser drag selection for
  `ISSUE817_BROWSER_DRAG_TEXT`.
- Browse-mode `Cmd+C` copies `ISSUE817_BROWSER_DRAG_TEXT` to the clipboard after
  the browser drag.
- Terminal-selection suppression is directly proven by a reliable observable
  that would fail if terminal selection were created during the browser drag,
  such as AppKit/Ghostboard trace logs showing the drag down/move/up were
  consumed by TermSurf overlay forwarding with no terminal fallback, or an
  equivalent selection-state/screenshot assertion.
- Existing text input, special-key, caret/focus, click-count, and modifier-click
  assertions in the scenario still pass.

Partial criteria:

- The drag move carries the active button modifier and browser drag selection is
  proven, but terminal-selection suppression still lacks a reliable observable.
- The app fix builds, but `browser-input-granularity` exposes a different
  already-existing failure unrelated to drag forwarding.

Fail criteria:

- The app build fails.
- Drag moves still reach Roamium without active button modifiers.
- Browser drag selection remains empty.
- The fix changes ordinary hover/move routing or regresses the already-passing
  keyboard/click rows.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, then
commit the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Banach`:

- **Initial verdict:** Changes required.
- **Finding 1:** The pass criteria treated Browse-mode clipboard copy as proof
  of terminal-selection suppression. Fixed by requiring a direct suppression
  observable that would fail if terminal selection were created during browser
  drag, such as AppKit/Ghostboard forwarded-overlay logs with no terminal
  fallback or an equivalent selection-state/screenshot assertion.
- **Final verdict:** Approved. The reviewer confirmed the prior Required finding
  was resolved and no new Required findings were introduced.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 817 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.

## Result

**Result:** Pass

Implemented the scoped Ghostboard drag-forwarding fix and tightened the
`browser-input-granularity` scenario so the passing row proves both browser drag
selection and direct TermSurf overlay consumption.

Source and harness changes:

- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  preserves explicit button-state modifier bits for AppKit drag move events
  before forwarding the move to Roamium. Left-button drags now carry modifier
  bit `64`; right- and other-button drags preserve their corresponding bits.
- The same AppKit forwarding path now emits a narrow `mouse_forwarded` geometry
  trace for TermSurf overlay mouse events and moves. The trace records the
  forwarded kind, AppKit event type, button, click count, modifiers, and
  `terminal_fallback=false`.
- `scripts/ghostboard-geometry-matrix.sh` now asserts that the browser drag
  down, AppKit dragged move, and drag up were consumed by TermSurf overlay
  forwarding in order. The move assertion specifically requires AppKit
  `ns_event=6` and the active left-button modifier.

Verification performed:

- `bash -n scripts/ghostboard-geometry-matrix.sh` — pass.
- `swiftc -typecheck scripts/ghostty-app/inject.swift` — pass.
- `git diff --check` — pass.
- `./build.nu --configuration Debug --action build` from `ghostboard/macos` —
  pass. The build still reports the pre-existing `GhosttyPackage.swift` Sendable
  warning, the pre-existing `SurfaceView_AppKit.swift` Swift 6 actor warning,
  and dSYM symbol warnings.
- `scripts/ghostboard-geometry-matrix.sh browser-input-granularity` — pass.

Passing runtime evidence:

- Harness log:
  `logs/ghostboard-geometry-browser-input-granularity-harness-20260618-010522.log`
- App log:
  `logs/ghostboard-geometry-browser-input-granularity-app-20260618-010522.log`
- Roamium trace:
  `logs/ghostboard-geometry-browser-input-granularity-roamium-20260618-010522.log`
- Web TUI state trace:
  `logs/ghostboard-geometry-browser-input-granularity-webtui-20260618-010522.log`
- Screenshot:
  `logs/ghostboard-geometry-browser-input-granularity-screenshot-20260618-010522.png`

The passing run proves:

- browser text input, special keys, caret insertion/deletion, tab focus, enter
  activation, single click, double click, triple click, and modifier-click still
  pass;
- the drag down was consumed by TermSurf overlay forwarding;
- the AppKit dragged move was consumed by TermSurf overlay forwarding after the
  down event with `ns_event=6` and `modifiers=64`;
- the drag up was consumed by TermSurf overlay forwarding after the dragged
  move;
- Roamium received browser drag moves with the active left-button modifier;
- the page reported non-empty browser drag selection for
  `ISSUE817_BROWSER_DRAG_TEXT`;
- Browse-mode `Cmd+C` copied `ISSUE817_BROWSER_DRAG_TEXT`, proving the browser
  selection, not terminal selection, owned the active copy target; and
- returning to Control mode cleared browser focus.

## Conclusion

The confirmed Ghostboard-owned drag gap was in the AppKit move forwarding path:
drag-generated mouse moves did not reliably carry an active mouse-button
modifier to Roamium. Preserving the drag button bit fixes Chromium drag
selection, and the new direct AppKit `mouse_forwarded` assertions make the
regression guard durable without relying only on clipboard state.

Issue 817 now has a focused automated row for browser text, special keys,
click-count granularity, modifier-click, drag selection, copy of browser
selection, focus return to Control mode, and direct overlay drag consumption.

## Completion Review

Fresh-context adversarial completion review by Codex subagent `Heisenberg`:

- **Initial verdict:** Changes required.
- **Finding 1:** The drag-move assertion could match a non-drag mouse move with
  the left-button modifier and did not prove ordered down → dragged move → up.
  Fixed by adding an ordered AppKit log matcher and requiring the drag move to
  include `ns_event=6` and `modifiers=64` after the down event and before the up
  event.
- **Re-verification:** After the fix,
  `bash -n scripts/ghostboard-geometry-matrix.sh`,
  `swiftc -typecheck scripts/ghostty-app/inject.swift`, `git diff --check`, and
  `scripts/ghostboard-geometry-matrix.sh browser-input-granularity` all passed.
- **Final verdict:** Approved. The reviewer confirmed the ordered assertions now
  search after the matched down line, require `ns_event=6` and `modifiers=64`
  for the dragged move, then search after the matched move line for the up
  event. The reviewer also confirmed the `20260618-010522` app and harness logs
  prove that order and reported no new Required findings.
