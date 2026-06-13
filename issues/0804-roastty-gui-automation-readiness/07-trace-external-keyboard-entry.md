# Experiment 7: Trace External Keyboard Entry

## Description

Determine whether the failing external keyboard events enter Roastty's AppKit
keyboard path at all.

Issue 802 left an inert trace hook in `SurfaceView_AppKit.swift`, enabled only
when `ROASTTY_UI_KEY_TRACE_PATH` is set. XCTest has already proven this hook
records `keyDown`, `setMarkedText`, `insertText`, and `committedPreeditText` for
successful UI automation. This experiment launches the real debug Roastty app
with that trace path, repeats the external System Events and CGEvent keyboard
attempts, and checks whether the trace file records anything.

This is the first instrumentation experiment after permission, restart, Input
Monitoring, and first-responder click hypotheses failed.

Per user instruction, this issue skips adversarial review.

## Changes

- `issues/0804-roastty-gui-automation-readiness/07-trace-external-keyboard-entry.md`
  - Record this trace experiment and result.
- `issues/0804-roastty-gui-automation-readiness/README.md`
  - Add Experiment 7 to the issue index.

No product code or harness code should change in this experiment. It reuses the
existing `ROASTTY_UI_KEY_TRACE_PATH` hook.

## Verification

Run from the repo root. Store transcripts in `logs/` with the `issue804-exp7-`
prefix. Store the trace file itself in `logs/`.

### 1. Confirm Trace Hook Exists

Commands:

```bash
rg -n 'ROASTTY_UI_KEY_TRACE_PATH|appendUITestKeyTrace|keyDown chars=|insertText accumulated|setMarkedText' \
  'roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift' \
  roastty/macos/RoasttyUITests/RoasttyDeadKeyUITests.swift
```

Pass criteria:

- The source contains the trace environment variable and the expected trace
  strings.
- The UI test still uses the trace hook as a known-good route.

### 2. Direct-Launch Roastty With Trace Enabled

Commands:

```bash
scripts/roastty-app/stop-app.sh || true
TRACE="$PWD/logs/issue804-exp7-key-trace.log"
rm -f "$TRACE"
ROASTTY_UI_KEY_TRACE_PATH="$TRACE" DISABLE_AUTO_UPDATE=true \
  roastty/macos/build/Debug/Roastty.app/Contents/MacOS/roastty \
  > logs/issue804-exp7-roastty-stdout.log \
  2> logs/issue804-exp7-roastty-stderr.log &
ROASTTY_PID="$!"
printf 'ROASTTY_PID=%s\nTRACE=%s\n' "$ROASTTY_PID" "$TRACE" > logs/issue804-exp7.env
sleep 3
pgrep -fl 'Roastty.app/Contents/MacOS/roastty'
swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID"
swift scripts/roastty-app/winid.swift "$ROASTTY_PID"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
osascript -e 'tell application "System Events" to name of first process whose frontmost is true'
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp7-before-keyboard
```

Pass criteria:

- Debug Roastty launches from the direct binary path.
- The visible terminal window is discovered.
- Roastty is frontmost.
- Screenshot capture works.
- The trace file path is under `logs/`.

### 3. Compute and Click Terminal Coordinates

Commands:

```bash
LINE="$(swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID" | awk '/layer=0/ { print; exit }')"
read -r X Y W H < <(printf '%s\n' "$LINE" |
  sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/' |
  awk '{ printf "%d %d %d %d\n", $1, $2, $3, $4 }')
FOCUS_X=$((X + 40))
FOCUS_Y=$((Y + 72))
SAFE_X=$((X + 120))
SAFE_Y=$((Y + 140))
printf 'X=%s\nY=%s\nW=%s\nH=%s\nFOCUS_X=%s\nFOCUS_Y=%s\nSAFE_X=%s\nSAFE_Y=%s\n' \
  "$X" "$Y" "$W" "$H" "$FOCUS_X" "$FOCUS_Y" "$SAFE_X" "$SAFE_Y" > logs/issue804-exp7-coords.env
swift scripts/ghostty-app/inject.swift click "$SAFE_X" "$SAFE_Y" left 1
swift scripts/ghostty-app/inject.swift click "$FOCUS_X" "$FOCUS_Y" left 1
osascript -e 'delay 0.7'
```

Pass criteria:

- Coordinates are inside the visible Roastty terminal window.
- The clicks return without error.

### 4. System Events Keyboard With Trace

Commands:

```bash
TS=/tmp/termsurf-issue804-exp7-system-events
mkdir -p "$TS"
rm -f "$TS/marker.txt"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
swift scripts/ghostty-app/inject.swift click "$FOCUS_X" "$FOCUS_Y" left 1
osascript -e 'delay 0.5'
osascript -e 'tell application "System Events" to key code 49'
printf 'printf "ISSUE804_EXP7_SYSTEM_EVENTS\n" > '"$TS"'/marker.txt' > "$TS/type.txt"
osascript -e 'tell application "System Events" to keystroke (read POSIX file "'"$TS"'/type.txt")'
osascript -e 'tell application "System Events" to key code 36'
osascript -e 'delay 0.7'
cat "$TRACE" || true
cat "$TS/marker.txt"
```

Pass criteria:

- If `marker.txt` exists, System Events keyboard now works.
- If `marker.txt` does not exist but the trace contains `keyDown`, `insertText`,
  or marked-text entries, the event enters Roastty/AppKit and is lost later.
- If `marker.txt` does not exist and the trace is absent or empty, the event
  does not reach Roastty's traced AppKit keyboard path.

### 5. CGEvent Keyboard With Trace

Commands:

```bash
TS=/tmp/termsurf-issue804-exp7-cgevent
mkdir -p "$TS"
rm -f "$TS/marker.txt"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
swift scripts/ghostty-app/inject.swift click "$FOCUS_X" "$FOCUS_Y" left 1
osascript -e 'delay 0.5'
swift scripts/ghostty-app/inject.swift key 49
printf 'printf "ISSUE804_EXP7_CGEVENT\n" > '"$TS"'/marker.txt' > "$TS/type.txt"
swift scripts/ghostty-app/inject.swift type "$TS/type.txt"
swift scripts/ghostty-app/inject.swift key 36
osascript -e 'delay 0.7'
cat "$TRACE" || true
cat "$TS/marker.txt"
```

Pass criteria:

- If `marker.txt` exists, CGEvent keyboard now works.
- If `marker.txt` does not exist but the trace contains `keyDown`, `insertText`,
  or marked-text entries, the event enters Roastty/AppKit and is lost later.
- If `marker.txt` does not exist and the trace is absent or empty, the event
  does not reach Roastty's traced AppKit keyboard path.

### 6. Positive Control and Cleanup

If the trace remains absent or empty, run the existing XCTest dead-key UI test
as a positive control for the trace hook:

```bash
cd roastty/macos
xcodebuild test \
  -project Roastty.xcodeproj \
  -scheme Roastty \
  -testPlan Roastty \
  -destination 'platform=macOS' \
  -only-testing:RoasttyUITests/RoasttyDeadKeyUITests/testDeadKeyCompositionCommitsText
cd ../..
```

Then clean up:

```bash
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp7-after-keyboard || true
osascript -e 'tell application "System Events" to name of first process whose frontmost is true' || true
osascript -e 'tell application "System Events" to tell (first process whose unix id is '"$ROASTTY_PID"') to get {name, frontmost, visible, enabled}' || true
kill "$ROASTTY_PID" || true
scripts/roastty-app/stop-app.sh || true
pgrep -fl 'Roastty.app/Contents/MacOS/roastty' || true
```

Pass criteria:

- Positive control passes if run.
- Cleanup leaves no debug Roastty process running.

Overall result:

- **Pass** if an external keyboard route creates its marker file.
- **Partial** if the marker still fails but the trace classifies where the event
  disappears.
- **Fail** if the app cannot be launched with tracing or the trace hook cannot
  be validated.

## Result

**Result:** Partial

The initial trace setup was started, but the System Events keyboard attempt was
invalid as a Roastty keyboard test.

Evidence:

- The trace hook exists in `SurfaceView_AppKit.swift`; the hook-check transcript
  was saved as `logs/issue804-exp7-trace-hook-check.log`.
- Roastty initially launched with `ROASTTY_UI_KEY_TRACE_PATH` set, and
  `logs/issue804-exp7-launch.log` recorded a visible layer-0 window and
  frontmost process confirmation.
- Before the keyboard attempt, the direct-launched Roastty process exited. The
  coordinate computation then produced an empty window line and bogus zero-based
  coordinates in `logs/issue804-exp7-coordinates.log`.
- The subsequent System Events activation failed with
  `Can’t get process 1 whose unix id = 94343` in
  `logs/issue804-exp7-keyboard-system-events.log`, so that attempted run did not
  target a live Roastty process.
- Most importantly, the marker command text appeared in the current
  Ghostty/Codex window. That was not user input. It proves that synthetic
  keyboard generation can work in this VM, but it landed in the wrong
  application window.

No external keyboard conclusion should be drawn from this run about Roastty's
AppKit `keyDown` or text input path. The failed trace file is not meaningful
because the keyboard event stream did not target Roastty.

## Conclusion

Experiment 7 corrected the failure classification. The current primary blocker
is no longer "the VM cannot synthesize keyboard input"; the stronger conclusion
is "the harness has not proven that Roastty is the focused keyboard target at
the moment of typing."

The next experiment should focus only on target ownership:

- keep Roastty alive through a normal app launch path;
- prove the window is frontmost and focused immediately before typing;
- record AppKit focus callbacks, first-responder state, and keyboard traces;
- refuse to type if the focused target cannot be proven to be Roastty.

Per user instruction, no adversarial review was run for this issue.
