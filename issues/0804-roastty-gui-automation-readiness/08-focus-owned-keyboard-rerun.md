# Experiment 8: Focus-Owned Keyboard Rerun

## Description

Prove that Roastty owns keyboard focus immediately before any external keyboard
attempt, then retry the marker-file keyboard oracle only if that focus proof
passes.

Experiment 7 showed that synthetic keyboard input can work in this VM, but it
landed in the current Ghostty/Codex window instead of Roastty. That invalidates
older "frontmost" assumptions. This experiment narrows the problem to target
ownership:

- launch Roastty through the normal `.app` path so it stays alive;
- pass `ROASTTY_UI_KEY_TRACE_PATH` through `launchctl setenv`;
- activate and click the visible layer-0 Roastty terminal window;
- query System Events / AX state for the frontmost process, focused window, and
  focused UI element;
- abort before typing unless the focus oracle proves Roastty is the keyboard
  target;
- if focus is proven, type the marker command and check both the marker file and
  the Roastty key trace.

Per user instruction, this issue skips adversarial review.

## Changes

- `issues/0804-roastty-gui-automation-readiness/08-focus-owned-keyboard-rerun.md`
  - Record this focus-owned rerun plan and result.
- `issues/0804-roastty-gui-automation-readiness/README.md`
  - Add Experiment 8 to the issue index.

No product code or committed harness code should change in this experiment. Any
temporary probes live in `logs/` and `/tmp/termsurf-issue804-exp8-*` only.

## Verification

Run from the repo root. Store transcripts in `logs/` with the `issue804-exp8-`
prefix. Store the Roastty trace in `logs/`.

### 1. Launch Roastty Normally With Trace Environment

Commands:

```bash
mkdir -p logs
scripts/roastty-app/stop-app.sh || true
TRACE="$PWD/logs/issue804-exp8-key-trace.log"
rm -f "$TRACE"
launchctl setenv ROASTTY_UI_KEY_TRACE_PATH "$TRACE"
launchctl setenv DISABLE_AUTO_UPDATE true
ROASTTY_PID="$(scripts/roastty-app/start-app.sh)"
printf 'ROASTTY_PID=%s\nTRACE=%s\n' "$ROASTTY_PID" "$TRACE" \
  > logs/issue804-exp8.env
pgrep -fl 'Roastty.app/Contents/MacOS/roastty'
swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID"
swift scripts/roastty-app/winid.swift "$ROASTTY_PID"
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp8-before-focus
```

Pass criteria:

- Debug Roastty launches through `scripts/roastty-app/start-app.sh`.
- A visible layer-0 terminal window is discovered.
- The trace path is under `logs/`.
- The initial screenshot captures the real Roastty window.

### 2. Compute Coordinates and Attempt Focus

Commands:

```bash
source logs/issue804-exp8.env
LINE="$(swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID" |
  awk '/layer=0/ { print; exit }')"
test -n "$LINE"
read -r X Y W H < <(printf '%s\n' "$LINE" |
  sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/' |
  awk '{ printf "%d %d %d %d\n", $1, $2, $3, $4 }')
FOCUS_X=$((X + 40))
FOCUS_Y=$((Y + 72))
SAFE_X=$((X + 120))
SAFE_Y=$((Y + 140))
printf 'X=%s\nY=%s\nW=%s\nH=%s\nFOCUS_X=%s\nFOCUS_Y=%s\nSAFE_X=%s\nSAFE_Y=%s\n' \
  "$X" "$Y" "$W" "$H" "$FOCUS_X" "$FOCUS_Y" "$SAFE_X" "$SAFE_Y" \
  > logs/issue804-exp8-coords.env
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
swift scripts/ghostty-app/inject.swift click "$SAFE_X" "$SAFE_Y" left 1
swift scripts/ghostty-app/inject.swift click "$FOCUS_X" "$FOCUS_Y" left 1
osascript -e 'delay 1.0'
```

Pass criteria:

- Coordinates are non-zero and inside the visible Roastty window.
- Activation and clicks return without error.

### 3. Prove Keyboard Target Ownership

Commands:

```bash
source logs/issue804-exp8.env
osascript > logs/issue804-exp8-focus-before-keyboard.log <<OSA
set targetPid to $ROASTTY_PID
tell application "System Events"
  set frontProc to first process whose frontmost is true
  set frontName to name of frontProc
  set frontPid to unix id of frontProc
  log "frontmost=" & frontName & " pid=" & frontPid

  set roastProc to first process whose unix id is targetPid
  log "roast-name=" & name of roastProc
  log "roast-frontmost=" & frontmost of roastProc
  log "roast-visible=" & visible of roastProc
  log "roast-enabled=" & enabled of roastProc

  try
    set focusedElement to value of attribute "AXFocusedUIElement" of roastProc
    log "focused-role=" & (role of focusedElement as text)
    try
      log "focused-subrole=" & (subrole of focusedElement as text)
    end try
    try
      log "focused-title=" & (title of focusedElement as text)
    end try
    try
      log "focused-description=" & (description of focusedElement as text)
    end try
  on error errText number errNum
    log "focused-element-error=" & errNum & " " & errText
  end try

  if frontPid is not targetPid then error "Roastty is not frontmost"
  if frontmost of roastProc is not true then error "Roastty process is not frontmost"
end tell
OSA
```

Pass criteria:

- The transcript reports Roastty's PID as the frontmost PID.
- The Roastty process reports `frontmost=true`.
- The focused element query either identifies a Roastty-owned focused element or
  reports a concrete AX limitation that must be handled before typing.
- If the frontmost checks fail, the experiment stops here and does not type.

### 4. Type Only After Focus Proof

Commands:

```bash
source logs/issue804-exp8.env
source logs/issue804-exp8-coords.env
TS=/tmp/termsurf-issue804-exp8-system-events
mkdir -p "$TS"
rm -f "$TS/marker.txt"
: > "$TRACE"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
swift scripts/ghostty-app/inject.swift click "$FOCUS_X" "$FOCUS_Y" left 1
osascript -e 'delay 0.5'
osascript -e 'tell application "System Events" to name of first process whose frontmost is true' \
  > logs/issue804-exp8-immediate-frontmost.txt
test "$(cat logs/issue804-exp8-immediate-frontmost.txt)" = "Roastty"
printf 'printf "ISSUE804_EXP8_SYSTEM_EVENTS\n" > '"$TS"'/marker.txt' \
  > "$TS/type.txt"
osascript -e 'tell application "System Events" to keystroke (read POSIX file "'"$TS"'/type.txt")'
osascript -e 'tell application "System Events" to key code 36'
osascript -e 'delay 1.0'
cat "$TRACE" || true
cat "$TS/marker.txt"
```

Pass criteria:

- The immediate pre-type frontmost process is Roastty.
- If `marker.txt` exists, System Events keyboard is working against Roastty.
- If the marker does not exist but the trace contains keyboard entries, input
  entered Roastty/AppKit and failed later.
- If the marker does not exist and the trace is empty, the frontmost/focused
  proof is still too weak or the event is blocked before Roastty's traced
  keyboard path.

### 5. Cleanup

Commands:

```bash
source logs/issue804-exp8.env || true
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp8-after-keyboard || true
scripts/roastty-app/stop-app.sh || true
launchctl unsetenv ROASTTY_UI_KEY_TRACE_PATH || true
launchctl unsetenv DISABLE_AUTO_UPDATE || true
pgrep -fl 'Roastty.app/Contents/MacOS/roastty' || true
```

Pass criteria:

- Cleanup leaves no debug Roastty process running.
- Environment variables used for the app launch are removed from launchd.

Overall result:

- **Pass** if focus is proven and the marker file is created by external
  keyboard input in Roastty.
- **Partial** if focus is proven but the marker fails and the trace classifies
  the loss point, or if AX focus proof fails without typing.
- **Fail** if Roastty cannot be launched normally with tracing or the run cannot
  safely avoid typing into the wrong window.
