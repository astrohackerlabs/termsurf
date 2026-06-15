# Experiment 167: macOS AppleScript Workflow Runtime

## Description

`RUNTIME-011B2` still groups the remaining live macOS
app/window/tab/split/menu/titlebar/fullscreen/quick-terminal and broader command
palette GUI effects. Experiment 166 proved copied workflow plumbing by source
parity and focused Swift tests, but it intentionally did not prove that the
built Roastty app can be driven through the live macOS automation surface.

A narrow live slice is available through Roastty's copied AppleScript dictionary
and handlers:

- launch the built debug app by absolute bundle path;
- enable AppleScript through an isolated debug config using
  `macos-applescript = true`;
- ask the running app for windows, tabs, and terminals;
- create a new window and tab;
- split a terminal, focus the new split, and close the created objects;
- send a small text input marker to the selected terminal through the
  AppleScript `input text` command, then prove the controlled child process
  received that marker.

This experiment will split `RUNTIME-011B2` into:

- `RUNTIME-011B2A`: **Oracle complete** for live AppleScript-driven Roastty app
  workflow automation covering launch, app dictionary access, window creation,
  tab creation/selection, terminal split/focus/close, and terminal text input
  command dispatch.
- `RUNTIME-011B2B`: **Gap** for remaining live macOS GUI behavior: native menu
  display/validation, titlebar/fullscreen/quick-terminal visual behavior,
  screenshot/pixel evidence, broader command-palette GUI behavior, and deeper
  input navigation/pixel walkthroughs.

This experiment will not claim visual parity with Ghostty, native menu display
or validation parity, fullscreen parity, quick-terminal parity, screenshot/pixel
parity, or complete keyboard/mouse walkthrough parity.

## Changes

- `issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py`
  - Add a live guard that builds on the macOS AppleScript testing instructions:
    target `roastty/macos/build/Debug/Roastty.app` by absolute app path, launch
    it, drive it with `osascript`, and quit/clean up in a `finally` path.
  - Create an isolated temporary config with `macos-applescript = true` and
    launch the debug binary with `ROASTTY_CONFIG_PATH` so the test does not
    depend on the user's normal `~/.config/roastty/config`.
  - Assert the live scripting surface can query windows/tabs/terminals, create a
    new window, create/select a tab, split/focus/close terminals, close the
    created tab/window, and dispatch `input text` to a live terminal.
  - For `input text`, create a terminal from a temporary surface configuration
    whose `command` waits for one stdin line and writes it to a temp file. Send
    the marker plus newline with AppleScript `input text`, then assert the file
    contains the marker before claiming input dispatch parity.
  - Keep assertions structural and command-based; do not infer pixel or visual
    parity from AppleScript object counts.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-011B2` into the complete live AppleScript workflow runtime
    row and the reduced remaining live macOS GUI gap.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223/runtime guards
  - Update expected counts from 73 runtime rows, 66 Oracle-complete rows, and 69
    closed rows to 74 runtime rows, 67 Oracle-complete rows, and 70 closed rows.
    Incomplete and gap counts remain 4.
  - Update references from `RUNTIME-011B2` to `RUNTIME-011B2B` where they mean
    the remaining visual/native-menu/fullscreen/quick-terminal GUI gap.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- The built debug Roastty app launches from an absolute app bundle path.
- The guard enables `macos-applescript` using an isolated debug config path, not
  the user's normal config.
- AppleScript can address the built app by absolute bundle path and read the
  expected application/window/tab/terminal object model.
- AppleScript can create a new window.
- AppleScript can create and select a tab in that window.
- AppleScript can split a terminal, focus the new terminal, and close the
  created split.
- AppleScript can dispatch `input text` to the focused terminal and the
  controlled child process records the exact marker in a temp file.
- The guard always quits or kills only the debug app process it launched.
- `RUNTIME-011B2A` is `Oracle complete` and cites the live guard.
- `RUNTIME-011B2B` remains `Gap` for native menu display/validation,
  titlebar/fullscreen/quick-terminal visuals, screenshot/pixel evidence, broader
  command-palette GUI behavior, and deeper input walkthroughs.
- CFG-223 remains `Gap`.

Commands:

```bash
(cd roastty && macos/build.nu --action build)
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$f"
done
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/167-macos-applescript-workflow-runtime.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Fail criteria:

- The guard targets the app by name instead of absolute bundle path.
- The guard depends on the user's normal config or leaves user defaults/config
  state behind.
- The guard leaves the debug app running after failure.
- The guard treats `input text` returning without error as sufficient evidence
  without asserting the marker reached the child process.
- The guard claims visual, menu, fullscreen, quick-terminal, screenshot, or
  broad command-palette parity from AppleScript object-count assertions.
- `RUNTIME-011B2B` omits any remaining live GUI visual or native-menu gaps.
- CFG-223 is marked complete.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

- Required: the `input text` pass criterion only required no AppleScript/runtime
  error. That would not prove the marker reached the terminal child process.

Fix:

- Tightened the design so the future live guard must create a terminal from a
  temporary surface configuration whose `command` reads one stdin line and
  writes it to a temp file. The guard must send the marker plus newline with
  AppleScript `input text` and assert the file contains the marker before
  claiming input dispatch parity.

Re-review verdict: **Approved**. The reviewer confirmed the pass/fail criteria
now require the controlled child process to record the exact marker and reject
treating `input text` returning without error as sufficient evidence.
