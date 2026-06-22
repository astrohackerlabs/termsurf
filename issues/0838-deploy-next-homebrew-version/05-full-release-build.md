# Experiment 5: Full Release Build

## Description

Stages 1 through 4 proved WebKit, Surfari, and Surfari packaging integration.
The next deployment stage is the full `1.4.0` release build. This experiment
builds every release component through the canonical script and verifies that
the expected release artifacts exist before package validation or publishing.

## Changes

No code changes are planned. This experiment should only run the full release
build and record the result.

## Verification

Run the canonical release build:

```bash
./scripts/build.sh all --release
```

Verify the core release artifacts:

```bash
test -x target/release/web
test -x target/release/roamium
test -x target/release/surfari
test -f surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib
test -x ghostboard/macos/build/Release/TermSurf.app/Contents/MacOS/termsurf
test -d webkit/src/WebKitBuild/Debug/WebKit.framework
test -d webkit/src/WebKitBuild/Debug/WebCore.framework
test -d webkit/src/WebKitBuild/Debug/JavaScriptCore.framework
test -d webkit/src/WebKitBuild/Debug/WebKitLegacy.framework
test -d webkit/src/WebKitBuild/Debug/WebInspectorUI.framework
test -d webkit/src/WebKitBuild/Debug/WebGPU.framework
test -f webkit/src/WebKitBuild/Debug/libANGLE-shared.dylib
test -f webkit/src/WebKitBuild/Debug/libwebrtc.dylib
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.GPU.xpc
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.Model.xpc
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.Networking.xpc
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.WebContent.CaptivePortal.xpc
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.WebContent.Development.xpc
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.WebContent.EnhancedSecurity.xpc
test -d webkit/src/WebKitBuild/Debug/com.apple.WebKit.WebContent.xpc
```

Run final hygiene checks after the build:

```bash
bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh \
  scripts/release.sh scripts/ghostboard-geometry-matrix.sh \
  scripts/surfari-resources.sh
prettier --check issues/0838-deploy-next-homebrew-version/README.md \
  issues/0838-deploy-next-homebrew-version/05-full-release-build.md
git diff --check
git status --short
```

Pass criteria:

- `scripts/build.sh all --release` completes successfully.
- Release WebTUI, Roamium, Surfari, `libtermsurf_webkit.dylib`, and
  `TermSurf.app` artifacts exist.
- All WebKit runtime artifacts needed by Surfari remain present.
- `git status --short` shows no tracked or untracked source/documentation
  changes after the build and verification.

Fail criteria:

- Any release component fails to build.
- The full build omits Surfari or `libtermsurf_webkit.dylib`.
- Any required WebKit runtime artifact is missing after the build.
- Build output dirties tracked or untracked source/documentation files.

## Design Review

Initial fresh-context adversarial design review returned **Changes Required**
with two required findings:

- WebKit runtime verification only checked `WebKit.framework` and
  `JavaScriptCore.framework`, not the full Surfari runtime closure.
- `git diff --check` did not prove the build left the worktree clean.

The design was updated to verify every Surfari WebKit runtime artifact and to
include an explicit `git status --short` cleanliness check.

Re-review returned **Approved** with no required findings.

## Result

**Result:** Pass

The canonical full release build completed successfully:

```bash
./scripts/build.sh all --release
```

The build completed Chromium first with no work to do, then built release
WebTUI, release Roamium, `libtermsurf_webkit.dylib`, release Surfari, and
Ghostboard Release. The Ghostboard Xcode phase completed with
`** BUILD SUCCEEDED **`, then `scripts/build.sh` codesigned `TermSurf.app` and
printed `Done (all).`

Verification passed:

- `test -x target/release/web`
- `test -x target/release/roamium`
- `test -x target/release/surfari`
- `test -f surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib`
- `test -x ghostboard/macos/build/Release/TermSurf.app/Contents/MacOS/termsurf`
- existence checks for the Surfari WebKit runtime closure in
  `webkit/src/WebKitBuild/Debug`:
  - `WebKit.framework`
  - `WebCore.framework`
  - `JavaScriptCore.framework`
  - `WebKitLegacy.framework`
  - `WebInspectorUI.framework`
  - `WebGPU.framework`
  - `libANGLE-shared.dylib`
  - `libwebrtc.dylib`
  - the seven WebKit XPC bundles
- `bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh scripts/release.sh scripts/ghostboard-geometry-matrix.sh scripts/surfari-resources.sh`
- `prettier --check issues/0838-deploy-next-homebrew-version/README.md issues/0838-deploy-next-homebrew-version/05-full-release-build.md`
- `git diff --check`
- `git status --short`, which produced no output

The build produced expected nonfatal warnings already seen in prior local
builds, including the Surfari WebKit SDK version linker warning and Ghostboard
SwiftLint/dSYM warnings. None caused the release build to fail.

## Conclusion

Stage 5 is complete. The repo now has current release artifacts for WebTUI,
Roamium, Surfari, `libtermsurf_webkit.dylib`, Ghostboard, Chromium, and the
Surfari WebKit runtime closure. Issue 838 can move to package-only release
validation.

## Completion Review

Fresh-context adversarial completion review returned **Approved** with no
required findings. The reviewer confirmed that the result stayed within the
approved no-code-change scope, that README status matched the experiment result
and Stage 5 completion, and that the result commit had not yet been made.
