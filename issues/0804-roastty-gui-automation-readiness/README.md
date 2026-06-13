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

External keyboard input into the live Roastty terminal is a **required outcome**
for this issue. XCTest keyboard automation and launch-time bootstrap command
delivery are useful fallback tools, but they do not satisfy the main readiness
gate by themselves: future Roastty GUI work must be able to synthesize keyboard
input into the actual running terminal window and prove that the terminal
received it.

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

### Current Keyboard Failure Hypotheses

Experiments 2 through 5 show that the failure is narrower than "Roastty cannot
handle keyboard input":

- XCTest can type into Roastty and observe terminal output.
- Launch-time bootstrap can run terminal commands.
- System Events and CGEvent keyboard posting return successfully while Roastty
  is frontmost and visible, but no marker command reaches the terminal.
- Accessibility, Automation to System Events, and Input Monitoring have been
  granted to the Ghostty-hosted agent path.

The working hypotheses are:

- **Terminal view focus / first responder:** Roastty may be frontmost without
  the terminal surface being the first responder. Issue 802 had a
  `focus_terminal_view` click step before typing in some harness paths, and
  XCTest explicitly clicks `"Terminal pane"` before typing.
- **Activation semantics:** `System Events` `set frontmost` may not be
  equivalent to a normal user click into the terminal surface, especially in a
  macOS VM.
- **Timing / focus settling:** Issue 802 found that the first key after
  activation could be dropped. The VM may need a longer sequence: activate,
  click terminal content, wait, warm up, then type.
- **VM-specific HID or TCC behavior:** macOS inside Parallels may route
  synthetic keyboard events differently than bare-metal macOS, even when
  Accessibility and Input Monitoring report as granted.
- **Responsible-process mismatch:** TCC may attribute event posting to a helper
  process differently than expected, even though the visible parent process is
  Ghostty.
- **Roastty/AppKit event dispatch:** Events may enter the app but miss
  `SurfaceView_AppKit.keyDown` / text input handling, or enter that path and
  fail later during terminal forwarding.

These are hypotheses, not conclusions. The true cause may be something else. The
next experiments should gather direct evidence, especially by clicking the
terminal surface before typing and by instrumenting Roastty's AppKit keyboard
entry points (`keyDown`, `insertText`, marked text, first-responder/focus
callbacks) to determine where the synthetic keyboard events disappear.

## Learnings

Record concrete, reproducible findings here as this issue discovers how to make
Roastty GUI automation work. Keep hypotheses in Analysis until they are proven.

- **Accessibility must be granted to Ghostty, the responsible host app for this
  Codex session.** Without it, the automation preflight fails. After the grant
  and Ghostty restart, `AXIsProcessTrusted()` returns `true`.
- **Automation permission from Ghostty to System Events is required and
  currently granted.** The System Settings Automation pane shows
  `Ghostty -> System Events` enabled, and
  `osascript -e 'tell application "System Events" to count processes'` succeeds.
- **Input Monitoring was granted to Ghostty but did not fix external keyboard
  delivery.** TCC logs show `kTCCServiceListenEvent` was modified for
  `com.mitchellh.ghostty`, but both System Events and CGEvent keyboard marker
  tests still failed afterward.
- **Window screenshots work.** `scripts/roastty-app/screenshot.sh` can capture
  the visible Roastty window with `screencapture -l`, producing `1600x1264px`
  PNGs for the `800x632pt` debug window.
- **`scripts/roastty-app/winid.swift` must prefer the visible onscreen layer-0
  window.** Experiment 2 fixed an early harness issue where the screenshot path
  selected an offscreen helper window instead of the real terminal window.
- **XCTest keyboard automation works against Roastty.**
  `RoasttyTerminalOutputUITests.testTerminalOutputIsVisibleToUIAutomation`
  passes and observes `TERMSURF_READY_158`; after the latest rerun,
  `RoasttyDeadKeyUITests.testDeadKeyCompositionCommitsText` also passes
  outright.
- **Launch-time bootstrap works and avoids external keyboard injection.**
  Launching `roastty/macos/build/Debug/Roastty.app/Contents/MacOS/roastty` with
  per-run `ZDOTDIR`, `XDG_CONFIG_HOME`, and shell startup files can run a recipe
  and display deterministic terminal content.
- **The actual debug app path is `roastty/macos/build/Debug/Roastty.app`.**
  Older derived-data-style paths such as
  `roastty/macos/build/Build/Products/Debug/Roastty.app` are stale for this
  harness.
- **CGEvent mouse scroll works against Roastty.** With bootstrap content
  `seq 1 200`, `scripts/roastty-app/scroll.swift` moves the viewport from tail
  lines `178..200` to top/history lines `1..24` and back.
- **CGEvent drag selection works when coordinates hit the text row.** A first
  drag at `windowY + 95pt` missed the text; a rerun at `windowY + 72pt` selected
  `DRAGSELECTME_TARGET_HERE`, and menu-driven Copy made `pbpaste` return that
  string.
- **Click/right-click events can be posted, but receipt is not yet strongly
  proven.** The commands return and Roastty remains frontmost, but the issue
  still needs a deterministic oracle such as a bootstrap-started byteprobe or
  mouse-reporting program.
- **External System Events keyboard and CGEvent keyboard still do not reach the
  Roastty terminal.** They return successfully while Roastty is frontmost and
  visible, but marker files are not created. This remains the primary blocker.
- **Clicking the terminal content before typing is not sufficient.** Experiment
  6 clicked both a safe terminal content point and the known text-row offset
  before retrying System Events and CGEvent keyboard input; both marker-file
  oracles still failed, and the post-attempt screenshot showed no typed text.
- **Keyboard synthesis can work in this VM, but focus targeting is unproven.**
  During Experiment 7, the System Events typing attempt produced the marker
  command text in the Ghostty/Codex window instead of Roastty. That proves the
  event source can generate keyboard input after the granted permissions, but it
  also invalidates the run as a Roastty keyboard test and shows the harness must
  prove the focused target immediately before typing.

## Verification

The issue is complete when an experiment proves, with logs or artifacts, that
the current environment can automatically:

- launch the copied Roastty macOS app;
- drive external synthetic keyboard input into the live Roastty terminal window
  and prove terminal receipt with a deterministic oracle;
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
  — **Partial** (XCTest, bootstrap, screenshots, scroll, and drag pass; external
  keyboard still fails, click/right-click lacks a strong receipt oracle)
- [Experiment 5: Rerun after Input Monitoring grant](05-rerun-after-input-monitoring-grant.md)
  — **Partial** (Input Monitoring grant confirmed for Ghostty; System Events and
  CGEvent keyboard still do not reach Roastty)
- [Experiment 6: Click terminal before keyboard](06-click-terminal-before-keyboard.md)
  — **Partial** (explicit terminal-content clicks did not make System Events or
  CGEvent keyboard input reach Roastty)
- [Experiment 7: Trace external keyboard entry](07-trace-external-keyboard-entry.md)
  — **Partial** (initial run invalidated because keyboard input targeted
  Ghostty/Codex, not Roastty; focus targeting must be proven before retrying)
- [Experiment 8: Focus-owned keyboard rerun](08-focus-owned-keyboard-rerun.md) —
  **Designed**
