# Experiment 6: Package-Only Release Validation

## Description

Stage 5 produced the full release build. Stage 6 validates the `1.4.0` release
tarball without publishing it or pushing the Homebrew tap. This experiment uses
`TERMSURF_RELEASE_PACKAGE_ONLY=1` to exercise the packaging path, then inspects
the staged release directory and compressed tarball for every installable
artifact.

## Changes

No code changes are planned. This experiment should only run package-only
release validation and record the result.

## Verification

Generate the package without publishing:

```bash
TERMSURF_RELEASE_PACKAGE_ONLY=1 scripts/release.sh 1.4.0 2>&1 |
  tee /tmp/termsurf-issue838-exp6-release.log
rg 'Package-only mode: skipping GitHub release upload and Homebrew cask update' \
  /tmp/termsurf-issue838-exp6-release.log
git -C homebrew diff --exit-code -- Casks/termsurf.rb
```

Verify top-level release contents:

```bash
test -f dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz
test -x dist/release/web
test -d dist/release/TermSurf.app
test -x dist/release/TermSurf.app/Contents/MacOS/termsurf
test -x dist/release/roamium/roamium
test -x dist/release/surfari/surfari
test -f dist/release/surfari/libtermsurf_webkit.dylib
```

Verify Roamium runtime resources:

```bash
test -f dist/release/roamium/icudtl.dat
test -f dist/release/roamium/gen/chrome/pdf_resources.pak
test -f dist/release/roamium/gen/chrome/generated_resources_en-US.pak
test -f dist/release/roamium/gen/chrome/common_resources.pak
test -f dist/release/roamium/gen/components/components_resources.pak
test -f dist/release/roamium/gen/components/strings/components_strings_en-US.pak
test -f dist/release/roamium/gen/extensions/extensions_renderer_resources.pak
```

Verify Surfari runtime resources:

```bash
for path in \
  WebKit.framework \
  WebCore.framework \
  JavaScriptCore.framework \
  WebKitLegacy.framework \
  WebInspectorUI.framework \
  WebGPU.framework \
  libANGLE-shared.dylib \
  libwebrtc.dylib \
  com.apple.WebKit.GPU.xpc \
  com.apple.WebKit.Model.xpc \
  com.apple.WebKit.Networking.xpc \
  com.apple.WebKit.WebContent.CaptivePortal.xpc \
  com.apple.WebKit.WebContent.Development.xpc \
  com.apple.WebKit.WebContent.EnhancedSecurity.xpc \
  com.apple.WebKit.WebContent.xpc
do
  test -e "dist/release/surfari/$path"
done

source scripts/surfari-resources.sh
surfari_macho_artifacts=(
  "dist/release/surfari/surfari"
  "dist/release/surfari/libtermsurf_webkit.dylib"
  "dist/release/surfari/libANGLE-shared.dylib"
  "dist/release/surfari/libwebrtc.dylib"
)
for framework in \
  WebKit.framework \
  WebCore.framework \
  JavaScriptCore.framework \
  WebKitLegacy.framework \
  WebInspectorUI.framework \
  WebGPU.framework
do
  surfari_macho_artifacts+=("dist/release/surfari/$(surfari_framework_binary "$framework")")
done
for xpc in dist/release/surfari/*.xpc; do
  surfari_macho_artifacts+=("$(surfari_xpc_executable "$xpc")")
done
for artifact in "${surfari_macho_artifacts[@]}"; do
  test -f "$artifact"
  ! otool -l "$artifact" | rg '/webkit/src/WebKitBuild/Debug'
done
```

Verify tarball contents independently of the staging directory:

```bash
tarball_listing="$(mktemp)"
tar tzf dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz >"$tarball_listing"
for path in \
  './web' \
  './TermSurf.app/' \
  './TermSurf.app/Contents/MacOS/termsurf' \
  './roamium/roamium' \
  './roamium/icudtl.dat' \
  './roamium/gen/chrome/pdf_resources.pak' \
  './roamium/gen/chrome/generated_resources_en-US.pak' \
  './roamium/gen/chrome/common_resources.pak' \
  './roamium/gen/components/components_resources.pak' \
  './roamium/gen/components/strings/components_strings_en-US.pak' \
  './roamium/gen/extensions/extensions_renderer_resources.pak' \
  './surfari/surfari' \
  './surfari/libtermsurf_webkit.dylib' \
  './surfari/WebKit.framework/' \
  './surfari/WebCore.framework/' \
  './surfari/JavaScriptCore.framework/' \
  './surfari/WebKitLegacy.framework/' \
  './surfari/WebInspectorUI.framework/' \
  './surfari/WebGPU.framework/' \
  './surfari/libANGLE-shared.dylib' \
  './surfari/libwebrtc.dylib' \
  './surfari/com.apple.WebKit.GPU.xpc/' \
  './surfari/com.apple.WebKit.Model.xpc/' \
  './surfari/com.apple.WebKit.Networking.xpc/' \
  './surfari/com.apple.WebKit.WebContent.CaptivePortal.xpc/' \
  './surfari/com.apple.WebKit.WebContent.Development.xpc/' \
  './surfari/com.apple.WebKit.WebContent.EnhancedSecurity.xpc/' \
  './surfari/com.apple.WebKit.WebContent.xpc/'
do
  rg "^${path}" "$tarball_listing"
done
rm -f "$tarball_listing"
```

Record the tarball SHA:

```bash
shasum -a 256 dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz
du -sh dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz dist/release/surfari
```

Final hygiene:

```bash
bash -n scripts/release.sh scripts/surfari-resources.sh
prettier --check issues/0838-deploy-next-homebrew-version/README.md \
  issues/0838-deploy-next-homebrew-version/06-package-only-release-validation.md
git diff --check
git status --short
```

Pass criteria:

- Package-only release completes without uploading or pushing.
- The release output explicitly reports that upload and Homebrew cask update
  were skipped.
- `homebrew/Casks/termsurf.rb` is unchanged by package-only mode.
- Staging and tarball contain `web`, `TermSurf.app`, Roamium, Surfari,
  `libtermsurf_webkit.dylib`, Roamium runtime resources, and Surfari WebKit
  runtime resources.
- The staged Surfari bridge dylib no longer contains a development WebKit rpath.
- The tarball SHA and approximate size are recorded.
- Final hygiene checks pass and the worktree is clean except for the experiment
  result docs before the result commit.

Fail criteria:

- Package-only release attempts to publish or push.
- Package-only release edits the Homebrew cask.
- Any required top-level, Roamium, or Surfari artifact is missing from staging
  or tarball contents.
- Surfari staged runtime still references `webkit/src/WebKitBuild/Debug`.
- The package command or checks dirty source files unexpectedly.

## Design Review

An adversarial subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

The reviewer found that the development WebKit rpath check only inspected
`dist/release/surfari/libtermsurf_webkit.dylib`, which was too narrow because
`scripts/surfari-resources.sh` also rewrites framework binaries, dylibs, the
Surfari executable, and XPC executables. The plan now sources
`scripts/surfari-resources.sh`, builds the full staged Surfari Mach-O artifact
list, and rejects `/webkit/src/WebKitBuild/Debug` in every one of those
artifacts.

The reviewer re-reviewed that fix and returned `VERDICT: APPROVED` with no
Required findings.
