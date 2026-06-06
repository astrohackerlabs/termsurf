+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 753: Config CLI Keybind Action Diagnostics

## Description

Validate CLI-loaded keybind actions during config loading and expose the first
real Roastty config diagnostics. Experiments 749 through 752 made simple
root-table CLI keybinds parseable, queryable, and dispatchable, but
`parse_config_keybind` still accepts any non-empty action bytes. That temporary
behavior lets unsupported actions become configured bindings, shadow static
defaults, and then fall through at dispatch.

Ghostty's binding parser parses the action during keybind config parsing and
returns an invalid-action error for unsupported actions. Roastty should move in
that direction now that configured dispatch exists: unsupported configured
actions should be rejected before they are stored, and the config should carry a
diagnostic explaining why the keybind was ignored.

This experiment keeps the existing simple root-table trigger grammar. It does
not implement key tables, key sequences, `clear`, `unbind`, `chain`,
global/all/unconsumed/performable flags, config-file loading, rich diagnostic
metadata, or full Ghostty replay semantics.

## Changes

- `roastty/src/lib.rs`
  - Add a `diagnostics` list to `Config`, stored as owned C strings.
  - Clone diagnostics in `roastty_config_clone`.
  - Make `roastty_config_diagnostics_count` report the config's diagnostic
    count, returning `0` for null configs.
  - Make `roastty_config_get_diagnostic` return the requested diagnostic message
    for valid indexes and the existing empty diagnostic for null configs or
    out-of-range indexes.
  - Refactor binding-action parsing so config validation can validate an action
    without a live `Surface`. `new_split` without an explicit direction should
    remain accepted by using a deterministic placeholder auto direction for
    validation.
  - Validate CLI keybind action bytes with the same supported binding-action
    parser before storing the keybind.
  - Reject malformed keybind values and unsupported action strings with
    diagnostics, and do not store those keybinds.
  - Preserve existing behavior for unsupported trigger grammar: malformed
    triggers are ignored, but now they produce diagnostics.
  - Preserve configured keybind semantics for valid actions, including duplicate
    trigger/action behavior, config/surface queries, and dispatch.
  - Supersede Experiment 752's temporary unsupported-action runtime behavior:
    because unsupported actions are no longer stored, they no longer shadow
    static defaults or configured queries.
- `roastty/tests/abi_harness.c`
  - Add C coverage for diagnostics count and messages after malformed
    `--keybind` values.
  - Assert diagnostics clone with `roastty_config_clone`.
  - Assert invalid indexes and null configs return a non-null empty diagnostic
    message.
  - Assert unsupported CLI keybind actions are not stored, do not appear in
    config/surface keybind queries, and do not shadow static defaults.
- Tests in `roastty/src/lib.rs`
  - Cover diagnostics for malformed trigger syntax, missing action, unsupported
    physical keys, and unsupported action names.
  - Cover supported action validation for representative parameterized actions:
    `text:hello`, `new_split`, `new_split:right`, `goto_tab:1`,
    `resize_split:right,10`, `copy_to_clipboard:plain`, `set_font_size:14`, and
    `navigate_search:next`.
  - Cover unsupported action parameters such as `goto_tab:not-a-number`,
    `resize_split:right`, `copy_to_clipboard:bad`, and `new_window:bad`.
  - Cover diagnostics cloning and out-of-range diagnostic access.
  - Update configured dispatch/query tests that used `unknown_action` so they
    now assert unsupported keybinds are rejected during config load and static
    defaults behave as if the configured keybind was never set.
  - Keep existing `config_cli_keybind`, `config_key_is_binding`,
    `surface_key_is_binding`, `surface_key`, `surface_key_default`,
    `binding_action`, and ABI harness tests passing.

## Verification

- `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
- `cargo test -p roastty config_diagnostic -- --nocapture --test-threads=1`
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty surface_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty surface_key_default -- --nocapture --test-threads=1`
- `cargo test -p roastty surface_key -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness -- --nocapture`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 753 design and approved it for the plan commit
with no blocking findings. The review confirmed that rejecting unsupported
actions during CLI config loading cleanly supersedes Experiment 752's temporary
unsupported-action shadowing behavior: invalid configured bindings are no longer
stored, so config/surface queries and dispatch naturally fall back to static
defaults when applicable.

The review also accepted the scoped diagnostics ABI plan: diagnostics are
config-owned, null configs report zero diagnostics, valid indexes return owned
messages, and null or out-of-range diagnostic access returns the existing empty
diagnostic. It found no must-fix verification gaps. As a non-blocking note, it
suggested pinning diagnostic ordering for multiple CLI failures in a future or
opportunistic test.

## Result

**Result:** Pass

Roastty now rejects invalid CLI keybind actions during config loading instead of
storing them for later dispatch. CLI keybind parsing reports diagnostics for
missing values, malformed trigger/action syntax, unsupported triggers, and
unsupported action strings. Diagnostics are stored on the config, cloned by
`roastty_config_clone`, counted by `roastty_config_diagnostics_count`, and
returned through `roastty_config_get_diagnostic` with the existing empty
diagnostic for null or out-of-range access.

The binding-action parser now has a surface-independent validation path, using a
deterministic placeholder direction for `new_split` auto validation while
runtime dispatch still computes the direction from the live surface. Valid CLI
keybinds keep their previous configured query and dispatch behavior. Invalid
configured actions are not stored, so they do not appear in config/surface
keybind queries and do not shadow static defaults.

Verification passed:

- `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
- `cargo test -p roastty config_diagnostic -- --nocapture --test-threads=1`
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty surface_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty surface_key_default -- --nocapture --test-threads=1`
- `cargo test -p roastty surface_key -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1` (130
  passed)
- `cargo test -p roastty --test abi_harness -- --nocapture`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

After the completion review, the non-blocking coverage suggestions were also
addressed with direct tests for missing-action diagnostics and empty
null/out-of-range diagnostic messages.

## Completion Review

Codex reviewed the completed implementation and found no blocking issues. The
review agreed that action validation now happens before storage, invalid
keybinds produce config diagnostics, diagnostics are config-owned and cloned,
and invalid configured actions fall back to static defaults instead of shadowing
them.

The review considered the verification sufficient for the result commit. It
noted two non-blocking coverage suggestions: direct C ABI assertions for
null/out-of-range empty diagnostics and a direct missing-action diagnostic pin.
Both were added before recording this result.

## Conclusion

Experiment 753 closes the unsafe gap left by Experiment 752's temporary
unsupported-action behavior. The simple CLI root-table keybind path now has
config-time action validation and real diagnostics, matching the direction of
Ghostty's parser while keeping richer config-file and key-table semantics out of
scope.
