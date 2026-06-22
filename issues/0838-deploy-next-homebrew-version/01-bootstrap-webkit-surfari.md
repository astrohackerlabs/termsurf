# Experiment 1: Bootstrap WebKit and Surfari

## Description

Prove that this machine can build the Surfari stack before changing packaging or
publishing anything. Issue 838 cannot ship Surfari through Homebrew until the
local WebKit checkout, TermSurf WebKit patch, `libtermsurf_webkit`, smoke test,
and Rust `surfari` binary are all verified on this VM.

This experiment covers Issue 838 stages 1 through 3 only:

1. WebKit workspace bootstrap.
2. WebKit debug build.
3. Local `libtermsurf_webkit` and `surfari` builds.

The experiment does not modify release scripts, Homebrew casks, Ghostboard
browser resolution, or package layout. Those are separate release-integration
risks and should be handled only after this local build path is proven.

## Changes

- `webkit/src`:
  - Create a shallow upstream WebKit checkout if it does not already exist.
  - Fetch and switch to documented base commit
    `1452a43959523449099b2616793fd2c5b6a6487e`.
  - Use the documented local branch `webkit-1452a439-issue-756-exp12`, because
    the required archived WebKit patch set belongs to Issue 756 and this
    experiment is consuming that patch set rather than creating new WebKit
    source changes.
  - Apply `webkit/patches/issue-756/*.patch`.
- `surfari/libtermsurf_webkit/build/`:
  - Build `libtermsurf_webkit.dylib` and the `smoke-test` helper with the
    existing `surfari/libtermsurf_webkit/build.sh`.
- `target/debug/surfari`:
  - Build the Rust Surfari binary with `cargo build -p surfari`.
- `issues/0838-deploy-next-homebrew-version/README.md`:
  - Update the Major Stages checklist for stages 1 through 3 after verification.
  - Update the Experiment 1 status after recording the result.
- `issues/0838-deploy-next-homebrew-version/01-bootstrap-webkit-surfari.md`:
  - Record exact commands, resulting WebKit state, verification output, result,
    and conclusion.

## Verification

Run the following checks from the TermSurf repository root.

Prerequisite inspection:

```bash
xcode-select -p
xcodebuild -version
xcodebuild -downloadComponent MetalToolchain
```

WebKit bootstrap:

```bash
mkdir -p webkit
git clone --depth 1 https://github.com/WebKit/WebKit.git webkit/src
git -C webkit/src fetch --depth 1 origin 1452a43959523449099b2616793fd2c5b6a6487e
git -C webkit/src switch -C webkit-1452a439-issue-756-exp12 1452a43959523449099b2616793fd2c5b6a6487e
git -C webkit/src am ../../webkit/patches/issue-756/*.patch
```

If `webkit/src` already exists, first inspect it with:

```bash
git -C webkit/src status --short
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

Then reuse it only if it can be cleanly returned to the same documented branch,
base commit, and patch state without discarding unrelated user work.

WebKit build:

```bash
webkit/src/Tools/Scripts/build-webkit --debug
```

Surfari C ABI build and smoke test:

```bash
surfari/libtermsurf_webkit/build.sh
DYLD_FRAMEWORK_PATH="$(pwd)/webkit/src/WebKitBuild/Debug" \
  surfari/libtermsurf_webkit/build/smoke-test \
  "$(pwd)/surfari/libtermsurf_webkit/test-content/index.html" \
  "$(pwd)/surfari/libtermsurf_webkit/test-content/navigation.html"
```

Surfari Rust build:

```bash
cargo build -p surfari
```

State capture:

```bash
git -C webkit/src status --short
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --is-shallow-repository
git -C webkit/src show --stat --oneline -1
find webkit/src/WebKitBuild -maxdepth 2 -type d | sort | head -50
ls -l surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib \
  surfari/libtermsurf_webkit/build/smoke-test \
  target/debug/surfari
git diff --check
```

Pass criteria:

- Xcode resolves to `/Applications/Xcode.app/Contents/Developer`.
- `xcodebuild -downloadComponent MetalToolchain` completes successfully, or
  reports the Metal toolchain is already installed.
- `webkit/src` is a shallow checkout on `webkit-1452a439-issue-756-exp12`.
- `git -C webkit/src rev-parse HEAD` equals the patched Issue 756 tip produced
  by applying `webkit/patches/issue-756/*.patch` to
  `1452a43959523449099b2616793fd2c5b6a6487e`.
- `git -C webkit/src show --stat --oneline -1` shows the archived Issue 756
  cursor notification patch at the branch tip.
- `git -C webkit/src status --short` has no unexpected changes after the patch
  application and build.
- `webkit/src/Tools/Scripts/build-webkit --debug` completes successfully.
- `surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib` exists.
- `surfari/libtermsurf_webkit/build/smoke-test` exists and passes against the
  deterministic local test pages.
- `target/debug/surfari` exists after `cargo build -p surfari`.
- `git diff --check` reports no whitespace errors in tracked TermSurf files.

Fail criteria:

- WebKit cannot be checked out at the documented commit.
- The Issue 756 WebKit patch archive does not apply cleanly.
- Xcode or Metal prerequisites cannot be verified.
- WebKit debug build fails.
- `libtermsurf_webkit` fails to build.
- The smoke test fails.
- `cargo build -p surfari` fails.

## Design Review

Adversarial subagent review, fresh context, completed before implementation.

Verdict: **Approved**.

Findings:

- Optional: make patch verification more auditable by capturing
  `git -C webkit/src show --stat --oneline -1` and checking that the archived
  Issue 756 cursor notification patch is present.

Resolution:

- Accepted. Added the tip commit/stat capture to verification and pass criteria.

## Result

**Result:** Partial

WebKit bootstrap and build succeeded on this VM.

Prerequisite checks:

- `xcode-select -p` resolved to `/Applications/Xcode.app/Contents/Developer`.
- `xcodebuild -version` reported Xcode `26.6`, build `17F109`.
- `xcodebuild -downloadComponent MetalToolchain` completed successfully.

WebKit checkout state after applying the archived Issue 756 patch:

```text
branch: webkit-1452a439-issue-756-exp12
HEAD: ff39892387ab076d7c0ab3d94fdc1bc5727c9ee3
shallow: true
tip: ff39892387 Notify TermSurf cursor changes
stat: Source/WebKit/UIProcess/mac/PageClientImplMac.mm | 9 +++++++++
status: clean
```

`webkit/src/Tools/Scripts/build-webkit --debug` completed successfully:

```text
** BUILD SUCCEEDED **
WebKit is now built (19m:45s).
```

The build produced the expected debug framework tree under
`webkit/src/WebKitBuild/Debug`, including `WebKit.framework`,
`WebCore.framework`, `JavaScriptCore.framework`, `WebGPU.framework`,
`WebInspectorUI.framework`, and `WebKitLegacy.framework`.

`surfari/libtermsurf_webkit/build.sh` completed and produced:

```text
surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib
surfari/libtermsurf_webkit/build/smoke-test
```

It also emitted this linker warning:

```text
ld: warning: building for macOS-26.0, but linking with dylib
'/System/Library/Frameworks/WebKit.framework/Versions/A/WebKit' which was built
for newer version 26.5
```

The smoke test loaded both deterministic local pages and reported CA context,
URL, title, loading, resize, and focus callbacks, but failed because focus was
not observed:

```text
SMOKE_FAIL focus was not observed
CALLBACK initialized
CALLBACK tab_ready tab_id=1
CALLBACK ca_context_id context_id=3723379767 width=320 height=240
CALLBACK loading_state loading=1 url=file:///Users/astrohacker/dev/termsurf/surfari/libtermsurf_webkit/test-content/index.html
CALLBACK title_changed title=Surfari ABI First Page
CALLBACK loading_state loading=0 url=file:///Users/astrohacker/dev/termsurf/surfari/libtermsurf_webkit/test-content/index.html
CALLBACK loading_state loading=1 url=file:///Users/astrohacker/dev/termsurf/surfari/libtermsurf_webkit/test-content/navigation.html
CALLBACK title_changed title=Surfari ABI Navigation Page
CALLBACK loading_state loading=0 url=file:///Users/astrohacker/dev/termsurf/surfari/libtermsurf_webkit/test-content/navigation.html
CALLBACK ca_context_id context_id=3723379767 width=640 height=480
CALLBACK focus_state {"focus":false,"focusIn":false,"hasFocus":false,"activeElement":""}
```

`cargo build -p surfari` completed successfully and produced
`target/debug/surfari`.

Final verification state:

- `surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib` exists.
- `surfari/libtermsurf_webkit/build/smoke-test` exists.
- `target/debug/surfari` exists.
- `git -C webkit/src status --short` was clean.
- `git diff --check` reported no whitespace errors.
- The main TermSurf working tree had no tracked changes before recording this
  result.

## Conclusion

The machine can build the patched WebKit workspace, `libtermsurf_webkit`, and
the Rust `surfari` binary. The remaining blocker for Stage 3 is the
`libtermsurf_webkit` smoke test's focus assertion. The next experiment should
investigate whether the smoke test is too strict for a headless/non-key window
context, whether the WebKit focus API changed under Xcode/macOS 26.5/26.6, or
whether `libtermsurf_webkit` needs an explicit focus activation step before the
focus check.

## Completion Review

Adversarial subagent review, fresh context, completed after implementation and
result recording.

Verdict: **Approved**.

Findings:

- No required fixes.
- Optional: the result text says "focus callbacks" plural, while the captured
  output shows one `focus_state` callback reporting false focus.

Resolution:

- No change required. The optional wording issue does not affect the recorded
  result or next-step decision.
