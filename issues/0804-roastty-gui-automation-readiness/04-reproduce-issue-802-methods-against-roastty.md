# Experiment 4: Reproduce Issue 802 Input Methods Against Roastty

## Description

Test every GUI automation method that Issue 802 proved useful against either
Ghostty or Roastty, but target only the current debug Roastty app in this VM.

Experiments 2 and 3 showed that two external keyboard routes return success but
do not create a terminal-side marker file in Roastty. Before designing a lower
level diagnostic, this experiment replays the full Issue 802 automation toolbox
against Roastty with independent oracles for each method:

1. System Events keyboard input, the successful Ghostty external-keyboard path.
2. CGEvent keyboard input, present in the generic helper and used by later
   harness code, but known to be focus-sensitive.
3. XCTest UI keyboard input, the successful Roastty native AppKit key route.
4. Launch-time bootstrap command delivery, the successful live A/B recipe path
   that avoids interactive keyboard injection.
5. CGEvent mouse input: move, click, drag, scroll, and context/right click.
6. Window screenshot capture and non-OCR oracles: marker files, accessibility
   output, pasteboard contents, and screenshots.

This experiment intentionally separates the methods. A failure in System Events
or CGEvent keyboard must not prevent testing XCTest, bootstrap, screenshots, or
mouse input.

Per user instruction, this issue skips adversarial review.

## Changes

Planned issue-doc changes:

- `issues/0804-roastty-gui-automation-readiness/04-reproduce-issue-802-methods-against-roastty.md`
  - Record the design, commands, result table, and conclusion.
- `issues/0804-roastty-gui-automation-readiness/README.md`
  - Add Experiment 4 to the issue index.

Allowed harness changes only if a reusable Issue 802 method is stale or cannot
be run on this VM for a harness reason:

- `scripts/roastty-app/*`
  - Add or fix Roastty wrappers for click, drag, scroll, screenshot,
    window-focus, or bootstrap execution.
- `scripts/ghostty-app/*`
  - Fix only generic helpers without breaking Ghostty workflows.

No Roastty product behavior should change in this experiment. If a product bug
is discovered, record it as a finding unless it directly blocks proving an
automation method.

## Verification

Run from the repo root. Write command transcripts to `logs/` with the prefix
`issue804-exp4-`. Write screenshots under the existing out-of-repo shot
directory used by the Roastty helpers.

### 1. Preflight and Launch

Commands:

```bash
git status --short
swift -e 'import ApplicationServices; print(AXIsProcessTrusted())'
osascript -e 'tell application "System Events" to count processes'
scripts/roastty-app/stop-app.sh || true
cd roastty && macos/build.nu --action build
cd ..
ROASTTY_PID="$(scripts/roastty-app/start-app.sh)"
export ROASTTY_PID
pgrep -fl 'Roastty.app/Contents/MacOS/roastty'
swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID"
swift scripts/roastty-app/winid.swift "$ROASTTY_PID"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
osascript -e 'tell application "System Events" to name of first process whose frontmost is true'
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp4-initial-window
```

Pass criteria:

- Accessibility is trusted.
- Apple Events to System Events work.
- Roastty builds and launches.
- The visible Roastty terminal window is discovered.
- Roastty is frontmost.
- A window screenshot captures the actual Roastty terminal.

### 2. System Events Keyboard to Roastty

Replay the Issue 802 successful Ghostty keyboard method against Roastty:
activate-first, warmup, bootstrap to bash, then type a marker-writing command.

Commands:

```bash
TS=/tmp/termsurf-issue804-exp4-system-events
mkdir -p "$TS"
rm -f "$TS/marker.txt"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
osascript -e 'tell application "System Events" to key code 49'
printf 'exec bash --norc --noprofile' > "$TS/type.txt"
osascript -e 'tell application "System Events" to keystroke (read POSIX file "'"$TS"'/type.txt")'
osascript -e 'tell application "System Events" to key code 36'
printf 'printf "ISSUE804_EXP4_SYSTEM_EVENTS\n" > '"$TS"'/marker.txt' > "$TS/type.txt"
osascript -e 'tell application "System Events" to keystroke (read POSIX file "'"$TS"'/type.txt")'
osascript -e 'tell application "System Events" to key code 36'
cat "$TS/marker.txt"
```

Pass criteria:

- `marker.txt` exists and contains `ISSUE804_EXP4_SYSTEM_EVENTS`.

Record if the command returns success but no text appears or no marker file is
created. That distinguishes "posting returned" from "Roastty received input."

### 3. CGEvent Keyboard to Roastty

Replay the generic Issue 802 helper's keyboard subcommands against Roastty.

Commands:

```bash
TS=/tmp/termsurf-issue804-exp4-cgevent
mkdir -p "$TS"
rm -f "$TS/marker.txt"
osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$ROASTTY_PID"' to true'
swift scripts/ghostty-app/inject.swift key 49
printf 'exec bash --norc --noprofile' > "$TS/type.txt"
swift scripts/ghostty-app/inject.swift type "$TS/type.txt"
swift scripts/ghostty-app/inject.swift key 36
printf 'printf "ISSUE804_EXP4_CGEVENT\n" > '"$TS"'/marker.txt' > "$TS/type.txt"
swift scripts/ghostty-app/inject.swift type "$TS/type.txt"
swift scripts/ghostty-app/inject.swift key 36
cat "$TS/marker.txt"
```

Pass criteria:

- `marker.txt` exists and contains `ISSUE804_EXP4_CGEVENT`.

If this fails, capture a screenshot and record frontmost state immediately after
the failed attempt.

### 4. XCTest Keyboard and Accessibility Output

Run the UI automation route that Issue 802 proved can reach Roastty's native
AppKit key path and the terminal accessibility oracle.

Commands:

```bash
cd roastty/macos
xcodebuild test \
  -project Roastty.xcodeproj \
  -scheme Roastty \
  -testPlan Roastty \
  -destination 'platform=macOS' \
  -only-testing:RoasttyUITests/RoasttyTerminalOutputUITests/testTerminalOutputIsVisibleToUIAutomation
xcodebuild test \
  -project Roastty.xcodeproj \
  -scheme Roastty \
  -testPlan Roastty \
  -destination 'platform=macOS' \
  -only-testing:RoasttyUITests/RoasttyDeadKeyUITests/testDeadKeyCompositionCommitsText
cd ../..
```

Pass criteria:

- `RoasttyTerminalOutputUITests.testTerminalOutputIsVisibleToUIAutomation`
  executes and passes.
- `RoasttyDeadKeyUITests.testDeadKeyCompositionCommitsText` executes its body
  and either passes or records the known Issue 802 route-proof skip, with a
  trace showing `keyDown`, `setMarkedText`, `insertText`, or
  `committedPreeditText`.

This route is considered successful for keyboard delivery if XCTest reaches the
Roastty AppKit key path, even if it does not prove the external-agent keyboard
path.

### 5. Launch-Time Bootstrap Command Delivery

Replay the Issue 802 live A/B approach that avoids interactive keyboard input:
launch Roastty directly with temporary shell startup files that run a recipe.

Commands:

```bash
scripts/roastty-app/stop-app.sh || true
BOOT="$(mktemp -d /tmp/termsurf-exp4-bootstrap.XXXXXX)"
mkdir -p "$BOOT/nushell"
cat > "$BOOT/recipe.sh" <<'SH'
#!/usr/bin/env bash
clear
printf 'ISSUE804_EXP4_BOOTSTRAP_READY\n'
printf 'BOOTSTRAP_MARKER\n' > /tmp/termsurf-issue804-exp4-bootstrap-marker.txt
sleep 20
SH
chmod +x "$BOOT/recipe.sh"
printf 'bash %q\n' "$BOOT/recipe.sh" > "$BOOT/.zshrc"
printf 'bash "%s/recipe.sh"\n' "$BOOT" > "$BOOT/nushell/config.nu"
rm -f /tmp/termsurf-issue804-exp4-bootstrap-marker.txt
ZDOTDIR="$BOOT" XDG_CONFIG_HOME="$BOOT" SHELL=/bin/zsh \
  roastty/macos/build/Build/Products/Debug/Roastty.app/Contents/MacOS/roastty \
  > logs/issue804-exp4-bootstrap-stdout.log \
  2> logs/issue804-exp4-bootstrap-stderr.log &
ROASTTY_BOOT_PID="$!"
sleep 3
cat /tmp/termsurf-issue804-exp4-bootstrap-marker.txt
swift scripts/roastty-app/list-windows.swift "$ROASTTY_BOOT_PID"
scripts/roastty-app/screenshot.sh "$ROASTTY_BOOT_PID" issue-804-exp4-bootstrap-window
kill "$ROASTTY_BOOT_PID" || true
rm -rf "$BOOT"
```

Pass criteria:

- The marker file exists and contains `BOOTSTRAP_MARKER`.
- The screenshot visibly contains `ISSUE804_EXP4_BOOTSTRAP_READY`.

This proves command delivery to Roastty without relying on synthetic keyboard
input.

### 6. CGEvent Mouse Click and Right Click

Use the Issue 802 CGEvent mouse driver against the visible Roastty window. Since
mouse click receipt can be hard to prove without a byteprobe, use screenshot and
frontmost/focus state as the basic oracle, then use stronger oracles for drag
and scroll in later steps.

Commands:

```bash
ROASTTY_PID="$(pgrep -f 'Roastty.app/Contents/MacOS/roastty' | head -1)"
LINE="$(swift scripts/roastty-app/list-windows.swift "$ROASTTY_PID" | awk '/layer=0/ { print; exit }')"
read -r X Y W H < <(printf '%s\n' "$LINE" |
  sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/' |
  awk '{ printf "%d %d %d %d\n", $1, $2, $3, $4 }')
CX=$((X + W / 2))
CY=$((Y + H / 2))
swift scripts/ghostty-app/inject.swift move "$CX" "$CY"
swift scripts/ghostty-app/inject.swift click "$CX" "$CY" left 1
swift scripts/ghostty-app/inject.swift click "$CX" "$CY" right 1
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-804-exp4-mouse-clicks
```

Pass criteria:

- Commands return without error.
- Roastty remains frontmost and screenshots show the Roastty window after the
  events.
- If a context menu is visible after right click, record that as receipt
  evidence. If not, classify right-click receipt as weakly observed unless a
  stronger oracle is available.

### 7. CGEvent Mouse Scroll

Use the Roastty-specific scroll driver that Issue 802 proved live against
Roastty. Prefer bootstrap content with enough scrollback so this does not depend
on keyboard input.

Commands:

```bash
scripts/roastty-app/stop-app.sh || true
BOOT="$(mktemp -d /tmp/termsurf-exp4-scroll.XXXXXX)"
mkdir -p "$BOOT/nushell"
cat > "$BOOT/recipe.sh" <<'SH'
#!/usr/bin/env bash
clear
seq 1 200
sleep 20
SH
chmod +x "$BOOT/recipe.sh"
printf 'bash %q\n' "$BOOT/recipe.sh" > "$BOOT/.zshrc"
printf 'bash "%s/recipe.sh"\n' "$BOOT" > "$BOOT/nushell/config.nu"
ZDOTDIR="$BOOT" XDG_CONFIG_HOME="$BOOT" SHELL=/bin/zsh \
  roastty/macos/build/Build/Products/Debug/Roastty.app/Contents/MacOS/roastty &
ROASTTY_SCROLL_PID="$!"
sleep 3
LINE="$(swift scripts/roastty-app/list-windows.swift "$ROASTTY_SCROLL_PID" | awk '/layer=0/ { print; exit }')"
read -r X Y W H < <(printf '%s\n' "$LINE" |
  sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/' |
  awk '{ printf "%d %d %d %d\n", $1, $2, $3, $4 }')
CX=$((X + W / 2))
CY=$((Y + H / 2))
scripts/roastty-app/screenshot.sh "$ROASTTY_SCROLL_PID" issue-804-exp4-scroll-before
swift scripts/roastty-app/scroll.swift "$CX" "$CY" 20
sleep 1
scripts/roastty-app/screenshot.sh "$ROASTTY_SCROLL_PID" issue-804-exp4-scroll-after-up
swift scripts/roastty-app/scroll.swift "$CX" "$CY" -20
sleep 1
scripts/roastty-app/screenshot.sh "$ROASTTY_SCROLL_PID" issue-804-exp4-scroll-after-down
kill "$ROASTTY_SCROLL_PID" || true
rm -rf "$BOOT"
```

Pass criteria:

- The before screenshot shows the tail of `seq 1 200`.
- The scroll-up screenshot shows earlier history lines.
- The scroll-down screenshot returns toward the tail.

### 8. CGEvent Drag Selection and Pasteboard

Use the Roastty-specific drag driver that Issue 802 proved live, then invoke the
copy action through the menu as Issue 802 did when CGEvent Command-C was
unreliable.

Commands:

```bash
scripts/roastty-app/stop-app.sh || true
BOOT="$(mktemp -d /tmp/termsurf-exp4-drag.XXXXXX)"
mkdir -p "$BOOT/nushell"
cat > "$BOOT/recipe.sh" <<'SH'
#!/usr/bin/env bash
clear
printf 'DRAGSELECTME_TARGET_HERE\n'
sleep 20
SH
chmod +x "$BOOT/recipe.sh"
printf 'bash %q\n' "$BOOT/recipe.sh" > "$BOOT/.zshrc"
printf 'bash "%s/recipe.sh"\n' "$BOOT" > "$BOOT/nushell/config.nu"
printf 'CLIPBOARD_PROBE_STALE' | pbcopy
ZDOTDIR="$BOOT" XDG_CONFIG_HOME="$BOOT" SHELL=/bin/zsh \
  roastty/macos/build/Build/Products/Debug/Roastty.app/Contents/MacOS/roastty &
ROASTTY_DRAG_PID="$!"
sleep 3
LINE="$(swift scripts/roastty-app/list-windows.swift "$ROASTTY_DRAG_PID" | awk '/layer=0/ { print; exit }')"
read -r X Y W H < <(printf '%s\n' "$LINE" |
  sed -E 's/.*bounds=\(([0-9.-]+),([0-9.-]+) ([0-9.-]+)x([0-9.-]+)\).*/\1 \2 \3 \4/' |
  awk '{ printf "%d %d %d %d\n", $1, $2, $3, $4 }')
swift scripts/roastty-app/drag.swift "$((X + 80))" "$((Y + 95))" "$((X + 310))" "$((Y + 95))" 18
scripts/roastty-app/screenshot.sh "$ROASTTY_DRAG_PID" issue-804-exp4-drag-selection
osascript <<OSA
tell application "System Events"
  tell first process whose unix id is $ROASTTY_DRAG_PID
    click menu item "Copy" of menu "Edit" of menu bar 1
  end tell
end tell
OSA
pbpaste
kill "$ROASTTY_DRAG_PID" || true
rm -rf "$BOOT"
```

Pass criteria:

- Screenshot shows a highlighted selection.
- `pbpaste` changes from `CLIPBOARD_PROBE_STALE` to a substring of
  `DRAGSELECTME_TARGET_HERE`.

### 9. Classification Table

Record a table in the result with one row per method:

| Method                    | Prior Issue 802 target   | Roastty result    | Oracle               | Notes |
| ------------------------- | ------------------------ | ----------------- | -------------------- | ----- |
| System Events keyboard    | Ghostty                  | Pass/Partial/Fail | marker file          |       |
| CGEvent keyboard          | helper/focus-sensitive   | Pass/Partial/Fail | marker file          |       |
| XCTest keyboard           | Roastty                  | Pass/Partial/Fail | xcodebuild trace     |       |
| Launch bootstrap          | Ghostty/Roastty live A/B | Pass/Partial/Fail | marker + screenshot  |       |
| CGEvent click/right-click | Ghostty                  | Pass/Partial/Fail | screenshot/menu      |       |
| CGEvent scroll            | Roastty                  | Pass/Partial/Fail | screenshots          |       |
| CGEvent drag selection    | Roastty                  | Pass/Partial/Fail | screenshot + pbpaste |       |
| Window screenshot         | Ghostty/Roastty          | Pass/Partial/Fail | PNG artifact         |       |

Overall result:

- **Pass** if every previously successful Issue 802 method either works against
  Roastty or has a stronger Roastty-specific replacement that works and is
  documented.
- **Partial** if one or more methods still fail but the experiment proves the
  other independent methods and classifies the failure.
- **Fail** if Roastty cannot be launched, observed, or interacted with at all.

## Result

Not run yet.
