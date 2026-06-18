# Experiment 2: Prove Browser Input Granularity

## Description

Experiment 1 proved that the current VM and harness can drive the main
Ghostboard keyboard, mouse, focus, activation, and clipboard paths. It also
classified the weakest remaining Issue 817 area as browser-input granularity:
browser-received special keys, caret behavior, double-click, triple-click,
modifier-click, drag selection, and terminal-selection suppression.

This experiment will add one focused runtime scenario that serves a local HTML
fixture, enters Browse mode, drives real keyboard and mouse input through
Ghostboard, and verifies browser-observed results through webtui state-trace
console/title markers. The goal is to prove the behavior from the page's point
of view, not merely to prove that Roamium received a generic key or mouse event.

## Changes

Planned harness changes:

- Extend `scripts/ghostboard-geometry-matrix.sh` with a
  `browser-input-granularity` scenario.
- Add the scenario to the scenario whitelist and generate a local HTML fixture
  served by `python3 -m http.server`.
- The fixture should contain:
  - a focused text input with JavaScript listeners for `keydown`, `input`,
    `selectionchange`, `click`, `dblclick`, mouse detail counts, modifier flags,
    drag/selection state, and context-safe console markers;
  - visible static text that can be drag-selected;
  - deterministic `console.log(...)` and `document.title = ...` updates for each
    phase, so the harness can assert outcomes through webtui's state trace.
- Reuse existing `enter_browser_browse`, `leave_browser_browse`,
  `wait_for_state_trace`, `type_marker_require_only`, and mouse coordinate
  helpers where possible.
- If modifier-click cannot be generated with the current injector, extend only
  `scripts/ghostty-app/inject.swift` so
  `inject.swift click ... [count] [control|command|shift|option]` attaches flags
  to the mouse down/up events. Do not change app, webtui, Roamium, or protocol
  source in this experiment.

Planned runtime phases:

1. Load the fixture and wait for `ISSUE817_INPUT_READY`.
2. Enter Browse mode and click the text input inside the browser frame.
3. Type a unique token and require page-observed input value markers.
4. Send special keys through real keyboard events:
   - left arrow moves the caret;
   - `x` inserts at the caret;
   - backspace deletes the inserted character;
   - tab moves focus to the next fixture control;
   - enter records an activation/submit marker.
5. Assert caret/focus state through page markers such as `selectionStart`,
   `selectionEnd`, and `document.activeElement.id`. This proves browser caret
   state logically; it does not need a screenshot-only visible-caret assertion
   unless the page markers are inconclusive.
6. Send single-click, double-click, and triple-click input to the text region
   and require page markers showing `event.detail` values `1`, `2`, and `3`.
7. Send a modifier-click, preferably Shift-click, and require page markers
   showing the modifier flag reached the browser.
8. Drag across selectable text and require a page marker proving non-empty
   browser selection.
9. After the browser drag, require no Ghostboard/AppKit terminal-selection log
   or visible terminal-selection evidence if such a log exists. If no reliable
   terminal-selection signal exists, classify this row as `Partial` with the
   exact missing signal and make that signal the next experiment.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0817-ghostboard-input-focus-regression-matrix/README.md issues/0817-ghostboard-input-focus-regression-matrix/02-prove-browser-input-granularity.md`.

Static checks:

1. `git diff --check`.
2. If `scripts/ghostboard-geometry-matrix.sh` changes, run
   `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. If `scripts/ghostty-app/inject.swift` changes, run
   `swiftc -typecheck scripts/ghostty-app/inject.swift`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh browser-input-granularity`.

Pass criteria:

- The scenario launches Ghostboard, webtui, and Roamium against the local
  fixture.
- The page reports the typed token in its input value.
- The page reports the expected special-key effects for left arrow, inserted
  character, backspace, tab focus movement, and enter activation.
- The page reports logical caret/focus state using `selectionStart`,
  `selectionEnd`, and `document.activeElement.id`.
- The page reports single-click, double-click, and triple-click detail counts.
- The page reports at least one modifier-click flag.
- The page reports non-empty browser selection after drag.
- Terminal-selection suppression is directly proven with a reliable signal, such
  as a Ghostboard/AppKit selection-state log, a screenshot/pixel assertion that
  distinguishes browser selection from terminal selection, or another explicit
  observable that fails when terminal selection is created during browser drag.

Partial criteria:

- Browser keyboard, caret, click count, modifier, and drag selection are proven,
  but terminal-selection suppression lacks a reliable observable signal.
- The current injector can drive all rows except one narrowly identified event
  class, and the missing injector capability is documented with a follow-up
  recommendation.

Fail criteria:

- The scenario cannot launch or cannot enter Browse mode.
- The page cannot report browser-observed keyboard or mouse results through
  webtui state-trace markers.
- Input reaches the wrong browser, reaches no browser, or app/source changes
  outside the approved harness/injector scope are required to continue.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, then
commit the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Einstein`:

- **Initial verdict:** Changes required.
- **Finding 1:** The pass criteria allowed missing terminal-selection
  suppression evidence to count as `Pass`. Fixed by requiring direct reliable
  suppression evidence for `Pass` and leaving missing suppression observability
  only under `Partial`.
- **Finding 2:** The design was missing an explicit completion/result gate.
  Fixed by adding the Completion Gate section below.
- **Final verdict:** Approved. The reviewer confirmed both prior Required
  findings were resolved and no new Required findings were introduced.

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
