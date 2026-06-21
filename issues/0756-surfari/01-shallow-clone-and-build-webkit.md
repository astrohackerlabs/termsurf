# Experiment 1: Shallow clone and build WebKit

## Description

This experiment proves that the current macOS VM can fetch and build upstream
WebKit from source before any Surfari code is written. The checkout should live
in a top-level `webkit/` workspace, mirroring the existing Chromium layout:

```text
webkit/
└── src/    # shallow upstream WebKit checkout
```

The goal is not to modify WebKit, vendor it into TermSurf history, or integrate
it with Ghostboard. The goal is to establish a repeatable local bootstrap path,
capture the exact dependency/environment requirements, and record whether a
source-built WebKit is viable on this machine.

Official WebKit documentation gives the macOS baseline:

- clone WebKit from `https://github.com/WebKit/WebKit.git`;
- install Xcode and the Xcode command line tools;
- install the Metal toolchain with
  `xcodebuild -downloadComponent MetalToolchain`;
- build with `Tools/Scripts/build-webkit`, using `--debug` for a debug build or
  `--release` for a release build.

This experiment will use a shallow clone because TermSurf only needs a buildable
starting point for the first Surfari spike. If later experiments need upstream
history for patch archaeology, they can deepen the clone or fetch specific
commits.

## Changes

- Create the top-level `webkit/` workspace if it does not already exist.
- Add a tracked `webkit/README.md` with local TermSurf notes for the WebKit
  workspace, including:
  - `webkit/src/` is the upstream WebKit checkout;
  - `webkit/src/` and WebKit build products are local-only and must not be
    committed;
  - the canonical shallow clone and build commands;
  - the verified macOS/Xcode/Zig/Homebrew-relevant environment facts discovered
    during the experiment.
- Update `.gitignore` so `webkit/src/` and likely WebKit build outputs stay out
  of the TermSurf repository. Use the Chromium-style pattern: ignore
  `/webkit/*`, then unignore `/webkit/README.md` so workspace notes remain
  tracked while the upstream checkout and build products remain local.
- Do not change Surfari, Ghostboard, Roamium, webtui, protocol, or WebKit source
  code in this experiment.

## Verification

Run the bootstrap from the TermSurf repo root:

```bash
mkdir -p webkit
git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
xcode-select -p
xcodebuild -version
xcodebuild -downloadComponent MetalToolchain
webkit/src/Tools/Scripts/build-webkit --debug
```

Also capture the exact upstream revision, shallow-clone state, build output
location, and TermSurf repo status from the TermSurf repo root:

```bash
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --is-shallow-repository
find webkit/src/WebKitBuild -maxdepth 2 -type d | sort | head -50
git status --short
```

**Pass** = `webkit/src` is a shallow upstream WebKit checkout, the Metal
toolchain command succeeds or is already satisfied,
`Tools/Scripts/build-webkit --debug` completes successfully, the successful
WebKit commit hash, `rev-parse --is-shallow-repository` result of `true`, and
build output path are recorded in this experiment, and `git status --short`
shows only the intended TermSurf documentation/gitignore changes.

**Partial** = WebKit is cloned and the build starts, but it fails because of a
specific missing dependency, Xcode/toolchain issue, disk/memory limit, or other
environment problem. The result must record the exact failing command, the
important error lines, the dependency or host constraint that appears to be
missing, and the next experiment needed to fix the environment.

**Fail** = WebKit cannot be shallow-cloned into `webkit/src`, the build command
cannot be reached, or the failure mode is too ambiguous to identify an
actionable next step.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

**Verdict:** Changes required.

Findings:

- **Required:** The verification commands changed into `webkit/src`, then later
  used repo-root-relative paths. That would make the revision/build-output
  capture commands point at `webkit/src/webkit/src`, and `git status --short`
  would inspect the WebKit checkout instead of TermSurf. Fixed by keeping all
  verification commands rooted at the TermSurf repo and invoking
  `webkit/src/Tools/Scripts/build-webkit --debug`.
- **Optional:** The pass criteria required a shallow checkout without explicitly
  recording shallow state. Fixed by adding
  `git -C webkit/src rev-parse --is-shallow-repository` and requiring `true`.
- **Optional:** The `.gitignore` hygiene was underspecified. Fixed by specifying
  a Chromium-style ignore rule: ignore `/webkit/*` and unignore
  `/webkit/README.md`.

The reviewer also confirmed the official WebKit build baseline matches this
plan: Xcode, command line tools, Metal toolchain, and
`Tools/Scripts/build-webkit --debug`.

The fixed design was re-reviewed by a fresh adversarial Codex subagent.

**Final verdict:** Approved.

The re-review confirmed that the repo-root command issue, shallow-state capture,
and concrete `.gitignore` pattern are all resolved, with no new required
findings.

## Result

**Result:** Pass

The shallow WebKit checkout and official macOS debug build succeeded on this VM.

Commands and relevant output:

```text
$ git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
Cloning into 'webkit/src'...
Updating files: 100% (456693/456693), done.

$ xcode-select -p
/Applications/Xcode.app/Contents/Developer

$ xcodebuild -version
Xcode 26.6
Build version 17F109

$ xcodebuild -downloadComponent MetalToolchain
Beginning asset download...
Downloaded asset to: /System/Library/AssetsV2/com_apple_MobileAsset_MetalToolchain/5f4a441a6d0a11f2e9b28c67384263afe92320f7.asset/AssetData/Restore/022-21788-058.dmg
Done downloading: Metal Toolchain 17F109.

$ webkit/src/Tools/Scripts/build-webkit --debug
** BUILD SUCCEEDED ** [2.187 sec]

====================================================================
 WebKit is now built (17m:21s).
 To run Safari with this newly-built code, use
 the command "webkit/src/Tools/Scripts/run-safari --debug".
====================================================================
```

Verification artifacts:

```text
$ git -C webkit/src rev-parse HEAD
1452a43959523449099b2616793fd2c5b6a6487e

$ git -C webkit/src rev-parse --is-shallow-repository
true

$ find webkit/src/WebKitBuild -maxdepth 2 -type d | sort | head -50
webkit/src/WebKitBuild
webkit/src/WebKitBuild/ANGLE.build
webkit/src/WebKitBuild/ANGLE.build/Debug
webkit/src/WebKitBuild/Debug
webkit/src/WebKitBuild/Debug/DerivedSources
webkit/src/WebKitBuild/Debug/DumpRenderTree.app
webkit/src/WebKitBuild/Debug/DumpRenderTree.dSYM
webkit/src/WebKitBuild/Debug/DumpRenderTree.resources
webkit/src/WebKitBuild/Debug/ImageDiff.dSYM
webkit/src/WebKitBuild/Debug/InjectedBundleTestWebKitAPI.bundle
webkit/src/WebKitBuild/Debug/InjectedBundleTestWebKitAPI.bundle.dSYM
webkit/src/WebKitBuild/Debug/JavaScriptCore.framework
webkit/src/WebKitBuild/Debug/LLIntOffsets
webkit/src/WebKitBuild/Debug/LayoutTestHelper.dSYM
webkit/src/WebKitBuild/Debug/MiniBrowser.app
webkit/src/WebKitBuild/Debug/MiniBrowser.app.dSYM
webkit/src/WebKitBuild/Debug/SwiftBrowser.app
webkit/src/WebKitBuild/Debug/SwiftBrowser.swiftmodule
webkit/src/WebKitBuild/Debug/TestIPC.dSYM
webkit/src/WebKitBuild/Debug/TestIPC.swiftmodule
webkit/src/WebKitBuild/Debug/TestWGSL.dSYM
webkit/src/WebKitBuild/Debug/TestWGSL.swiftmodule
webkit/src/WebKitBuild/Debug/TestWTF.dSYM
webkit/src/WebKitBuild/Debug/TestWTF.swiftmodule
webkit/src/WebKitBuild/Debug/TestWebKitAPI.app
webkit/src/WebKitBuild/Debug/TestWebKitAPI.app.dSYM
webkit/src/WebKitBuild/Debug/TestWebKitAPI.dSYM
webkit/src/WebKitBuild/Debug/TestWebKitAPI.swiftmodule
webkit/src/WebKitBuild/Debug/TestWebKitAPI.wkbundle
webkit/src/WebKitBuild/Debug/TestWebKitAPI.wkbundle.dSYM
webkit/src/WebKitBuild/Debug/TestWebKitAPIBundle.swiftmodule
webkit/src/WebKitBuild/Debug/TestWebKitAPILibrary.swiftmodule
webkit/src/WebKitBuild/Debug/TestWebKitAPIResources.bundle
webkit/src/WebKitBuild/Debug/Testing.framework
webkit/src/WebKitBuild/Debug/WebCore.framework
webkit/src/WebKitBuild/Debug/WebGPU.framework
webkit/src/WebKitBuild/Debug/WebInspectorUI.framework
webkit/src/WebKitBuild/Debug/WebKit.framework
webkit/src/WebKitBuild/Debug/WebKitLegacy.framework
webkit/src/WebKitBuild/Debug/WebKitSwift.swiftmodule
webkit/src/WebKitBuild/Debug/WebKitTestRunner.dSYM
webkit/src/WebKitBuild/Debug/WebKitTestRunnerApp.app
webkit/src/WebKitBuild/Debug/WebKitTestRunnerInjectedBundle.bundle
webkit/src/WebKitBuild/Debug/WebKitTestRunnerInjectedBundle.bundle.dSYM
webkit/src/WebKitBuild/Debug/WebKitTestSupport
webkit/src/WebKitBuild/Debug/_Testing_AppKit.framework
webkit/src/WebKitBuild/Debug/_Testing_CoreGraphics.framework
webkit/src/WebKitBuild/Debug/_Testing_CoreImage.framework
webkit/src/WebKitBuild/Debug/_Testing_Foundation.framework
webkit/src/WebKitBuild/Debug/_Testing_UIKit.framework

$ du -sh webkit/src webkit/src/WebKitBuild
45G     webkit/src
37G     webkit/src/WebKitBuild

$ df -h .
Filesystem      Size    Used   Avail Capacity iused ifree %iused  Mounted on
/dev/disk2s5   384Gi   230Gi   133Gi    64%    2.7M  1.4G    0%   /System/Volumes/Data

$ git status --short
 M .gitignore
 M issues/0756-surfari/README.md
 M issues/0756-surfari/01-shallow-clone-and-build-webkit.md
?? webkit/
```

The `webkit/` entry is expected because `webkit/README.md` is the only tracked
file under the new workspace; `webkit/src/` and `WebKitBuild/` are ignored by
the new `.gitignore` rule.

## Conclusion

The macOS VM can shallow-clone upstream WebKit into `webkit/src` and complete
the official WebKit debug build from the TermSurf repo root. No source changes
to WebKit, Surfari, Ghostboard, Roamium, webtui, or the protocol were needed.

The next experiment can assume a source-built WebKit exists locally and should
begin researching the compositing hook needed for Surfari, especially where
WebKit owns or exports the layer-hosting context that could map onto
Ghostboard's existing `CAContext` / `CALayerHost` path.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

**Verdict:** Approved.

Findings: none.

The reviewer independently confirmed that:

- the issue README marks Experiment 1 as `Pass`;
- this experiment file has `Result` and `Conclusion` sections;
- `.gitignore` ignores `/webkit/*` and unignores `/webkit/README.md`;
- `webkit/src` is ignored while `webkit/README.md` is trackable;
- local WebKit state matches the recorded commit
  `1452a43959523449099b2616793fd2c5b6a6487e`;
- the checkout is shallow;
- `webkit/src/WebKitBuild/Debug` products exist;
- the result commit had not yet been made at review time.
