+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 651: Bundle Identifier Confusion

## Goal

Prevent macOS from launching the debug build when the user clicks the installed
TermSurf in `/Applications`. After a debug build, macOS Launch Services may
resolve the TermSurf app to the build tree copy instead of the installed copy.

## Problem

macOS Launch Services maintains a registry of every `.app` bundle it discovers.
When the user clicks "TermSurf" in Finder, Spotlight, or the Dock, Launch
Services picks which copy to launch. With three `TermSurf.app` bundles on disk:

1. `/Applications/TermSurf.app` (installed release)
2. `gui/macos/build/ReleaseLocal/TermSurf.app` (release build output)
3. `gui/macos/build/Debug/TermSurf.app` (debug build output)

Launch Services may launch the wrong one — typically the most recently built
copy, which after a debug build is the debug version.

## Current state

The bundle IDs are already separate:

- **Debug**: `com.termsurf.debug`
  (`gui/macos/Ghostty.xcodeproj/project.pbxproj:942`)
- **Release**: `com.termsurf` (`gui/macos/Ghostty.xcodeproj/project.pbxproj` —
  release config)

Despite different bundle IDs, Launch Services still registers all three copies
because they share the same display name ("TermSurf"). The `lsregister` dump
shows all three:

```
path: gui/macos/build/ReleaseLocal/TermSurf.app
path: /Applications/TermSurf.app
path: gui/macos/build/Debug/TermSurf.app
```

## Why this happens

Launch Services indexes apps by multiple signals: bundle ID, display name,
filesystem location. Even with different bundle IDs, macOS treats apps with the
same display name as related. When resolving which app to open (e.g., from
Spotlight or "Open With"), it may prefer the most recently modified copy.

The installed copy in `/Applications/` should take priority, but Launch Services
doesn't always respect this after a fresh build registers a newer copy.

## Proposed fix

Two complementary changes:

### 1. Rename the debug app bundle

Change the debug build's `CFBundleDisplayName` (or `CFBundleName`) to "TermSurf
Debug" so it appears as a visually distinct app. This also helps the user tell
which version they're running.

The Xcode project already has separate Debug/Release configurations. Add a
`PRODUCT_NAME` override for Debug:

- Debug: `PRODUCT_NAME = TermSurf Debug` (produces `TermSurf Debug.app`)
- Release: `PRODUCT_NAME = TermSurf` (produces `TermSurf.app`)

This requires updating `GhosttyXcodebuild.zig` to use the correct app name when
constructing the path to the built app.

### 2. Unregister build copies in install.sh

Add an `lsregister -u` call to `install.sh` to unregister the build tree copies
from Launch Services:

```bash
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
"$LSREGISTER" -u "$REPO_DIR/gui/macos/build/Debug/TermSurf.app" 2>/dev/null || true
"$LSREGISTER" -u "$REPO_DIR/gui/macos/build/ReleaseLocal/TermSurf.app" 2>/dev/null || true
```

This ensures that after installation, Finder/Spotlight only see the
`/Applications/` copy.

## Key files

| File                                          | Purpose                                |
| --------------------------------------------- | -------------------------------------- |
| `gui/macos/Ghostty.xcodeproj/project.pbxproj` | Bundle ID and product name per config  |
| `gui/src/build_config.zig:58`                 | `bundle_id = "com.termsurf"`           |
| `gui/src/build/GhosttyXcodebuild.zig`         | Path to built `.app` bundle            |
| `install.sh`                                  | Install script (needs `lsregister -u`) |

## Experiments

### Experiment 1: Rename debug bundle and unregister build copies

**Goal:** Make the debug app bundle visually distinct from the release app so
Spotlight never confuses them. Unregister build tree copies in `install.sh` as a
safety net.

#### Changes

**1. `gui/macos/Ghostty.xcodeproj/project.pbxproj:943`** — Change the debug
config's product name:

```
PRODUCT_NAME = TermSurf;
```

To:

```
PRODUCT_NAME = "TermSurf Debug";
```

This is the Debug build settings block (alongside
`PRODUCT_BUNDLE_IDENTIFIER = com.termsurf.debug` at line 942). Only the Debug
config changes — the Release (line 635) and ReleaseLocal (line 998) configs keep
`PRODUCT_NAME = TermSurf`.

The debug build will now produce `gui/macos/build/Debug/TermSurf Debug.app`
instead of `gui/macos/build/Debug/TermSurf.app`. Spotlight will show it as
"TermSurf Debug", making it immediately distinguishable.

**2. `gui/src/build/GhosttyXcodebuild.zig:52`** — Update the app path to use the
correct name per build mode:

```zig
const app_path = b.fmt("macos/build/{s}/TermSurf.app", .{xc_config});
```

To:

```zig
const app_name = if (cfg.optimize == .Debug) "TermSurf Debug" else "TermSurf";
const app_path = b.fmt("macos/build/{s}/{s}.app", .{ xc_config, app_name });
```

This ensures `zig build` and `zig build run` find the app at the correct path
for both debug and release configurations.

**3. `install.sh`** — Add `lsregister -u` calls after copying the app bundle:

```bash
# Unregister build tree copies so Spotlight only finds /Applications.
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
"$LSREGISTER" -u "$REPO_DIR/gui/macos/build/Debug/TermSurf Debug.app" 2>/dev/null || true
"$LSREGISTER" -u "$REPO_DIR/gui/macos/build/Debug/TermSurf.app" 2>/dev/null || true
"$LSREGISTER" -u "$REPO_DIR/gui/macos/build/ReleaseLocal/TermSurf.app" 2>/dev/null || true
```

Unregisters both the old name (`TermSurf.app`) and the new name
(`TermSurf Debug.app`) from the debug build directory, plus the release build
directory copy. After this, Spotlight only finds `/Applications/TermSurf.app`.

#### Verification

1. **Debug build**: `cd gui && zig build`. Verify the app is at
   `gui/macos/build/Debug/TermSurf Debug.app` (not `TermSurf.app`). Run it with
   `open "gui/macos/build/Debug/TermSurf Debug.app"`. It works normally.
2. **Release build**: `cd gui && zig build -Doptimize=ReleaseFast`. Verify the
   app is at `gui/macos/build/ReleaseLocal/TermSurf.app` (unchanged).
3. **Spotlight**: After a debug build, press F4 and type "termsurf". Spotlight
   should show "TermSurf Debug" and "TermSurf" as separate entries.
4. **Install**: Run `install.sh`. Press F4 and type "termsurf". Only the
   `/Applications/TermSurf.app` should appear (plus "TermSurf Debug" if the
   debug build is present — but never the release build tree copy).

**Result: Pass.** Debug build produces `TermSurf Debug.app`, Spotlight shows it
as a separate entry from the installed release. The `build-debug.sh` script was
also updated to reference the new app name.

## Conclusion

Debug and release builds are now visually distinct in Spotlight. The debug app
bundle is named "TermSurf Debug" (`com.termsurf.debug`) while the release and
installed copies remain "TermSurf" (`com.termsurf`). Pressing F4 and typing
"termsurf" no longer risks launching the wrong build.

Three changes made it work:

1. **Xcode project** — Debug config's `PRODUCT_NAME` changed from `TermSurf` to
   `"TermSurf Debug"`, producing `TermSurf Debug.app` in the build directory.
2. **Zig build system** — `GhosttyXcodebuild.zig` now selects the correct app
   name per build mode so `zig build` and `zig build run` find the right bundle.
3. **Install script** — `lsregister -u` unregisters build tree copies from
   Launch Services after installation, ensuring Spotlight only finds the
   `/Applications/` copy.
