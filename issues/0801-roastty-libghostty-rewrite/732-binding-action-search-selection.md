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

# Experiment 732: Binding Action Search Selection

## Description

Experiment 731 added the parameterless search overlay controls: `start_search`
and `end_search`. The next narrow upstream search binding gap is
`search_selection`.

Upstream Ghostty handles `search_selection` by reading the active selection as
plain text. If there is no active selection, the action returns `false`. If
there is a selection, it forwards a surface-target runtime `.start_search`
action with the selected text as the initial needle.

Roastty already has active-selection formatting for clipboard actions. This
experiment reuses that behavior to add `search_selection` without implementing
the larger internal search engine path. Full `search:<needle>`,
`navigate_search:<direction>`, search match state, and search worker lifecycle
remain out of scope.

## Changes

- `roastty/src/lib.rs`
  - Add a parsed binding-action variant, or equivalent handling, for
    `search_selection`.
  - Extend `parse_binding_action` to accept parameterless `search_selection`.
  - Reject `search_selection:` and non-empty parameters such as
    `search_selection:now`.
  - Add a surface helper that:
    - returns `false` for detached surfaces, missing workers, missing callbacks,
      no active selection, invalid selection formatting, or false callback
      results;
    - formats the active selection as plain text with the same unwrap/trim
      behavior used by `copy_to_clipboard:plain`;
    - forwards `ROASTTY_ACTION_START_SEARCH` through the existing surface-target
      runtime callback with `storage[0]` pointing to a borrowed C string
      containing the selection text, valid only during the callback;
    - keeps all remaining storage slots zeroed.
  - Preserve the existing parameterless `start_search` empty-needle behavior
    from Experiment 731.

- `roastty/tests/abi_harness.c`
  - Add malformed `search_selection` parser rejection checks.
  - Add valid no-callback coverage returning `false`.

- Tests in `roastty/src/lib.rs`
  - Cover parser false paths for empty-colon and non-empty parameters.
  - Cover null, detached, no-worker, no-selection, missing-callback, and false
    callback cases returning `false`.
  - Cover forwarding to the runtime callback with surface target,
    `ROASTTY_ACTION_START_SEARCH`, a borrowed selected-text needle, and zeroed
    storage after `storage[0]`.
  - Cover the plain unwrap/trim formatting semantics with a selection whose raw
    formatted text would differ without trimming trailing spaces.
  - Cover that parameterless `start_search` still forwards an empty needle.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty search_selection -- --nocapture --test-threads=1`
- `cargo test -p roastty search_overlay -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 732 design and found one real test-plan gap: the
design required `search_selection` to use the same plain unwrap/trim behavior as
`copy_to_clipboard:plain`, but the original test plan only covered a generic
selected-text needle. The test plan now explicitly requires a selection whose
raw formatted text would differ without trimming trailing spaces.

The review also found one workflow blocker: the design-review result had not yet
been recorded in the experiment frontmatter, this section, or the README tuple.
This section and the `[review.design]` frontmatter now record the review
outcome, and the README tuple is `Codex/Codex/-`.

With that test-plan fix, the review found no remaining technical design
blockers. It approved the parser scope, borrowed C string lifetime plan, false
paths, reuse of `ROASTTY_ACTION_START_SEARCH`, and preservation of parameterless
`start_search`.
