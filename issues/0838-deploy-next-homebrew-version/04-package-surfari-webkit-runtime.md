# Experiment 4: Package Surfari WebKit Runtime

## Description

Experiment 3 made Surfari visible to the build, install, release, Homebrew, and
installed Ghostboard resolver paths, but a real release-mode launch exposed that
the runtime package is incomplete.

`libtermsurf_webkit.dylib` links to the repo-built patched WebKit framework:

```text
@rpath/WebKit.framework/Versions/A/WebKit
```

It also carries an rpath back to the development workspace:

```text
webkit/src/WebKitBuild/Debug
```

That means a packaged Surfari launch can accidentally mix the custom WebKit
framework with system JavaScriptCore. The observed failure was:

```text
dyld: Symbol not found: __Z20WTFCrashWithInfoImpliPKcS0_y
Referenced from: webkit/src/WebKitBuild/Debug/WebKit.framework/Versions/A/WebKit
Expected in: /System/Library/Frameworks/JavaScriptCore.framework/Versions/A/JavaScriptCore
```

This experiment packages the custom WebKit runtime closure needed by Surfari and
updates rpaths/install layout so installed Surfari can launch without relying on
the development WebKit build directory.

## Changes

- Add a shared Surfari runtime resource helper, analogous to
  `scripts/roamium-resources.sh`, that defines and copies the required WebKit
  runtime artifacts from `webkit/src/WebKitBuild/Debug`.
- Include at least the directly required patched WebKit runtime artifacts:
  - `WebKit.framework`
  - `WebCore.framework`
  - `JavaScriptCore.framework`
  - `WebKitLegacy.framework`
  - `WebInspectorUI.framework`
  - `WebGPU.framework`
  - `libANGLE-shared.dylib`
  - `libwebrtc.dylib`
  - `com.apple.WebKit.GPU.xpc`
  - `com.apple.WebKit.Model.xpc`
  - `com.apple.WebKit.Networking.xpc`
  - `com.apple.WebKit.WebContent.CaptivePortal.xpc`
  - `com.apple.WebKit.WebContent.Development.xpc`
  - `com.apple.WebKit.WebContent.EnhancedSecurity.xpc`
  - `com.apple.WebKit.WebContent.xpc`
- Rewrite or arrange rpaths/install names so installed Surfari and
  `libtermsurf_webkit.dylib` resolve the custom WebKit runtime from the
  installed Surfari directory, not from `webkit/src/WebKitBuild/Debug`.
- Update `scripts/install.sh surfari` to install the Surfari binary,
  `libtermsurf_webkit.dylib`, and the WebKit runtime resources into
  `/opt/homebrew/opt/termsurf-surfari/`.
- Update `scripts/release.sh` to stage the same Surfari runtime resources in the
  release tarball under `surfari/`.
- Update `scripts/build.sh surfari --release` if needed so the built
  `libtermsurf_webkit.dylib` can load WebKit from the packaged Surfari directory
  before falling back to the development build directory.
- Update `homebrew/Casks/termsurf.rb` so postflight signing and quarantine
  clearing cover the full Surfari runtime closure, including frameworks, dylibs,
  and XPC bundles.
- Update `docs/ghostboard-launch-discovery.md` or add packaging docs only if the
  installed Surfari runtime layout changes user-visible behavior.
- Update the Experiment 3 release harness if needed so
  `installed-surfari-release-launch` waits for `BrowserReady` and overlay
  presentation after the packaged runtime works.

## Verification

Script and formatting checks:

```bash
bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh \
  scripts/release.sh scripts/ghostboard-geometry-matrix.sh \
  scripts/surfari-resources.sh
prettier --check issues/0838-deploy-next-homebrew-version/README.md \
  issues/0838-deploy-next-homebrew-version/04-package-surfari-webkit-runtime.md \
  docs/ghostboard-launch-discovery.md
zig fmt --check ghostboard/src/apprt/termsurf.zig
git diff --check
```

Build Surfari:

```bash
./scripts/build.sh surfari --release
```

Verify non-root install layout:

```bash
tmpdir="$(mktemp -d)"
TERMSURF_SURFARI_INSTALL_DIR="$tmpdir/surfari" ./scripts/install.sh surfari
test -x "$tmpdir/surfari/surfari"
test -f "$tmpdir/surfari/libtermsurf_webkit.dylib"
test -d "$tmpdir/surfari/WebKit.framework"
test -d "$tmpdir/surfari/WebCore.framework"
test -d "$tmpdir/surfari/JavaScriptCore.framework"
test -d "$tmpdir/surfari/WebKitLegacy.framework"
test -d "$tmpdir/surfari/WebInspectorUI.framework"
test -d "$tmpdir/surfari/WebGPU.framework"
test -f "$tmpdir/surfari/libANGLE-shared.dylib"
test -f "$tmpdir/surfari/libwebrtc.dylib"
test -d "$tmpdir/surfari/com.apple.WebKit.GPU.xpc"
test -d "$tmpdir/surfari/com.apple.WebKit.Model.xpc"
test -d "$tmpdir/surfari/com.apple.WebKit.Networking.xpc"
test -d "$tmpdir/surfari/com.apple.WebKit.WebContent.CaptivePortal.xpc"
test -d "$tmpdir/surfari/com.apple.WebKit.WebContent.Development.xpc"
test -d "$tmpdir/surfari/com.apple.WebKit.WebContent.EnhancedSecurity.xpc"
test -d "$tmpdir/surfari/com.apple.WebKit.WebContent.xpc"
! otool -l "$tmpdir/surfari/libtermsurf_webkit.dylib" |
  rg '/webkit/src/WebKitBuild/Debug'
otool -l "$tmpdir/surfari/libtermsurf_webkit.dylib" |
  rg '@loader_path|@executable_path|@rpath'
for artifact in \
  surfari \
  libtermsurf_webkit.dylib \
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
  codesign --verify "$tmpdir/surfari/$artifact"
done
TERMSURF_SURFARI_INSTALL_DIR="$tmpdir/surfari" ./scripts/uninstall.sh surfari
test ! -e "$tmpdir/surfari"
rm -rf "$tmpdir"
```

Verify package-only release staging:

```bash
TERMSURF_RELEASE_PACKAGE_ONLY=1 scripts/release.sh 1.4.0
test -x dist/release/surfari/surfari
test -f dist/release/surfari/libtermsurf_webkit.dylib
test -d dist/release/surfari/WebKit.framework
test -d dist/release/surfari/WebCore.framework
test -d dist/release/surfari/JavaScriptCore.framework
test -d dist/release/surfari/WebKitLegacy.framework
test -d dist/release/surfari/WebInspectorUI.framework
test -d dist/release/surfari/WebGPU.framework
test -f dist/release/surfari/libANGLE-shared.dylib
test -f dist/release/surfari/libwebrtc.dylib
test -d dist/release/surfari/com.apple.WebKit.GPU.xpc
test -d dist/release/surfari/com.apple.WebKit.Model.xpc
test -d dist/release/surfari/com.apple.WebKit.Networking.xpc
test -d dist/release/surfari/com.apple.WebKit.WebContent.CaptivePortal.xpc
test -d dist/release/surfari/com.apple.WebKit.WebContent.Development.xpc
test -d dist/release/surfari/com.apple.WebKit.WebContent.EnhancedSecurity.xpc
test -d dist/release/surfari/com.apple.WebKit.WebContent.xpc
tarball_listing="$(mktemp)"
tar tzf dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz >"$tarball_listing"
for path in \
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
! otool -l dist/release/surfari/libtermsurf_webkit.dylib |
  rg '/webkit/src/WebKitBuild/Debug'
for artifact in \
  surfari \
  libtermsurf_webkit.dylib \
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
  rg "surfari_runtime_artifacts.*${artifact}|${artifact}.*surfari_runtime_artifacts" \
    homebrew/Casks/termsurf.rb
done
ruby -e '
  cask = File.read("homebrew/Casks/termsurf.rb")
  list = cask[/surfari_runtime_artifacts\s*=\s*\[(.*?)\]/m, 1] or abort("missing surfari_runtime_artifacts")
  postflight = cask[/postflight do(.*?)\n  end/m, 1] or abort("missing postflight")
  loops = postflight.scan(/surfari_runtime_artifacts[.]each\s+do\s+\|artifact\|(.*?)^\s*end/m).flatten
  abort("missing surfari_runtime_artifacts.each") if loops.empty?
  abort("missing codesign loop") unless loops.any? { |body| body.include?("codesign") }
  abort("missing xattr loop") unless loops.any? { |body| body.include?("xattr") }
  %w[
    surfari
    libtermsurf_webkit.dylib
    WebKit.framework
    WebCore.framework
    JavaScriptCore.framework
    WebKitLegacy.framework
    WebInspectorUI.framework
    WebGPU.framework
    libANGLE-shared.dylib
    libwebrtc.dylib
    com.apple.WebKit.GPU.xpc
    com.apple.WebKit.Model.xpc
    com.apple.WebKit.Networking.xpc
    com.apple.WebKit.WebContent.CaptivePortal.xpc
    com.apple.WebKit.WebContent.Development.xpc
    com.apple.WebKit.WebContent.EnhancedSecurity.xpc
    com.apple.WebKit.WebContent.xpc
  ].each { |artifact| abort("missing #{artifact}") unless list.include?(artifact) }
'
```

Verify installed-style Surfari launch without development-only
`TERMSURF_SURFARI_PATH`:

```bash
tmpdir="$(mktemp -d)"
TERMSURF_SURFARI_INSTALL_DIR="$tmpdir/surfari" ./scripts/install.sh surfari
TERMSURF_INSTALLED_SURFARI_PATH="$tmpdir/surfari/surfari" \
  scripts/ghostboard-geometry-matrix.sh installed-surfari-release-launch
TERMSURF_SURFARI_INSTALL_DIR="$tmpdir/surfari" ./scripts/uninstall.sh surfari
rm -rf "$tmpdir"
```

Pass criteria:

- Surfari's installed directory contains the Surfari binary,
  `libtermsurf_webkit.dylib`, and the required custom WebKit runtime artifacts.
- Package-only release staging and tarball inspection show the same runtime
  artifacts under `surfari/`.
- Release-mode Ghostboard resolves named `surfari` through the installed path or
  installed override, not `TERMSURF_SURFARI_PATH`.
- `installed-surfari-release-launch` reaches `BrowserReady` and overlay
  presentation using the installed Surfari layout.
- The dyld JavaScriptCore mismatch does not appear in the app log.
- `otool -l` checks prove installed and staged Surfari artifacts no longer
  contain the development WebKit build rpath.
- Codesign verification passes for every installed Surfari runtime artifact
  listed in this experiment.
- Script syntax, markdown formatting, Zig formatting, and `git diff --check`
  pass.

Fail criteria:

- Surfari still depends on `webkit/src/WebKitBuild/Debug` at runtime after
  installation.
- Surfari launches but mixes custom WebKit with system JavaScriptCore.
- The release tarball omits any required WebKit runtime artifact.
- Homebrew cask installation would leave unsigned or quarantined Surfari runtime
  artifacts.
- The implementation changes Roamium packaging or debug browser fallback
  behavior.

## Design Review

Initial fresh-context adversarial design review returned **Changes Required**
with four required findings:

- the runtime closure omitted WebKit XPC services;
- rpath verification did not enforce that development WebKit paths were absent;
- tarball checks did not cover all declared runtime artifacts; and
- signing/quarantine handling was named but not verifiable.

The design was updated to include the WebKit XPC bundles, explicit
development-rpath rejection checks, complete install/tarball assertions, and
codesign/quarantine verification requirements.

Follow-up reviews found that the tarball assertion had to prove every runtime
artifact individually and that the Homebrew cask verification had to prove
codesign/xattr coverage inside the actual `surfari_runtime_artifacts.each`
blocks. The design was updated with per-artifact tarball checks and a Ruby cask
parser that extracts the runtime artifact list and validates codesign/xattr loop
bodies. Final re-review returned **Approved** with no required findings.
