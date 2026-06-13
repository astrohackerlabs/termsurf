+++
status = "open"
opened = "2026-06-13"
+++

# Issue 804: Roastty GUI Automation Readiness

## Goal

Prove that this development environment can automatically drive and verify the
full Roastty macOS GUI window with keyboard input, mouse input, screenshots, and
observable oracles. If any automation path is blocked or unreliable, fix the
blocker or document the exact host permission/setup requirement before doing
more Roastty product work.

## Background

Issue 802 completed the copied, lightly renamed Ghostty macOS app port to
`libroastty` and relied heavily on real GUI automation. That work established a
tooling stack for driving Ghostty and Roastty:

- `scripts/ghostty-app/input-matrix.sh` for keyboard and mouse input mapping;
- `scripts/ghostty-app/inject.swift` for CGEvent mouse, scroll, key, and text
  injection;
- `scripts/ghostty-app/byteprobe.py` for raw PTY byte-level input oracles;
- `scripts/roastty-app/live-ab-smoke.sh` and
  `scripts/roastty-app/live-ab-matrix.sh` for live Ghostty-vs-Roastty visual
  comparison;
- focused helpers such as `scripts/roastty-app/click.swift`, `drag.swift`,
  `scroll.swift`, `keychord.swift`, `shiftclick.swift`, and `shiftdrag.swift`;
- XCTest UI suites under `roastty/macos/RoasttyUITests/`.

The most important operational lessons from Issue 802 were:

- keyboard input via System Events reaches only the frontmost app;
- mouse CGEvents hit-test only in the active Space;
- the reliable model is activate-first, then post global events;
- window screenshots use `screencapture -l` and window IDs, with artifacts
  outside the repo;
- Accessibility permission is required for System Events / CGEvent posting;
- Screen Recording permission is required for screenshot oracles;
- event-tap and global shortcut receipt can be constrained by macOS TCC
  permissions.

Before adding new Roastty behavior, this issue should re-prove that the current
VM/session has the necessary permissions and that the automation harnesses still
work end to end.

## Analysis

This issue is not a Roastty feature issue. It is a readiness gate for future
Roastty feature work.

The first experiment should audit and exercise the existing automation
capabilities without changing Roastty product code unless a real blocker is
found. It should cover:

- current macOS permission state for Accessibility, Screen Recording, and any
  event-tap/global shortcut requirements that can be checked from code;
- launching and stopping the debug Roastty app without leaving stale processes;
- activating the Roastty window and confirming it is frontmost;
- typing text and special keys into the terminal and proving receipt through a
  deterministic oracle;
- mouse click, drag, shift-click, and scroll injection against the actual
  Roastty window;
- screenshot capture for the full Roastty window and, where useful, cropped
  content-region capture;
- live A/B smoke or matrix execution against Ghostty when the real Ghostty app
  is available;
- XCTest UI automation availability for the copied Roastty app, including any
  host-level permission or automation-session requirements.

If any capability fails, the experiment should classify the failure precisely:

- missing host permission that the user must grant;
- VM or macOS session limitation;
- stale or broken helper script;
- app launch/build problem;
- product bug in Roastty;
- test oracle weakness.

Real helper or harness bugs should be fixed in this issue. Product bugs found
while validating automation may be fixed here only when the bug blocks proving
automation readiness; otherwise they should become their own focused Roastty
issue after this readiness gate is complete.

## Verification

The issue is complete when an experiment proves, with logs or artifacts, that
the current environment can automatically:

- launch the copied Roastty macOS app;
- drive keyboard input into the terminal;
- drive mouse click, drag, and scroll input into the terminal window;
- capture the full Roastty window;
- use deterministic non-OCR oracles where possible, such as PTY bytes,
  pasteboard contents, app/window state, or structured test output;
- run at least one live Roastty GUI smoke path without manual interaction after
  permissions are granted;
- clean up all launched app processes.

Any required System Settings permissions must be listed explicitly with the
responsible app/process name the user should grant.

As with other current issues, experiments should be created one at a time. Do
not add the `## Experiments` index until Experiment 1 is designed.

## Experiments

- [Experiment 1: Automation readiness audit](01-automation-readiness-audit.md) —
  **Partial** (blocked at Accessibility preflight; grant Ghostty Accessibility
  permission and rerun)
- [Experiment 2: Rerun after Accessibility grant](02-rerun-after-accessibility-grant.md)
  — **Partial** (build, launch, screenshot, and XCTest pass; external keyboard
  injection still does not reach Roastty)
- [Experiment 3: Keyboard rerun after Ghostty restart](03-keyboard-rerun-after-ghostty-restart.md)
  — **Partial** (restart did not make CGEvent or System Events keyboard input
  reach Roastty)
- [Experiment 4: Reproduce Issue 802 input methods against Roastty](04-reproduce-issue-802-methods-against-roastty.md)
  — **Designed**
