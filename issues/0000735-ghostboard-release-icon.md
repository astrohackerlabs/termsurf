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
