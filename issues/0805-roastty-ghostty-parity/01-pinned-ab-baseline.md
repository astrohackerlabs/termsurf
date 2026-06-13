# Experiment 1: Pinned A/B Baseline

## Description

Before starting source, config, or workflow parity audits, prove that this VM
can build and run both sides of the comparison:

- pinned upstream Ghostty at commit `2c62d182cec246764ff725096a70b9ef44996f7f`;
- current Roastty.

The purpose is to establish a trustworthy A/B baseline for the rest of
Issue 805. If the reference Ghostty app cannot be built, launched,
window-identified, screenshot, driven, and cleaned up in this VM, later parity
work would be forced into source-only inference and would not satisfy the issue
goal.

This experiment does not attempt broad feature parity. It proves the comparison
rig itself.

## Changes

Planned issue-doc changes:

- `issues/0805-roastty-ghostty-parity/01-pinned-ab-baseline.md`
  - Record the plan, review, commands, result, and conclusion.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add Experiment 1 to the issue index with status `Designed`.

Allowed harness changes only if the existing app launch or screenshot scripts
cannot distinguish the two apps reliably:

- `scripts/ghostty-app/*`
  - Fix stale paths or PID/window selection for the pinned Ghostty app.
- `scripts/roastty-app/*`
  - Fix stale paths or PID/window selection for the current Roastty app.

No Roastty product behavior should change in this experiment. Any product parity
gap discovered while bootstrapping the A/B rig should be recorded for a later
experiment unless it directly prevents the baseline from running.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

Required findings and fixes:

- The original pinned Ghostty build command suggested hand-running `build.zig`
  from `vendor/ghostty/macos`, which contradicted the resolved Issue 802
  workflow. Fixed by requiring `scripts/ghostty-app/build-macos-app.sh Debug`
  and documenting the wrapper's Zig 0.15.2, macOS-only xcframework, CLT SDK,
  Metal, `xcodebuild`, and `nu macos/build.nu` responsibilities.
- The same-input verification was too vague. Fixed by adding concrete System
  Events commands that activate each PID, type a marker-file `touch` command
  character-by-character, verify marker files, and capture post-input
  screenshots.
- Hygiene checks were missing even though the experiment allows harness edits.
  Fixed by adding `git status --short`, `git diff --check`, `bash -n` for
  changed shell harness scripts, and a no-generated-artifacts working-tree
  criterion.

Re-review verdict: **Approved**. The reviewer confirmed all prior required
findings were resolved and no new required findings were introduced.

## Verification

Run from the repo root. Write transcripts to `logs/` with the prefix
`issue805-exp1-`. Write screenshots through the existing app screenshot helpers
under `$TERMSURF_SHOT_DIR` or `~/.cache/termsurf/shots`.

### 1. Confirm the Pinned Ghostty Checkout

Commands:

```bash
git -C vendor/ghostty rev-parse HEAD
git -C vendor/ghostty log -1 --format='%H %s'
git -C vendor/ghostty describe --always --tags --dirty
```

Pass criteria:

- `rev-parse HEAD` is exactly `2c62d182cec246764ff725096a70b9ef44996f7f`.
- `git describe` matches or is compatible with the Issue 802 recorded
  `tip-1608-g2c62d182c` pin.
- The checkout is not dirty unless the dirty files are generated build outputs
  that are explicitly documented and ignored for the baseline.

### 2. Build the Pinned Ghostty App

Use the resolved Issue 802 build wrapper. Do not hand-run `build.zig` from the
Ghostty macOS directory; the pinned Ghostty build requires the local Zig 0.15.2
toolchain, the macOS-only xcframework patch, the CommandLineTools SDK for the
Zig-built macOS library, Xcode's Metal toolchain,
`xcodebuild -create-xcframework`, and then `nu macos/build.nu` for the Swift
app. The wrapper encodes that workflow.

Commands:

```bash
sed -n '1,220p' vendor/ghostty/macos/AGENTS.md
sed -n '1,120p' scripts/ghostty-app/README.md
scripts/ghostty-app/build-macos-app.sh Debug
```

Pass criteria:

- The pinned Ghostty macOS app builds successfully.
- The output app path is recorded as
  `vendor/ghostty/macos/build/Debug/Ghostty.app`, unless the build wrapper
  reports a different path.
- Any required toolchain or cache cleanup is recorded in the result.

### 3. Build the Current Roastty App

Commands:

```bash
scripts/roastty-app/build-roastty-kit.sh
cd roastty/macos
xcodebuild build \
  -project Roastty.xcodeproj \
  -scheme Roastty \
  -configuration Debug \
  -derivedDataPath build
cd ../..
```

Pass criteria:

- `RoasttyKit.xcframework` rebuilds successfully.
- The current Roastty debug app builds successfully.
- The output app path is recorded.

### 4. Launch Both Apps Side by Side

Launch pinned Ghostty and current Roastty at the same time. Prefer app-bundle
launches (`open`) over direct executable launches unless a documented harness
reason requires direct execution.

Commands should prove:

```bash
scripts/ghostty-app/stop-app.sh || true
scripts/roastty-app/stop-app.sh || true

# Launch Ghostty and Roastty, recording their app paths and PIDs.
# Use existing start helpers if they are reliable, otherwise record the manual
# launch commands and add the minimal helper fix allowed by this experiment.

pgrep -fl 'Ghostty.app/Contents/MacOS/ghostty' || true
pgrep -fl 'Roastty.app/Contents/MacOS/roastty' || true
```

Pass criteria:

- Both apps are running at the same time.
- Each PID is recorded.
- Each app can be activated independently.
- Window lookup returns a real visible layer-0 terminal window for each app.
- The harness does not confuse Ghostty and Roastty PIDs or window IDs.

### 5. Capture Full-Window Screenshots for Both Apps

Commands:

```bash
scripts/ghostty-app/screenshot.sh "$GHOSTTY_PID" issue-805-exp1-ghostty-launch
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-805-exp1-roastty-launch
```

Pass criteria:

- Both screenshots are created.
- Screenshot dimensions match the captured window bounds and display scale.
- The screenshots visibly correspond to the correct app.

### 6. Drive One Simple Same-Input Recipe

Use the System Events route proven in Issues 802 and 804. The recipe is
deliberately small: activate a target PID, send a warmup key to absorb focus
settling, type a `touch` command character-by-character, press Return, and prove
receipt with a marker file plus a screenshot.

Commands:

```bash
type_marker_command() {
  local pid="$1"
  local marker="$2"
  local command_file="$3"

  rm -f "$marker"
  printf 'touch %s' "$marker" > "$command_file"
  osascript -e 'tell application "System Events" to set frontmost of first process whose unix id is '"$pid"' to true'
  sleep 0.75
  osascript -e 'tell application "System Events" to key code 49'
  sleep 0.25
  osascript <<OSA
set typedText to read POSIX file "$command_file"
tell application "System Events"
  repeat with i from 1 to length of typedText
    keystroke (character i of typedText)
    delay 0.05
  end repeat
  key code 36
end tell
OSA
  for _ in $(seq 1 80); do
    [ -f "$marker" ] && return 0
    sleep 0.25
  done
  return 1
}

GHOSTTY_MARKER=/tmp/termsurf-issue805-exp1-ghostty-marker
ROASTTY_MARKER=/tmp/termsurf-issue805-exp1-roastty-marker
type_marker_command "$GHOSTTY_PID" "$GHOSTTY_MARKER" "$PWD/logs/issue805-exp1-ghostty-command.txt"
type_marker_command "$ROASTTY_PID" "$ROASTTY_MARKER" "$PWD/logs/issue805-exp1-roastty-command.txt"
test -f "$GHOSTTY_MARKER"
test -f "$ROASTTY_MARKER"
scripts/ghostty-app/screenshot.sh "$GHOSTTY_PID" issue-805-exp1-ghostty-after-input
scripts/roastty-app/screenshot.sh "$ROASTTY_PID" issue-805-exp1-roastty-after-input
```

Pass criteria:

- The same recipe shape is delivered to Ghostty and Roastty through System
  Events.
- `GHOSTTY_MARKER` and `ROASTTY_MARKER` are created by commands typed into their
  respective terminal windows.
- Post-input screenshots are captured for both apps.
- The result records frontmost PID/name checks if either marker fails, so target
  confusion is distinguishable from app input failure.

### 7. Cleanup

Commands:

```bash
scripts/ghostty-app/stop-app.sh || true
scripts/roastty-app/stop-app.sh || true
rm -f /tmp/termsurf-issue805-exp1-ghostty-marker
rm -f /tmp/termsurf-issue805-exp1-roastty-marker
pgrep -fl 'Ghostty.app/Contents/MacOS/ghostty|Roastty.app/Contents/MacOS/roastty' || true
```

Pass criteria:

- No debug Ghostty or Roastty process launched by the experiment remains.
- Temporary launchd environment variables and temporary recipe directories are
  removed or unset.

### 8. Hygiene

Commands:

```bash
git status --short
git diff --check

# Run syntax checks for any shell harness script changed by this experiment.
git diff --name-only -- scripts/ghostty-app scripts/roastty-app |
  awk '/\\.sh$/ { print }' |
  xargs -r bash -n
```

Pass criteria:

- No screenshots, generated logs, temporary recipes, marker files, app bundles,
  or build outputs appear as tracked or untracked repo changes.
- `git diff --check` passes.
- `bash -n` passes for every changed shell harness script.

Overall result:

- **Pass** if the pinned Ghostty app and current Roastty app build, launch
  side-by-side, produce independent screenshots, receive the same simple recipe,
  and clean up.
- **Partial** if both apps build and launch, but one automation/oracle route
  needs a harness fix before broader parity work.
- **Fail** if the pinned Ghostty app cannot be built/launched or if the harness
  cannot distinguish the two apps.
