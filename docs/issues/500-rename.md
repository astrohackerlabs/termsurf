# Issue 500: Rename Ghostty to TermSurf in ts5

## Goal

Rename high-impact uses of "Ghostty" to "TermSurf" in ts5, and replace the
Ghostty logo with the TermSurf logo. The ts5 codebase was imported as unmodified
upstream Ghostty (Issue 418). This issue tracks making it ours.

## Approach

ts1 created a parallel `termsurf-macos/` directory alongside the upstream
`macos/` to preserve the working Ghostty app during development. That made sense
at the time but is unnecessary for ts5 — we're just forking Ghostty. We modify
`ts5/macos/` directly.

ts1's rename research is still the reference for _what_ to change. Internal
identifiers (`com.mitchellh.ghostty.*` notification names, `GhosttyKit`
framework name, `Ghostty.*` Swift namespace, C API function names) stay
unchanged — they're internal plumbing and renaming them creates unnecessary
merge conflicts with upstream.

## What ts1 Changed

### 1. Parallel macOS App Directory

Created `ts1/termsurf-macos/` alongside `ts1/macos/`:

```
ts1/termsurf-macos/
├── TermSurf.xcodeproj/       # New Xcode project
├── TermSurf-Info.plist        # Custom Info.plist
├── TermSurf.entitlements      # Entitlements (release)
├── TermSurfDebug.entitlements # Entitlements (debug)
├── TermSurfReleaseLocal.entitlements
├── Assets.xcassets/           # TermSurf icons and assets
├── icon-source/               # Source icon files
│   ├── termsurf-icon.png
│   └── termsurf-debug-icon.png
├── Sources/                   # Swift source (copied from macos/Sources)
└── Tests/
```

The key is that `macos/Sources/` was copied to `termsurf-macos/Sources/`, then
modified. The original `macos/` stays untouched for clean upstream merges.

### 2. Xcode Project Configuration

In `TermSurf.xcodeproj/project.pbxproj`:

| Setting                                     | Ghostty value                 | TermSurf value       |
| ------------------------------------------- | ----------------------------- | -------------------- |
| Product Name                                | `Ghostty`                     | `TermSurf`           |
| `PRODUCT_BUNDLE_IDENTIFIER`                 | `com.mitchellh.ghostty`       | `com.termsurf`       |
| `PRODUCT_BUNDLE_IDENTIFIER` (debug)         | `com.mitchellh.ghostty.debug` | `com.termsurf.debug` |
| `INFOPLIST_KEY_CFBundleDisplayName`         | `Ghostty`                     | `TermSurf`           |
| `INFOPLIST_KEY_CFBundleDisplayName` (debug) | `Ghostty[DEBUG]`              | `TermSurf[DEBUG]`    |
| Executable name                             | `ghostty`                     | (unchanged)          |

### 3. Info.plist

`TermSurf-Info.plist` changes from `Ghostty-Info.plist`:

- Added `TermSurfBuild` and `TermSurfCommit` keys (parallel to Ghostty's)
- Changed UTType description: `"Ghostty Surface Identifier"` →
  `"TermSurf Surface Identifier"`
- Menu items use `$(INFOPLIST_KEY_CFBundleDisplayName)` so they automatically
  read "New TermSurf Tab Here" / "New TermSurf Window Here"
- Kept `GHOSTTY_MAC_LAUNCH_SOURCE` and `com.mitchellh.ghosttySurfaceId` as-is
  (internal compatibility)

### 4. CLI Text (Shared Zig Code)

These changes are in `ts5/src/` (shared between macOS and other platforms):

**`src/cli/help.zig`:**

```
"Usage: ghostty"     → "Usage: termsurf"
"Run the Ghostty"    → "Run the TermSurf"
"ghostty -e top"     → "termsurf -e top"
"open -na Ghostty.app" → "open -na TermSurf.app"
```

**`src/cli/version.zig`:**

```
"Ghostty {version}"  → "TermSurf {version}"
```

**`src/cli/list_themes.zig`:**

```
"👻 Ghostty Theme Preview 👻" → "🏄 TermSurf Theme Preview 🏄"
```

### 5. Config Paths

In `termsurf-macos/Sources/Ghostty/Ghostty.Config.swift`:

```swift
// Was: ghostty_config_load_default_files(cfg)
ghostty_config_load_files(cfg, "termsurf", "com.termsurf")
```

This makes the app read config from `~/.config/termsurf/` with fallback to
`~/Library/Application Support/com.termsurf/`, instead of Ghostty's default
`~/.config/ghostty/`.

In `termsurf-macos/Sources/Ghostty/Ghostty.Config.swift` (icon path):

```swift
// Was: "~/.config/ghostty/Ghostty.icns"
"~/.config/termsurf/TermSurf.icns"
```

In `termsurf-macos/Sources/Features/Settings/SettingsView.swift`:

```swift
// Was: "$HOME/.config/ghostty/config and restart Ghostty"
"$HOME/.config/termsurf/config and restart TermSurf"
```

### 6. About View

In `termsurf-macos/Sources/Features/About/AboutView.swift`:

```swift
Text("TermSurf")
  .bold()
  .font(.title)
Text("Terminal emulator with integrated browser,\nbuilt on Ghostty.")
```

GitHub URL changed to `https://github.com/termsurf/termsurf`.

### 7. Icons

Custom TermSurf icon set in `termsurf-macos/Assets.xcassets/`:

- `AppIcon.appiconset/` — Multiple sizes (16–1024px)
- `TermSurfDebugIcon.imageset/` — Debug build icon
- `AppIconImage.imageset/` — Icon variants
- Source files in `termsurf-macos/icon-source/`

### 8. Build System

In `build.zig`, added a second xcframework target:

```zig
// TermSurf xcframework (for termsurf-macos/)
const xcframework_termsurf = try buildpkg.GhosttyXCFramework.initWithPath(
    b, &deps, config.xcframework_target,
    "termsurf-macos/GhosttyKit.xcframework",
);
```

Both `xcframework.install()` and `xcframework_termsurf.install()` are called
when building.

### 9. TermSurf-Specific Swift Code

New files added for browser integration (not rename-related, but present in
`termsurf-macos/` and not in `macos/`):

- `Sources/Features/Socket/TermsurfEnvironment.swift` — Injects
  `TERMSURF_SOCKET` and `TERMSURF_PANE_ID` env vars
- `Sources/Features/Socket/TermsurfProtocol.swift` — Socket protocol
- `Sources/Features/WebView/` — WebView integration files

### 10. What Was NOT Renamed

These internal identifiers stayed as-is in ts1 to minimize merge conflicts:

- `com.mitchellh.ghostty.*` notification names (dozens in `Package.swift`)
- `com.mitchellh.ghosttySurfaceId` UTType identifier
- `GhosttyKit.xcframework` framework name
- `Ghostty.*` Swift namespace (`Ghostty.Config`, `Ghostty.App`, etc.)
- `ghostty_config_*` C API function names
- `GHOSTTY_MAC_LAUNCH_SOURCE` environment variable
- Swift file names like `AppDelegate+Ghostty.swift`, `Ghostty.Config.swift`

## Changes for ts5

### Change 1: Update Xcode Project

Modify `ts5/macos/Ghostty.xcodeproj/project.pbxproj` directly:

1. Bundle identifier: `com.mitchellh.ghostty` → `com.termsurf`
2. Bundle identifier (debug): `com.mitchellh.ghostty.debug` →
   `com.termsurf.debug`
3. Display name: `Ghostty` → `TermSurf`
4. Display name (debug): `Ghostty[DEBUG]` → `TermSurf[DEBUG]`
5. Product name: `Ghostty` → `TermSurf`

Rename files in `ts5/macos/`:

- `Ghostty-Info.plist` → `TermSurf-Info.plist`
- `Ghostty.entitlements` → `TermSurf.entitlements`
- `GhosttyDebug.entitlements` → `TermSurfDebug.entitlements`
- `GhosttyReleaseLocal.entitlements` → `TermSurfReleaseLocal.entitlements`

Update references in `project.pbxproj` to match the new filenames.

### Change 2: Update Info.plist

In `ts5/macos/TermSurf-Info.plist` (after rename):

1. Add `TermSurfBuild` and `TermSurfCommit` keys
2. Change UTType description: `"Ghostty Surface Identifier"` →
   `"TermSurf Surface Identifier"`

Menu items already use `$(INFOPLIST_KEY_CFBundleDisplayName)` so they'll
automatically read "New TermSurf Tab Here" once the display name is changed.

### Change 3: Update CLI Text

In `ts5/src/`:

1. `src/cli/help.zig` — Replace "ghostty"/"Ghostty" with "termsurf"/"TermSurf"
2. `src/cli/version.zig` — `"Ghostty {version}"` → `"TermSurf {version}"`
3. `src/cli/list_themes.zig` — Theme preview title

### Change 4: Update Config Paths

In `ts5/macos/Sources/`:

1. `Ghostty/Ghostty.Config.swift` — Use
   `ghostty_config_load_files(cfg,
   "termsurf", "com.termsurf")` instead of
   `ghostty_config_load_default_files`
2. `Ghostty/Ghostty.Config.swift` — Icon path →
   `~/.config/termsurf/TermSurf.icns`
3. `Features/Settings/SettingsView.swift` — Config path and app name in
   instructions
4. `Features/About/AboutView.swift` — Display name, description, GitHub URL

### Change 5: Replace Icons

Copy TermSurf icon assets from ts1 into `ts5/macos/Assets.xcassets/`:

- `ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset/` →
  `ts5/macos/Assets.xcassets/AppIcon.appiconset/`
- `ts1/termsurf-macos/Assets.xcassets/TermSurfDebugIcon.imageset/` →
  `ts5/macos/Assets.xcassets/TermSurfDebugIcon.imageset/`
- `ts1/termsurf-macos/Assets.xcassets/AppIconImage.imageset/` →
  `ts5/macos/Assets.xcassets/AppIconImage.imageset/`

Copy icon source files:

- `ts1/termsurf-macos/icon-source/` → `ts5/macos/icon-source/`

## Scope

This issue covers renaming and icon replacement only. Browser integration (the
`web` command, socket server, WebView panes) is separate future work that builds
on top of the renamed app.

## Merge Conflict Expectations

All changes are in files that upstream Ghostty also modifies, so future
`git subtree pull` may produce conflicts. The conflicts will be small and
predictable — keep our version of the renamed strings, resolve Xcode project
changes manually if upstream restructures build settings.

## Experiments

### Experiment 1: Implement All Name Changes

#### Hypothesis

Applying all five changes (Xcode project, Info.plist, CLI text, config paths,
icons) in one pass will produce a working `TermSurf.app` that builds with
`cd ts5 && zig build`.

#### Steps

##### Step 1: Rename plist and entitlements files

```bash
cd ts5/macos
git mv Ghostty-Info.plist TermSurf-Info.plist
git mv Ghostty.entitlements TermSurf.entitlements
git mv GhosttyDebug.entitlements TermSurfDebug.entitlements
git mv GhosttyReleaseLocal.entitlements TermSurfReleaseLocal.entitlements
```

##### Step 2: Update `project.pbxproj`

In `ts5/macos/Ghostty.xcodeproj/project.pbxproj`:

- Replace file references: `Ghostty-Info.plist` → `TermSurf-Info.plist`,
  `Ghostty.entitlements` → `TermSurf.entitlements`, `GhosttyDebug.entitlements`
  → `TermSurfDebug.entitlements`, `GhosttyReleaseLocal.entitlements` →
  `TermSurfReleaseLocal.entitlements`
- Replace `PRODUCT_BUNDLE_IDENTIFIER = com.mitchellh.ghostty` →
  `PRODUCT_BUNDLE_IDENTIFIER = com.termsurf` (all variants including `.debug`)
- Replace `INFOPLIST_KEY_CFBundleDisplayName = Ghostty` →
  `INFOPLIST_KEY_CFBundleDisplayName = TermSurf` (including debug `[DEBUG]`
  variant)
- Replace `PRODUCT_NAME = Ghostty` → `PRODUCT_NAME = TermSurf`

Do NOT rename the `Ghostty.xcodeproj` directory itself,
`GhosttyKit.xcframework`, or any `Ghostty.*` Swift namespace references — those
are internal.

##### Step 3: Update `TermSurf-Info.plist`

- Add `TermSurfBuild` key (empty string, like ts1)
- Add `TermSurfCommit` key (empty string, like ts1)
- Change UTType description: `"Ghostty Surface Identifier"` →
  `"TermSurf Surface Identifier"`

##### Step 4: Update CLI text

- `ts5/src/cli/help.zig` — All instances of "ghostty"/"Ghostty" →
  "termsurf"/"TermSurf"
- `ts5/src/cli/version.zig` — `"Ghostty {s}"` → `"TermSurf {s}"`
- `ts5/src/cli/list_themes.zig` — `"👻 Ghostty Theme Preview 👻"` →
  `"🏄 TermSurf Theme Preview 🏄"`

##### Step 5: Update config paths in Swift

- `ts5/macos/Sources/Ghostty/Ghostty.Config.swift` — Change
  `ghostty_config_load_default_files(cfg)` to
  `ghostty_config_load_files(cfg, "termsurf", "com.termsurf")`
- `ts5/macos/Sources/Ghostty/Ghostty.Config.swift` — Change custom icon path
  from `~/.config/ghostty/Ghostty.icns` to `~/.config/termsurf/TermSurf.icns`
- `ts5/macos/Sources/Features/Settings/SettingsView.swift` — Change
  `$HOME/.config/ghostty/config.ghostty and restart Ghostty` to
  `$HOME/.config/termsurf/config and restart TermSurf`

##### Step 6: Update About view

- `ts5/macos/Sources/Features/About/AboutView.swift` — Change `"Ghostty"` title
  to `"TermSurf"`
- Add subtitle:
  `"Terminal emulator with integrated browser,\nbuilt on Ghostty."`
- Change GitHub URL to `https://github.com/termsurf/termsurf`

##### Step 7: Replace icons

Copy icon assets from ts1:

```bash
# App icon set
rm -rf ts5/macos/Assets.xcassets/AppIcon.appiconset
cp -R ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset \
  ts5/macos/Assets.xcassets/AppIcon.appiconset

# App icon image
rm -rf ts5/macos/Assets.xcassets/AppIconImage.imageset
cp -R ts1/termsurf-macos/Assets.xcassets/AppIconImage.imageset \
  ts5/macos/Assets.xcassets/AppIconImage.imageset

# Debug icon
cp -R ts1/termsurf-macos/Assets.xcassets/TermSurfDebugIcon.imageset \
  ts5/macos/Assets.xcassets/TermSurfDebugIcon.imageset

# Icon source files
cp -R ts1/termsurf-macos/icon-source ts5/macos/icon-source
```

##### Step 8: Build and verify

```bash
cd ts5 && zig build
```

Verify the output app is named `TermSurf.app` (not `Ghostty.app`).

#### Result

Partial success. `TermSurf.app` builds and runs at `ts5/zig-out/TermSurf.app`.
Menu bar shows "TermSurf" correctly. However, the app icon is still the Ghostty
ghost — the icon replacement in step 7 did not take effect. Needs investigation
in a follow-up experiment.

Steps 1–7 went as planned. Step 8 revealed two issues that required additional
work beyond what the plan specified:

1. **`ghostty_config_load_files` doesn't exist in upstream Ghostty.** This was a
   custom C API function added in ts1. Ported three pieces from ts1:
   - `appSupportDirWithBundleId` in `ts5/src/os/macos.zig`
   - `loadFiles` method in `ts5/src/config/Config.zig`
   - `ghostty_config_load_files` C API export in `ts5/src/config/CApi.zig`
   - C header declaration in `ts5/include/ghostty.h`

2. **`PRODUCT_NAME = "$(TARGET_NAME)"` resolves to `Ghostty`.** Changed
   `PRODUCT_NAME` to `TermSurf` in the three main app build configurations
   (Debug, Release, ReleaseLocal). Also updated the Zig build system's app path
   in `ts5/src/build/GhosttyXcodebuild.zig` from `Ghostty.app` to
   `TermSurf.app`.

Additionally changed `within Ghostty` → `within TermSurf` in all permission
dialog strings in `project.pbxproj` (user-facing but not in the original plan).

### Experiment 2: Revert Icon Changes

#### Hypothesis

The icon changes from experiment 1 didn't take effect because upstream Ghostty
uses Icon Composer (`.icon` format), not `AppIcon.appiconset`. Rather than fight
the icon system now, revert the icon-related additions from experiment 1 and
keep the Ghostty icon until a proper icon replacement is tackled separately.

#### Steps

##### Step 1: Remove copied icon assets

Delete the icon files that were copied from ts1 in experiment 1:

```bash
rm -rf ts5/macos/Assets.xcassets/AppIcon.appiconset
rm -rf ts5/macos/Assets.xcassets/AppIconImage.imageset
rm -rf ts5/macos/Assets.xcassets/TermSurfDebugIcon.imageset
rm -rf ts5/macos/icon-source
```

##### Step 2: Build and verify

```bash
cd ts5 && zig build
```

Verify the app still builds cleanly. The icon will remain the upstream Ghostty
icon for now.

#### Result

Success. Removed all four icon additions from experiment 1. Build passes and
`TermSurf.app` is produced. The app uses the upstream Ghostty icon for now.
