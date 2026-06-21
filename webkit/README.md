# WebKit Workspace

This directory is TermSurf's local WebKit build workspace for Surfari research.

## Layout

```text
webkit/
└── src/    # shallow upstream WebKit checkout
```

`webkit/src/` is a local checkout of upstream WebKit and is intentionally
ignored by git. WebKit build products are also local-only. Keep durable notes in
this README or in issue documents, not inside the ignored checkout.

## Bootstrap

From the TermSurf repo root:

```bash
mkdir -p webkit
git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
xcode-select -p
xcodebuild -version
xcodebuild -downloadComponent MetalToolchain
webkit/src/Tools/Scripts/build-webkit --debug
```

Capture the local state after the build:

```bash
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --is-shallow-repository
find webkit/src/WebKitBuild -maxdepth 2 -type d | sort | head -50
git status --short
```

## Verified Environment

Issue 756 Experiment 1 recorded the first verified build result for this VM:

- WebKit commit: `1452a43959523449099b2616793fd2c5b6a6487e`
- Shallow checkout: `true`
- Developer directory: `/Applications/Xcode.app/Contents/Developer`
- Xcode: `26.6` (`17F109`)
- Metal toolchain: `xcodebuild -downloadComponent MetalToolchain` completed
  successfully with Metal Toolchain `17F109`.
- Build command: `webkit/src/Tools/Scripts/build-webkit --debug`
- Build result: pass, `WebKit is now built (17m:21s)`
- Build output: `webkit/src/WebKitBuild/Debug`
- Build products observed include `WebKit.framework`, `WebCore.framework`,
  `JavaScriptCore.framework`, `WebGPU.framework`, `WebKitLegacy.framework`,
  `MiniBrowser.app`, `SwiftBrowser.app`, and `TestWebKitAPI.app`.
