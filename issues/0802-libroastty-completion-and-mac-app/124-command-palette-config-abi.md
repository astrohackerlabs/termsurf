# Experiment 124: Phase G — command palette config ABI

## Description

Expose the already-ported `command-palette-entry` config list through the
embedded C config ABI. Experiment 85 implemented the parser/formatter surface
for the pinned upstream default command list, custom repeatable entries,
`clear`, empty reset, duplicate fields, and action validation, but explicitly
left C ABI command-list exposure out of scope.

The header already contains the upstream-shaped `roastty_command_s` and
`roastty_config_command_list_s` structs. This experiment fills the runtime gap:
`roastty_config_get(config, ..., "command-palette-entry", ...)` should return a
borrowed list of C command entries backed by the `RoasttyConfig` handle,
matching upstream `RepeatableCommand.cval()` semantics closely enough for the
renamed macOS app to consume the command palette catalog.

## Changes

- `roastty/src/lib.rs`
  - Add C-layout Rust mirrors for:
    - `roastty_command_s` with `action_key`, `action`, `title`, and
      `description` pointers;
    - `roastty_config_command_list_s` with pointer plus length.
  - Add cache storage to the `Config` handle for command-palette C entries and
    their nul-terminated strings.
  - Rebuild the command cache whenever parsed config state is synchronized,
    cloned, finalized, loaded from files/default files/CLI, or directly changed
    through config setters.
  - Populate each C command entry from `Config::command_palette_entry.entries`:
    - `action` is the canonical action string already stored by Experiment 85;
    - `title` and `description` are the parsed strings;
    - `action_key` is the upstream action tag name, derived from the canonical
      action string before its first `:` parameter.
  - Ensure all string pointers are nul-terminated and stable until the config is
    mutated, cloned, or freed.
  - Extend `roastty_config_get` so the key `command-palette-entry` writes a
    `RoasttyConfigCommandList` and returns `true`. Empty lists should return a
    non-null or null pointer consistently with Rust slice/vector behavior, but
    must always report `len = 0` and be safe for C callers that only iterate
    `len` entries.
  - Preserve existing config-get failure semantics for null config/output/key
    and unknown keys.
- `roastty/tests/abi_harness.c`
  - Assert the config command-list ABI is readable from C.
  - Verify a default config returns the pinned upstream count of 88 entries.
  - Spot-check representative default entries: title, description, action, and
    action key.
  - Verify `command-palette-entry=clear` returns `len = 0`.
  - Verify a custom entry returns the canonical action string and action key,
    including shorthand canonicalization such as `copy_to_clipboard` becoming
    `copy_to_clipboard:mixed`.
  - Verify clones retain independent, readable command-list storage after the
    source config is freed.
- `roastty/src/lib.rs` unit tests
  - Add focused Rust-side tests for cache rebuilding and pointer stability
    across config mutation and clone.

Out of scope:

- Command-palette UI behavior.
- App-side command dispatch beyond exposing the catalog.
- Adding or changing command defaults from Experiment 85's pinned 88-entry list.
- Implementing the remaining `crash` binding action.
- Native keymaps, native global shortcut registration, and broader global/all
  routing.

## Verification

- Run formatting:
  - `cargo fmt`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/124-command-palette-config-abi.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Run targeted tests:
  - `cargo test -p roastty command_palette`
  - `cargo test -p roastty config_get`
  - `cargo test -p roastty --test abi_harness`
- Run full Roastty tests:
  - `cargo test -p roastty -- --test-threads=1`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run the same Prettier command with `--check`.

**Pass** = the command-palette command list is exposed through
`roastty_config_get`, C and Rust tests prove default/custom/clear/clone
behavior, and targeted plus full tests pass.

**Partial** = the default list is exposed, but custom or clone/cache behavior
needs a follow-up.

**Fail** = exposing the command list requires a larger config ABI redesign.

## Design Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb7f6-71df-76e1-9b59-b25990f90660`)

**Verdict:** Approved

**Findings:** None.
