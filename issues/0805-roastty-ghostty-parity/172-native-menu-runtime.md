# Experiment 172: Native Menu Runtime

## Description

`RUNTIME-011B2B` still includes native menu display/validation in the remaining
live macOS GUI gap. Earlier experiments proved AppleScript app/window/tab/split
commands and lower-level AppleScript keyboard/mouse input delivery, but they did
not prove that Roastty's real macOS menu bar is visible to the OS accessibility
tree, validates representative menu items correctly, or dispatches
representative native menu actions into the running app.

This experiment will split a narrow native-menu slice out of `RUNTIME-011B2B` by
adding a live debug-app guard that uses System Events against the exact launched
Roastty PID. The guard will prove:

- the native menu bar is present for the debug app process;
- expected top-level menus and representative menu items are visible;
- representative validation states are correct with a live terminal window;
- representative native menu actions mutate app state and are observable through
  the existing AppleScript object model.

This experiment will not claim titlebar/fullscreen/quick-terminal visuals,
screenshot/pixel evidence, split visual/layout parity, broader command-palette
GUI behavior, cursor/pointer pixels, broad keyboard/mouse walkthrough parity, or
notification/link/bell GUI effects.

## Changes

- `issues/0805-roastty-ghostty-parity/macos_native_menu_runtime.py`
  - Add a new live debug-app guard using the same absolute app bundle, isolated
    config, scoped process cleanup, and new-crash-report failure pattern used by
    `macos_applescript_workflow_runtime.py`.
  - Launch the debug app with `macos-applescript = true` and a controlled child
    command so the app has a real terminal window.
  - Use System Events to resolve the application process by exact Unix PID and
    fail if the frontmost process is not that PID before inspecting or clicking
    menus.
  - Assert the menu bar exposes the expected top-level menu names, including the
    application, File, Edit, View, Window, and Help menus.
  - Assert representative menu items exist and are enabled when a terminal
    window is active: New Window, New Tab, Split Right, Split Left, Split Down,
    Split Up, Close, Toggle Full Screen, Quick Terminal, and Command Palette.
  - Assert representative validated menu items reflect app state: Undo and Redo
    are disabled with no undo stack, and Float on Top / Use as Default are
    enabled only while a primary terminal window is key.
  - Click New Tab through the native File menu and assert the selected window's
    tab count increases through AppleScript.
  - Click Split Right through the native File menu and assert the selected tab's
    terminal count increases through AppleScript.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split a new Oracle-complete row from `RUNTIME-011B2B` for live native menu
    visibility, representative validation, and representative action dispatch.
  - Reduce the remaining `RUNTIME-011B2B` gap so it no longer lists native menu
    display/validation.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223/runtime guards
  - Update expected counts from 77 runtime rows, 70 Oracle-complete rows, and 73
    closed rows to 78 runtime rows, 71 Oracle-complete rows, and 74 closed rows.
    Incomplete and gap counts remain 4.
  - Update references that describe the remaining macOS app GUI gap so they no
    longer require native menu display/validation.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- The built debug Roastty app launches from the absolute app bundle path.
- The guard uses an isolated config with `macos-applescript = true` and does not
  depend on the user's normal config.
- System Events targets the exact launched Roastty Unix PID before inspecting or
  clicking menus.
- The native menu bar exposes expected top-level menus and representative menu
  items.
- Representative menu validation is observed through System Events, including
  disabled Undo/Redo and enabled terminal-window items when a primary terminal
  window is key.
- Clicking New Tab through the native menu increases the selected window's tab
  count.
- Clicking Split Right through the native menu increases the selected tab's
  terminal count.
- The live guard still fails if a new Roastty crash report appears during the
  workflow.
- The new runtime inventory row is `Oracle complete`.
- `RUNTIME-011B2B` remains `Gap` for titlebar/fullscreen/quick-terminal visuals,
  screenshot/pixel evidence, broader command-palette GUI behavior, split
  visual/layout parity, cursor/pointer pixels, and broader keyboard/mouse
  walkthroughs.
- CFG-223 remains `Gap`.

Commands:

```bash
(cd roastty && macos/build.nu --action build)
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_native_menu_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$f"
done
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_app_workflow_plumbing_parity.py
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/172-native-menu-runtime.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
git diff --check
```

Fail criteria:

- The guard targets the app by process name only instead of exact Unix PID.
- The guard treats app launch or AppleScript command success as sufficient
  without inspecting the real native menu accessibility tree.
- The guard only checks menu item existence and does not check representative
  validation state.
- The guard clicks menu items without proving resulting app state changes.
- The guard depends on the user's normal config or leaves the debug app running.
- The inventory claims titlebar/fullscreen/quick-terminal visuals,
  screenshot/pixel evidence, split visual/layout parity, cursor/pointer pixels,
  broader command-palette GUI behavior, or broad keyboard/mouse walkthrough
  parity.
- CFG-223 is marked complete.

## Design Review

Adversarial review was performed by a fresh-context Codex subagent.

Verdict: Approved.

Findings: none.
