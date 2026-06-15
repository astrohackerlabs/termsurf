# Experiment 170: AppleScript Split Terminal Lifecycle

## Description

`RUNTIME-011B2B` still includes returned split-terminal object re-resolution and
focus/close commands in the remaining live macOS GUI/workflow gap. Experiment
167 proved that AppleScript can create a split terminal whose controlled command
runs, but it intentionally stopped short of proving that the returned split
terminal object can be re-resolved through its stable scripting ID and used for
follow-up terminal commands.

This experiment will split the AppleScript object-lifecycle slice out of
`RUNTIME-011B2B`:

- returned split terminal objects have a non-empty stable `id`;
- `terminal id "<id>"` can re-resolve that returned split terminal from the
  application, window, and selected-tab collections;
- `input text` sent to the re-resolved split terminal reaches the split's
  controlled child process;
- `focus` on the re-resolved split terminal makes it the selected tab's focused
  terminal;
- `close` on the re-resolved split terminal removes that terminal from the live
  tab's terminal collection.

This will not claim native menu display/validation, titlebar/fullscreen/quick
terminal visual behavior, screenshot/pixel parity, split pixel/layout parity,
broader command-palette GUI behavior, or deeper keyboard/mouse walkthrough
parity.

## Changes

- `issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py`
  - Extend the live debug-app workflow after split creation.
  - Store the returned split terminal in AppleScript and assert its `id` is
    non-empty.
  - Re-resolve the split terminal by stable ID from:
    - `terminal id <id>` at application scope;
    - `terminal id <id> of w`;
    - `terminal id <id> of selected tab of w`.
  - Send a second controlled input marker to the re-resolved split terminal and
    assert the split child process records it in a temp file.
  - Run `focus` on the re-resolved split terminal and assert
    `id of focused terminal of selected tab of w` matches the split terminal ID.
  - Run `close` on the re-resolved split terminal and assert the selected tab's
    terminal count decreases and `terminal id <id>` no longer resolves.
  - Keep the existing absolute app bundle launch, isolated config, scoped
    process cleanup, and crash-report guard behavior.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split a new Oracle-complete row from `RUNTIME-011B2B` for live AppleScript
    split-terminal object lifecycle, re-resolution, focus, input, and close
    commands.
  - Reduce the remaining `RUNTIME-011B2B` gap so it no longer lists returned
    split-terminal object re-resolution and focus/close commands.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- Existing CFG-223/runtime guards
  - Update expected counts from 75 runtime rows, 68 Oracle-complete rows, and 71
    closed rows to 76 runtime rows, 69 Oracle-complete rows, and 72 closed rows.
    Incomplete and gap counts remain 4.
  - Update references that describe the remaining macOS app GUI gap so they no
    longer require the split-terminal lifecycle slice.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- The built debug Roastty app launches from the absolute app bundle path.
- The guard uses an isolated config with `macos-applescript = true` and does not
  depend on the user's normal config.
- The guard creates a split terminal from a controlled command.
- The returned split terminal's `id` is non-empty.
- The returned split terminal can be re-resolved by stable ID at application,
  window, and selected-tab scope.
- `input text` sent to the re-resolved split terminal reaches the split child
  process and records an exact second marker.
- `focus` on the re-resolved split terminal changes the selected tab's focused
  terminal ID to the split terminal ID.
- `close` on the re-resolved split terminal reduces the selected tab terminal
  count and makes the split terminal ID no longer resolve.
- The live guard still fails if a new Roastty crash report appears during the
  workflow.
- The new runtime inventory row is `Oracle complete`.
- `RUNTIME-011B2B` remains `Gap` for native menu display/validation,
  titlebar/fullscreen/quick-terminal visuals, screenshot/pixel evidence, broader
  command-palette GUI behavior, split visual/layout parity, and deeper
  keyboard/mouse walkthroughs.
- CFG-223 remains `Gap`.

Commands:

```bash
(cd roastty && macos/build.nu --action build)
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$f"
done
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/170-applescript-split-terminal-lifecycle.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Fail criteria:

- The guard treats split creation as enough without re-resolving the returned
  terminal by ID.
- The guard treats an AppleScript command returning without error as enough
  without checking child-process marker files or object-count/focused-terminal
  state.
- The guard depends on the user's normal config or leaves the debug app running.
- The guard closes the wrong terminal or only proves tab/window close behavior.
- The inventory claims visual, menu, fullscreen, quick-terminal, screenshot, or
  broader command-palette GUI parity.
- CFG-223 is marked complete.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer confirmed that the README links Experiment 170 as `Designed`, the
experiment has the required design sections, the scope is narrow and avoids
visual/menu/fullscreen/quick-terminal/pixel claims, the AppleScript plan is
plausible against `Roastty.sdef` and the `ScriptTerminal`/`ScriptTab`/
`ScriptWindow` APIs, the verification proves re-resolution, input side effects,
focus, and close/count behavior rather than no-error command execution, the
build/runtime/prettier/diff hygiene checks are present, and the planned runtime
count transition is consistent with the current 75/68/71/4/4 CFG-223 baseline.

## Result

**Result:** Pass

The live AppleScript guard now proves the split-terminal object lifecycle
against the built debug Roastty app:

- the returned split terminal has a non-empty stable ID;
- that ID re-resolves at application, window, and selected-tab scope;
- `input text` sent to the selected-tab re-resolved terminal reaches the split
  child process and records the exact `ISSUE805_EXP170_SPLIT_INPUT_MARKER`;
- `focus` changes the selected tab's focused terminal ID to the split terminal
  ID;
- `close` reduces the selected tab terminal count and the closed terminal ID no
  longer resolves;
- the live guard still uses an isolated config, scoped debug app cleanup, and a
  crash-report check.

`config_runtime_inventory.py` now adds `RUNTIME-011B2D` as Oracle-complete for
the live AppleScript split-terminal object lifecycle, and the remaining
`RUNTIME-011B2B` gap no longer lists returned split-terminal object
re-resolution or focus/close commands. CFG-223 remains `Gap`.

Verification run:

```text
(cd roastty && macos/build.nu --action build)
** BUILD SUCCEEDED **

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
runtime_rows=76
oracle_complete=69
closed=72
audit_covered=0
incomplete=4
gap=4
cfg223=Gap

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_applescript_workflow_runtime.py
macos_applescript_workflow_runtime=pass

PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/platform_runtime_classification.py --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
platform_options=32
gap=15
not_applicable=15
oracle_complete=2

for f in issues/0805-roastty-ghostty-parity/*_runtime_parity.py issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py issues/0805-roastty-ghostty-parity/link_hover_preview_dispatch_parity.py issues/0805-roastty-ghostty-parity/link_hover_modifier_refresh_parity.py issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$f" || exit 1
done
runtime_guards=pass

prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/170-applescript-split-terminal-lifecycle.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md issues/0805-roastty-ghostty-parity/platform-runtime-classification.md
git diff --check
```

## Conclusion

Experiment 170 closes the live AppleScript split-terminal object lifecycle
slice. The macOS app gap is now narrower: native menu display/validation,
titlebar/fullscreen/quick-terminal visuals, screenshot/pixel evidence, broader
command-palette GUI behavior, split visual/layout parity, and deeper
keyboard/mouse walkthroughs remain in `RUNTIME-011B2B`.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer independently confirmed that the result commit had not yet been
made, `RUNTIME-011B2D` is present and Oracle-complete, CFG-223 remains `Gap`
with 76 runtime rows, 69 Oracle-complete rows, 72 closed rows, 4 incomplete
rows, and 4 gaps, the AppleScript guard checks app/window/tab ID re-resolution,
child-process marker side effects, focus state, close count decrement, and
closed-ID non-resolution, the remaining visual/menu/fullscreen/
quick-terminal/screenshot/split-layout/broader GUI gaps stay in
`RUNTIME-011B2B`, the README marks Experiment 170 as `Pass`, the experiment has
`Result` and `Conclusion`, the edited Python files parse, and `git diff --check`
passes.
