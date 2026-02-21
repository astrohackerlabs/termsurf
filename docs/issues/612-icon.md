# Issue 612: App icon

## Goal

The app icon in the dock, Finder, and app switcher shows the TermSurf surfing
ghost icon for both release and debug builds. Release shows a cyan wave, debug
shows a green wave.

## Background

Issue 610 attempted to replace the Ghostty icon but failed because macOS Launch
Services caches icons by bundle identifier. Our app shared
`com.mitchellh.ghostty` with the official Ghostty installation, so macOS always
served Ghostty's cached icon regardless of what was in our app bundle.

Issue 611 resolved the blocker by changing the bundle identifier to
`com.termsurf`. macOS now treats TermSurf as a distinct app with no cached icon.

### Issue 610's experiments

Five experiments explored icon replacement:

1. **Replaced `AppIconImage.imageset` PNGs** — Partial. Debug override worked
   (dock), but `AppIconImage.imageset` is not the bundle icon source.
2. **Replaced `Ghostty.icon` with minimal Icon Composer doc** — Failed. Minimal
   `icon.json` produced a degraded 256x256 `.icns`.
3. **Swapped ghost layer in original Icon Composer doc** — The `.icns` was
   correct (verified with `iconutil`), but macOS served a cached icon.
4. **Clean build with cache clearing** — Same result. Bundle ID collision.
5. **Release build** — Same result. `Assets.car` also cached.

All experiments failed due to the bundle ID collision, not the icon changes
themselves. Experiment 3's approach produced a correct `.icns`, but the Icon
Composer format is complex and fragile. None of those changes were committed.

### ts1's approach

ts1 solved this problem simply. Instead of using Ghostty's Icon Composer
(`.icon`) format, ts1 uses a traditional `AppIcon.appiconset/` with pre-rendered
PNGs at standard macOS sizes:

| File            | Pixels    | Used for             |
| --------------- | --------- | -------------------- |
| `icon-16.png`   | 16x16     | 16pt @1x             |
| `icon-32.png`   | 32x32     | 16pt @2x, 32pt @1x   |
| `icon-64.png`   | 64x64     | 32pt @2x             |
| `icon-128.png`  | 128x128   | 128pt @1x            |
| `icon-256.png`  | 256x256   | 128pt @2x, 256pt @1x |
| `icon-512.png`  | 512x512   | 256pt @2x, 512pt @1x |
| `icon-1024.png` | 1024x1024 | 512pt @2x            |

The Xcode project references this appiconset via
`ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon`. This bypasses Icon Composer
entirely — `actool` compiles the PNGs directly into `Assets.car`.

For debug builds, ts1 uses a runtime override in `AppDelegate.swift`:

```swift
#if DEBUG
  if appIcon == nil {
    NSApplication.shared.applicationIconImage = NSImage(named: "TermSurfDebugIcon")
  }
#endif
```

This sets the dock icon at runtime without modifying the app bundle (preserving
code signing). The `TermSurfDebugIcon` is a regular imageset containing only the
debug icon PNG.

### What needs to change in Ghost

Ghost currently uses `ASSETCATALOG_COMPILER_APPICON_NAME = Ghostty`, which
points to `ghost/images/Ghostty.icon/` — an Icon Composer bundle. This needs to
change to a traditional `AppIcon.appiconset/` using the ts1 icon files.

**Source images** (already in ts1):

- `ts1/termsurf-macos/icon-source/termsurf-icon.png` — Release icon (cyan wave)
- `ts1/termsurf-macos/icon-source/termsurf-debug-icon.png` — Debug icon (green
  wave)

**Pre-rendered sizes** (already in ts1):

- `ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset/icon-*.png` — 7 sizes
  from 16px to 1024px

These exact files will be copied into Ghost's asset catalog. No image generation
or resizing needed.

### Key files to change

1. **`ghost/macos/Assets.xcassets/AppIcon.appiconset/`** — New directory. Copy
   all PNGs and `Contents.json` from ts1.
2. **`ghost/macos/Assets.xcassets/TermSurfDebugIcon.imageset/`** — New
   directory. Copy from ts1.
3. **`ghost/macos/Ghostty.xcodeproj/project.pbxproj`** — Change
   `ASSETCATALOG_COMPILER_APPICON_NAME` from `Ghostty` to `AppIcon`.
4. **`ghost/macos/Sources/App/macOS/AppDelegate.swift`** — Change debug icon
   from `"BlueprintImage"` to `"TermSurfDebugIcon"`.
5. **`ghost/macos/Assets.xcassets/AppIconImage.imageset/`** — Replace PNGs with
   TermSurf icon (used by runtime icon-switching system, not the bundle icon).

## Experiments

### Experiment 1: Replace icon using ts1 approach

#### Goal

`cd ghost && zig build` produces `TermSurf.app` with the TermSurf surfing ghost
icon (cyan wave) in Finder and the dock. Debug builds show the green wave debug
icon in the dock.

#### Approach

Copy ts1's icon files into Ghost's asset catalog and switch the Xcode project
from Icon Composer (`Ghostty.icon`) to a traditional `AppIcon.appiconset/`. This
is the same approach ts1 uses, with the same image files.

#### Steps

##### Step 1: Copy `AppIcon.appiconset/` from ts1

Copy the entire appiconset directory (7 PNGs + `Contents.json`) from ts1 into
Ghost's asset catalog:

```bash
cp -R ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset/ \
      ghost/macos/Assets.xcassets/AppIcon.appiconset/
```

This creates `ghost/macos/Assets.xcassets/AppIcon.appiconset/` with pre-rendered
icons at all standard macOS sizes (16px–1024px).

##### Step 2: Copy `TermSurfDebugIcon.imageset/` from ts1

```bash
cp -R ts1/termsurf-macos/Assets.xcassets/TermSurfDebugIcon.imageset/ \
      ghost/macos/Assets.xcassets/TermSurfDebugIcon.imageset/
```

This adds the green wave debug icon as a named imageset.

##### Step 3: Change `ASSETCATALOG_COMPILER_APPICON_NAME` in `project.pbxproj`

In `ghost/macos/Ghostty.xcodeproj/project.pbxproj`, change from `Ghostty` to
`AppIcon` in the 3 macOS build configurations only:

- Line 596 (ReleaseLocal)
- Line 905 (Debug)
- Line 960 (Release)

Do NOT change the 3 iOS configurations (lines 1014, 1053, 1092) — not our
platform.

##### Step 4: Change debug icon in `AppDelegate.swift`

In `ghost/macos/Sources/App/macOS/AppDelegate.swift`, line 1003:

```
"BlueprintImage" → "TermSurfDebugIcon"
```

This changes the `#if DEBUG` runtime override to use the green wave icon instead
of Ghostty's blueprint icon.

##### Step 5: Replace `AppIconImage.imageset/` PNGs

Replace the 3 PNGs in `ghost/macos/Assets.xcassets/AppIconImage.imageset/` with
TermSurf icon versions. Copy from the appiconset (already the right sizes):

```bash
cp ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset/icon-256.png \
   ghost/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-256px-128pt@2x.png

cp ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset/icon-512.png \
   ghost/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-512px.png

cp ts1/termsurf-macos/Assets.xcassets/AppIcon.appiconset/icon-1024.png \
   ghost/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-1024px.png
```

No changes to `Contents.json` — filenames are preserved.

##### Step 6: Build and verify

```bash
rm -rf ~/Library/Developer/Xcode/DerivedData/Ghostty-*
rm -rf ghost/macos/build/
cd ghost && zig build
```

#### Verification

1. **App name:** `ls ghost/zig-out/` shows `TermSurf.app`
2. **Finder icon:** `ghost/zig-out/TermSurf.app` shows the TermSurf surfing
   ghost icon (cyan wave on CRT) in Finder
3. **Dock icon:** Launch the app — the surfing ghost icon appears in the dock.
   In a debug build, the green wave debug icon appears (runtime override).
4. **App switcher (Cmd+Tab):** Shows the surfing ghost icon
5. **No Ghostty icon:** The old blue rounded-square Ghostty icon does not appear
   anywhere
