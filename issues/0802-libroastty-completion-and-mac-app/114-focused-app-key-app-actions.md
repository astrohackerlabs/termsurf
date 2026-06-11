# Experiment 114: Phase G — focused app key app actions

## Description

Extend Roastty's embedded `roastty_app_key` path from Exp 113 to match
upstream's focused-app behavior for non-global app-scoped keybindings.

Upstream `ghostty_app_key` calls `App.keyEvent(.app, ...)`. That path already
handles `global:` bindings whether the app is focused or not. When the app is
focused, it also handles non-global keybindings only if every action is
app-scoped; surface-scoped actions are left for `Surface.keyEvent`. Roastty now
handles configured `global:` app-key dispatch, but still returns `false` for
focused non-global app actions such as `ctrl+x=quit`.

This experiment implements that focused, app-scoped non-global subset for
configured single-trigger bindings. It does not add multi-key sequences/chords,
key tables, native keymaps, keyboard-layout reload, default global bindings, or
the remaining upstream action catalog.

## Changes

- `roastty/src/lib.rs`
  - Update the stale `roastty_app_key` comment to describe the implemented
    app-key behavior and the remaining native-keymap/key-table gaps.
  - Extend the app-level key-dispatch helper so:
    - `global:` bindings keep the Exp 113 behavior and are processed regardless
      of `app.focused`;
    - focused, non-global bindings are processed only when the parsed action is
      `ParsedBindingAction::AppRuntimeAction`;
    - focused, non-global surface-scoped actions return `false`, leaving them
      for `roastty_surface_key`;
    - unfocused, non-global bindings return `false`.
  - Keep plain `all:` non-global bindings out of `roastty_app_key` unless their
    action is app-scoped and the app is focused. Surface-scoped `all:` fanout
    remains surface-key-path behavior.
  - Keep `roastty_surface_key` behavior unchanged.
- Tests in `roastty/src/lib.rs`
  - A focused app dispatches a non-global app-scoped binding once to the app
    target and returns `true`.
  - An unfocused app does not dispatch the same non-global app-scoped binding.
  - A focused app returns `false` for a non-global surface-scoped binding and
    records no action.
  - Focus updates through `roastty_app_set_focus` affect `roastty_app_key`
    immediately.
  - Existing Exp 113 global dispatch behavior still works while unfocused.

## Verification

- Add unit coverage for the helper behavior above.
- Run:
  - `cargo test -p roastty app_key`
  - `cargo test -p roastty app_set_focus`
  - `cargo test -p roastty surface_key_configured_runtime_and_app_actions_dispatch`
  - `cargo test -p roastty --test abi_harness`
  - `cargo test -p roastty -- --test-threads=1`
  - if the known foreground-PID or mouse-reporting races fail, rerun the failing
    test in isolation, then rerun `cargo test -p roastty -- --test-threads=1`
  - `cargo fmt`
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/114-focused-app-key-app-actions.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Design Review

Codex-native adversarial review ran in a fresh-context subagent
(`multi_agent_v1.spawn_agent`, agent `019eb708-5593-70c2-a3c4-b9fb3ee03eff`).

Verdict: **Approved.** The reviewer reported no findings.

The reviewer confirmed that the README links Experiment 114 as Designed, the
experiment has the required design sections, the scope is narrow, and the
planned focus/global/app-scope behavior matches upstream `App.keyEvent` for the
currently implemented single-action binding surface. The reviewer independently
verified the docs with `git diff --check` and Prettier.
