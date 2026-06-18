# Experiment 6: Align Config Path Documentation

## Description

Experiment 5 proved the current Ghostboard config-loading contract:

1. `GHOSTTY_CONFIG_PATH` is the highest-priority explicit config path.
2. Without `GHOSTTY_CONFIG_PATH`, Ghostboard loads
   `$XDG_CONFIG_HOME/termsurf/config`.
3. If `$XDG_CONFIG_HOME/termsurf/config` is absent, Ghostboard does not fall
   back to the tested inherited Ghostty XDG or macOS Application Support paths.

This experiment will align user-facing config documentation and Settings UI copy
with that proven contract. It should not change config-loading behavior.

## Changes

Planned source changes:

- `ghostboard/macos/Sources/Features/Settings/SettingsView.swift`
  - Replace the path-neutral config message with the proven normal config path:
    `$XDG_CONFIG_HOME/termsurf/config`, with `~/.config/termsurf/config` as the
    default when `XDG_CONFIG_HOME` is unset.

Planned documentation/template changes:

- `ghostboard/src/build/mdgen/ghostty_1_header.md`
- `ghostboard/src/build/mdgen/ghostty_1_footer.md`
- `ghostboard/src/build/mdgen/ghostty_5_header.md`
- `ghostboard/src/build/mdgen/ghostty_5_footer.md`
  - Replace user-facing Ghostty config path and macOS Application Support
    fallback claims with the proven TermSurf Ghostboard XDG config path.
  - Remove claims that macOS prefers
    `$HOME/Library/Application Support/com.mitchellh.ghostty/config.ghostty`.
  - Remove inherited Windows `LOCALAPPDATA/ghostty/config.ghostty` path claims
    from the targeted templates rather than preserving another unsupported
    Ghostty config location.
  - Keep implementation-only file names such as `ghostty_*.md` unchanged.
- `docs/xdg.md`
  - Replace stale “Ghostty configuration” wording with TermSurf Ghostboard
    wording where it describes `XDG_CONFIG_HOME`.

Planned issue-document changes:

- Add `## Result` and `## Conclusion` after verification.
- Update the Issue 819 README experiment status after verification.

Explicitly out of scope:

- Changing config-loading behavior.
- Renaming `GHOSTTY_CONFIG_PATH` or implementation-only Ghostty symbols.
- Regenerating or rewriting unrelated generated documentation beyond the
  targeted templates/docs above.
- Release/Homebrew packaging changes.

## Verification

Formatting actions:

1. Preserve the existing Swift and mdgen template style manually.
2. Format edited issue/docs Markdown:

   ```bash
   prettier --write --prose-wrap always --print-width 80 \
     docs/xdg.md \
     issues/0819-ghostboard-packaging-identity-hardening/README.md \
     issues/0819-ghostboard-packaging-identity-hardening/06-align-config-path-documentation.md
   ```

Static checks:

1. `git diff --check`.
2. Search for stale user-facing config-path claims in the targeted files:

   ```bash
   rg -n 'ghostty/config\\.ghostty|com\\.mitchellh\\.ghostty/config\\.ghostty|LOCALAPPDATA/ghostty/config\\.ghostty|Application Support/com\\.mitchellh\\.ghostty|Ghostty configuration|Ghostty config file|handled by|Ghostty for its config' \
     ghostboard/src/build/mdgen/ghostty_1_header.md \
     ghostboard/src/build/mdgen/ghostty_1_footer.md \
     ghostboard/src/build/mdgen/ghostty_5_header.md \
     ghostboard/src/build/mdgen/ghostty_5_footer.md \
     docs/xdg.md \
     ghostboard/macos/Sources/Features/Settings/SettingsView.swift
   ```

Runtime/build checks:

1. Build the debug app:

   ```bash
   (cd ghostboard/macos && ./build.nu --configuration Debug --action build)
   ```

2. Re-run the config-path proof to ensure no behavior changed:

   ```bash
   scripts/ghostboard-geometry-matrix.sh ghostboard-config-paths
   ```

Pass criteria:

- Settings UI tells users the proven normal Ghostboard config path.
- Targeted mdgen templates no longer claim the normal config path is
  `$XDG_CONFIG_HOME/ghostty/config.ghostty` or the macOS Application Support
  Ghostty path.
- `docs/xdg.md` no longer describes `XDG_CONFIG_HOME/termsurf/` as Ghostty
  configuration.
- The debug app builds.
- The `ghostboard-config-paths` scenario still passes.
- No config-loading behavior changes.

Partial criteria:

- Docs/templates are fixed, but Settings UI cannot be safely made path-specific
  without adding runtime path formatting.
- Settings UI and docs are fixed, but mdgen generated output requires a separate
  generator step that is not available in this environment.

Fail criteria:

- The experiment changes config-loading behavior.
- User-facing docs still point at inherited Ghostty config paths after the
  change.
- The config-path proof regresses.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Planck the 2nd`:

- **Initial verdict:** Changes required.
- **Required finding:** The stale-text verification only searched for
  `Ghostty configuration`, but `docs/xdg.md` also contained stale variants such
  as `Ghostty config file` and `handled by Ghostty`. Fixed by expanding the
  targeted stale-text search pattern to include those variants.
- **Required finding:** The build verification command used
  `cd ghostboard/macos && ...`, after which the next command could be read as
  running from the wrong directory. Fixed by making the build command a
  subshell:
  `(cd ghostboard/macos && ./build.nu --configuration Debug --action build)`.
- **Required finding:** The stale-text verification still missed inherited
  Windows `LOCALAPPDATA/ghostty/config.ghostty` path claims in the targeted
  mdgen footer templates. Fixed by adding that path family to the planned
  cleanup and stale-text search.
- **Required finding:** The `handled by Ghostty` search did not catch current
  wrapped `docs/xdg.md` wording. Fixed by searching separately for `handled by`
  and `Ghostty for its config`.
- **Re-review verdict:** Approved.

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

Aligned the targeted user-facing config path docs and Settings copy with the
runtime contract proven in Experiment 5.

Changed files:

- `ghostboard/macos/Sources/Features/Settings/SettingsView.swift`
  - Settings now tells users to edit `$XDG_CONFIG_HOME/termsurf/config`, or
    `~/.config/termsurf/config` when `XDG_CONFIG_HOME` is unset, and restart
    TermSurf Ghostboard.
- `ghostboard/src/build/mdgen/ghostty_5_header.md`
  - The configuration section now documents TermSurf Ghostboard's
    `$XDG_CONFIG_HOME/termsurf/config` path and says inherited Ghostty XDG and
    macOS Application Support paths are not loaded.
- `ghostboard/src/build/mdgen/ghostty_1_footer.md`
- `ghostboard/src/build/mdgen/ghostty_5_footer.md`
  - File listings now point at `$XDG_CONFIG_HOME/termsurf/config`.
  - Removed inherited macOS Application Support and Windows
    `LOCALAPPDATA/ghostty/config.ghostty` config path claims.
- `docs/xdg.md`
  - Replaced stale Ghostty config wording with TermSurf Ghostboard config
    wording for the TermSurf XDG config directory.

Verification:

```bash
prettier --write --prose-wrap always --print-width 80 \
  docs/xdg.md \
  issues/0819-ghostboard-packaging-identity-hardening/README.md \
  issues/0819-ghostboard-packaging-identity-hardening/06-align-config-path-documentation.md
git diff --check
rg -n 'ghostty/config\.ghostty|com\.mitchellh\.ghostty/config\.ghostty|LOCALAPPDATA/ghostty/config\.ghostty|Application Support/com\.mitchellh\.ghostty|Ghostty configuration|Ghostty config file|handled by|Ghostty for its config' \
  ghostboard/src/build/mdgen/ghostty_1_header.md \
  ghostboard/src/build/mdgen/ghostty_1_footer.md \
  ghostboard/src/build/mdgen/ghostty_5_header.md \
  ghostboard/src/build/mdgen/ghostty_5_footer.md \
  docs/xdg.md \
  ghostboard/macos/Sources/Features/Settings/SettingsView.swift
(cd ghostboard/macos && ./build.nu --configuration Debug --action build)
scripts/ghostboard-geometry-matrix.sh ghostboard-config-paths
```

The stale-text search returned no matches. The debug app build succeeded. The
config-path proof passed all three cases: `explicit-env`, `xdg-default`, and
`no-current-xdg`.

The build emitted existing warnings unrelated to this experiment:

- SwiftLint optional `Data` to `String` conversion warning in
  `SurfaceView_AppKit.swift`.
- Existing `UnsafeMutableRawPointer`/`Sendable` warning in
  `GhosttyPackage.swift`.
- dSYM warnings for missing ImGui symbols in `libghostty.a(ext.o)`.

## Conclusion

The targeted user-facing config path surfaces now agree with the proven
Ghostboard runtime behavior. The normal documented config path is
`$XDG_CONFIG_HOME/termsurf/config`, with `~/.config/termsurf/config` as the
default when `XDG_CONFIG_HOME` is unset. This experiment did not change config
loading behavior.

## Completion Review

Fresh-context adversarial completion review by Codex subagent `Plato the 2nd`:

- **Verdict:** Approved.
