# Experiment 133: Phase G — XCTest host lifecycle

## Description

Make the copied Roastty macOS app's hosted unit-test gate deterministic after
Experiment 132 fixed the old config/menu assertion cluster. The remaining
failure mode is not a known Swift assertion: focused `RoasttyTests/ConfigTests`
hangs before an individual test frame, with the host process sampled in XCTest
IDE-session preparation.

This experiment should isolate and fix the CLI test-host lifecycle/session setup
path. It must not reopen the config/menu semantics solved in Experiment 132, and
it must not broaden into UI-test permissions, screenshots, or visual automation.

## Changes

- Inspect the current Xcode test setup:
  - `roastty/macos/Roastty.xcodeproj/xcshareddata/xcschemes/Roastty.xcscheme`
  - `roastty/macos/Roastty.xctestplan`
  - `roastty/macos/Roastty.xcodeproj/project.pbxproj`
  - `roastty/macos/build.nu`
- Add a deterministic non-UI unit-test runner path for `RoasttyTests`. Prefer a
  mechanical project/scheme/test-runner change over copied app logic changes,
  for example:
  - an explicit unit-test-only shared scheme or test plan that includes
    `RoasttyTests` but not `RoasttyUITests`;
  - CLI runner flags that remove avoidable IDE/session ambiguity, such as an
    explicit macOS destination, disabled parallel/concurrent testing for hosted
    unit tests, bounded result-bundle paths under `logs/`, and xcodebuild test
    timeouts where supported;
  - a `build.nu` option or default test-path adjustment that keeps UI tests
    opt-in via `--ui-tests`.
- Preserve the copied app's source behavior. App source edits are out of scope
  unless the investigation proves a test-only lifecycle hook is required; any
  such hook must be compile-time/test-only, minimal, and documented in the
  result.
- Keep generated or diagnostic artifacts out of the repo. If `.xcresult`,
  samples, spindumps, or xcodebuild logs are needed, write them under
  `logs/issue-0802/exp-133/` or `/tmp` and reference their paths in the result.
- Update this experiment's result, Issue 802 operating notes, and the Issue 802
  roadmap/checklist after verification.

## Verification

Pass criteria:

- `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/133-xctest-host-lifecycle.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `cargo fmt --check`
- `cargo build -p roastty`
- `cargo test -p roastty config_get_ -- --test-threads=1`
- `cargo test -p roastty config_trigger_ -- --test-threads=1`
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/ConfigTests`
- `cd roastty && macos/build.nu --action test --only-testing RoasttyTests/MenuShortcutManagerTests`
- `cd roastty && macos/build.nu --action test`
- `git diff --check`

The focused `ConfigTests` and `MenuShortcutManagerTests` commands must produce
finalized xcodebuild results instead of hanging. The full non-UI macOS unit-test
gate must either pass or fail with concrete post-Experiment-132 assertions that
identify the next libroastty/app gap. If a hang remains, the result must include
fresh process, sample/spindump, and xcodebuild-result evidence proving where the
new blocker is.

Every `xcodebuild`, spawned `Roastty.app`, and helper process started by this
experiment must be cleaned up by exact PID or exact build-output path, never by
broad process-name matching. The result must record the cleanup method and a
post-run process check showing no experiment-spawned processes remain.

## Design Review

**Reviewer:** Codex-native adversarial subagents (`multi_agent_v1.spawn_agent`,
fresh context, `Darwin` then `Banach`)

**Verdict:** Approved after fixes

The initial design review returned **Changes Required** with two workflow
findings:

- The result-step maintenance list mentioned operating notes but omitted the
  Issue 802 roadmap/checklist update required by the issue process.
- The verification section omitted an explicit scoped
  cleanup/no-dangling-process requirement for hang-prone `xcodebuild` and hosted
  app processes.

Both findings were fixed in the design. The final re-review approved the plan
with no remaining required findings.
