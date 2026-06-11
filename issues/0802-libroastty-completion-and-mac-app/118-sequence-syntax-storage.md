# Experiment 118: Phase G — sequence syntax storage

## Description

Add the parser/storage foundation for upstream multi-key sequence keybindings in
Roastty's configured keybinding path.

Upstream Ghostty accepts triggers separated by `>` such as
`ctrl+a>n=new_window`. Internally, the binding set is a trie: intermediate
triggers are leaders, and the final trigger stores the action. This experiment
ports that storage shape for configured keybindings without enabling runtime
sequence matching yet.

This mirrors the key-table rollout from Experiments 116 and 117: first parse and
store the syntax faithfully, then activate runtime lookup in a follow-up slice.
This experiment does not implement runtime `sequence_set` state, queued leader
encoding, invalid-sequence flush/drop behavior, `ignore`, `end_key_sequence`,
`chain=`, app `ROASTTY_ACTION_KEY_SEQUENCE` notifications, native keymaps,
native global shortcuts, or sequence-aware `roastty_app_key`.

## Changes

- `roastty/src/lib.rs`
  - Add owned configured-keybind set storage that can represent:
    - leaf bindings;
    - leader nodes with nested sets.
  - Keep the existing flat `keybind_triggers` vector as the runtime root binding
    source for this experiment, so existing root single-key behavior is
    unchanged.
  - Add parallel sequence-capable storage for:
    - root configured keybindings;
    - each named key table's bindings.
  - Extend configured keybind parsing so the trigger side can split on `>`
    before parsing the action.
    - Empty segments are invalid.
    - Each segment uses the existing single-trigger parser.
    - `global:` and `all:` prefixes are invalid for sequences, matching
      upstream.
    - Single-trigger inputs continue to store exactly as they do today.
  - Store sequence bindings in the sequence-capable set:
    - intermediate triggers become leader nodes;
    - the final trigger stores the binding action and flags;
    - adding a sequence under an existing leaf replaces that leaf with a leader;
    - adding a direct leaf over an existing leader removes that prior sequence
      subtree;
    - later bindings override earlier bindings at the same final trigger.
  - Apply the same sequence storage rules inside named key tables.
  - Continue to keep table sequence bindings out of root `keybind_triggers`, and
    keep all sequence bindings out of runtime single-key lookup until the
    runtime sequence experiment.
  - Clone sequence-capable storage through `roastty_config_clone`,
    `roastty_app_new`, and `roastty_app_update_config`.
  - Keep `roastty_config_trigger`, `roastty_config_key_is_binding_handle`,
    `roastty_surface_key_is_binding_handle`, `roastty_surface_key`, and
    `roastty_app_key` behavior unchanged for sequences in this slice.
- `roastty/tests/abi_harness.c`
  - Add C ABI coverage proving CLI sequence keybinds parse without diagnostics,
    stay out of root config/surface single-key binding checks, and survive
    config clone/app copy paths without crashing.
- Tests in `roastty/src/lib.rs`
  - Parse/store a root sequence such as `ctrl+a>n=new_window` as a leader plus
    final leaf without adding either trigger to the flat runtime root vector.
  - Parse/store nested sequences with more than two segments.
  - Reject malformed sequence syntax:
    - empty segment before, between, or after `>`;
    - invalid segment trigger;
    - `global:` or `all:` sequence prefixes.
  - Preserve single-key root behavior for non-sequence keybinds.
  - Verify upstream override rules:
    - a sequence under an existing leaf replaces the leaf with a leader;
    - a direct leaf over an existing leader removes the sequence subtree;
    - a later sequence leaf overrides an earlier sequence leaf at the same path.
  - Store table-local sequences under their named table without adding root
    bindings.
  - Clear table-local sequence storage when parsing `table-name/`.
  - Clone sequence storage through config clone and app update.
  - Verify sequence bindings are intentionally inert for runtime config/app/
    surface lookup in this experiment.

## Verification

- Run:
  - `cargo test -p roastty sequence`
  - `cargo test -p roastty parse_config_keybind`
  - `cargo test -p roastty key_table`
  - `cargo test -p roastty surface_key`
  - `cargo test -p roastty app_key`
  - `cargo test -p roastty --test abi_harness`
  - `cargo test -p roastty -- --test-threads=1`
  - `cargo fmt`
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/118-sequence-syntax-storage.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Design Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb760-4859-79d1-b656-99a68b3a38c6`)

**Verdict:** Approved

**Findings:** None.

The reviewer verified that the issue README links Experiment 118 as `Designed`,
the experiment has Description, Changes, and Verification sections, the scope
matches Phase G sequence/keybinding work, the storage plan matches upstream
leader/leaf replacement semantics, and the verification list includes focused
tests, the ABI harness, full Roastty tests, formatting checks,
`git diff --check`, and Prettier.

## Result

**Result:** Pass

Implemented the configured-keybind sequence syntax/storage foundation:

- Added `ConfigKeybindSet` trie storage with leaf bindings and leader nodes.
- Parsed `>`-separated configured keybind triggers into sequence bindings.
- Rejected empty sequence segments and `global:` / `all:` sequence prefixes.
- Stored root and table-local sequences separately from current runtime flat
  single-key vectors.
- Preserved current runtime behavior for sequence bindings: sequence-only
  prefixes do not match `roastty_config_key_is_binding_handle`,
  `roastty_surface_key_is_binding_handle`, `roastty_surface_key`, or
  `roastty_app_key` yet.
- Applied upstream-style direct/sequence replacement rules in root and named
  table storage.
- Cloned sequence storage through config clone, app construction, and app config
  update.
- Added Rust unit coverage for root sequences, nested sequences, malformed
  syntax, direct/sequence overrides, table-local sequences and clear behavior,
  table-local overrides, clone/app update propagation, and inert runtime lookup.
- Added C ABI coverage for CLI sequence keybind parsing, invalid global
  sequences, clone/app paths, and current single-key runtime non-matches.

Verification:

- `cargo test -p roastty sequence` — 22 passed.
- `cargo test -p roastty parse_config_keybind` — 16 passed.
- `cargo test -p roastty key_table` — 13 passed.
- `cargo test -p roastty surface_key` — 60 passed.
- `cargo test -p roastty app_key` — 11 passed.
- `cargo test -p roastty --test abi_harness` — 1 passed.
- `cargo test -p roastty -- --test-threads=1` — 4,662 unit tests passed, plus
  the ABI harness and doc tests.
- `cargo fmt` — passed.
- `cargo fmt --check` — passed.
- `git diff --check` — passed.

## Conclusion

Roastty now has a sequence-capable configured-keybinding storage foundation that
matches upstream's leader/leaf trie shape and replacement semantics while
leaving runtime sequence activation intentionally disabled. The next experiment
should activate runtime sequence state: leader matching, queued prefix encoding,
invalid-sequence flush/drop behavior, sequence completion, and eventual
`ROASTTY_ACTION_KEY_SEQUENCE` notifications.

## Completion Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb770-39d0-7a33-bee5-428148da8a01`)

**Verdict:** Approved

**Findings:** None.

The reviewer checked the workflow contract, issue README, experiment file,
implementation diff, changed Rust/C sources, and upstream Ghostty parser,
storage, and runtime references. It confirmed that the implementation remains in
parser/storage scope: `keybind_sequences` is maintained and cloned, while
runtime lookup still uses `keybind_triggers` and table `bindings`, not the
sequence tries.

The reviewer independently reran:

- `cargo test -p roastty sequence` — 22 passed.
- `cargo test -p roastty parse_config_keybind` — 16 passed.
- `cargo test -p roastty key_table` — 13 passed.
- `cargo test -p roastty surface_key` — 60 passed.
- `cargo test -p roastty app_key` — 11 passed.
- `cargo test -p roastty --test abi_harness` — 1 passed.
- `cargo fmt --check` — passed.
- `git diff --check` — passed.
- Prettier markdown check — passed.

It did not rerun the full `cargo test -p roastty -- --test-threads=1` suite; the
implementing pass had already run it successfully. The reviewer also confirmed
that the result commit had not been made before review.
