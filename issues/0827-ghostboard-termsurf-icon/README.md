+++
status = "closed"
opened = "2026-06-19"
closed = "2026-06-19"
+++

# Issue 827: Ghostboard TermSurf Icon

## Goal

Make Ghostboard use the same custom TermSurf app icon as Wezboard everywhere the
user sees the application icon.

When solved, `TermSurf.app` built from `ghostboard/` should use the TermSurf
wave/prompt logo, matching `TermSurf Wezboard.app`, rather than the inherited
terminal-style `$W` icon.

## Background

After Issue 826, Ghostboard installs as:

```text
/Applications/TermSurf.app
```

The app name and executable are correct, but the icon is wrong. The current
Ghostboard asset catalog points the app icon setting at:

```text
ghostboard/macos/Assets.xcassets/TermSurf.appiconset/
```

That appiconset currently contains `termsurf-icon-*.png` files with the old
terminal-style `$W` artwork. The installed app also contains:

```text
/Applications/TermSurf.app/Contents/Resources/TermSurf.icns
```

but that `.icns` is generated from the wrong Ghostboard appiconset.

The correct TermSurf icon already exists in the Wezboard app template:

```text
wezboard/assets/macos/TermSurf Wezboard.app/Contents/Resources/wezboard.icns
```

That icon is the cyan TermSurf wave/prompt logo. Ghostboard should use that same
visual identity for its Dock icon, Finder icon, app switcher icon, About view,
settings view, error view, and any other user-facing app icon surfaces.

## Analysis

The likely affected Ghostboard assets are:

```text
ghostboard/macos/Assets.xcassets/TermSurf.appiconset/
ghostboard/macos/Assets.xcassets/AppIconImage.imageset/
```

`TermSurf.appiconset` controls the bundle icon selected by:

```text
ASSETCATALOG_COMPILER_APPICON_NAME = TermSurf
```

`AppIconImage.imageset` is used directly in SwiftUI surfaces such as the About
view, settings view, error view, and custom icon fallback paths.

The fix should derive the required Ghostboard PNG sizes from the existing
Wezboard `.icns` or from the canonical source image that produced it, then
replace the Ghostboard source assets. The issue should prefer reusing the
canonical TermSurf icon artwork over creating a new design.

## Acceptance Criteria

- Ghostboard source assets use the same TermSurf wave/prompt logo as Wezboard.
- `ghostboard/macos/Assets.xcassets/TermSurf.appiconset/` no longer contains the
  `$W` artwork.
- `ghostboard/macos/Assets.xcassets/AppIconImage.imageset/` uses the same
  TermSurf logo visual identity.
- A rebuilt release `TermSurf.app` contains an app icon generated from the
  TermSurf logo.
- Installing Ghostboard places `/Applications/TermSurf.app` with the correct
  icon resources.
- The About view and other in-app icon surfaces use the corrected icon image.
- The verification accounts for macOS LaunchServices/icon cache behavior so a
  stale cached icon is not mistaken for a source asset failure.

## Notes

Do not rename the app. The app name remains `TermSurf`, the CLI executable
remains `termsurf`, and the config path remains `~/.config/termsurf/config`.

Do not create experiments upfront. Design Experiment 1 after this issue is open.

## Experiments

- [Experiment 1: Replace Ghostboard icon assets with the TermSurf logo](01-replace-ghostboard-icon-assets.md)
  — **Pass**

## Conclusion

Ghostboard now uses the same TermSurf wave/prompt logo as Wezboard for its macOS
app icon source assets and in-app icon image source assets.

Experiment 1 replaced the inherited `$W` artwork in:

```text
ghostboard/macos/Assets.xcassets/TermSurf.appiconset/
ghostboard/macos/Assets.xcassets/AppIconImage.imageset/
```

The replacement assets were generated from the existing Wezboard TermSurf icon:

```text
wezboard/assets/macos/TermSurf Wezboard.app/Contents/Resources/wezboard.icns
```

Verification rebuilt the release `TermSurf.app`, inspected the generated
`TermSurf.icns` directly with `iconutil`, confirmed `Assets.car` contains both
`TermSurf` and `AppIconImage` renditions, and installed Ghostboard into a
temporary Applications directory. The checks inspect bundle resources directly
instead of relying on Finder, Dock, or LaunchServices icon caches.

Issue 827 is closed.
