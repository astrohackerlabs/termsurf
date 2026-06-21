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
