# Experiment 1: Audit Packaging Identity Contract

## Description

Issue 819 should not start by renaming files or changing release paths blindly.
Ghostboard is a fresh Ghostty fork with intentional upstream inheritance, while
TermSurf distribution needs deliberate public names, config paths, bundle
metadata, installed browser locations, and debug-vs-installed behavior.

This experiment will establish the current packaging/identity baseline and write
down a concrete contract for what must be Ghostboard or TermSurf, what may
remain Ghostty for upstream compatibility, and what is currently inconsistent.
It is an audit-first experiment. No app source changes are planned.

## Changes

Planned issue-document changes:

- Add a result section to this experiment that records:
  - intended naming policy for user-visible app names, binary names, bundle ids,
    config paths, release artifacts, and inherited upstream source names;
  - current evidence from the built debug app bundle and source metadata;
  - current evidence from release/install/Homebrew scripts;
  - current evidence from config loading and docs;
  - a classification table of findings: `OK`, `Needs fix`, or `Needs decision`.
- Update the Issue 819 README experiment status after verification.

Planned inspection targets:

- App bundle and macOS metadata:
  - `ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist`
  - `ghostboard/macos/Ghostty-Info.plist`
  - `ghostboard/macos/Ghostty.xcodeproj/project.pbxproj`
  - `ghostboard/macos/Ghostty.sdef`
  - `ghostboard/macos/Ghostty*.entitlements`
  - `ghostboard/macos/Sources/Helpers/AppInfo.swift`
- Build and packaging scripts:
  - `ghostboard/macos/build.nu`
  - `ghostboard/build.zig`
  - `ghostboard/src/build/GhosttyXcodebuild.zig`
  - `ghostboard/src/apprt/termsurf.zig`
  - `scripts/build.sh`
  - `scripts/install.sh`
  - `scripts/uninstall.sh`
  - `scripts/release.sh`
  - `scripts/ghostboard-geometry-matrix.sh`
  - `homebrew/Casks/termsurf.rb`
- Docs and prior contracts:
  - `docs/ghostboard-launch-discovery.md`
  - `docs/xdg.md`
  - `issues/0814-ghostboard-launch-discovery-workflow/README.md`
  - `issues/0814-ghostboard-launch-discovery-workflow/01-resolve-named-roamium-debug-launch.md`
  - `issues/0814-ghostboard-launch-discovery-workflow/02-document-launch-discovery-contract.md`

Planned source changes:

- None.
- If the audit exposes a straightforward app/source bug, record it as
  `Needs fix` and design the fix in a later experiment.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0819-ghostboard-packaging-identity-hardening/README.md issues/0819-ghostboard-packaging-identity-hardening/01-audit-packaging-identity-contract.md`.

Static checks:

1. `git diff --check`.

Audit commands:

1. Inspect the built debug app bundle:

   ```bash
   /usr/libexec/PlistBuddy -c 'Print :CFBundleName' ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist
   /usr/libexec/PlistBuddy -c 'Print :CFBundleDisplayName' ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist || true
   /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist
   /usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist
   ```

2. Inspect source metadata and build settings:

   ```bash
   rg -n 'CFBundle(Name|DisplayName|Identifier|Executable)|PRODUCT_(NAME|BUNDLE_IDENTIFIER)|INFOPLIST_FILE|EXECUTABLE_NAME|application-groups|app-sandbox|TermSurf|Ghostty|Ghostboard' ghostboard/macos/Ghostty-Info.plist ghostboard/macos/Ghostty.xcodeproj/project.pbxproj ghostboard/macos/Sources/Helpers/AppInfo.swift ghostboard/macos/Ghostty.sdef ghostboard/macos/Ghostty*.entitlements
   ```

3. Inspect packaging/install scripts:

   ```bash
   rg -n 'Ghostboard|Ghostty|TermSurf.app|roamium|termsurf-roamium|/usr/local/roamium|/opt/homebrew|cask|artifact|install|release' scripts/build.sh scripts/install.sh scripts/uninstall.sh scripts/release.sh homebrew/Casks/termsurf.rb
   ```

4. Inspect debug-vs-installed browser resolution:

   ```bash
   rg -n 'TERMSURF_ROAMIUM_PATH|named browser|roamium|/usr/local/roamium|/usr/local/bin/roamium|/opt/homebrew/opt/termsurf-roamium|installed|stale' ghostboard/src/apprt/termsurf.zig scripts/ghostboard-geometry-matrix.sh docs/ghostboard-launch-discovery.md issues/0814-ghostboard-launch-discovery-workflow
   ```

5. Inspect config-path references:

   ```bash
   rg -n 'config|XDG|ghostty|termsurf|roastty|GHOSTTY_CONFIG|GHOSTTY_CONFIG_PATH' docs/xdg.md docs/ghostboard-launch-discovery.md ghostboard/src ghostboard/macos/Sources
   ```

Pass criteria:

- The experiment records the current debug app bundle identity from the built
  `TermSurf.app`.
- The experiment records source metadata for app name, bundle id, executable
  name, AppleScript dictionary naming, and entitlements.
- The experiment records install/release/Homebrew packaging paths for app,
  webtui, Roamium, and any Ghostboard binary if present.
- The experiment records current config-path behavior and documentation.
- The experiment classifies the current debug-vs-installed browser resolution
  boundary using both Issue 814 docs and the actual Ghostboard resolver/harness
  implementation.
- The experiment explicitly defines which identities are intended to be
  user-visible TermSurf/Ghostboard names and which may remain upstream Ghostty
  implementation names.
- Every finding is classified as `OK`, `Needs fix`, or `Needs decision`, with a
  concrete next experiment recommendation for each non-OK item.
- No source changes are made.

Partial criteria:

- The audit maps app/source/script identity but cannot inspect the built app
  bundle because it is missing or stale.
- The audit maps debug identity but release packaging cannot be classified
  without a release artifact.
- The audit finds conflicting intended names that require user/product decision
  before implementation.

Fail criteria:

- The experiment changes app/source behavior before recording the identity
  contract.
- The audit cannot classify app bundle identity, config paths, release scripts,
  and debug-vs-installed boundaries.
- The result closes Issue 819 without follow-up experiments despite unresolved
  `Needs fix` or `Needs decision` findings.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Darwin the 2nd`:

- **Initial verdict:** Changes required.
- **Required finding:** The audit plan did not inspect the code and harness that
  enforce the debug-vs-installed boundary. Fixed by adding
  `ghostboard/src/apprt/termsurf.zig`, `scripts/ghostboard-geometry-matrix.sh`,
  and the Issue 814 README/experiment docs to the inspection targets and audit
  command, and by requiring the result to classify current debug-vs-installed
  browser resolution behavior.
- **Optional finding:** Entitlement files were listed as targets but omitted
  from the concrete metadata command. Fixed by including
  `ghostboard/macos/Ghostty*.entitlements` and entitlement keys in the metadata
  command.
- **Re-review verdict:** Approved. The reviewer confirmed the debug-vs-installed
  resolver/harness/docs targets are now included, the result must classify the
  boundary using docs and implementation, entitlement files are included in the
  concrete command, and no new Required issue was introduced.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 819 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.
