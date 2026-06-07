+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 778: Narrow Surface Binding Checklist Sync

## Description

Use a narrower reviewed verification strategy to sync the Issue 801 C ABI
checklist wording for surface key dispatch and binding-action parsing.

Experiment 776 found code evidence that the checklist's
`keybinding/action dispatch` and `full binding-action parsing` missing-work
phrases are likely stale, but its broad `surface_binding_action_` proof did not
complete. Experiment 777 showed the PTY-backed paste-path tests pass
individually but are too timing-heavy and variable to use as broad
checklist-proof coverage. This experiment therefore avoids the broad filter and
verifies the same checklist claim with focused action-family tests plus source
inspection.

The experiment only removes or rewrites those two stale phrases if the focused
verification passes. It must leave the app/surface C ABI checklist item
unchecked because frontend selection routing and split tree/frontend mutations
remain listed as missing.

## Changes

- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Remove or rewrite only the stale `keybinding/action dispatch` and
    `full binding-action parsing` missing-work phrases if targeted verification
    proves them complete enough for checklist wording.
  - Keep the app/surface C ABI item unchecked and preserve the remaining missing
    work.

## Verification

- Inspect `roastty/include/roastty.h` to confirm the public C ABI exposes
  `roastty_surface_key`, `roastty_surface_key_is_binding`, and
  `roastty_surface_binding_action`.
- Inspect `roastty/src/lib.rs` to confirm:
  - `Surface::key` dispatches configured and default keybindings;
  - configured/default key dispatch routes through the shared binding-action
    parser/executor;
  - `roastty_surface_binding_action` exposes binding-action string invocation
    through the C ABI;
  - parser coverage includes the non-PTY action families listed in the C ABI
    checklist wording.
- Run focused key dispatch tests:
  - `cargo test -p roastty surface_key_default -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_key_configured -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_key_is_binding -- --nocapture --test-threads=1`
- Run focused binding-action tests that avoid the slow PTY paste-path proof:
  - `cargo test -p roastty surface_binding_action_false_paths_do_not_forward -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_forwards_supported_split_actions -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_close_surface -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_text_ -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_csi_esc -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_title -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_reset -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_clear_screen -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_select_all -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_adjust_selection -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_copy_to_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_copy_url_to_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_selection_file_false_paths -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_screen_file_false_paths -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_scrollback_file_false_paths -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_selection_file_copy -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_screen_file_copy -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_scrollback_file_copy -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_selection_file_open -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_screen_file_open -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_write_scrollback_file_open -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_copy_title_to_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_paste_from_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_font_size -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_jump_to_prompt -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_scroll_ -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_auto_split -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_tab_window -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_tab_navigation -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_new_window -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_search_overlay -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_navigate_search -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_search_selection -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_app_runtime -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_runtime_ui -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_runtime_control -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_undo_redo -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_toggle_mouse_reporting -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_toggle_readonly -- --nocapture --test-threads=1`
- Carry forward Experiment 777's paste-path evidence in the result:
  - the PTY-backed paste queue tests passed individually;
  - they are not rerun here because Experiment 777 showed they are too slow and
    variable for broad checklist-proof coverage;
  - any checklist wording change must state that paste-path parsing/execution is
    covered by targeted slow tests rather than by this focused filter set.
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/778-narrow-surface-binding-checklist-sync.md`
- Run:
  - `git diff --check`

The experiment passes if the README update is documentation-only, removes only
the two verified-stale missing-work phrases, leaves the app/surface C ABI item
unchecked, and all focused verification commands pass. It is Partial if the
source inspection supports the checklist update but any focused test filter is
too slow, too broad, or inconclusive. It fails if focused verification shows the
phrases are still accurate.

## Design Review

Codex reviewed the initial design and found two issues: the focused test set
omitted implemented action families needed to justify removing
`full binding-action parsing`, and the plan did not explicitly carry forward
Experiment 777's paste-path evidence.

The design was updated to add focused filters for `close_surface`,
`copy_title_to_clipboard`, `new_window`, `navigate_search`, and `auto_split`. It
was also updated to require the result to record that PTY-backed paste-path
coverage comes from Experiment 777's targeted slow tests rather than this
focused filter set. Codex reviewed the revision, found no blockers, and approved
the Experiment 778 plan commit.
