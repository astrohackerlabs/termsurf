# Experiment 6: Package-Only Release Validation

## Description

Stage 5 produced the full release build. Stage 6 validates the `1.4.0` release
tarball without publishing it or pushing the Homebrew tap. This experiment uses
`TERMSURF_RELEASE_PACKAGE_ONLY=1` to exercise the packaging path, then inspects
the staged release directory and compressed tarball for every installable
artifact.

## Changes

The initial design expected no code changes, but completion review found a real
release packaging defect: `install_name_tool` invalidated Surfari runtime code
signatures and `scripts/release.sh` did not re-sign the staged Surfari runtime
before creating the tarball.

- `scripts/release.sh` — re-sign the staged Surfari runtime after rewriting
  install names and rpaths.
- `scripts/surfari-resources.sh` — include `libWebKitSwift.dylib` in the
  packaged Surfari runtime, materialize WebKit framework symlinks that point to
  top-level runtime artifacts, remove the known dangling WebCore framework
  symlink, rewrite nested WebKit framework XPC/Swift artifacts, and make Surfari
  runtime signing fail closed.
- `homebrew/Casks/termsurf.rb` — include `libWebKitSwift.dylib` in the cask
  postflight signing and quarantine-clearing list.
- `issues/0838-deploy-next-homebrew-version/README.md` — mark Stage 6 and
  Experiment 6 as pass.
- `issues/0838-deploy-next-homebrew-version/06-package-only-release-validation.md`
  — record the final verification result.

## Verification

Generate the package without publishing:

```bash
TERMSURF_RELEASE_PACKAGE_ONLY=1 scripts/release.sh 1.4.0 2>&1 |
  tee /tmp/termsurf-issue838-exp6-release.log
rg 'Package-only mode: skipping GitHub upload and Homebrew cask update' \
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
  libWebKitSwift.dylib \
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
  "dist/release/surfari/libWebKitSwift.dylib"
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
for xpc in \
  dist/release/surfari/*.xpc \
  dist/release/surfari/WebKit.framework/Versions/A/XPCServices/*.xpc
do
  surfari_macho_artifacts+=("$(surfari_xpc_executable "$xpc")")
done
for artifact in "${surfari_macho_artifacts[@]}"; do
  test -f "$artifact"
  ! otool -l "$artifact" | rg '/webkit/src/WebKitBuild/Debug'
done

broken_symlinks="$(find dist/release/surfari -type l -exec sh -c '
  for l do
    if [ ! -e "$l" ]; then
      printf "%s -> %s\n" "$l" "$(readlink "$l")"
    fi
  done
' sh {} +)"
test -z "$broken_symlinks"

surfari_signed_artifacts=(
  "dist/release/surfari/surfari"
  "dist/release/surfari/libtermsurf_webkit.dylib"
)
for resource in "${SURFARI_REQUIRED_RUNTIME_RESOURCES[@]}"; do
  surfari_signed_artifacts+=("dist/release/surfari/$resource")
done
for artifact in "${surfari_signed_artifacts[@]}"; do
  codesign --verify --deep --strict "$artifact"
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
  './surfari/libWebKitSwift.dylib' \
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
- The staged Surfari Mach-O artifacts no longer contain a development WebKit
  rpath.
- The staged Surfari runtime has no broken symlinks.
- Staged Surfari runtime artifacts pass strict code-signature verification.
- The tarball SHA and approximate size are recorded.
- Final hygiene checks pass and the worktree contains only the expected
  release-packaging fix, Homebrew submodule pointer update, and experiment
  result docs before the result commit.

Fail criteria:

- Package-only release attempts to publish or push.
- Package-only release edits the Homebrew cask.
- Any required top-level, Roamium, or Surfari artifact is missing from staging
  or tarball contents.
- Surfari staged runtime still references `webkit/src/WebKitBuild/Debug`.
- Surfari staged runtime contains broken symlinks or invalid code signatures.
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

## Result

**Result:** Pass

Package-only release validation succeeded.

The package command completed without publishing:

```bash
TERMSURF_RELEASE_PACKAGE_ONLY=1 scripts/release.sh 1.4.0 2>&1 |
  tee /tmp/termsurf-issue838-exp6-release.log
```

The release script reported:

```text
==> Package-only mode: skipping GitHub upload and Homebrew cask update.
```

The design expected the same skip confirmation but used the older wording
`GitHub release upload`. The verification used the actual script output above.

The Homebrew cask stayed unchanged during package-only release execution:

```bash
git -C homebrew diff --exit-code -- Casks/termsurf.rb
```

The Homebrew cask was intentionally updated before the final package-only rerun
to include `libWebKitSwift.dylib` in its Surfari signing list, and that
submodule change was committed separately as `d91e075`.

The validation confirmed the staged release contains:

- `dist/release/web`
- `dist/release/TermSurf.app`
- `dist/release/TermSurf.app/Contents/MacOS/termsurf`
- `dist/release/roamium/roamium`
- the required Roamium runtime resources under `dist/release/roamium/`
- `dist/release/surfari/surfari`
- `dist/release/surfari/libtermsurf_webkit.dylib`
- `dist/release/surfari/libWebKitSwift.dylib`
- the required Surfari WebKit frameworks, dylibs, and XPC bundles under
  `dist/release/surfari/`

The validation also confirmed the compressed tarball contains those top-level,
Roamium, and Surfari artifacts independently of the staging directory.

The Surfari rpath check inspected the staged `surfari` executable,
`libtermsurf_webkit.dylib`, `libANGLE-shared.dylib`, `libWebKitSwift.dylib`,
`libwebrtc.dylib`, all required framework binaries, each top-level staged XPC
executable, and each materialized WebKit-framework XPC executable. None
contained `/webkit/src/WebKitBuild/Debug`.

The validation found no broken symlinks under `dist/release/surfari`.

Strict code-signature verification passed for the staged Surfari executable,
bridge dylib, frameworks, dylibs, and XPC bundles:

```bash
codesign --verify --deep --strict "$artifact"
```

The generated tarball SHA and sizes were:

```text
1557503a788c7453baffe056570ba45e892d2e6c6435939da511e50d94395ae0  dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz
432M    dist/termsurf-1.4.0-aarch64-apple-darwin.tar.gz
1.2G    dist/release/surfari
```

Final hygiene passed:

```bash
bash -n scripts/release.sh scripts/surfari-resources.sh
prettier --check issues/0838-deploy-next-homebrew-version/README.md \
  issues/0838-deploy-next-homebrew-version/06-package-only-release-validation.md
git diff --check
git status --short
```

`git status --short` showed only the expected result changes:

```text
 M homebrew
 M issues/0838-deploy-next-homebrew-version/06-package-only-release-validation.md
 M issues/0838-deploy-next-homebrew-version/README.md
 M scripts/release.sh
 M scripts/surfari-resources.sh
```

The completion reviewer initially returned `VERDICT: CHANGES REQUIRED` because
staged Surfari runtime artifacts failed strict code-signature verification after
package-time `install_name_tool` rewrites. The fix re-signed the staged Surfari
runtime before tarball creation, included `libWebKitSwift.dylib` in the runtime
closure, materialized WebKit framework symlinks that point to top-level runtime
artifacts, removed a dangling WebCore framework symlink from the packaged copy,
and reran package-only validation successfully.

The completion reviewer then found a second required issue: the signing helper
used `codesign ... || true`, so release packaging could still create or publish
a tarball if signing failed. The helper now lets `codesign` failures propagate,
and package-only validation succeeded again with the final SHA above.

The focused re-review returned `VERDICT: APPROVED` with no findings. It verified
that signing failures now propagate, release packaging calls the signer before
tarball creation, package-only validation reran successfully, and the result
records the final SHA.

## Conclusion

Stage 6 is complete. The `1.4.0` package-only release path produces a tarball
that contains TermSurf.app, WebTUI, Roamium, Surfari, Roamium runtime resources,
and Surfari's WebKit runtime closure. The staged Surfari runtime has no broken
symlinks, passes strict code-signature verification, and no longer contains the
development WebKit rpath. Package-only mode skipped upload and Homebrew cask
mutation as intended.

The next experiment should publish GitHub Release `v1.4.0`, update and push the
Homebrew cask, and record the generated release SHA.
