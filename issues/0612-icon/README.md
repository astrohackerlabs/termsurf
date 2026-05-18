+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

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

**Result:** Partial

The debug icon works — the green wave surfer appears in the dock via the
`#if DEBUG` runtime override. But the release/bundle icon still shows the old
Ghostty icon, which flashes briefly before the debug override replaces it.

#### Conclusion

The `AppIcon.appiconset`, `TermSurfDebugIcon.imageset`, `AppIconImage.imageset`
replacements, and the `AppDelegate.swift` debug override all work correctly. The
remaining problem is the bundle icon.

`Ghostty.icon` (the Icon Composer document at `ghost/images/Ghostty.icon/`) is
still referenced in `project.pbxproj` as a direct resource. Xcode compiles it
into `Ghostty.icns` in the app bundle's `Contents/Resources/` regardless of what
`ASSETCATALOG_COMPILER_APPICON_NAME` points to. The generated `Info.plist` still
has `CFBundleIconName = Ghostty` (matching the Icon Composer document), so macOS
reads the old Ghostty icon from `Assets.car` or the compiled `.icns` instead of
the new `AppIcon` appiconset.

The next experiment should remove the `Ghostty.icon` reference from the Xcode
project so it stops being compiled into the app bundle. With only
`AppIcon.appiconset` remaining, `ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon`
should control the bundle icon.

### Experiment 2: Remove Ghostty.icon from Xcode project

#### Goal

The bundle icon shows the TermSurf surfing ghost (cyan wave). No old Ghostty
icon flash before the debug override.

#### Approach

Remove all references to `Ghostty.icon` from `project.pbxproj`. The file stays
on disk at `ghost/images/Ghostty.icon/` (upstream Ghostty asset), but the Xcode
project no longer includes it as a build resource. With the Icon Composer
document gone from the build, `ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon`
becomes the sole icon source.

#### Steps

##### Step 1: Remove `Ghostty.icon` from `project.pbxproj`

Remove these 6 lines from `ghost/macos/Ghostty.xcodeproj/project.pbxproj`:

**PBXBuildFile entries (lines 18–19):**

```
A553F4132E06EB1600257779 /* Ghostty.icon in Resources */ = {isa = PBXBuildFile; ...};
A553F4142E06EB1600257779 /* Ghostty.icon in Resources */ = {isa = PBXBuildFile; ...};
```

**PBXFileReference (line 57):**

```
A553F4122E06EB1600257779 /* Ghostty.icon */ = {isa = PBXFileReference; ...};
```

**PBXGroup entry (line 272):**

```
A553F4122E06EB1600257779 /* Ghostty.icon */,
```

**PBXResourcesBuildPhase entries (lines 468, 487):**

```
A553F4142E06EB1600257779 /* Ghostty.icon in Resources */,
A553F4132E06EB1600257779 /* Ghostty.icon in Resources */,
```

##### Step 2: Build and verify

```bash
rm -rf ~/Library/Developer/Xcode/DerivedData/Ghostty-*
rm -rf ghost/macos/build/
cd ghost && zig build
```

#### Verification

1. **Build succeeds:** No Xcode errors about missing `Ghostty.icon`
2. **Bundle icon:** `ghost/zig-out/TermSurf.app` shows the TermSurf surfing
   ghost (cyan wave) in Finder — not the old Ghostty icon
3. **Dock icon (debug):** The green wave debug icon appears without any old
   Ghostty icon flash before it
4. **No `Ghostty.icns` in bundle:**
   `ls ghost/zig-out/TermSurf.app/Contents/Resources/` does not contain
   `Ghostty.icns`

**Result:** Pass

The fresh build at `ghost/macos/build/Debug/TermSurf.app` has the correct icon.
The bundle contains `AppIcon.icns` (TermSurf surfing ghost) with no
`Ghostty.icns`. The dock shows the green wave debug icon with no old Ghostty
icon flash. Finder shows the cyan wave release icon.

The `zig-out/` copy initially appeared broken because it retained a stale
`Ghostty.icns` from a previous build — `GhosttyXcodebuild.zig` uses `cp -R`
which overlays without deleting old files. Deleting `zig-out/` before building
resolves this.

#### Conclusion

Removing the 6 `Ghostty.icon` references from `project.pbxproj` was the fix.
With the Icon Composer document gone from the build, `actool` compiles only the
`AppIcon.appiconset` into `Assets.car` and generates `AppIcon.icns`. The
generated `Info.plist` has `CFBundleIconName = AppIcon` and
`CFBundleIconFile = AppIcon`, so macOS reads the TermSurf icon correctly.

The `zig-out/` stale file issue is a pre-existing quirk of the build system's
`cp -R` step — not specific to this change. Cleaning `zig-out/` before building
avoids it.

### Experiment 3: Generate icons from new source image

#### Goal

The release icon shows `assets/termsurf-2-black.png` at all sizes. The debug
icon remains unchanged (green wave).

#### Approach

Copy the `generate-icons.sh` script from ts1 into Ghost, adapt it for Ghost's
directory structure, and run it with `assets/termsurf-2-black.png` as the source
image. This generates all 7 icon sizes in `AppIcon.appiconset/` and replaces the
3 PNGs in `AppIconImage.imageset/`. Then rebuild and verify the new icon loads.

#### Steps

##### Step 1: Copy and adapt `generate-icons.sh`

Copy `ts1/scripts/generate-icons.sh` to `ghost/scripts/generate-icons.sh` and
update paths:

- Source image: `assets/termsurf-2-black.png` (relative to repo root)
- `AppIcon.appiconset`: `ghost/macos/Assets.xcassets/AppIcon.appiconset/`
- `AppIconImage.imageset`: `ghost/macos/Assets.xcassets/AppIconImage.imageset/`
- No debug icon changes — only generate release icon sizes

The script should also update the 3 PNGs in `AppIconImage.imageset/` (used by
the runtime icon-switching system) by copying the 256, 512, and 1024 sizes with
the expected filenames.

##### Step 2: Run the script

```bash
chmod +x ghost/scripts/generate-icons.sh
ghost/scripts/generate-icons.sh
```

##### Step 3: Build and verify

```bash
rm -rf ghost/zig-out/
rm -rf ~/Library/Developer/Xcode/DerivedData/Ghostty-*
rm -rf ghost/macos/build/
cd ghost && zig build
```

#### Verification

1. **Build succeeds**
2. **Finder icon:** `ghost/macos/build/Debug/TermSurf.app` shows the
   `termsurf-2-black.png` icon — visually distinct from the previous cyan wave
   surfer
3. **Dock icon (debug):** The green wave debug icon still appears (unchanged)
4. **No old icon:** Neither the Ghostty icon nor the previous cyan wave surfer
   appears as the release icon

**Result:** Pass

The `generate-icons.sh` script produced all 7 sizes from `termsurf-2-black.png`.
The release icon shows the new image in Finder and the dock. The debug icon
remains the green wave (unchanged). No old icons appear.

#### Conclusion

The icon generation pipeline works end-to-end: source image in `assets/`, script
in `ghost/scripts/`, generated PNGs in the asset catalog, correct icon in the
built app. Changing the release icon is now a one-command operation:
`ghost/scripts/generate-icons.sh`.

## Conclusion

TermSurf has its own icon and a pipeline to change it. The release icon comes
from a traditional `AppIcon.appiconset/` with pre-rendered PNGs at all standard
macOS sizes (16–1024px). Debug builds show a distinct green wave icon in the
dock via a runtime `#if DEBUG` override. The Icon Composer document
(`Ghostty.icon`) is no longer part of the build.

To change the release icon: replace `assets/termsurf-2-black.png` with a new
1024x1024 source image and run `ghost/scripts/generate-icons.sh`.

Key changes:

- `ghost/macos/Assets.xcassets/AppIcon.appiconset/` — Release icon at 7 sizes
- `ghost/macos/Assets.xcassets/TermSurfDebugIcon.imageset/` — Debug dock icon
- `ghost/macos/Assets.xcassets/AppIconImage.imageset/` — Runtime icon-switching
- `ghost/macos/Ghostty.xcodeproj/project.pbxproj` —
  `ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon`, `Ghostty.icon` removed
- `ghost/macos/Sources/App/macOS/AppDelegate.swift` — Debug icon override
- `ghost/scripts/generate-icons.sh` — Icon generation from source image
