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
