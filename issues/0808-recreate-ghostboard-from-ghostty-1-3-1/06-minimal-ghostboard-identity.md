# Experiment 6: Establish Minimal TermSurf Identity

## Description

Experiment 5 proved that the imported Ghostty `v1.3.1` baseline builds and
launches locally with documented build-only deviations. The next step is to
convert only the user-facing identity surfaces required by Issue 808 while
leaving upstream Ghostty implementation names intact.

This experiment should keep `ghostboard/` as the internal source folder while
making the built macOS app appear as TermSurf. The app bundle should be
`TermSurf.app`, its bundle executable should be `termsurf`, configuration should
load from `~/.config/termsurf/config`, and the primary app icon should use the
current Wezboard icon. It must not port the TermSurf protocol yet, must not
install a standalone CLI tool, and must not run the historical wholesale
Ghostty-to-TermSurf rename.

## Changes

- macOS app build settings only — make the app bundle executable name `termsurf`
  while continuing to use the existing Ghostty entrypoint internally. Do not
  make `emit-exe` imply installation, and do not install a standalone CLI in
  this experiment.
- `ghostboard/src/main_ghostty.zig` and closely related CLI/help text only if
  needed — update user-facing CLI usage text from `ghostty`/`Ghostty.app` to
  `termsurf`/`TermSurf.app`.
- `ghostboard/src/build_config.zig` — change the macOS bundle identifier used by
  Zig runtime logging/app-support helpers to a TermSurf-owned identifier.
- `ghostboard/src/config/file_load.zig` — make the default config path resolve
  to `~/.config/termsurf/config`. Do not keep `~/.config/ghostty/config` as the
  preferred path for the app built from `ghostboard/`. On macOS, this must also
  remove the current Application Support preference from
  `preferredDefaultFilePath`; if neither config file exists, the preferred
  returned path must be `~/.config/termsurf/config`, not Application Support.
- `ghostboard/src/config/Config.zig` — update config loading/creation so macOS
  creates the default template at `~/.config/termsurf/config` instead of
  Application Support, and update only user-facing config-path documentation
  that would otherwise tell users to use `~/.config/ghostty`.
- `ghostboard/macos/Ghostty.xcodeproj/project.pbxproj` — update the macOS app
  target build settings so the built app product is `TermSurf.app`, the
  executable is `termsurf`, the display name is `TermSurf`, and the bundle
  identifier is TermSurf-owned. The Xcode project, target, and scheme names may
  remain `Ghostty` for now to avoid broad project churn.
- `ghostboard/macos/Ghostty-Info.plist` and macOS user-facing permission strings
  only if needed — update app-visible names to `TermSurf` without renaming
  internal scripting classes or FFI symbols.
- `ghostboard/macos/Sources/App/macOS/MainMenu.xib` — update the Apple/menu bar
  hard-coded app menu title and About menu item from Ghostty to TermSurf.
- `ghostboard/macos/Sources/Features/About/AboutView.swift` — update the About
  window heading from Ghostty to TermSurf.
- `ghostboard/macos/Assets.xcassets` and/or `ghostboard/images/Ghostty.icon` —
  replace the primary app icon with a generated raster icon derived from
  `wezboard/assets/icon/wezboard-icon.svg`. Do not use the upstream Ghostty icon
  for the primary Dock/Finder/About identity.
- Issue docs — record the result and update the experiment index.

This experiment intentionally does not rename:

- `ghostboard/` source directory;
- Xcode project/scheme/target names;
- Swift module names;
- Zig module/type/function names;
- `ghostty_*` C ABI symbols;
- `GhosttyKit.xcframework`;
- internal notification names;
- AppleScript class names;
- shell integration resources;
- terminfo names;
- Linux/Windows packaging.

Those may be revisited only if later experiments prove they are required.

## Verification

1. Run Zig formatting on edited Zig files.
2. If Swift files are edited, run SwiftLint formatting/linting:

   ```bash
   cd ghostboard
   swiftlint lint --strict --fix
   ```

3. Format edited markdown.
4. Build the native GhosttyKit framework:

   ```bash
   cd ghostboard
   zig build -Demit-xcframework=true -Dxcframework-target=native -Demit-macos-app=false
   ```

5. Build the macOS app through the repo's macOS build wrapper:

   ```bash
   cd ghostboard
   macos/build.nu --scheme Ghostty --configuration Debug --action build
   ```

6. Verify the built app bundle and executable names:

   ```bash
   test -d "ghostboard/macos/build/Debug/TermSurf.app"
   test -x "ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf"
   /usr/libexec/PlistBuddy -c 'Print :CFBundleDisplayName' \
     "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist"
   /usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' \
     "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist"
   /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
     "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist"
   ```

7. Launch the built app by absolute path, confirm a process is running from
   `TermSurf.app/Contents/MacOS/termsurf`, then terminate only that process.
8. Verify config discovery with a temporary home or explicit test harness so
   `~/.config/termsurf/config` is the default path for the app built from
   `ghostboard/`. If a direct runtime assertion is not practical in this
   experiment, add a focused Zig test or command output that proves
   `config.file_load.defaultXdgPath` and the preferred default path resolve to
   the TermSurf config location. Also verify the no-config case creates the
   template at `~/.config/termsurf/config`, not Application Support.
9. Verify the primary built app icon deterministically:
   - name `wezboard/assets/icon/wezboard-icon.svg` as the source asset;
   - generate the app icon assets from that source;
   - inspect the built `TermSurf.app` bundle to find the icon resource used by
     `CFBundleIconFile`/asset catalog output;
   - compare the generated built icon against the Wezboard-derived source using
     a reproducible hash or pixel comparison, and record the command output.
10. Confirm the diff did not touch protocol, `webtui`, or `roamium` code.

Pass criteria:

- The app builds as `TermSurf.app`.
- The app executable is `Contents/MacOS/termsurf`.
- The app display name, Dock/menu/about-facing bundle name, and bundle
  identifier are TermSurf/TermSurf-owned.
- The default config path is `~/.config/termsurf/config`.
- On macOS, the no-config template is created at `~/.config/termsurf/config`,
  not Application Support.
- The primary app icon is generated from
  `wezboard/assets/icon/wezboard-icon.svg` and the built bundle verification
  proves it.
- Internal Ghostty implementation names remain mostly unchanged, limited to the
  exceptions required for user-facing identity.
- The experiment does not install a standalone CLI and does not make `emit-exe`
  imply installation.
- No protocol, `webtui`, or `roamium` changes are made.

Fail criteria:

- The identity changes break the build or launch.
- The implementation requires broad internal renaming.
- The app still advertises itself as Ghostty in the required user-facing
  surfaces.
- The config path still defaults to `~/.config/ghostty/config`.
- The icon remains the upstream Ghostty icon.
- The experiment installs a standalone CLI or changes `emit-exe` semantics.
- The experiment drifts into TermSurf protocol implementation.

## Notes

If this passes, the next experiment can start protocol integration from a
properly named app. If it fails, the result should identify which identity
surface is coupled too tightly to upstream Ghostty and narrow the next
experiment to that coupling.

## Design Review

Fresh-context adversarial review initially returned `CHANGES REQUIRED`.

Required findings accepted and fixed:

- The config plan did not cover macOS Application Support preference and
  template creation. The design now requires `~/.config/termsurf/config` to be
  the preferred/default path and no-config template target on macOS.
- The menu/about branding scope missed concrete hard-coded files. The design now
  includes `MainMenu.xib` and `AboutView.swift`.
- Swift formatting was underspecified. The verification now requires
  `swiftlint lint --strict --fix` when Swift files are edited.
- The app build command used raw `xcodebuild`. The verification now uses
  `macos/build.nu --scheme Ghostty --configuration Debug --action build`.
- Icon verification was too vague. The design now names
  `wezboard/assets/icon/wezboard-icon.svg` as the source and requires a
  deterministic built-bundle icon comparison.

Focused re-review returned `APPROVED` with no remaining required findings.
