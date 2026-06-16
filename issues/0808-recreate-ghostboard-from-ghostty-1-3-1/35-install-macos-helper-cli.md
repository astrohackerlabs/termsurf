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

## Result

**Result:** Fail

Experiment 35 tried the approved `build.zig` wiring: install the already-created
main executable in the `.none` runtime branch when `config.emit_exe` is true.
That change correctly caused the build graph to attempt `install termsurf` and
`compile exe termsurf`, but the helper CLI did not compile.

The failure is in the existing `.none` helper path, not in the artifact name.
Zig 0.15 no longer provides `std.io.getStdOut()`, and
`ghostboard/src/main_ghostty.zig` still uses it in the `.none` runtime usage
message path. Because fixing `src/main_ghostty.zig` was outside the approved
Experiment 35 file set, the `build.zig` implementation change was reverted and
the experiment is recorded as a failed attempt.

### Verification

- `zig fmt ghostboard/build.zig` succeeded.
- `logs/ghostboard-exp35-positive-cli-build-20260616.log` records the attempted
  positive build:
  - command:
    `cd ghostboard && rm -rf zig-out && zig build -Demit-macos-app=false -Demit-xcframework=false -Demit-docs=false`;
  - the build attempted `install termsurf`;
  - the build attempted `compile exe termsurf`;
  - the build failed with
    `src/main_ghostty.zig:75:30: error: root source file struct 'Io' has no member named 'getStdOut'`;
  - no `zig-out/bin/termsurf` executable was produced.
- The attempted `build.zig` change was reverted after the failure, so no product
  code remains changed by this experiment.
- `git diff --check` succeeded after the revert.

## Conclusion

The build graph wiring idea is still likely necessary, but Experiment 35 proved
that it is not sufficient by itself. Before the helper can be installed as
`zig-out/bin/termsurf`, the `.none` helper CLI path in `src/main_ghostty.zig`
must be updated for Zig 0.15's stdout API.

The next experiment should fix the helper CLI compile error and then repeat the
minimal install wiring with both positive and negative checks:

- `emit-exe=true` produces and runs `zig-out/bin/termsurf`;
- `emit-exe=false` does not produce `zig-out/bin/termsurf`;
- no `zig-out/bin/ghostty` is produced.

## Completion Review

A fresh-context adversarial reviewer returned **APPROVED** with no required,
optional, or nit findings.

The reviewer confirmed that the result is correctly marked **Fail**, the README
status matches, the verification log supports the `install termsurf` /
`compile exe termsurf` attempt and `std.io.getStdOut()` failure, product code
was reverted, only the two issue docs remain modified, the conclusion identifies
the next concrete experiment, and the result commit had not yet been made.
