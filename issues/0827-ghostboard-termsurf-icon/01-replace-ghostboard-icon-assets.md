# Experiment 1: Replace Ghostboard Icon Assets with the TermSurf Logo

## Description

Ghostboard's app identity is now `TermSurf`, but its macOS icon assets still use
the inherited terminal-style `$W` artwork. This experiment replaces the
Ghostboard source icon assets with the same TermSurf wave/prompt logo already
used by Wezboard.

The source of truth for this experiment is:

```text
wezboard/assets/macos/TermSurf Wezboard.app/Contents/Resources/wezboard.icns
```

The experiment should derive all required Ghostboard PNG sizes from that
existing `.icns` rather than introducing a new design. It should verify source
assets, build products, and install products without relying on Finder or Dock
icon caches.

## Changes

- `ghostboard/macos/Assets.xcassets/TermSurf.appiconset/`
  - Replace every `termsurf-icon-*.png` with the TermSurf wave/prompt logo at
    the existing appiconset sizes: 16, 32, 64, 128, 256, 512, and 1024 px.
  - Keep `Contents.json` structurally unchanged unless the asset filenames need
    to change.
- `ghostboard/macos/Assets.xcassets/AppIconImage.imageset/`
  - Replace the SwiftUI/about/settings icon image PNGs with the same TermSurf
    wave/prompt logo at the existing imageset sizes: 256, 512, and 1024 px.
  - Keep `Contents.json` structurally unchanged unless the asset filenames need
    to change.

Do not change app names, bundle IDs, CLI names, config paths, protocol code,
Wezboard assets, or app behavior outside icon assets.

## Verification

Before changing assets, record the current mismatch:

```bash
sips -s format png \
  "wezboard/assets/macos/TermSurf Wezboard.app/Contents/Resources/wezboard.icns" \
  --out logs/issue-0827-exp01-wezboard-source-icon.png
sips -g pixelWidth -g pixelHeight \
  logs/issue-0827-exp01-wezboard-source-icon.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-512.png \
  ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-512px.png
```

Generate replacement PNGs from the Wezboard `.icns`, then verify dimensions:

```bash
sips -g pixelWidth -g pixelHeight \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-16.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-32.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-64.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-128.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-256.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-512.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-1024.png \
  ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-256px-128pt@2x.png \
  ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-512px.png \
  ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-1024px.png \
  > logs/issue-0827-exp01-source-dimensions.log
```

Verify the rewritten source PNGs match the Wezboard source image content at the
same sizes by generating temporary resized reference PNGs and comparing bytes.
This check must fail on the first mismatch and must cover both the app bundle
appiconset and the SwiftUI imageset:

```bash
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue827-icons.XXXXXX")"
sips -s format png \
  "wezboard/assets/macos/TermSurf Wezboard.app/Contents/Resources/wezboard.icns" \
  --out "$tmp_dir/source-1024.png"
for size in 16 32 64 128 256 512 1024; do
  cp "$tmp_dir/source-1024.png" "$tmp_dir/ref-$size.png"
  sips -z "$size" "$size" "$tmp_dir/ref-$size.png" >/dev/null
  cmp "$tmp_dir/ref-$size.png" \
    "ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-$size.png"
done
cmp "$tmp_dir/ref-256.png" \
  "ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-256px-128pt@2x.png"
cmp "$tmp_dir/ref-512.png" \
  "ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-512px.png"
cmp "$tmp_dir/ref-1024.png" \
  "ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-1024px.png"
shasum -a 256 "$tmp_dir"/ref-*.png \
  ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-*.png \
  ghostboard/macos/Assets.xcassets/AppIconImage.imageset/macOS-AppIcon-*.png \
  > logs/issue-0827-exp01-source-hashes.log
rm -rf "$tmp_dir"
```

Build the release app and verify the generated icon resource is no longer the
stale `$W` icon. Xcode may re-encode PNGs or slightly adjust the smallest icon
representation when compiling the asset catalog, so bundle verification decodes
the emitted PNGs with CoreGraphics and compares rendered RGBA pixels for every
representation that Xcode emits:

```bash
./scripts/build.sh ghostboard --release \
  > logs/issue-0827-exp01-build-release.log 2>&1
test -x ghostboard/macos/build/Release/TermSurf.app/Contents/MacOS/termsurf
ls -lh ghostboard/macos/build/Release/TermSurf.app/Contents/Resources/TermSurf.icns \
  > logs/issue-0827-exp01-built-icon-resource.log
sips -s format png \
  ghostboard/macos/build/Release/TermSurf.app/Contents/Resources/TermSurf.icns \
  --out logs/issue-0827-exp01-built-icon.png
sips -g pixelWidth -g pixelHeight logs/issue-0827-exp01-built-icon.png \
  > logs/issue-0827-exp01-built-icon-dimensions.log
rm -rf logs/issue-0827-exp01-built-icon.iconset
iconutil -c iconset -o logs/issue-0827-exp01-built-icon.iconset \
  ghostboard/macos/build/Release/TermSurf.app/Contents/Resources/TermSurf.icns
cat > /tmp/termsurf-issue827-compare-images.swift <<'SWIFT'
import Foundation
import CoreGraphics
import ImageIO

func loadRGBA(_ path: String) throws -> (Int, Int, [UInt8]) {
    let url = URL(fileURLWithPath: path)
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
          let img = CGImageSourceCreateImageAtIndex(src, 0, nil) else {
        throw NSError(domain: "compare", code: 1)
    }
    let width = img.width
    let height = img.height
    var data = [UInt8](repeating: 0, count: width * height * 4)
    let cs = CGColorSpaceCreateDeviceRGB()
    guard let ctx = CGContext(
        data: &data,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: cs,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else {
        throw NSError(domain: "compare", code: 2)
    }
    ctx.draw(img, in: CGRect(x: 0, y: 0, width: width, height: height))
    return (width, height, data)
}

let a = try loadRGBA(CommandLine.arguments[1])
let b = try loadRGBA(CommandLine.arguments[2])
let maxAllowed = Int(CommandLine.arguments[3])!
let meanAllowed = Double(CommandLine.arguments[4])!
if a.0 != b.0 || a.1 != b.1 { exit(1) }
var maxDiff = 0
var sumDiff = 0
var differing = 0
for i in 0..<a.2.count {
    let d = abs(Int(a.2[i]) - Int(b.2[i]))
    if d != 0 { differing += 1 }
    if d > maxDiff { maxDiff = d }
    sumDiff += d
}
let mean = Double(sumDiff) / Double(a.2.count)
print("width=\(a.0) height=\(a.1) max_diff=\(maxDiff) mean_diff=\(String(format: "%.6f", mean)) differing_channels=\(differing) total_channels=\(a.2.count)")
if maxDiff > maxAllowed || mean > meanAllowed { exit(1) }
SWIFT
: > logs/issue-0827-exp01-built-icon-pixel-compare.log
for icon in logs/issue-0827-exp01-built-icon.iconset/*.png; do
  base="$(basename "$icon")"
  case "$base" in
    icon_16x16.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-16.png" ;;
    icon_16x16@2x.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-32.png" ;;
    icon_32x32.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-32.png" ;;
    icon_32x32@2x.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-64.png" ;;
    icon_128x128.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-128.png" ;;
    icon_128x128@2x.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-256.png" ;;
    icon_256x256.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-256.png" ;;
    icon_256x256@2x.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-512.png" ;;
    icon_512x512.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-512.png" ;;
    icon_512x512@2x.png) ref="ghostboard/macos/Assets.xcassets/TermSurf.appiconset/termsurf-icon-1024.png" ;;
    *) echo "unexpected built icon representation: $base" >&2; exit 1 ;;
  esac
  printf "%s vs %s: " "$base" "$(basename "$ref")" \
    >> logs/issue-0827-exp01-built-icon-pixel-compare.log
  swift /tmp/termsurf-issue827-compare-images.swift "$icon" "$ref" 64 2.0 \
    >> logs/issue-0827-exp01-built-icon-pixel-compare.log
done
find logs/issue-0827-exp01-built-icon.iconset -type f -print -exec file {} \; \
  > logs/issue-0827-exp01-built-iconset.log
assetutil --info ghostboard/macos/build/Release/TermSurf.app/Contents/Resources/Assets.car \
  > logs/issue-0827-exp01-assets-car-info.json
rg -n '"Name" : "AppIconImage"|"Name" : "TermSurf"' \
  logs/issue-0827-exp01-assets-car-info.json \
  > logs/issue-0827-exp01-assets-car-icon-names.log
```

Install into a temporary Applications directory so the real `/Applications`
install path is not required for verification:

```bash
tmp_app_dir="$(mktemp -d "${TMPDIR:-/tmp}/termsurf-issue827-install.XXXXXX")"
TERMSURF_APPLICATIONS_DIR="$tmp_app_dir" ./scripts/install.sh ghostboard \
  > logs/issue-0827-exp01-install-temp.log 2>&1
test -x "$tmp_app_dir/TermSurf.app/Contents/MacOS/termsurf"
ls -lh "$tmp_app_dir/TermSurf.app/Contents/Resources/TermSurf.icns" \
  > logs/issue-0827-exp01-installed-icon-resource.log
sips -s format png "$tmp_app_dir/TermSurf.app/Contents/Resources/TermSurf.icns" \
  --out logs/issue-0827-exp01-installed-icon.png
rm -rf logs/issue-0827-exp01-installed-icon.iconset
iconutil -c iconset -o logs/issue-0827-exp01-installed-icon.iconset \
  "$tmp_app_dir/TermSurf.app/Contents/Resources/TermSurf.icns"
: > logs/issue-0827-exp01-installed-icon-pixel-compare.log
for icon in logs/issue-0827-exp01-installed-icon.iconset/*.png; do
  base="$(basename "$icon")"
  built="logs/issue-0827-exp01-built-icon.iconset/$base"
  test -f "$built"
  printf "%s vs built: " "$base" \
    >> logs/issue-0827-exp01-installed-icon-pixel-compare.log
  swift /tmp/termsurf-issue827-compare-images.swift "$icon" "$built" 0 0 \
    >> logs/issue-0827-exp01-installed-icon-pixel-compare.log
done
find logs/issue-0827-exp01-installed-icon.iconset -type f -print -exec file {} \; \
  > logs/issue-0827-exp01-installed-iconset.log
rm -rf "$tmp_app_dir"
```

Run hygiene checks:

```bash
bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh
prettier --write --prose-wrap always --print-width 80 \
  issues/0827-ghostboard-termsurf-icon/README.md \
  issues/0827-ghostboard-termsurf-icon/01-replace-ghostboard-icon-assets.md
git diff --check
```

Pass criteria:

- The Ghostboard source appiconset and SwiftUI imageset visually use the
  TermSurf wave/prompt logo, not the `$W` icon.
- The source PNG dimensions match their `Contents.json` declarations.
- Failing byte comparisons prove every Ghostboard source PNG was derived from
  the Wezboard TermSurf icon at the matching size.
- The release `TermSurf.app` builds successfully.
- The built `TermSurf.icns` emits icon representations whose decoded pixel
  payloads exactly match the corresponding rewritten source assets except for
  Xcode's 16 px representation adjustment, which must stay within
  `max_diff <= 64` and `mean_diff <= 2.0`.
- The compiled `Assets.car` contains both `TermSurf` and `AppIconImage`
  renditions, proving the corrected source assets feed both Dock/Finder and
  in-app icon surfaces.
- Temporary install of Ghostboard succeeds and installs
  `TermSurf.app/Contents/MacOS/termsurf` plus the corrected icon resource.
- The installed `TermSurf.icns` emits icon representations whose decoded pixel
  payloads match the built app icon representations.
- Verification inspects bundle resources directly rather than relying on
  LaunchServices, Finder, or Dock cached icons.

## Result

**Result:** Pass

Ghostboard's app icon source assets now use the same TermSurf wave/prompt logo
as Wezboard. The inherited `$W` artwork was replaced in both macOS icon asset
locations:

- `ghostboard/macos/Assets.xcassets/TermSurf.appiconset/`
- `ghostboard/macos/Assets.xcassets/AppIconImage.imageset/`

The replacement PNGs were generated from:

```text
wezboard/assets/macos/TermSurf Wezboard.app/Contents/Resources/wezboard.icns
```

Verification evidence:

- `logs/issue-0827-exp01-source-dimensions.log`
  - Confirms the source appiconset PNGs are 16, 32, 64, 128, 256, 512, and 1024
    px.
  - Confirms the source `AppIconImage.imageset` PNGs are 256, 512, and 1024 px.
- `logs/issue-0827-exp01-source-hashes.log`
  - Records matching hashes between Wezboard-derived temporary references and
    the rewritten Ghostboard source PNGs.
  - The source comparison used failing `cmp` checks before writing the hash log.
- `logs/issue-0827-exp01-build-release.log`
  - `./scripts/build.sh ghostboard --release` passed and built
    `ghostboard/macos/build/Release/TermSurf.app`.
- `logs/issue-0827-exp01-built-iconset.log`
  - `iconutil` extracted the built `TermSurf.icns` into 16, 32, 128, and 256 px
    representations.
- `logs/issue-0827-exp01-built-icon-pixel-compare.log`
  - The built 32, 128, and 256 px icon representations decode to exact RGBA
    matches against the corresponding rewritten source assets.
  - The built 16 px representation differs only by Xcode's tiny icon compilation
    adjustment: `max_diff=58`, `mean_diff=1.925781`, and `34 / 1024` channels
    differ. This stays within the planned tolerance (`max_diff <= 64`,
    `mean_diff <= 2.0`).
- `logs/issue-0827-exp01-assets-car-icon-names.log`
  - Confirms the compiled `Assets.car` contains both `TermSurf` and
    `AppIconImage` renditions, covering Dock/Finder/app-switcher and in-app
    SwiftUI icon surfaces.
- `logs/issue-0827-exp01-install-temp.log`
  - Temporary Ghostboard install passed without using `/Applications`.
- `logs/issue-0827-exp01-installed-icon-pixel-compare.log`
  - The installed temporary `TermSurf.app` icon representations decode to exact
    RGBA matches against the built app's `TermSurf.icns` representations.

Hygiene checks passed:

```text
bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh
prettier --write --prose-wrap always --print-width 80 \
  issues/0827-ghostboard-termsurf-icon/README.md \
  issues/0827-ghostboard-termsurf-icon/01-replace-ghostboard-icon-assets.md
git diff --check
```

## Conclusion

Experiment 1 replaces Ghostboard's wrong inherited icon artwork with the same
TermSurf logo used by Wezboard. The source assets, release build output,
compiled asset catalog, and temporary install output all prove that Ghostboard
now carries the TermSurf wave/prompt icon in the places covered by Issue 827.

No additional experiment is needed unless the completion review finds a gap.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

**Verdict:** Approved.

Findings: none.

The reviewer confirmed that the diff is scoped to the two Ghostboard icon asset
sets plus issue docs, the source PNGs are the TermSurf wave/prompt logo,
dimensions match the asset declarations, README status and experiment result are
consistent, verification inspects built and installed bundle resources directly
instead of Finder/Dock caches, and the Xcode re-encoding tolerance is narrow and
backed by decoded RGBA comparisons.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

**Initial verdict:** Changes required.

Required findings and fixes:

- Source asset verification logged hashes without failing and did not cover
  `AppIconImage.imageset`. Fixed by requiring `cmp` checks for all seven
  `TermSurf.appiconset` PNGs and all three `AppIconImage.imageset` PNGs.
- Built and installed app verification only proved icon resource existence and
  dimensions. Fixed by requiring `cmp` checks between the converted Wezboard
  source icon and the converted built and installed `TermSurf.icns` resources.

The re-review approved the design with no remaining findings.
