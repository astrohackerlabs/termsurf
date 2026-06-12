# Experiment 138: Phase G — app keymap state

## Description

Wire the Rust `KeymapDarwin` foundation from Experiment 137 into Roastty's app
state so the embedded app object owns and reloads the current platform keymap,
matching upstream `apprt/embedded.zig`'s `App.keymap` shape.

Experiment 137 intentionally stopped at a standalone `input::keymap_darwin`
module. Upstream's embedded app owns an `input.Keymap`, initializes it during
app init, deinitializes it during terminate, reloads it from
`reloadKeymap`/`ghostty_app_keyboard_changed`, and uses its source ID for
keyboard-layout detection. Roastty currently stores only the lightweight
`keyboard_layout: input_keyboard::Layout` value and calls `Layout::current()`
directly during app creation and keyboard-change notifications.

This experiment makes the app ownership/reload boundary faithful without
changing copied Swift keyDown behavior or changing `roastty_surface_key` text
semantics yet. It is the prerequisite state wiring for a later experiment that
can safely apply keymap translation to by-value surface/app key events.

## Changes

- `roastty/src/input/keymap_darwin.rs`
  - Expose enough safe crate-internal API for app ownership: initialization,
    reload, source ID, and an explicit unsupported/fallback state on non-macOS.
  - Keep `translate` unchanged from Experiment 137.
  - Add test support that lets unit tests avoid hitting Carbon/TIS for every app
    construction while still compiling and smoke-testing the production path.
- `roastty/src/lib.rs`
  - Import `input::keymap_darwin`.
  - Add keymap state to `App`, matching upstream's embedded app ownership
    boundary. On macOS production builds, initialize it from
    `KeymapDarwin::new()` when possible; on unsupported/unavailable layouts,
    keep a documented fallback that preserves app creation.
  - Derive `App.keyboard_layout` from the app-owned keymap source ID when the
    keymap is available, falling back to the existing `Layout::current()` path.
  - Change `roastty_app_keyboard_changed(app)` to reload the app-owned keymap
    first and then refresh `keyboard_layout` from that keymap when possible.
  - Preserve explicit `macos-option-as-alt` precedence and
    `roastty_surface_key_translation_mods` behavior.
  - Do not change `roastty_surface_key`, `roastty_app_key`,
    `input_key_to_event`, copied Swift `keyDown`, or the public
    `roastty_input_key_s` ABI in this experiment.
- Tests
  - Add deterministic Rust tests proving:
    - app creation initializes keyboard-layout fallback from app-owned keymap
      source metadata when a test keymap is provided;
    - `roastty_app_keyboard_changed` reloads the app-owned keymap and updates
      the layout used by `roastty_surface_key_translation_mods`;
    - explicit `macos-option-as-alt` surface config still overrides the
      refreshed app keymap layout;
    - unsupported/unavailable keymap initialization preserves existing
      `Layout::current()` fallback behavior;
    - production `KeymapDarwin::new()` / `reload()` still has a targeted macOS
      smoke test, but ordinary app unit tests do not all call Carbon/TIS.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After implementation, narrow the Phase G native-key note to distinguish
    app-owned keymap state/reload from still-unwired surface/app key-event text
    enrichment.

Out of scope:

- Replacing Swift/AppKit text with Rust-side `KeymapDarwin` output.
- Changing the public embedded key ABI or adding new public C functions.
- Changing `roastty_surface_key`, `roastty_app_key`, or keybinding dispatch
  semantics.
- Full dead-key/preedit runtime behavior and hosted UI automation for it.
- Permission-dependent live global shortcut installation.

## Verification

- Run formatting:
  - `cargo fmt`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/138-app-keymap-state.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Run targeted tests:
  - `cargo test -p roastty keymap_darwin`
  - `cargo test -p roastty keyboard_layout`
  - `cargo test -p roastty key_translation_mods`
  - `cargo test -p roastty app_keymap`
- Run build coverage:
  - `cargo build -p roastty`
- Run full Roastty tests:
  - `cargo test -p roastty -- --test-threads=1`
- Run checks:
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/138-app-keymap-state.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = Roastty's embedded app state owns an upstream-shaped keymap,
keyboard-change notifications reload that keymap and refresh the detected layout
used by option-as-alt fallback, tests avoid broad Carbon/TIS dependency, and all
existing key input behavior remains unchanged.

**Partial** = app ownership is wired but production keymap initialization has to
remain disabled or weaker than upstream because making it reliable requires the
later key-event text enrichment work.

**Fail** = app-owned keymap state cannot be introduced without changing copied
Swift keyDown behavior or the public input ABI first.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Lagrange`, fresh
context.

**Verdict:** Approved.

**Findings:** None.

The reviewer confirmed the README links Experiment 138 as `Designed`, the
experiment has the required sections, the scope is narrow and faithful to
upstream embedded app keymap ownership/reload state, the design does not
overclaim Swift keyDown text replacement or public ABI changes, and
`git diff --check` plus the Prettier check passed.

## Result

**Result:** Pass

Implemented app-owned keymap state in `roastty/src/lib.rs`. `App` now owns an
`AppKeymap`, initializes it at `roastty_app_new`, derives the stored
`keyboard_layout` from the keymap source ID when available, and reloads the
keymap before refreshing the layout in `roastty_app_keyboard_changed`.
Unsupported or unavailable keymap initialization falls back to the existing
`Layout::current()` path, so app creation remains non-fatal.

The experiment keeps the public input ABI and `roastty_surface_key` /
`roastty_app_key` semantics unchanged. Deterministic Rust tests cover app-new
layout detection from app-owned keymap source metadata, fallback to the current
layout when no keymap is available, keyboard-change reload behavior, and
explicit `macos-option-as-alt` config precedence after reload.

Verification run:

- `cargo fmt -- roastty/src/lib.rs` — pass
- `cargo test -p roastty app_keymap` — pass, 4 passed
- `cargo test -p roastty key_translation_mods` — pass, 10 passed
- `cargo test -p roastty keyboard_layout` — pass, 4 passed
- `cargo test -p roastty keymap_darwin` — pass, 5 passed
- `cargo build -p roastty` — pass
- `cargo test -p roastty -- --test-threads=1` — failed only the known
  foreground-PID race:
  `tests::surface_foreground_pid_reports_worker_foreground_pid_after_start`
  reported `left: 61406`, `right: 61401`; summary was 4759 passed, 1 failed.
- `cargo test -p roastty surface_foreground_pid_reports_worker_foreground_pid_after_start -- --test-threads=1`
  — pass, 1 passed
- `cargo fmt --check` — pass
- `git diff --check` — pass

## Conclusion

Roastty now has the upstream-shaped app keymap ownership/reload boundary needed
before Rust-side native text translation can be applied to by-value key events.
The remaining native-key work is to use the app-owned keymap in
`roastty_surface_key` / `roastty_app_key` text enrichment, validate hosted
dead-key/preedit behavior, and finish permission-dependent live global shortcut
registration.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Arendt`, fresh context.

**Verdict:** Approved.

**Findings:** None.

The reviewer checked the working-tree diff against the approved scope and
workflow contract, confirmed the result commit had not been made before review,
and approved the app-owned keymap state/reload implementation plus result
documentation with no required findings.
