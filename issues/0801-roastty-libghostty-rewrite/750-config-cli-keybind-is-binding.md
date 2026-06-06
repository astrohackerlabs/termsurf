+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 750: Config CLI Keybind Is Binding

## Description

Make CLI-loaded keybinds participate in `roastty_config_key_is_binding`.
Experiment 749 taught `roastty_config_load_cli_args` to parse simple root-table
`--keybind` values and taught `roastty_config_trigger` to return configured
action triggers, but the config-level event query still only checks static
defaults.

Ghostty keeps a forward keybind set for event matching and a reverse map for
menu/display trigger lookup. Roastty does not have the full keybind set yet, so
this experiment adds the minimal forward matching needed for configured
root-table keybinds while preserving `roastty_config_trigger` fallback behavior.

This remains config-only. It does not implement custom surface dispatch,
binding-action execution, table keybinds, key sequences, `clear`, `unbind`,
global/all prefixes, config-file loading, diagnostics, or keybind flags for
custom bindings.

## Changes

- `roastty/src/lib.rs`
  - Keep all valid loaded root keybinds available for forward event matching on
    `Config` instead of replacing earlier bindings for the same action.
  - Preserve the existing `roastty_config_trigger` behavior of returning the
    last configured trigger for an action before falling back to static defaults
    by scanning configured bindings from newest to oldest.
  - Make `roastty_config_key_is_binding(config, event)` return true when the
    event matches a configured keybind trigger before checking static default
    bindings.
  - Match configured physical triggers against `event.key`.
  - Match configured Unicode triggers against a single-codepoint UTF-8 event
    value, and against `event.unshifted_codepoint` when present.
  - Match modifiers through the existing binding-modifier normalization path so
    left/right modifier variants collapse to the same raw Roastty modifiers used
    by CLI trigger parsing.
  - Reject release events for configured keybind matching, matching the current
    default-binding query behavior.
  - Ignore malformed CLI keybinds exactly as Experiment 749 does; they must not
    affect configured or default event matching.
- `roastty/tests/abi_harness.c`
  - Add C coverage that CLI-loaded keybinds make `roastty_config_key_is_binding`
    return true for matching physical and Unicode key events.
  - Assert nonmatching modifiers, release events, malformed keybinds, null
    config, and null event cases return false or fall back to defaults as
    appropriate.
- Tests in `roastty/src/lib.rs`
  - Cover configured physical, Unicode, and unshifted-codepoint event matching.
  - Cover duplicate action behavior with a concrete sequence such as
    `ctrl+n=new_window` followed by `cmd+n=new_window`: `roastty_config_trigger`
    reports `cmd+n` for display, while configured event matching recognizes both
    `ctrl+n` and `cmd+n`.
  - Cover cloning a config with loaded keybinds and querying the clone.
  - Cover malformed CLI values not creating configured binding matches.

## Verification

- `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty config_trigger -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness -- --nocapture`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the initial Experiment 750 design and found two blocking semantic
gaps: forward event matching conflicted with Experiment 749's replace-by-action
storage unless this experiment explicitly preserved all valid loaded root
bindings, and the duplicate-action verification needed to prove both the old and
latest triggers match events while `roastty_config_trigger` still reports the
latest trigger.

The design was updated so configured keybind storage becomes append-only for
valid root bindings, `roastty_config_trigger` keeps latest-trigger behavior by
reverse scanning, and the duplicate-action tests explicitly cover both forward
matches plus latest-trigger display behavior.

Codex also noted that result-review metadata was premature before
implementation. The new experiment file now records only the design-review
metadata until the completion review is performed.

Codex re-reviewed the corrected design and approved it for the plan commit with
no remaining blocking findings.
