# Experiment 2: Rerun After Accessibility Grant

## Description

Rerun the automation readiness audit after the user granted macOS Accessibility
permission in System Settings. This experiment intentionally skips adversarial
review for this issue, per user instruction.

The goal is to continue past the Experiment 1 blocker and determine which GUI
automation surfaces now work:

- Accessibility and Apple Events preflight;
- Roastty build and launch;
- real window discovery and screenshot capture;
- external keyboard injection through CGEvent and System Events;
- XCTest UI automation;
- cleanup.

## Changes

- `scripts/roastty-app/winid.swift`
  - Fix the window resolver to prefer onscreen layer-0 windows. The first
    screenshot run selected an offscreen `500x500` helper window instead of the
    visible `800x632` Roastty terminal window.
- `issues/0804-roastty-gui-automation-readiness/02-rerun-after-accessibility-grant.md`
  - Record this rerun and its result.
- `issues/0804-roastty-gui-automation-readiness/README.md`
  - Add Experiment 2 to the issue index.

Environment setup performed on the VM:

- Installed `nushell` with Homebrew because `roastty/macos/build.nu` requires
  `nu`.
- Installed `cmake` with Homebrew because `shaderc-sys` needs it when building
  from source.
- Cloned `https://github.com/ghostty-org/ghostty` into gitignored
  `vendor/ghostty` and checked out `2c62d182c`, matching the documented Ghostty
  pin used by Roastty resources.

## Verification

Commands were run from the repo root unless noted. Transcripts were written to
`logs/`.

### Preflight

```bash
git status --short
git config --global user.name
git config --global user.email
swift -e 'import ApplicationServices; print(AXIsProcessTrusted())'
osascript -e 'tell application "System Events" to count processes'
```

Expected:

- Git identity is `Max Commits <maxcommits@ryanxcharles.com>`.
- Accessibility prints `true`.
- Apple Events to `System Events` returns a process count.

### Build and Launch

```bash
scripts/roastty-app/stop-app.sh || true
cd roastty && macos/build.nu --action build
cd ..
ROASTTY_PID="$(scripts/roastty-app/start-app.sh)"
pgrep -fl 'Roastty.app/Contents/MacOS/roastty'
swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID"
```

Expected:

- Build succeeds.
- Debug Roastty app launches.
- A visible Roastty terminal window is listed.

### Screenshot

```bash
scripts/roastty-app/screenshot.sh --list "$ROASTTY_PID"
swift scripts/roastty-app/winid.swift "$ROASTTY_PID"
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp2-window-visible
```

Expected:

- `winid.swift` selects the visible Roastty window.
- Screenshot captures the visible terminal window.

### Keyboard Injection

Both keyboard paths were tested:

```bash
swift scripts/ghostty-app/inject.swift type /tmp/termsurf-issue804-exp2/type.txt
swift scripts/ghostty-app/inject.swift key 36
```

and:

```bash
osascript -e 'tell application "System Events" to keystroke (read POSIX file "/tmp/termsurf-issue804-exp2/system-events-type.txt")'
osascript -e 'tell application "System Events" to key code 36'
```

Expected:

- A command typed into Roastty creates a marker file under
  `/tmp/termsurf-issue804-exp2/`.

### XCTest UI Automation

```bash
cd roastty/macos
xcodebuild test \
  -project Roastty.xcodeproj \
  -scheme Roastty \
  -testPlan Roastty \
  -destination 'platform=macOS' \
  -only-testing:RoasttyUITests/RoasttyTerminalOutputUITests/testTerminalOutputIsVisibleToUIAutomation
```

Expected:

- The focused UI test starts an automation session and passes.

### Cleanup

```bash
scripts/roastty-app/stop-app.sh
pgrep -fl 'Roastty.app/Contents/MacOS/roastty' || true
```

Expected:

- No debug Roastty process remains.

## Result

**Result:** Partial

Accessibility and Apple Events now pass:

- `logs/issue804-exp2-preflight.log`
  - `AXIsProcessTrusted()` printed `true`.
  - Apple Events to `System Events` returned process count `56`.
  - Process ancestry still shows the automation host as
    `/Applications/Ghostty.app/Contents/MacOS/ghostty`.

The VM was missing build prerequisites:

- `logs/issue804-exp2-build-launch.log`
  - Initial build failed because `nu` was missing.
- `logs/issue804-exp2-install-nushell.log`
  - Installed Homebrew `nushell` `0.113.1`.
- `logs/issue804-exp2-build-launch-rerun2.log`
  - Next build failed because `cmake` was missing for `shaderc-sys`.
- `logs/issue804-exp2-install-cmake.log`
  - Installed Homebrew `cmake` `4.3.3`.
- `logs/issue804-exp2-build-launch-rerun2.log`
  - After `cmake`, the Rust build reached `roastty` but failed because
    `vendor/ghostty/src/font/res/*` and
    `vendor/ghostty/src/renderer/shaders/shadertoy_prefix.glsl` were absent.
- `logs/issue804-exp2-clone-ghostty.log`
  - Cloned `vendor/ghostty` and checked out `2c62d182c`.

Build and launch then passed:

- `logs/issue804-exp2-build-launch-rerun3.log`
  - `roastty/macos/build.nu --action build` completed with
    `** BUILD SUCCEEDED **`.
  - Debug Roastty launched as PID `83005`.
- `logs/issue804-exp2-window-investigation.log`
  - After a short settle, Roastty had a visible window:
    `id=161 layer=0 bounds=(489,161 800x632) name="👻"`.

Screenshot capture passed after a harness fix:

- First screenshot selected offscreen window id `162`, `500x500`, because
  `winid.swift` chose the first layer-0 candidate without checking onscreen
  state.
- Updated `scripts/roastty-app/winid.swift` to prefer onscreen layer-0 windows.
- `logs/issue804-exp2-screenshot-rerun.log`
  - `winid.swift` then selected visible window `161`.
  - Screenshot captured `1600x1264` pixels from an `800x632` point window:
    `/Users/astrohacker/.cache/termsurf/shots/issue-804-exp2-window-visible-20260613-125641.png`.
- Visual inspection confirmed the screenshot contains the actual Roastty debug
  terminal window.

External keyboard injection failed:

- `logs/issue804-exp2-keyboard.log`
  - CGEvent text/key injection returned successfully but did not create
    `/tmp/termsurf-issue804-exp2/keyboard.txt`.
- `logs/issue804-exp2-keyboard-system-events.log`
  - System Events `keystroke` and `key code 36` also returned successfully but
    did not create `/tmp/termsurf-issue804-exp2/keyboard-system-events.txt`.
- Screenshots after both attempts showed no typed text in the Roastty prompt.
- `logs/issue804-exp2-accessibility-state-rerun.log`
  - Roastty was alive, visible, frontmost, and had window `👻`.
- `logs/issue804-exp2-input-blocker-investigation.log`
  - No Secure Input marker or TCC denial appeared in the checked logs.

Because keyboard injection failed, the byteprobe mouse oracle could not be
started inside Roastty. A direct click command completed, but without byteprobe
there was no deterministic mouse receipt oracle in this run.

XCTest UI automation passed:

- `logs/issue804-exp2-xctest.log`
  - `RoasttyTerminalOutputUITests.testTerminalOutputIsVisibleToUIAutomation()`
    passed.
  - XCTest reported `Executed 1 test, with 0 failures`.
  - `** TEST SUCCEEDED **`.

Cleanup passed:

- `logs/issue804-exp2-final-cleanup.log`
  - No debug Roastty process remained.

## Conclusion

The Accessibility permission change worked. The VM can now build Roastty, launch
the debug GUI, discover and capture the real visible window, and run focused
XCTest UI automation.

The remaining blocker is external keyboard injection into the live Roastty
window. Both CGEvent and System Events paths returned success but produced no
input in the terminal and no marker-file oracle. There was no TCC denial or
Secure Input signal in the checked logs, so the next step is to isolate why the
frontmost Roastty terminal is not accepting synthetic keyboard events. Possible
next checks include granting Input Monitoring to the automation host, testing
manual keyboard entry in the same debug window, and adding a focused harness
probe that reports whether Roastty receives any `keyDown` events from synthetic
input.
