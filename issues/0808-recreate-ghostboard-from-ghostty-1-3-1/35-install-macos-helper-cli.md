# Experiment 35: Install macOS helper CLI

## Description

Experiment 34 renamed the executable target to `termsurf`, but it did not prove
the Issue 808 CLI-command requirement because the macOS default runtime is
`.none`, and the `.none` branch in `ghostboard/build.zig` does not currently
connect the main executable install step to the build graph.

This experiment will make the CLI command real for the macOS `.none` runtime by
installing the existing helper CLI executable when `emit-exe` is true. The
helper behavior already exists in `ghostboard/src/main_ghostty.zig`: when
`build_config.app_runtime == .none`, the program prints
`Usage: termsurf +<action> [flags]` and tells users to launch `TermSurf.app` for
the graphical terminal.

The important distinction from the earlier rejected approach is:

- do not make `emit-exe` imply new app-bundle install behavior;
- do not change `emit-exe` option semantics globally;
- do connect the already-created `GhosttyExe` install step in the `.none`
  runtime so the helper command appears at `zig-out/bin/termsurf`;
- keep the `.app` bundle build path separate.

## Changes

Expected files:

- `ghostboard/build.zig`
  - in the `config.app_runtime == .none` branch, install the main executable
    when `config.emit_exe` is true, so macOS gets the helper CLI at
    `zig-out/bin/termsurf`;
  - keep existing non-Darwin libghostty install behavior intact;
  - do not move or broaden `resources.install()`, `i18n.install()`, or
    `macos_app.install()` behavior.
- `issues/0808-recreate-ghostboard-from-ghostty-1-3-1/35-install-macos-helper-cli.md`
  - record the experiment result.
- `issues/0808-recreate-ghostboard-from-ghostty-1-3-1/README.md`
  - add Experiment 35 to the experiment index.

No changes are planned to:

- `webtui/`;
- `roamium/`;
- `chromium/`;
- `proto/termsurf.proto`;
- TermSurf protocol handling;
- app bundle identity, icon, menu, or config paths.

## Verification

Pass criteria:

- `zig fmt ghostboard/build.zig` succeeds.
- `prettier --write --prose-wrap always --print-width 80` succeeds on the
  changed Markdown files.
- `git diff --check` is clean.
- Static source checks show `ghostboard/build.zig` installs `exe` in the `.none`
  runtime branch only behind `config.emit_exe`.
- `cd ghostboard && rm -rf zig-out && zig build -Demit-macos-app=false -Demit-xcframework=false -Demit-docs=false`
  succeeds.
- That build produces executable `ghostboard/zig-out/bin/termsurf`.
- That build does not produce `ghostboard/zig-out/bin/ghostty`.
- Running `ghostboard/zig-out/bin/termsurf` exits successfully and prints the
  helper CLI usage text containing `Usage: termsurf +<action> [flags]`.
- `cd ghostboard && rm -rf zig-out && zig build -Demit-exe=false -Demit-macos-app=false -Demit-xcframework=false -Demit-docs=false`
  succeeds without producing `ghostboard/zig-out/bin/termsurf`, proving the
  helper remains gated by `emit-exe`.
- `git status --short --untracked-files=all` contains only the declared files.

Fail criteria:

- `emit-exe=false` still installs `zig-out/bin/termsurf`.
- The produced helper command is named `ghostty`.
- The change causes the macOS app bundle install path to regress.
- The experiment changes app/protocol/runtime behavior outside the helper CLI
  build wiring.

## Design Review

A fresh-context adversarial reviewer returned **APPROVED** with no findings. The
reviewer confirmed that the design is narrowly scoped to the Experiment 34 CLI
blocker, that `build.zig` currently creates the executable without installing it
in the `.none` runtime, that the `.none` helper behavior already exists in
`src/main_ghostty.zig`, and that the verification covers positive and negative
CLI install behavior.
