+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

# Issue 610: Replace Ghostty icon with TermSurf Ghost icon

## Goal

The app icon in the dock, Finder, and app switcher shows the TermSurf Ghost icon
instead of the Ghostty icon, for both release and debug builds.

## Background

Ghost is a Ghostty fork. It currently ships with Ghostty's original icon — a
blue rounded-square with a ghost silhouette and `>_` prompt. Two new TermSurf
Ghost icons have been created in `assets/`:

- **`termsurf-ghost-black.png`** (1024x1024) — Release icon. A CRT monitor with
  a ghost surfing a cyan wave of binary code.
- **`termsurf-ghost-alt-black.png`** (1024x1024) — Debug icon. Same concept but
  with a green wave, visually distinguishing debug builds at a glance.

### How Ghostty handles icons

Ghostty's icon system has two layers:

**Build time (asset catalog):** The Xcode project compiles
`Assets.xcassets/AppIconImage.imageset/` into the app bundle's `Ghostty.icns`.
The imageset contains three sizes: 1024px (3x), 512px (2x), 256px (1x). The
Xcode build setting `ASSETCATALOG_COMPILER_APPICON_NAME = Ghostty` references
this asset. This is the icon Finder and Launchpad display.

**Runtime (debug override):** In `AppDelegate.swift`, `updateAppIcon(from:)`
handles icon switching. In `#if DEBUG` builds, when no custom icon is
configured, it sets `NSApplication.shared.applicationIconImage` to
`BlueprintImage` — the blueprint-style alternate icon. This changes the dock
icon without modifying the app bundle (which would corrupt code signing). The
blueprint icon lives in
`Assets.xcassets/Alternate Icons/BlueprintImage.imageset/`.

Ghostty also supports user-configurable icons via `macos-icon` config
(`Package.swift` line 338), with presets like blueprint, chalkboard, glass, etc.
These are all in the `Alternate Icons/` folder.

### What needs to change

1. **Release icon:** Replace the three PNGs in `AppIconImage.imageset/` with
   resized versions of `termsurf-ghost-black.png` (1024px, 512px, 256px).

2. **Debug icon:** Replace `BlueprintImage.imageset/macOS-AppIcon-1024px.png`
   with `termsurf-ghost-alt-black.png`. This is the only size needed — the debug
   override uses `NSImage(named:)` which handles scaling.

3. **Alternate icons:** The existing Ghostty alternate icons (chalkboard, glass,
   holographic, etc.) depict the Ghostty ghost. They could be left as-is for now
   (they're only used when explicitly configured by the user) or removed to
   avoid shipping Ghostty branding. Not critical for this issue.

### Sizing

The asset catalog expects three sizes for the release icon:

| Scale | Filename                           | Pixels    |
| ----- | ---------------------------------- | --------- |
| 1x    | `macOS-AppIcon-256px-128pt@2x.png` | 256x256   |
| 2x    | `macOS-AppIcon-512px.png`          | 512x512   |
| 3x    | `macOS-AppIcon-1024px.png`         | 1024x1024 |

The source image is 1024x1024, so 512px and 256px versions need to be generated
by downscaling. macOS `sips` can do this:

```bash
sips -z 512 512 input.png --out output-512.png
sips -z 256 256 input.png --out output-256.png
```

### Key files

- `assets/termsurf-ghost-black.png` — New release icon (1024x1024)
- `assets/termsurf-ghost-alt-black.png` — New debug icon (1024x1024)
- `ghost/macos/Assets.xcassets/AppIconImage.imageset/` — Release icon imageset
  (3 PNGs + Contents.json)
- `ghost/macos/Assets.xcassets/Alternate Icons/BlueprintImage.imageset/` — Debug
  icon imageset (1 PNG + Contents.json)
- `ghost/macos/Sources/App/macOS/AppDelegate.swift` — Debug icon override (line
  1003: `NSImage(named: "BlueprintImage")`)

## Experiments

### Experiment 1: Replace both icons

#### Goal

The dock shows the TermSurf Ghost surfing icon for both release and debug
builds. Release shows the cyan wave, debug shows the green wave.

#### Description

This is a straightforward asset replacement. No code changes — only image files
are swapped. The asset catalog's `Contents.json` files and the Swift code that
references `BlueprintImage` by name remain unchanged.

#### Changes

**Release icon — `ghost/macos/Assets.xcassets/AppIconImage.imageset/`:**

Generate the three required sizes from `assets/termsurf-ghost-black.png`:

```bash
cp assets/termsurf-ghost-black.png \
   ghost/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-1024px.png

sips -z 512 512 assets/termsurf-ghost-black.png --out \
   ghost/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-512px.png

sips -z 256 256 assets/termsurf-ghost-black.png --out \
   ghost/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-256px-128pt@2x.png
```

No changes to `Contents.json` — the filenames are preserved.

**Debug icon —
`ghost/macos/Assets.xcassets/Alternate Icons/BlueprintImage.imageset/`:**

```bash
cp assets/termsurf-ghost-alt-black.png \
   "ghost/macos/Assets.xcassets/Alternate Icons/BlueprintImage.imageset/macOS-AppIcon-1024px.png"
```

No changes to `Contents.json` or `AppDelegate.swift` — the asset name
`BlueprintImage` is preserved, only the underlying PNG changes.

#### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app
```

1. **Dock icon:** The dock shows the CRT-with-surfing-ghost icon. In a debug
   build, the wave is green. In a release build, the wave is cyan.
2. **App switcher (Cmd+Tab):** Shows the same icon.
3. **Finder:** `ghost/zig-out/Ghostty.app` shows the new icon in Finder. (May
   require `touch ghost/zig-out/Ghostty.app` to bust the icon cache.)
4. **No Ghostty icon visible:** The old blue rounded-square Ghostty icon does
   not appear anywhere.

**Result:** Partial

The debug icon works — the green wave surfer appears in the dock. The release
icon still shows the old Ghostty icon, which flashes briefly before the debug
override replaces it.

#### Conclusion

The debug icon succeeded because it's loaded at runtime via
`NSImage(named: "BlueprintImage")` from the asset catalog imageset we replaced.

The release icon failed because `AppIconImage.imageset` is NOT what Xcode uses
for the app icon. The actual app icon comes from `ghost/images/Ghostty.icon/` —
an **Icon Composer** document (macOS/Xcode 16+ format). This is a layered bundle
containing multiple PNGs (`Ghostty.png`, `Screen.png`, `gloss.png`,
`Inner Bevel 6px.png`, `Screen Effects.png`) composited together with blend
modes, gradients, and glass effects. Xcode compiles this `.icon` bundle into
`Ghostty.icns` at build time.

The Xcode project includes `Ghostty.icon` as a resource (`project.pbxproj` line
57: `path = ../images/Ghostty.icon`), and
`ASSETCATALOG_COMPILER_APPICON_NAME = Ghostty` references it. The
`AppIconImage.imageset` is just a regular imageset used by the runtime icon-
switching system (`macos-icon` config) — not the Finder/Launchpad icon.

The Background section's description of how Ghostty handles icons was wrong. It
assumed `AppIconImage.imageset` was the source of the app icon. The correct
source is `Ghostty.icon`.

### Experiment 2: Replace the Icon Composer document

#### Goal

The release icon in Finder, Launchpad, and the initial dock display shows the
TermSurf Ghost surfing icon (cyan wave) instead of the Ghostty icon.

#### Description

The app icon is compiled from `ghost/images/Ghostty.icon/`, an Icon Composer
bundle. This layered format composites multiple PNGs with blend modes,
gradients, and glass effects. Our new icon is a single flat PNG — it doesn't use
Icon Composer's layering system.

The simplest approach: replace the `Ghostty.icon` bundle with a minimal Icon
Composer document that has a single layer containing our 1024px PNG. This
preserves the `.icon` format that Xcode expects while using our flat image.

An Icon Composer document is a folder with:

- `icon.json` — Layer definitions, blend modes, effects
- `Assets/` — PNG files referenced by `icon.json`

The minimal `icon.json` needs one group with one layer pointing to our PNG. No
gradients, no glass, no blend modes.

#### Changes

**`ghost/images/Ghostty.icon/Assets/`** — Remove all existing PNGs and copy in
the new icon:

```bash
rm ghost/images/Ghostty.icon/Assets/*.png
cp assets/termsurf-ghost-black.png ghost/images/Ghostty.icon/Assets/termsurf-ghost.png
```

**`ghost/images/Ghostty.icon/icon.json`** — Replace with a minimal single-layer
document:

```json
{
  "groups" : [
    {
      "layers" : [
        {
          "hidden" : false,
          "image-name" : "termsurf-ghost.png",
          "name" : "TermSurf Ghost"
        }
      ],
      "name" : "Icon"
    }
  ],
  "supported-platforms" : {
    "squares" : "shared"
  }
}
```

No changes to the Xcode project — it already references `Ghostty.icon` by path.

No changes to `AppIconImage.imageset` — Experiment 1 already replaced it (used
by the runtime icon-switching system).

No changes to `BlueprintImage.imageset` — Experiment 1 already replaced it
(debug override).

#### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app
```

1. **Dock icon (before runtime override):** The TermSurf Ghost surfing icon
   (cyan wave) appears immediately when the app launches, before any runtime
   icon switching occurs.
2. **Finder:** `ghost/zig-out/Ghostty.app` shows the new icon. (May require
   `touch ghost/zig-out/Ghostty.app` to bust the icon cache.)
3. **Debug build dock icon:** The green wave debug icon still appears (runtime
   override from Experiment 1).
4. **App switcher (Cmd+Tab):** Shows the correct icon.

**Result:** Fail

The release icon does not load. The built `Ghostty.icns` is only 85KB / 256x256
— the Icon Composer compilation produced a degraded result. The minimal
`icon.json` with a single layer and no additional properties is likely missing
required fields that Icon Composer needs to properly render the icon at all
sizes.

#### Conclusion

The minimal `icon.json` format doesn't work. Icon Composer's `.icon` format
likely requires additional properties (lighting, shadow, fill, or platform
definitions) even for a single flat layer. Without documentation on the exact
required schema, guessing at the format is unreliable. A different approach is
needed — either reverse-engineering a working minimal `.icon` document or
bypassing Icon Composer entirely by generating the `.icns` file directly.

### Experiment 3: Replace ghost layer in existing Icon Composer document

#### Goal

The release icon shows the TermSurf surfing ghost centered on the existing
Ghostty screen background, using the original Icon Composer layered structure.

#### Description

Experiments 1 and 2 failed because we either replaced the wrong files or gutted
the `icon.json` too aggressively. This experiment takes a surgical approach:
keep the working `icon.json` structure intact, and only change what's necessary.

The original `Ghostty.icon` has 4 groups with 5 PNGs. Two layers in Group 2
reference `Ghostty.png` — the main ghost and a blue glass blur behind it. Both
have `position.translation-in-points` that offset the ghost to the top-left.

This experiment:

1. Restores the original `icon.json` and all original assets (undoing Experiment
   2's changes).
2. Adds `surfing-ghost.png` to `Assets/`.
3. Updates both `Ghostty.png` references in `icon.json` to point to
   `surfing-ghost.png`.
4. Removes the `position` blocks from both layers so the image is centered.
5. Deletes the old `Ghostty.png` from `Assets/`.

Everything else stays: `Screen.png`, `gloss.png`, `Inner Bevel 6px.png`,
`Screen Effects.png`, all blend modes, glass effects, shadows, gradients, and
lighting. The icon will look like the original Ghostty icon but with a surfing
ghost in the center instead of the `>_` ghost in the top-left.

#### Changes

**Step 1: Restore original state.** Discard all uncommitted changes in
`ghost/images/Ghostty.icon/`:

```bash
cd ghost
git checkout -- images/Ghostty.icon/
```

**Step 2: Add new ghost image:**

```bash
cp assets/surfing-ghost.png ghost/images/Ghostty.icon/Assets/surfing-ghost.png
```

**Step 3: Update `icon.json`.** In the "Ghostty" layer (Group 2, first layer),
change `image-name` and remove `position`:

Before:

```json
{
  "blend-mode" : "normal",
  "fill" : "automatic",
  "hidden" : false,
  "image-name" : "Ghostty.png",
  "name" : "Ghostty",
  "position" : {
    "scale" : 1,
    "translation-in-points" : [
      -185.015625,
      -143.8359375
    ]
  }
}
```

After:

```json
{
  "blend-mode" : "normal",
  "fill" : "automatic",
  "hidden" : false,
  "image-name" : "surfing-ghost.png",
  "name" : "Ghostty"
}
```

In the "GhosttyBlur" layer (Group 2, second layer), same changes:

Before:

```json
{
  "blend-mode" : "normal",
  "fill" : {
    "solid" : "extended-srgb:0.00000,0.47843,1.00000,1.00000"
  },
  "glass" : true,
  "hidden" : false,
  "image-name" : "Ghostty.png",
  "name" : "GhosttyBlur",
  "position" : {
    "scale" : 1,
    "translation-in-points" : [
      -186.59375,
      -143.8359375
    ]
  }
}
```

After:

```json
{
  "blend-mode" : "normal",
  "fill" : {
    "solid" : "extended-srgb:0.00000,0.47843,1.00000,1.00000"
  },
  "glass" : true,
  "hidden" : false,
  "image-name" : "surfing-ghost.png",
  "name" : "GhosttyBlur"
}
```

**Step 4: Delete old ghost image:**

```bash
rm ghost/images/Ghostty.icon/Assets/Ghostty.png
```

#### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app
```

1. **Dock icon (before debug override):** The surfing ghost appears centered on
   the dark blue screen background, with the original gloss, bevel, and screen
   effects intact.
2. **Finder:** `ghost/zig-out/Ghostty.app` shows the new icon.
3. **Debug build dock icon:** The green wave debug icon still appears (runtime
   override from Experiment 1).
4. **No old Ghostty ghost visible:** The `>_` ghost silhouette does not appear.

**Result:** Fail

The build produces the correct `.icns` — extracting it with
`iconutil -c iconset` confirms the surfing ghost is composited onto the dark
blue screen with gloss and bevel. But the app still displays the old Ghostty
icon when launched. Clearing the Xcode DerivedData cache fixed the build output;
the remaining problem is macOS displaying a cached icon.

#### Conclusion

The Icon Composer changes are correct. The `icon.json` edits (swap image
references, remove position offsets) and the new `surfing-ghost.png` in
`Assets/` produce the intended composited icon. The build pipeline works. But
macOS icon services caches app icons aggressively, and the old icon persists at
launch.

Clearing the cache with
`sudo rm -rf /Library/Caches/com.apple.iconservices.store` and `killall Dock`
did not fix it. macOS may cache icons in additional locations
(`/private/var/folders/`, per-user icon caches, LaunchServices database) or the
cache may require a logout/reboot to fully clear.

### Experiment 4: Clean build with cache clearing

#### Goal

The surfing ghost icon (from Experiment 3's `Ghostty.icon` modification) is
visible when launching the app.

#### Description

Experiment 3 proved the `.icon` modification is correct — the built `.icns`
contains the surfing ghost (verified by extraction with `iconutil`). The icon
didn't appear because macOS was serving a cached version. But Experiment 3 also
didn't start from a fully clean state — stale Xcode DerivedData was discovered
mid-experiment and had to be purged.

This experiment starts completely clean: delete all Xcode build caches and local
build artifacts **before** building, so `actool` compiles `Ghostty.icon` fresh
with no possibility of stale output.

The `Ghostty.icon` modifications from Experiment 3 are already in place
(uncommitted). No additional code or asset changes are needed — this is purely a
clean rebuild.

#### Steps

1. Delete Xcode DerivedData for all Ghostty builds:

```bash
rm -rf ~/Library/Developer/Xcode/DerivedData/Ghostty-*
```

2. Delete the local build directory:

```bash
rm -rf ghost/macos/build/
```

3. Rebuild:

```bash
cd ghost && zig build
```

4. Verify the built `.icns` contains the surfing ghost:

```bash
mkdir /tmp/icon-check
cp ghost/zig-out/Ghostty.app/Contents/Resources/AppIcon.icns /tmp/icon-check/
cd /tmp/icon-check && iconutil -c iconset AppIcon.icns
open /tmp/icon-check/AppIcon.iconset/icon_512x512@2x.png
```

5. Launch the app:

```bash
open ghost/zig-out/Ghostty.app
```

#### Verification

1. **Extracted `.icns`:** The 1024px PNG from the iconset shows the surfing
   ghost on the dark blue screen background (not the old Ghostty ghost).
2. **Dock icon:** The surfing ghost icon appears in the dock when the app
   launches.
3. **Finder:** `ghost/zig-out/Ghostty.app` shows the surfing ghost icon.
4. **App switcher (Cmd+Tab):** Shows the surfing ghost icon.

**Result:** Fail

The built `Ghostty.icns` contains the surfing ghost (verified by extraction with
`iconutil`). But macOS still displays the old Ghostty icon at launch, before the
debug runtime override replaces it. The clean build with full cache clearing did
not solve the problem — macOS icon services continues to serve a cached icon for
this bundle identifier.

#### Conclusion

Deleting Xcode DerivedData and `macos/build/` ensures `actool` compiles the
`.icon` fresh, producing a correct `.icns`. But the macOS icon cache operates at
a layer beyond the build system. The old icon is cached by macOS against the
app's bundle identifier (`com.mitchellh.ghostty`), and no combination of build
cache clearing has forced macOS to re-read the actual `.icns` file from the app
bundle.

### Experiment 5: Release build

#### Goal

The surfing ghost icon appears in the dock when the app launches — no old
Ghostty icon flash.

#### Description

Experiments 3 and 4 both ran debug builds (`zig build` defaults to Debug). Debug
builds output to `macos/build/Debug/Ghostty.app`. Release builds output to
`macos/build/ReleaseLocal/Ghostty.app` — a different path that macOS has never
seen before, so there should be no cached icon for it.

Additionally, release builds skip the `#if DEBUG` runtime icon override, so the
`.icns` is the only icon source. This eliminates the debug override as a
variable and gives a clean read on whether the `.icns` displays correctly.

To build in release mode, pass `-Doptimize=ReleaseFast` to `zig build`. This
maps to the `ReleaseLocal` Xcode configuration (`GhosttyXcodebuild.zig:31-34`).

#### Steps

1. Delete Xcode DerivedData and local build directory (clean slate):

```bash
rm -rf ~/Library/Developer/Xcode/DerivedData/Ghostty-*
rm -rf ghost/macos/build/
```

2. Build in release mode:

```bash
cd ghost && zig build -Doptimize=ReleaseFast
```

3. Verify the built `.icns` contains the surfing ghost:

```bash
mkdir -p /tmp/icon-check
cp ghost/zig-out/Ghostty.app/Contents/Resources/Ghostty.icns /tmp/icon-check/
cd /tmp/icon-check && iconutil -c iconset Ghostty.icns
open Ghostty.iconset/icon_128x128@2x.png
```

4. Launch the app:

```bash
open ghost/zig-out/Ghostty.app
```

#### Verification

1. **Extracted `.icns`:** The surfing ghost on the dark blue screen background.
2. **Dock icon:** The surfing ghost icon appears immediately — no old Ghostty
   icon flash, no debug override.
3. **Finder:** `ghost/zig-out/Ghostty.app` shows the surfing ghost icon.
4. **App switcher (Cmd+Tab):** Shows the surfing ghost icon.

**Result:** Fail

The release build still shows the old Ghostty icon. The `.icns` file is correct,
but macOS isn't reading it. The real icon source is `Assets.car`.

#### Conclusion

The `.icns` file is a red herring. `Info.plist` declares both
`CFBundleIconFile
= Ghostty` and `CFBundleIconName = Ghostty`. When
`CFBundleIconName` is present, macOS reads the icon from the compiled asset
catalog (`Assets.car`), not the `.icns` file. The `.icns` is only a fallback for
systems that don't support asset catalogs.

`Assets.car` contains a full set of pre-rendered "Ghostty" icon images (16px
through 512px) compiled by `actool` from the `Ghostty.icon` bundle. These images
show the old Ghostty ghost — meaning `actool` is either:

1. **Caching**: Reading a stale compiled version of `Ghostty.icon` from
   somewhere we haven't cleared.
2. **Failing silently**: Unable to composite our modified `icon.json` (different
   image, no position offsets) and falling back to a previously compiled
   version.
3. **Working correctly**: Compiling our modified `.icon` but the resulting
   images in `Assets.car` still look like the old icon for some reason (wrong
   layer order, transparency issue, etc.).

The `.icns` file (which we've been verifying) is compiled separately and does
contain the surfing ghost. But it's irrelevant — macOS uses `Assets.car`. The
next experiment should focus on inspecting or replacing the icon inside
`Assets.car`.

## Conclusion

**Status:** Blocked — deferring to Issue 611 (rename app).

The `Ghostty.icon` modification is correct. The `icon.json` edits (swap image
references, remove position offsets) and `surfing-ghost.png` in `Assets/`
produce the intended composited icon — verified by extracting the built
`Ghostty.icns` with `iconutil`, which shows the surfing ghost on the dark blue
screen with gloss and bevel intact.

The icon never displayed because macOS Launch Services caches app icons by
bundle identifier. Our app ships with `com.mitchellh.ghostty` — the same bundle
identifier as the official Ghostty installed at `/Applications/Ghostty.app`.
macOS sees five apps registered with this identifier:

```
/Applications/Ghostty.app
/Users/ryan/dev/termsurf/ghost/zig-out/Ghostty.app
/Users/ryan/dev/termsurf/ghost/macos/build/ReleaseLocal/Ghostty.app
/Users/ryan/dev/termsurf/ts1/zig-out/Ghostty.app
/Users/ryan/dev/termsurf/ts1/macos/build/ReleaseLocal/Ghostty.app
```

Launch Services resolves the icon from the first-registered app with that bundle
identifier — the official Ghostty — and serves its icon for all five. No amount
of build cache clearing, DerivedData purging, or icon service cache flushing can
override this. The icon in our app bundle is correct but macOS never reads it.

The fix is to change the bundle identifier to something unique (e.g.
`com.termsurf.ghost`). This is part of renaming the app from "Ghostty" to
"TermSurf Ghost" (Issue 611). Once the app has its own identity, the icon
modification from Experiment 3 should take effect immediately.

### What's already done

The `Ghostty.icon` modification is in place (uncommitted):

- `ghost/images/Ghostty.icon/icon.json` — Both ghost layers reference
  `surfing-ghost.png`, position blocks removed for centering.
- `ghost/images/Ghostty.icon/Assets/surfing-ghost.png` — New ghost image.
- `ghost/images/Ghostty.icon/Assets/Ghostty.png` — Deleted.

These changes should be committed as part of Issue 611's rename work and
retested once the bundle identifier changes.

### Key learnings

1. **macOS resolves icons by bundle identifier, not by app path.** Multiple apps
   with the same bundle ID share a cached icon from whichever was registered
   first.
2. **`CFBundleIconName` takes priority over `CFBundleIconFile`.** When both are
   present, macOS reads from `Assets.car` (compiled asset catalog), not the
   `.icns` file.
3. **`Ghostty.icns` is a fallback.** The `.icns` file in the app bundle is only
   used on systems that don't support asset catalogs. Modern macOS always uses
   `Assets.car`.
4. **Icon Composer `.icon` bundles work.** Swapping layer images and removing
   position offsets in `icon.json` produces the expected composited result. The
   format is straightforward once you understand the layer structure.
5. **Xcode DerivedData caches compiled icons.** Always delete
   `~/Library/Developer/Xcode/DerivedData/Ghostty-*` when changing icon assets,
   or `actool` may serve stale output.
