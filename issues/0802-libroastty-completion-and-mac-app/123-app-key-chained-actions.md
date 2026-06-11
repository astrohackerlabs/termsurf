# Experiment 123: Phase G — app-key chained actions

## Description

Port upstream app-level handling for configured direct chained keybinding
leaves. Experiment 122 made chained leaves work on the surface key path, but
`roastty_app_key` still rejects any configured binding with more than one
action. Upstream `App.keyEvent` does handle `leaf_chained` for direct app-key
events: global leaves perform each chained action through the app-wide action
path, while focused non-global leaves only run when every action is app-scoped.

This experiment closes that direct app-key gap without changing sequence or key
table behavior. Upstream app-level key events intentionally do not support
multi-key sequences, and global sequences are invalid at parse time, so
sequence/table dispatch stays out of scope.

## Changes

- `roastty/src/lib.rs`
  - Replace the early `binding.is_chained()` rejection in `App::key` with
    ordered chained-action dispatch.
  - Add an app-key action-scope helper that follows upstream
    `Binding.Action.scope()` for the currently parsed action set, rather than
    treating Roastty's current `AppRuntimeAction` / `RuntimeAction` enum split
    as the scope boundary. This must classify `ignore`, `new_window`, `undo`,
    and `redo` as app-scoped for app-key handling, matching upstream.
  - Parse every action in the configured binding before performing anything. If
    any action is invalid for the app path, return `false` and leave callbacks
    untouched.
  - For `global:` configured bindings:
    - perform app-scoped actions once against the app target;
    - perform surface-scoped actions on every live surface, in action order;
    - return `true` after dispatch, matching upstream app-wide chained action
      semantics.
  - For focused non-global configured bindings:
    - require every action in the chain to be app-scoped;
    - dispatch the app actions in order once;
    - return `true`.
  - For unfocused non-global bindings, continue returning `false`.
  - Keep `all:` handling unchanged from Experiments 113–114: it is not a global
    shortcut when unfocused, and focused app-scoped actions may dispatch through
    the app path.
  - Treat `ignore` as an app-scoped no-op that still lets a matched app-key
    binding return `true`.
  - Keep sequence leaders, active key tables, key-table actions, and
    `end_key_sequence` out of `roastty_app_key`.
- Tests in `roastty/src/lib.rs`
  - A focused non-global chain of app-scoped actions dispatches each visible app
    action once in order, including upstream app-scoped actions that Roastty
    currently parses as runtime actions (`new_window`, `undo`, `redo`).
  - A focused non-global chain containing any surface-scoped action returns
    `false` and dispatches nothing.
  - An unfocused non-global chain returns `false`.
  - A `global:` chain with app-scoped actions dispatches each app action once in
    order while unfocused.
  - A `global:` chain mixing app-scoped and surface-scoped actions dispatches in
    order: app actions once, surface actions once per live surface.
  - Focused and global chains containing `ignore` return `true`; `ignore` does
    not emit a runtime callback and does not stop later actions.
  - Detached surfaces are skipped for global surface-scoped chain actions.
  - Existing sequence/table/key-table-action/`end_key_sequence` app-key
    exclusions continue to return `false`.

## Verification

- Run:
  - `cargo test -p roastty app_key`
  - `cargo test -p roastty chain`
  - `cargo test -p roastty surface_key`
  - `cargo test -p roastty parse_config_keybind`
  - `cargo test -p roastty --test abi_harness`
  - `cargo test -p roastty -- --test-threads=1`
  - `cargo fmt`
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/123-app-key-chained-actions.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Design Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb7e3-4147-7af1-8121-c4a9972ce73b`)

**Initial verdict:** Changes required

**Required finding 1:** The initial design kept `ignore` out of
`roastty_app_key`, but upstream treats `ignore` as app-scoped and handles it as
a no-op in `App.performAction`.

**Fix 1:** Updated the plan so `ignore` is app-scoped for app-key handling and
added focused/global chain tests containing `ignore`.

**Required finding 2:** The initial design did not require upstream app-scope
fidelity for actions Roastty currently parses as runtime actions. Upstream marks
`new_window`, `undo`, and `redo` as app-scoped, while the draft only referred
generically to app-scoped actions.

**Fix 2:** Added an explicit app-key action-scope helper requirement based on
upstream `Binding.Action.scope()` and required focused non-global chain tests
covering `new_window`, `undo`, and `redo`.

**Final verdict:** Approved

**Final findings:** None.
