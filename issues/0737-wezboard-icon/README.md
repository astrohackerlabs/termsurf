+++
status = "closed"
opened = "2026-03-10"
closed = "2026-03-10"
+++

# Issue 737: Wezboard app icon

## Goal

Replace Wezboard's default WezTerm icon (`terminal.icns`) with the latest
TermSurf icon (`assets/termsurf-11.png`).

## Background

Wezboard currently ships with the stock WezTerm icon at
`wezboard/assets/macos/Wezboard.app/Contents/Resources/terminal.icns`. The
`Info.plist` references it as `CFBundleIconFile = terminal.icns` (and again
under `CFBundleDocumentTypes`).

The latest TermSurf icon is `assets/termsurf-11.png`. Ghostboard already has its
own icon pipeline (`scripts/generate-icons.sh`) that uses `sips` to resize a
source PNG into all required sizes, but that script targets Ghostboard's Xcode
asset catalogs. Wezboard doesn't use Xcode — it has a bare `.app` template with
a single `.icns` file.

### What needs to change

1. Generate a `wezboard.icns` from `assets/termsurf-11.png` using `iconutil`
   (create an `.iconset` directory with all required sizes, then convert to
   `.icns`).
2. Replace `wezboard/assets/macos/Wezboard.app/Contents/Resources/terminal.icns`
   with the new `wezboard.icns`.
3. Update `Info.plist` to reference `wezboard.icns` instead of `terminal.icns`
   (both in `CFBundleIconFile` and `CFBundleDocumentTypes`).

## Experiments

### Experiment 1: Generate icns and update app template

#### Description

Generate `wezboard.icns` from `assets/termsurf-11.png`, replace the stock
WezTerm icon, and update the plist references. The source image is 900x900 — the
512x512@2x (1024px) variant will be a slight upscale, which is acceptable for an
app icon.

#### Changes

**Generate the `.icns` file**

Use `sips` and `iconutil` to create the icon:

```bash
mkdir -p /tmp/wezboard.iconset
SRC=assets/termsurf-11.png
for size in 16 32 128 256 512; do
  sips -z $size $size "$SRC" --out "/tmp/wezboard.iconset/icon_${size}x${size}.png"
  double=$((size * 2))
  sips -z $double $double "$SRC" --out "/tmp/wezboard.iconset/icon_${size}x${size}@2x.png"
done
iconutil --convert icns --output wezboard/assets/macos/Wezboard.app/Contents/Resources/wezboard.icns /tmp/wezboard.iconset
```

**`wezboard/assets/macos/Wezboard.app/Contents/Resources/`**

1. Remove `terminal.icns`.
2. Add `wezboard.icns` (generated above).

**`wezboard/assets/macos/Wezboard.app/Contents/Info.plist`**

1. Change `CFBundleIconFile` from `terminal.icns` to `wezboard.icns` (line 26).
2. Change `CFBundleTypeIconFile` from `terminal.icns` to `wezboard.icns` (line
   73).

#### Verification

1. `./scripts/build.sh wezboard --release && ./scripts/install.sh wezboard`
2. Open `/Applications/Wezboard.app` — the dock icon should show the TermSurf
   surfer, not the old WezTerm terminal icon.
3. Right-click a `.sh` file → Get Info — the document icon should also show the
   new icon.

**Result:** Pass

Generated `wezboard.icns` from `assets/termsurf-11.png` using `sips` +
`iconutil` with all 10 required sizes (16–1024px). Replaced `terminal.icns` in
the app template, updated both `Info.plist` references. The dock icon now shows
the TermSurf surfer.

#### Conclusion

Straightforward icon swap. The `.icns` generation is manual (not scripted) since
Wezboard only needs it done once — unlike Ghostboard which has
`generate-icons.sh` for its Xcode asset catalogs.

## Conclusion

Wezboard now uses the TermSurf icon (`termsurf-11.png`) instead of the stock
WezTerm terminal icon. The app template, plist references, and `.icns` file are
all updated.
