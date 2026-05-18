+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 671: Update App Icon

Update the app icon to the new source image and add a clean-zig script to
simplify cache clearing.

## Background

Issue 612 established the icon pipeline: a source image in `assets/`, a
`generate-icons.sh` script, and pre-rendered PNGs in the asset catalog. Changing
the release icon is a one-command operation: `gui/scripts/generate-icons.sh`.

The source image has been updated (`assets/termsurf-2-black-2.png`), but the
generated icon PNGs haven't been regenerated yet. Additionally, rebuilding the
icon requires clearing the zig build cache (stale `zig-out/` retains old icons
via `cp -R`), and there's no script for that. You have to remember three
separate `rm -rf` commands and be careful not to nuke the Chromium build in
`chromium/src/out/`.

## Goals

1. Create `gui/scripts/clean-zig.sh` — clears zig build artifacts without
   touching the Chromium cache.
2. Run `gui/scripts/generate-icons.sh` to regenerate all icon sizes from the
   updated source image.
3. Rebuild with a clean zig cache and verify the new icon appears.

## Experiment 1: Clean-zig script and icon regeneration

### Hypothesis

A `clean-zig.sh` script that removes `gui/zig-out/`, `gui/.zig-cache/`,
`gui/macos/build/`, and the Xcode DerivedData cache will ensure clean builds
without affecting the Chromium build at `chromium/src/out/`. Running
`generate-icons.sh` followed by a clean build will produce the updated icon.

### Changes

#### 1. Create `gui/scripts/clean-zig.sh`

```bash
#!/bin/bash
# Clean zig build artifacts without touching the Chromium cache.
# Usage: ./scripts/clean-zig.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUI_DIR="$(dirname "$SCRIPT_DIR")"

echo "Cleaning zig build artifacts..."
rm -rf "$GUI_DIR/zig-out/"
rm -rf "$GUI_DIR/.zig-cache/"
rm -rf "$GUI_DIR/macos/build/"
rm -rf ~/Library/Developer/Xcode/DerivedData/Ghostty-*

echo "Done. Chromium cache at chromium/src/out/ is untouched."
```

#### 2. Run `gui/scripts/generate-icons.sh`

Regenerate all icon sizes from the updated `assets/termsurf-2-black-2.png`.

#### 3. Clean and rebuild

```bash
gui/scripts/clean-zig.sh
cd gui && zig build
```

### Result: PASS

All steps completed. `clean-zig.sh` created and runs without errors.
`generate-icons.sh` updated to accept an optional filename argument. Icons
regenerated from `assets/termsurf-2-black-2.png`. Clean build succeeded. New
icon appears in Finder and dock.

## Experiment 2: Update icon to new source image

### Hypothesis

Running the icon pipeline with `assets/termsurf-2-black-3.png` and a clean
rebuild will update the app icon. Also update `generate-icons.sh` to default to
the new source image.

### Changes

#### 1. Update default source in `gui/scripts/generate-icons.sh`

Change the default from `termsurf-2-black-2.png` to `termsurf-2-black-3.png`.

#### 2. Run the pipeline

```bash
gui/scripts/generate-icons.sh assets/termsurf-2-black-3.png
gui/scripts/clean-zig.sh
cd gui && zig build
```

### Result: PASS

Icons regenerated from `assets/termsurf-2-black-3.png`. Default updated in
`generate-icons.sh`. Clean build succeeded. New icon appears in Finder and dock.

## Conclusion

The icon pipeline is fully operational across two iterations:

1. **`gui/scripts/clean-zig.sh`** — clears `zig-out/`, `.zig-cache/`,
   `macos/build/`, and Xcode DerivedData without touching the Chromium cache.
2. **`gui/scripts/generate-icons.sh [source]`** — generates all 7 AppIcon sizes
   - 3 AppIconImage sizes from a source PNG. Defaults to
     `assets/termsurf-2-black-3.png`.

To update the icon in the future:

```bash
gui/scripts/generate-icons.sh path/to/new-icon.png
gui/scripts/clean-zig.sh
cd gui && zig build
```
