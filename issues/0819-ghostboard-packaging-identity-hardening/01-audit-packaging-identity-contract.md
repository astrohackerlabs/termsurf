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

## Result

**Result:** Pass

Experiment 1 completed the audit without changing app source, build scripts, or
packaging behavior. The result is a baseline contract plus a classified backlog
for the follow-up hardening experiments.

### Identity Contract

The intended contract for Issue 819 is:

| Surface                                                    | Intended identity                                                                                             | Status         |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | -------------- |
| macOS app display name                                     | TermSurf, unless a later product decision chooses a GUI-qualified name such as TermSurf Ghostboard            | Needs decision |
| macOS executable name                                      | `termsurf`, matching the current app bundle                                                                   | OK             |
| macOS bundle id                                            | `com.termsurf` for release and `com.termsurf.debug` for debug, unless multiple TermSurf GUI apps must coexist | Needs decision |
| Ghostboard source tree and upstream implementation symbols | May remain Ghostty-named where preserving upstream structure reduces merge risk and they are not user-visible | OK             |
| User-visible scripting/docs/settings strings               | Must not tell users they are running or configuring Ghostty                                                   | Needs fix      |
| Config location                                            | Must be documented and loaded consistently as a TermSurf/Ghostboard config path                               | Needs fix      |
| Debug Roamium resolution                                   | Must use explicit debug paths and avoid installed Roamium fallback                                            | OK             |
| Installed Roamium resolution                               | Must be defined separately from the debug contract                                                            | Needs fix      |
| Release/Homebrew packaging                                 | Must include the intended Ghostboard/TermSurf app and agree with install/uninstall docs                       | Needs fix      |

### Current Evidence

The built debug app bundle currently reports:

```text
CFBundleName: TermSurf
CFBundleDisplayName: TermSurf
CFBundleIdentifier: com.termsurf.debug
CFBundleExecutable: termsurf
```

The Xcode project agrees for the macOS app target:

- `ASSETCATALOG_COMPILER_APPICON_NAME = TermSurf`
- `EXECUTABLE_NAME = termsurf`
- `INFOPLIST_FILE = "Ghostty-Info.plist"`
- `INFOPLIST_KEY_CFBundleDisplayName = TermSurf`
- release/local bundle id `com.termsurf`
- debug bundle id `com.termsurf.debug`
- `PRODUCT_NAME = TermSurf`

The same project still contains inherited Ghostty identities outside the main
macOS app target:

- the project and app target are still named `Ghostty`;
- test bundle identifiers remain `com.mitchellh.GhosttyTests` and
  `com.mitchellh.GhosttyUITests`;
- the iOS target still uses display name `Ghostty`, bundle id
  `com.mitchellh.ghostty-ios`, and the `Ghostty` app icon;
- the Dock Tile plugin uses display name `TermSurf Dock Tile Plugin` and bundle
  id `com.termsurf-dock-tile`.

`ghostboard/macos/Ghostty-Info.plist` still exposes inherited metadata keys and
resources:

- `GhosttyBuild`
- `GhosttyCommit`
- `Ghostty.sdef`
- `Ghostty Surface Identifier`

The AppleScript dictionary is user-visible and still Ghostty-branded:

- dictionary title `Ghostty Scripting Dictionary`;
- suite name `Ghostty Suite`;
- descriptions such as `The Ghostty application` and `frontmost Ghostty window`;
- Cocoa class names such as `GhosttyScriptWindow`, `GhosttyScriptTab`, and
  `GhosttyScriptTerminal`.

The entitlements are inherited from Ghostty file names but contain capability
keys rather than product names. Current files are:

- `ghostboard/macos/Ghostty.entitlements`
- `ghostboard/macos/GhosttyDebug.entitlements`
- `ghostboard/macos/GhosttyReleaseLocal.entitlements`

They grant Apple Events, audio, camera, address book, calendars, location,
photos, and debug/local library-validation exceptions where applicable.

### Packaging Evidence

The current repo-level build/install/release scripts are still Wezboard/Roamium
oriented:

- `scripts/build.sh` lists components
  `wezboard, roamium, webtui, chromium, all`; there is no `ghostboard`
  component.
- `scripts/install.sh` lists components `wezboard, roamium, webtui, all`; it
  installs Roamium to `/usr/local/roamium` and Wezboard to
  `/Applications/TermSurf Wezboard.app`.
- `scripts/uninstall.sh` removes `/usr/local/roamium`, `/usr/local/bin/roamium`,
  `/usr/local/lib/roamium`, and `/Applications/TermSurf Wezboard.app`; it has no
  Ghostboard app path.
- `scripts/release.sh` packages `web`, `wezboard`, `roamium`, Chromium
  resources, and `TermSurf Wezboard.app`; it has no Ghostboard/TermSurf.app
  release path.
- `homebrew/Casks/termsurf.rb` installs `TermSurf Wezboard.app`, `web`,
  `wezboard`, and a `roamium` artifact at `/opt/homebrew/opt/termsurf-roamium`;
  it does not install Ghostboard.

This means Ghostboard currently has a debug app identity but no normal
repo-level install/release/Homebrew path.

### Config Evidence

Config-path evidence is inconsistent:

- `docs/xdg.md` states TermSurf should use `~/.config/termsurf/`.
- `ghostboard/src/cli/edit_config.zig` documents
  `$XDG_CONFIG_HOME/termsurf/config`.
- `ghostboard/src/config/Config.zig` documents the main configuration file at
  `$XDG_CONFIG_HOME/termsurf/config` and themes under
  `$XDG_CONFIG_HOME/termsurf/themes`.
- `ghostboard/src/cli/list_themes.zig` documents TermSurf theme paths.
- `ghostboard/macos/Sources/Features/Settings/SettingsView.swift` still tells
  users to edit `$HOME/.config/ghostty/config.ghostty` and restart Ghostty.
- generated manpage fragments under `ghostboard/src/build/mdgen/` still document
  Ghostty paths such as `$XDG_CONFIG_HOME/ghostty/config.ghostty` and
  `$HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty`.
- `ghostboard/src/config/file_load.zig` still calls
  `internal_os.macos.appSupportDir(alloc, "config.ghostty")`, so macOS fallback
  behavior needs a focused runtime check before changing docs.

### Debug-vs-Installed Evidence

The debug browser boundary is explicit and currently correct:

- `docs/ghostboard-launch-discovery.md` says named/default `roamium` resolves
  through `TERMSURF_ROAMIUM_PATH`.
- `ghostboard/src/apprt/termsurf.zig` defines `default_browser = "roamium"` and
  `roamium_path_env = "TERMSURF_ROAMIUM_PATH"`.
- the resolver accepts absolute browser paths directly;
- named `roamium` fails clearly if `TERMSURF_ROAMIUM_PATH` is missing, empty, or
  relative;
- `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch` asserts the
  debug path is used;
- `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch` also
  asserts no spawned path starts with `/usr/local/roamium`,
  `/usr/local/bin/roamium`, or `/opt/homebrew/opt/termsurf-roamium`.

Installed-app behavior is not yet defined. The current install script uses
`/usr/local/roamium`, while the Homebrew cask uses
`/opt/homebrew/opt/termsurf-roamium`.

### Classification

| Finding                                                                                                                                   | Classification | Next experiment                                                                                                                 |
| ----------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Main debug app bundle is named TermSurf with executable `termsurf` and debug bundle id `com.termsurf.debug`.                              | OK             | Keep as baseline unless the app-name decision changes.                                                                          |
| Release bundle id `com.termsurf` may collide with other future TermSurf GUI apps if Wezboard and Ghostboard are both distributed as apps. | Needs decision | Decide whether Ghostboard should ship as `TermSurf`, `TermSurf Ghostboard`, or another GUI-qualified app identity.              |
| AppleScript dictionary remains user-visible Ghostty.                                                                                      | Needs fix      | Rename user-visible scripting dictionary title/suite/descriptions while preserving or deliberately migrating Cocoa class names. |
| Settings UI still points to Ghostty config and says restart Ghostty.                                                                      | Needs fix      | Update Settings UI after the config path is proven.                                                                             |
| Config docs/source are split between TermSurf paths and inherited Ghostty macOS fallback/manpage paths.                                   | Needs fix      | Run a focused config-location runtime proof, then align docs and user-visible strings with the real supported path.             |
| Repo-level build/install/release scripts have no Ghostboard component.                                                                    | Needs fix      | Add Ghostboard build/install/release packaging only after app identity is decided.                                              |
| Homebrew cask installs Wezboard but not Ghostboard.                                                                                       | Needs fix      | Extend or split cask packaging after app identity and release artifact paths are decided.                                       |
| Debug named Roamium resolution is explicit and avoids installed fallback.                                                                 | OK             | Preserve existing Issue 814 regression scenarios.                                                                               |
| Installed Roamium discovery has conflicting paths between install script and Homebrew cask.                                               | Needs fix      | Define installed Roamium location and app environment behavior, then add a regression check.                                    |
| Entitlement file names remain Ghostty but entitlement contents are capability-only.                                                       | OK             | Leave file names unless a later rename is needed for packaging clarity.                                                         |
| `GhosttyBuild` and `GhosttyCommit` plist keys remain inherited implementation metadata.                                                   | OK             | Leave unless a future public diagnostics path exposes these keys to users.                                                      |
| `Ghostty.sdef` and `Ghostty Surface Identifier` are bundled user-visible or automation-facing plist resources.                            | Needs fix      | Rename the user-visible scripting dictionary/resource strings in the AppleScript identity experiment.                           |
| Xcode project, target, tests, and many source symbols remain Ghostty-named.                                                               | OK             | Treat as inherited implementation identity unless user-visible leakage or packaging requires a targeted rename.                 |
| iOS target remains Ghostty-branded.                                                                                                       | Needs decision | Defer unless Issue 819 includes iOS packaging; the current issue is macOS distribution focused.                                 |

Verification completed:

1. Inspected the built debug app bundle with `PlistBuddy`.
2. Inspected Xcode project, plist, scripting dictionary, and entitlement
   metadata with `rg` and `plutil`.
3. Inspected repo-level build/install/uninstall/release scripts and Homebrew
   cask paths.
4. Inspected Issue 814 docs, Ghostboard resolver code, and harness assertions
   for the debug-vs-installed boundary.
5. Inspected config-path references across docs, Ghostboard source, and macOS
   settings UI.

## Conclusion

Issue 819 should proceed with a decision/fix split:

1. Decide the public Ghostboard app identity before changing release packaging.
   The current implementation says `TermSurf`, but the issue title and future
   multi-GUI product shape may require a GUI-qualified app name.
2. Prove the real config loading location at runtime, then align Settings UI,
   docs, and generated documentation around that path.
3. Define installed Roamium discovery and package Ghostboard only after the app
   identity and config contract are explicit.

The next experiment should resolve the public macOS app identity contract
without changing behavior, because release packaging, bundle ids, Homebrew
artifacts, and installed browser discovery all depend on that decision.

## Completion Review

Fresh-context adversarial completion review by Codex subagent `Singer the 2nd`:

- **Initial verdict:** Changes required.
- **Required finding:** The result recorded inherited `Ghostty-Info.plist`
  metadata/resources, but the classification table did not classify
  `GhosttyBuild`, `GhosttyCommit`, `Ghostty.sdef`, or
  `Ghostty Surface Identifier`. Fixed by adding classification rows for
  inherited diagnostic metadata and user-visible/automation-facing plist
  resources.
- **Re-review verdict:** Approved. The reviewer confirmed the inherited plist
  metadata/resources are now classified with next-experiment recommendations and
  no new Required finding was introduced.
