+++
status = "closed"
opened = "2026-03-10"
closed = "2026-03-10"
+++

# Issue 735: Ghostboard app icons

## Goal

Use `ghostboard-1.png` as the release icon and `ghostboard-1-debug.png` as the
debug icon for Ghostboard.

## Background

The current release icon is generated from `assets/termsurf-2-black-3.png` via
`scripts/generate-icons.sh`. The script runs `sips` to resize the source image
into all required sizes for the `AppIcon.appiconset` and `AppIconImage.imageset`
asset catalogs.

A new icon, `assets/ghostboard-1.png`, has been created for Ghostboard. It
should replace the current icon in release builds. A debug variant
(`ghostboard-1-debug.png`) also exists in `assets/`.

### Icon pipeline

1. `scripts/generate-icons.sh [source]` takes a source PNG and generates:
   - `ghostboard/macos/Assets.xcassets/AppIcon.appiconset/icon-{16,32,64,128,256,512,1024}.png`
   - `ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-{256px-128pt@2x,512px,1024px}.png`
2. The script defaults to `assets/termsurf-2-black-3.png` when no argument is
   given.

### Debug icon

The debug icon is a single PNG at
`ghostboard/macos/Assets.xcassets/TermSurfDebugIcon.imageset/termsurf-debug-icon.png`.
In debug builds, `AppDelegate.swift` sets the dock icon at runtime via
`NSImage(named: "TermSurfDebugIcon")`. The new debug source is
`assets/ghostboard-1-debug.png`.

### What needs to change

- Update `generate-icons.sh` to default to `assets/ghostboard-1.png` instead of
  `assets/termsurf-2-black-3.png`.
- Regenerate all release icon assets from the new source.
- Replace the debug icon PNG in `TermSurfDebugIcon.imageset/` with
  `ghostboard-1-debug.png`.

## Experiments

### Experiment 1: Update icons and generation default

#### Description

Change the default source image in `generate-icons.sh`, regenerate all release
icon assets, and replace the debug icon PNG. Three files change, plus the
regenerated icon assets.

#### Changes

**1. `scripts/generate-icons.sh`**

Change the default on line 14 from:

```bash
PROD_SOURCE="${1:-$REPO_ROOT/assets/termsurf-2-black-3.png}"
```

to:

```bash
PROD_SOURCE="${1:-$REPO_ROOT/assets/ghostboard-1.png}"
```

**2. Run `scripts/generate-icons.sh`**

Regenerates all icon sizes in:

- `ghostboard/macos/Assets.xcassets/AppIcon.appiconset/icon-{16,32,64,128,256,512,1024}.png`
- `ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-{256px-128pt@2x,512px,1024px}.png`

**3. Replace debug icon**

Copy `assets/ghostboard-1-debug.png` to
`ghostboard/macos/Assets.xcassets/TermSurfDebugIcon.imageset/termsurf-debug-icon.png`,
overwriting the existing file. The filename stays the same so `Contents.json`
needs no change.

#### Verification

1. `scripts/generate-icons.sh` runs without errors and produces 7 + 3 icon
   files.
2. `file ghostboard/macos/Assets.xcassets/AppIcon.appiconset/icon-1024.png`
   confirms it is a valid PNG.
3. The debug icon file at `TermSurfDebugIcon.imageset/termsurf-debug-icon.png`
   matches `assets/ghostboard-1-debug.png` (compare with `cmp`).

**Result:** Partial

The debug icon updated correctly — the orange/green ghost appears in the Dock
during debug builds. The release icon did not update visually despite the files
on disk changing. All three verification checks passed (script ran clean, PNGs
are valid, debug icon matches source), but the built app still shows the old
surfer-boy release icon.

The release icon appears briefly on launch before `AppDelegate.swift` swaps it
to the debug icon at runtime. This confirms the release icon (from
`AppIcon.appiconset`) is still the old image in the compiled app.

The icon files on disk are correct — `icon-1024.png` contains the new blue ghost
from `ghostboard-1.png`. The problem is that the Zig/Xcode build did not pick up
the changed asset catalog PNGs. Likely causes:

1. **macOS icon cache.** macOS caches app icons aggressively in
   `/Library/Caches/com.apple.iconservices.store`. Even after rebuilding, the OS
   may serve the old icon until the cache is cleared
   (`sudo rm -rf /Library/Caches/com.apple.iconservices.store && killall Dock`).
2. **Xcode DerivedData cache.** If the asset catalog was previously compiled,
   the build system may skip recompiling it because the `Contents.json` didn't
   change — only the PNGs did. A clean build (`scripts/clean-zig.sh` or deleting
   DerivedData) would force recompilation.
3. **Zig build system.** The Zig build may copy the asset catalog at configure
   time and not track changes to individual PNGs within it.

The debug icon works because it bypasses the asset catalog entirely —
`AppDelegate.swift` loads the `TermSurfDebugIcon` image from
`AppIconImage.imageset` at runtime via `NSImage(named:)`, which reads the
current file on disk rather than a compiled `.car` archive.

#### Conclusion

The file-level changes are correct. The next experiment needs to address the
build/cache layer so the release icon actually appears in the running app. This
likely means clearing the macOS icon cache and/or performing a clean build to
force the asset catalog to recompile.

### Experiment 2: Clear macOS icon cache

#### Description

The release icon files were correct on disk after Experiment 1, but macOS served
the old cached icon. Clear the icon services cache and restart the Dock to force
macOS to re-read the icon from the compiled app.

#### Changes

No code changes. Run:

```bash
sudo rm -rf /Library/Caches/com.apple.iconservices.store && killall Dock
```

#### Verification

1. Launch Ghostboard. The release icon (blue ghost) appears in the Dock
   immediately — no flash of the old surfer-boy icon.
2. In debug mode, the debug icon (orange/green ghost) still appears after the
   runtime swap.

**Result:** Pass

After clearing `/Library/Caches/com.apple.iconservices.store` and restarting the
Dock, the new blue ghost release icon appears immediately. The debug icon
continues to work as before. Both icons now display correctly.

#### Conclusion

The build system compiled the new icons correctly in Experiment 1. The only
problem was the macOS icon services cache, which aggressively caches app icons
and does not invalidate when an app is rebuilt with new assets. Clearing the
cache resolved the issue.

### Experiment 3: Update debug icon with new design

#### Description

The debug icon source (`assets/ghostboard-1-debug.png`) has been updated — it
now uses the same blue ghost as the release icon but with a golden "DEBUG" label
beneath it. Copy the new source into the asset catalog so the running app picks
it up.

#### Changes

**1. Replace debug icon in asset catalog**

Copy `assets/ghostboard-1-debug.png` →
`ghostboard/macos/Assets.xcassets/TermSurfDebugIcon.imageset/termsurf-debug-icon.png`

**2. Clear macOS icon cache**

```bash
sudo rm -rf /Library/Caches/com.apple.iconservices.store && killall Dock
```

#### Verification

1. `cmp assets/ghostboard-1-debug.png ghostboard/macos/Assets.xcassets/TermSurfDebugIcon.imageset/termsurf-debug-icon.png`
   shows files match.
2. Launch Ghostboard in debug mode. The Dock icon shows the blue ghost with
   golden "DEBUG" text.

**Result:** Pass

The debug icon in the asset catalog now matches the updated source. The Dock
shows the blue ghost with golden "DEBUG" label in debug builds.

#### Conclusion

Simple asset replacement — the new debug icon design is live.

## Conclusion

All three icon assets are now using the new Ghostboard designs. The release icon
(blue ghost) was updated in Experiment 1, the macOS icon cache issue was resolved
in Experiment 2, and the debug icon (blue ghost with golden "DEBUG" label) was
updated in Experiment 3. Both release and debug builds display the correct icons.
