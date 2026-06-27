# WebKit Workspace

This directory is TermSurf's local WebKit build workspace for Surfari research.

## Agent Instructions

Agent-facing build, branch, and patch workflow instructions live in
[AGENTS.md](AGENTS.md). Read that file before modifying or building WebKit. This
README is the human workspace overview and branch ledger.

## Repository

| Remote | URL                                  |
| ------ | ------------------------------------ |
| origin | https://github.com/WebKit/WebKit.git |

`webkit/src` is an upstream WebKit checkout. TermSurf does not vendor the WebKit
source tree into the main repo; only this README and `webkit/patches/` are
tracked.

## Current State

- Upstream commit: `1452a43959523449099b2616793fd2c5b6a6487e`
- Local branch: `webkit-1452a439-issue-853-exp13`
- Shallow checkout: `true`
- Purpose: Issue 853 Surfari PDF response handoff census

## Layout

```text
webkit/
├── README.md
├── patches/    # tracked TermSurf WebKit patch archives
└── src/        # shallow upstream WebKit checkout, ignored
```

`webkit/src/` is a local checkout of upstream WebKit and is intentionally
ignored by git. WebKit build products are also local-only. Keep durable notes in
this README or in issue documents, not inside the ignored checkout.

## Branch Strategy

WebKit branch names encode the upstream base commit and TermSurf issue:

```text
webkit-{short-upstream-commit}-issue-{N}
```

For follow-up experiments within the same issue, use:

```text
webkit-{short-upstream-commit}-issue-{N}-exp{M}
```

Every issue that modifies WebKit source gets its own branch. Create the branch
from the most relevant current WebKit base, record it in the Branches table, and
archive patches under `webkit/patches/issue-{N}/` after committing inside
`webkit/src`.

Do not commit directly to `main` in `webkit/src` for TermSurf changes. Keep
`main` as the upstream checkout branch unless an issue explicitly records a
temporary exception.

## Branches

| Branch                            | Base commit                                | Issue                                                    | Description                     |
| --------------------------------- | ------------------------------------------ | -------------------------------------------------------- | ------------------------------- |
| `webkit-1452a439-issue-756`       | `1452a43959523449099b2616793fd2c5b6a6487e` | [Issue 756](../issues/0756-surfari/README.md)            | Surfari WebKit integration base |
| `webkit-1452a439-issue-756-exp12` | `1452a43959523449099b2616793fd2c5b6a6487e` | [Issue 756](../issues/0756-surfari/README.md)            | Cursor notification hook        |
| `webkit-1452a439-issue-853-exp11` | `ff39892387ab076d7c0ab3d94fdc1bc5727c9ee3` | [Issue 853](../issues/0853-finish-pdf-support/README.md) | PDF form trace execution proof  |
| `webkit-1452a439-issue-853-exp12` | `3afabe9a5993cf806a33968478dca95a97af94a9` | [Issue 853](../issues/0853-finish-pdf-support/README.md) | PDF path census                 |
| `webkit-1452a439-issue-853-exp13` | `fe82783248bf44eb48635d03d1d38decbcd9b701` | [Issue 853](../issues/0853-finish-pdf-support/README.md) | PDF response handoff census     |

## Patches

`webkit/patches/` contains `git format-patch` output for TermSurf WebKit
branches. Each issue gets its own subdirectory:

```text
webkit/patches/
├── README.md
└── issue-{N}/
    └── *.patch
```

Patch sets are generated from the recorded upstream base commit to the branch
tip. If an issue has no WebKit source commits yet, its patch directory may not
exist or may contain only documentation placeholders.

### Creating an Issue Branch

From the TermSurf repo root:

```bash
git -C webkit/src switch -C webkit-1452a439-issue-756 \
  1452a43959523449099b2616793fd2c5b6a6487e
```

For future issues, replace the branch name and base commit, then update the
Branches table above.

### Generating Patches

After committing WebKit changes inside `webkit/src`:

```bash
rm -rf webkit/patches/issue-{N}
mkdir -p webkit/patches/issue-{N}
git -C webkit/src format-patch {base-commit}..HEAD \
  -o ../../webkit/patches/issue-{N}
```

Then commit the updated patch directory in the main TermSurf repo.

### Applying Patches

From a fresh checkout:

```bash
mkdir -p webkit
git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
git -C webkit/src fetch --depth 1 origin {base-commit}
git -C webkit/src switch -C webkit-{short-base}-issue-{N} {base-commit}
git -C webkit/src am ../../webkit/patches/issue-{N}/*.patch
```

If `git am` reports no patch files, the issue has not archived WebKit source
changes yet.

### Deepening the Checkout

Keep the checkout shallow while experiments only need the current base commit.
Deepen the checkout only when an experiment needs upstream history for patch
archaeology, merge-base analysis, or upstream cherry-picks. Record the reason in
the relevant issue experiment before deepening.

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

## Build Helper

For normal TermSurf development builds, use the repository helper:

```bash
scripts/build.sh webkit
```

That command builds Debug WebKit by delegating to
`webkit/src/Tools/Scripts/build-webkit --debug`. Release builds are available
with:

```bash
scripts/build.sh webkit --release
```

`scripts/build.sh webkit --clean` explicitly cleans WebKit's upstream build
products before rebuilding. Build outputs remain under
`webkit/src/WebKitBuild/`.

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
