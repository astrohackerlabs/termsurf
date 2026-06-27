# WebKit Workspace

This file is the agent-facing source of truth for TermSurf's WebKit workspace.
Read it before modifying or building anything under `webkit/`.

## Purpose

`webkit/` contains the local WebKit checkout used by Surfari. The source
checkout and build products are local workspace state; TermSurf tracks
documentation and patch archives in the main repo.

## Layout

```text
webkit/
├── AGENTS.md      # agent workflow instructions
├── README.md      # human workspace overview and branch table
├── patches/       # tracked TermSurf WebKit patch archives
└── src/           # upstream WebKit git checkout, ignored by main repo
```

`webkit/src/` is a shallow upstream WebKit checkout. Do not move or rename it
without updating the workspace docs and issue records.

## Current State

- Upstream remote: `origin` -> `https://github.com/WebKit/WebKit.git`
- Base commit: `d144dd782ee6ba6fe20cd04b9c8d3e492f3c4254`
- Active documented branch: `webkit-d144dd78-issue-857`
- Shallow checkout: `true`
- Build output: `webkit/src/WebKitBuild/Debug`
- Main build command: `scripts/build.sh webkit`

Keep the current branch and branch table in `webkit/README.md` current when
creating, switching, or publishing WebKit branches.

## Prerequisites

WebKit builds on macOS require full Xcode and the Metal toolchain.

Check the environment:

```bash
xcode-select -p
xcodebuild -version
xcodebuild -downloadComponent MetalToolchain
```

Issue 756 first verified this VM with:

- developer directory: `/Applications/Xcode.app/Contents/Developer`;
- Xcode `26.6` (`17F109`);
- Metal Toolchain `17F109`.

## Build Rules

Use the repository build helper for normal TermSurf workflows:

```bash
scripts/build.sh webkit
```

That helper delegates to WebKit's own build script:

```bash
webkit/src/Tools/Scripts/build-webkit --debug --only=WebKit --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
```

Use `scripts/build.sh webkit --release` for a Release WebKit build. Use
`scripts/build.sh webkit --clean` only when you explicitly intend to clean the
upstream WebKit build products before rebuilding.

Set `TERMSURF_WEBKIT_ARCH` to override the helper's default `arm64`
architecture.

The helper intentionally builds only the `WebKit` scheme by default because
TermSurf and Surfari need `WebKit.framework`, not WebKit's full test/tool graph.
Set `TERMSURF_WEBKIT_FULL_BUILD=1` when an issue explicitly needs upstream
WebKit's default full target graph, including tools such as TestWebKitAPI and
ImageDiff.

Build outputs stay under `webkit/src/WebKitBuild/`. Do not install frameworks,
apps, or build products outside the repo unless the user explicitly approves.

After switching WebKit release bases, preserve `WebKitBuild` but refresh staged
prerequisite products before building `WebKit.framework`:

```bash
webkit/src/Tools/Scripts/build-webkit --debug --only=bmalloc --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
webkit/src/Tools/Scripts/build-webkit --debug --only=WTF --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
webkit/src/Tools/Scripts/build-webkit --debug --only=JavaScriptCore --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
webkit/src/Tools/Scripts/build-webkit --debug --only=libwebrtc --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
webkit/src/Tools/Scripts/build-webkit --debug --only=WebGPU --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
webkit/src/Tools/Scripts/build-webkit --debug --only=ANGLE --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
webkit/src/Tools/Scripts/build-webkit --debug --only=WebCore --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
scripts/build.sh webkit
```

Issue 857 used this sequence to refresh stale staged bmalloc, WebRTC, WebGPU,
and ANGLE products without running `--clean` or deleting `WebKitBuild`.

Keep the checkout shallow unless an experiment needs upstream history for
merge-base analysis, patch archaeology, or cherry-picks. Record the reason in
the experiment before deepening.

## Setup From Scratch

From the repo root:

```bash
mkdir -p webkit
git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
git -C webkit/src fetch --depth 1 origin d144dd782ee6ba6fe20cd04b9c8d3e492f3c4254
git -C webkit/src switch -C webkit-d144dd78-issue-857 d144dd782ee6ba6fe20cd04b9c8d3e492f3c4254
git -C webkit/src am ../../webkit/patches/issue-857/*.patch
xcode-select -p
xcodebuild -version
xcodebuild -downloadComponent MetalToolchain
scripts/build.sh webkit
```

If applying TermSurf WebKit patches, use the issue patch directory recorded in
`webkit/README.md`.

## Normal Build

From the repo root:

```bash
scripts/build.sh webkit
```

Equivalent direct WebKit command:

```bash
webkit/src/Tools/Scripts/build-webkit --debug --only=WebKit --architecture=arm64 \
  OVERRIDE_ENABLE_MODULE_VERIFIER=NO ENABLE_WK_LIBRARY_MODULE_VERIFIER=NO
```

Capture useful state after a build:

```bash
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
find webkit/src/WebKitBuild -maxdepth 2 -type d | sort | head -50
```

For incremental-build cache diagnosis, capture no-op timings and rebuild actions
without `--clean`:

```bash
mkdir -p ~/dev/termsurf/logs
/usr/bin/time -p scripts/build.sh webkit --release \
  > ~/dev/termsurf/logs/webkit-release-incremental.log 2>&1
rg -n 'CompileC|SwiftCompile|Ld |Libtool|GenerateVerifyModuleInput|BUILD SUCCEEDED|WebKit is now built|real |user |sys' \
  ~/dev/termsurf/logs/webkit-release-incremental.log
```

## Branch Workflow

Every issue that modifies WebKit source gets its own WebKit branch. Do not
commit TermSurf changes directly to `main` or to a stale issue branch.

Branch names encode the upstream base commit and TermSurf issue:

```text
webkit-{short-upstream-commit}-issue-{N}
webkit-{short-upstream-commit}-issue-{N}-exp{M}
```

Typical workflow:

```bash
cd /Users/astrohacker/dev/termsurf-com
git -C webkit/src switch -C webkit-{short-base}-issue-{N} {base-commit}
```

After creating a branch, update `webkit/README.md`:

- current state if this becomes the active documented branch;
- Branches table with branch name, base commit, issue link, and description.

## Patch Archives

TermSurf tracks WebKit source changes as `git format-patch` archives under
`webkit/patches/`.

Each issue patch directory contains patches from the recorded upstream base
commit to the branch tip:

```text
webkit/patches/issue-{N}/
```

Generate patches after committing inside `webkit/src`:

```bash
cd /Users/astrohacker/dev/termsurf-com
rm -rf webkit/patches/issue-{N}
mkdir -p webkit/patches/issue-{N}
git -C webkit/src format-patch {base-commit}..HEAD \
  -o ../../webkit/patches/issue-{N}
```

Then commit in the main repo:

- `webkit/patches/issue-{N}/`;
- `webkit/README.md` branch table/current-state updates;
- issue experiment docs.

Apply patches from a fresh checkout:

```bash
cd /Users/astrohacker/dev/termsurf-com
mkdir -p webkit
git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
git -C webkit/src fetch --depth 1 origin {base-commit}
git -C webkit/src switch -C webkit-{short-base}-issue-{N} {base-commit}
git -C webkit/src am ../../webkit/patches/issue-{N}/*.patch
```

If `git am` reports no patch files, that issue has not archived WebKit source
changes yet.

## Verification Commands

Use these checks before recording a WebKit experiment result:

```bash
cd /Users/astrohacker/dev/termsurf-com
git status --short
git -C webkit/src status --short
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --is-shallow-repository
git diff --check
```

When WebKit source changed, also verify:

```bash
scripts/build.sh webkit
```

Use issue-specific tests in addition to the build.

## Do Not

- Do not commit TermSurf changes directly to WebKit `main`.
- Do not deepen the shallow checkout without recording why in the issue.
- Do not install WebKit build products outside the repo without explicit
  approval.
- Do not change WebKit source without updating branch docs and patch archives.
- Do not create a separate WebKit skill for basic workflow discovery; this file
  is the workspace-local agent source of truth.
