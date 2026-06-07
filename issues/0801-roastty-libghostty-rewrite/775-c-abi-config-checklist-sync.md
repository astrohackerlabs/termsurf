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

# Experiment 775: C ABI Config Checklist Sync

## Description

Sync the Issue 801 C ABI checklist after the completed `roastty_config_get` and
config-trigger experiments.

The checklist still marks `config_get` (12 defaults only) plus keybind triggers
as incomplete and says keybind parsing/storage and real trigger lookup are
missing. The experiment index and code now show the relevant slices have landed:
default triggers, keybind parsing/storage, keybind diagnostics, trigger lookup,
`key_is_binding`, and the 12 default `roastty_config_get` keys.

This experiment only updates issue documentation if file/test verification
confirms that the checklist item is now complete. It does not change Roastty
code.

## Changes

- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Update the C ABI checklist item for `config_get` and keybind triggers from
    incomplete to complete if verification confirms the implemented surface.
  - Replace stale missing-work text with a concise note identifying the
    completed pieces.

## Verification

- Inspect `roastty/src/lib.rs` to confirm `roastty_config_get` handles the 12
  configured keys currently in scope:
  - `initial-window`
  - `quit-after-last-window-closed`
  - `window-save-state`
  - `window-decoration`
  - `window-theme`
  - `background-opacity`
  - `bell-audio-volume`
  - `notify-on-command-finish-after`
  - `title`
  - `window-position-x`
  - `window-position-y`
  - `bell-audio-path`
- Inspect `roastty/include/roastty.h` to confirm the public C ABI still exposes
  `roastty_config_get`, `roastty_config_trigger`, and
  `roastty_config_key_is_binding`.
- Inspect the existing tests around config triggers and keybind parsing/storage
  in `roastty/src/lib.rs`.
- Run:
  - `cargo test -p roastty config_get_ -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_trigger -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/775-c-abi-config-checklist-sync.md`
  - `git diff --check`

The experiment passes if the checklist update is documentation-only, matches the
implemented/tested C ABI surface, and the verification commands pass.

## Design Review

Codex reviewed the design and found two verification gaps: the original plan did
not inspect the public header and did not run the default
`config_key_is_binding` tests. The plan was updated to inspect
`roastty/include/roastty.h` and add the missing test filter.

The follow-up review found no blockers and approved the design for the plan
commit.
