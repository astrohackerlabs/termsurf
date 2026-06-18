# Experiment 4: Clean User-Visible Ghostty Identity

## Description

Experiment 3 renamed the macOS app bundle to `TermSurf Ghostboard.app`, but
Experiment 1 found user-visible inherited Ghostty strings still leaking through
Settings UI and AppleScript-facing resources. This experiment will clean the
user-visible product identity surfaces that can be updated without changing the
actual config loading path, release packaging, Homebrew packaging, or broad
implementation symbols.

The key rule is to rename what users see, not the upstream implementation
structure. Swift types, Objective-C runtime class names, source directories, and
Xcode target names may remain Ghostty unless they are directly exposed to users
or required by the visible resource rename.

## Changes

Planned source changes:

- `ghostboard/macos/Sources/Features/Settings/SettingsView.swift`
  - Replace visible Ghostty wording with TermSurf Ghostboard wording.
  - Do not claim a final config path until the config-path runtime proof
    experiment establishes it.
- `ghostboard/macos/Ghostty.sdef`
  - Rename AppleScript dictionary title, suite name, and user-facing
    descriptions from Ghostty to TermSurf Ghostboard.
  - Preserve Cocoa class names such as `GhosttyScriptWindow` unless changing
    them is required for functionality; class-name migration is not needed for
    visible identity.
- `ghostboard/macos/Ghostty-Info.plist`
  - Rename user-facing metadata strings such as `Ghostty Surface Identifier`
    where they are visible through AppleScript or system metadata.
  - Leave implementation diagnostic keys such as `GhosttyBuild` and
    `GhosttyCommit` untouched unless they are visible in normal UI.
- Generated docs/manpage template files under `ghostboard/src/build/mdgen/` are
  inspection-only in this experiment. Product wording there is documented as
  remaining user-visible debt, but actual generated-doc/config-path wording
  changes are deferred until the config-path runtime proof defines the final
  documented path.

Planned issue-document changes:

- Add `## Result` and `## Conclusion` after verification.
- Update the Issue 819 README experiment status after verification.

Explicitly out of scope:

- Changing actual config loading behavior or config search paths.
- Repo-level install/release/Homebrew packaging.
- Broad source, target, module, Objective-C class, generated protobuf, or
  implementation-symbol renames.
- iOS target identity.
- Any behavior unrelated to user-visible product wording.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0819-ghostboard-packaging-identity-hardening/README.md issues/0819-ghostboard-packaging-identity-hardening/04-clean-user-visible-ghostty-identity.md`.

Static checks:

1. `git diff --check`.

Build and metadata checks:

1. Build the debug Ghostboard macOS app:

   ```bash
   cd ghostboard/macos && ./build.nu --configuration Debug --action build
   ```

2. Inspect bundled AppleScript metadata and require every remaining `Ghostty`
   occurrence in the bundled scripting definition to be either gone or
   explicitly allowlisted as implementation-only:

   ```bash
   rg -n 'Ghostty' ghostboard/macos/build/Debug/TermSurf\\ Ghostboard.app/Contents/Resources/Ghostty.sdef
   rg -n 'TermSurf Ghostboard' ghostboard/macos/build/Debug/TermSurf\\ Ghostboard.app/Contents
   ```

3. Inspect source user-visible strings and require every remaining `Ghostty`
   occurrence in `Ghostty.sdef` to be either gone or explicitly allowlisted as
   implementation-only:

   ```bash
   rg -n 'Ghostty|\\.config/ghostty|config\\.ghostty' ghostboard/macos/Sources/Features/Settings/SettingsView.swift ghostboard/macos/Ghostty.sdef ghostboard/macos/Ghostty-Info.plist
   rg -n 'Ghostty|\\.config/ghostty|config\\.ghostty' ghostboard/src/build/mdgen
   ```

Allowlisted implementation-only AppleScript tokens, if still present after this
experiment:

- Cocoa class names such as `GhosttyScriptWindow`,
  `GhosttyScriptInputTextCommand`, and related `GhosttyScript...` runtime class
  references;
- four-character AppleEvent codes such as `Ghst` where changing the code would
  be an automation compatibility migration rather than visible wording.

Pass criteria:

- Settings UI no longer tells users to restart Ghostty or edit a Ghostty config
  path.
- AppleScript dictionary title, suite, and descriptions use TermSurf Ghostboard
  user-visible wording.
- Bundled app metadata no longer contains the targeted user-visible Ghostty
  strings from this experiment.
- Implementation-only names such as `GhosttyScript...`, `GhosttyBuild`, and
  `GhosttyCommit` are either untouched or explicitly justified in the result.
- Generated docs/manpage template Ghostty product/config wording is inventoried
  but not changed unless the wording can be updated without deciding config-path
  behavior.
- The debug app still builds after the string cleanup.
- No config loading behavior, Homebrew/release packaging, iOS target identity,
  or broad implementation symbols are changed.

Partial criteria:

- Settings UI and AppleScript dictionary are fixed, but generated docs/manpage
  templates need a follow-up because they require config-path decisions.
- Source strings are fixed, but a system metadata cache requires manual refresh
  before bundled metadata inspection stops showing old labels.

Fail criteria:

- The app fails to build.
- The experiment changes actual config loading behavior before the config-path
  runtime proof.
- User-visible Ghostty identity remains in the targeted Settings UI, AppleScript
  dictionary, or bundled metadata surfaces.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Einstein the 2nd`:

- **Initial verdict:** Changes required.
- **Required finding:** AppleScript verification only checked selected Ghostty
  phrases even though `Ghostty.sdef` contains other user-visible Ghostty
  descriptions. Fixed by requiring all `Ghostty` occurrences in source and
  bundled scripting definitions to be removed or explicitly allowlisted as
  implementation-only.
- **Optional finding:** mdgen inspection did not cover all user-facing Ghostty
  product wording despite mdgen being listed in planned scope. Fixed by making
  mdgen inspection broad and documenting generated docs/manpage wording as
  inventory/debt unless it can be changed without deciding config-path behavior.
- **Re-review verdict:** Approved. The reviewer confirmed AppleScript
  verification now searches all `Ghostty` occurrences with an implementation
  allowlist, mdgen scope is clear, and no new Required finding was introduced.

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

Experiment 4 cleaned the targeted user-visible Ghostty identity leaks in the
macOS Settings UI, AppleScript scripting dictionary descriptions, and exported
surface identifier metadata without changing config loading behavior, release
packaging, Homebrew packaging, iOS identity, or broad implementation symbols.

### Changes

Changed files:

- `ghostboard/macos/Sources/Features/Settings/SettingsView.swift`
  - Replaced the visible Ghostty config-path/restart instruction with a
    path-neutral TermSurf Ghostboard message:
    `Edit the configuration file and restart TermSurf Ghostboard.`
  - This deliberately avoids claiming a final config path before the config-path
    runtime proof.
- `ghostboard/macos/Ghostty.sdef`
  - Renamed AppleScript dictionary title to
    `TermSurf Ghostboard Scripting Dictionary`.
  - Renamed suite name/description to TermSurf Ghostboard wording.
  - Replaced visible Ghostty descriptions for app, front window, window, tab,
    perform action, new window, new tab, and activate window.
  - Preserved Cocoa runtime class names such as `GhosttyScriptWindow`.
- `ghostboard/macos/Ghostty-Info.plist`
  - Renamed `Ghostty Surface Identifier` to
    `TermSurf Ghostboard Surface Identifier`.
  - Left the exported UTI identifier `com.mitchellh.ghosttySurfaceId` unchanged
    as compatibility debt because it is coupled to drag/drop runtime code such
    as `UTType.ghosttySurfaceId`.

No generated mdgen files were changed. Their Ghostty product/config wording was
inventoried and left as follow-up debt because many strings are tied to
unsettled config-path behavior.

### Verification

Formatting and static checks:

1. `prettier --write --prose-wrap always --print-width 80 issues/0819-ghostboard-packaging-identity-hardening/README.md issues/0819-ghostboard-packaging-identity-hardening/04-clean-user-visible-ghostty-identity.md`
   passed.
2. `git diff --check` passed.

Build:

1. `cd ghostboard/macos && ./build.nu --configuration Debug --action build`
   passed with `** BUILD SUCCEEDED **`.

Source identity inspection:

1. Targeted source search:

   ```bash
   rg -n 'Ghostty|ghostty|\.config/ghostty|config\.ghostty|com\.mitchellh\.ghostty' \
     ghostboard/macos/Sources/Features/Settings/SettingsView.swift \
     ghostboard/macos/Ghostty.sdef \
     ghostboard/macos/Ghostty-Info.plist
   ```

   Remaining `Ghostty` hits are allowlisted implementation-only or compatibility
   identifiers:

   - `GhosttyScript...` Cocoa class references in `Ghostty.sdef`;
   - `GhosttyBuild` and `GhosttyCommit` diagnostic keys;
   - `Ghostty.sdef` resource filename.
   - `com.mitchellh.ghosttySurfaceId` exported UTI identifier.

   No targeted Settings UI Ghostty config path or restart text remains.

   The exported UTI identifier is deferred compatibility debt rather than
   changed here because it is tied to macOS drag/drop type handling in
   `SurfaceView+Transferable.swift` and related split drop code. Renaming that
   identifier should be handled by a dedicated compatibility experiment that can
   preserve or migrate existing pasteboard/drag data behavior.

2. mdgen inventory:

   ```bash
   rg -n 'Ghostty|\.config/ghostty|config\.ghostty' ghostboard/src/build/mdgen
   ```

   This still finds Ghostty product/config wording in mdgen header/footer
   templates, including config paths such as
   `$XDG_CONFIG_HOME/ghostty/config.ghostty` and product descriptions such as
   `Ghostty terminal emulator`. These are documented as deferred debt for the
   config-path/documentation experiment.

Bundled metadata inspection:

1. Bundled scripting definition search:

   ```bash
   rg -n 'Ghostty' \
     'ghostboard/macos/build/Debug/TermSurf Ghostboard.app/Contents/Resources/Ghostty.sdef'
   ```

   Remaining bundled hits are only allowlisted `GhosttyScript...` Cocoa class
   references.

2. Bundled TermSurf Ghostboard search:

   ```bash
   rg -n 'TermSurf Ghostboard|Ghostty Surface Identifier|TermSurf Ghostboard Surface Identifier' \
     'ghostboard/macos/build/Debug/TermSurf Ghostboard.app/Contents'
   ```

   The built app now contains TermSurf Ghostboard wording for:

   - app plist names and service titles;
   - Dock Tile plugin display name;
   - AppleScript dictionary title/suite/descriptions;
   - exported surface identifier description.

3. Info.plist inspection:

   ```text
   GhosttyBuild => ""
   GhosttyCommit => ""
   OSAScriptingDefinition => "Ghostty.sdef"
   UTTypeIdentifier => "com.mitchellh.ghosttySurfaceId"
   UTTypeDescription => "TermSurf Ghostboard Surface Identifier"
   ```

   The remaining Ghostty plist keys/resource name and lower-case exported UTI
   identifier are implementation/diagnostic or compatibility identifiers and are
   not changed in this experiment.

## Conclusion

The targeted user-visible identity leaks are cleaned for the macOS Settings UI,
AppleScript dictionary text, and exported surface identifier metadata. Remaining
Ghostty names in the inspected surfaces are either implementation-only
allowlisted identifiers, compatibility debt (`com.mitchellh.ghosttySurfaceId`),
or generated documentation/config-path debt.

The next experiment should prove the real config loading path at runtime and
then align config-path docs, Settings UI specificity, and generated mdgen
documentation around the proven TermSurf Ghostboard path.

## Completion Review

Fresh-context adversarial completion review by Codex subagent
`McClintock the 2nd`:

- **Initial verdict:** Changes required.
- **Required finding:** The result missed the lower-case exported UTI identifier
  `com.mitchellh.ghosttySurfaceId` in source and built Info.plist metadata.
  Fixed by documenting the identifier as deferred compatibility debt, broadening
  verification to search lower-case `ghostty` / `com.mitchellh.ghostty`, and
  allowlisting the exported UTI identifier with rationale tied to drag/drop
  runtime code.
- **Re-review verdict:** Approved. The reviewer confirmed the UTI identifier is
  explicitly documented as compatibility debt, the verification search is
  broadened, the allowlist includes the exported UTI identifier,
  `git diff --check` passes, and no new Required finding was introduced.
