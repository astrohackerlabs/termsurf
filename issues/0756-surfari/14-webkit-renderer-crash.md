# Experiment 14: Implement WebKit renderer crash callbacks

## Description

Experiment 13 implemented console messages, leaving renderer crash reporting and
DevTools unsupported in `libtermsurf_webkit`. Renderer crash reporting is the
remaining browser-state callback in the existing C ABI:
`ts_set_on_renderer_crashed`.

WebKit exposes public process termination notification through
`WKNavigationDelegate`'s `webViewWebContentProcessDidTerminate:` and a private
reason-bearing callback through `WKNavigationDelegatePrivate`:
`_webView:webContentProcessDidTerminateWithReason:`. WebKit's own
`WebContentProcessDidTerminate` API tests use private `WKWebView` helpers such
as `_killWebContentProcess` and `_webProcessIdentifier` to trigger deterministic
termination in tests.

This experiment should implement `ts_set_on_renderer_crashed` through WebKit's
real process termination delegate path, add a smoke-only test helper to trigger
termination deterministically, and prove the callback payload through the C
smoke harness.

This experiment should not implement DevTools, the Surfari Rust binary,
Ghostboard integration, protocol changes, or new WebKit source patches.

## Changes

- Study local WebKit renderer-termination references:
  - `Source/WebKit/UIProcess/API/Cocoa/WKNavigationDelegate.h`;
  - `Source/WebKit/UIProcess/API/Cocoa/WKNavigationDelegatePrivate.h`;
  - `Source/WebKit/UIProcess/API/Cocoa/WKWebViewPrivate.h`;
  - `Tools/TestWebKitAPI/Tests/WebKit/WKWebView/WebContentProcessDidTerminate.mm`;
  - `Tools/MiniBrowser/mac/WK2BrowserWindowController.m`.
- Import the private WebKit headers needed by `libtermsurf_webkit`:
  - `<WebKit/WKNavigationDelegatePrivate.h>`;
  - `<WebKit/WKWebViewPrivate.h>`.
- Make `TSNavigationDelegate` adopt `WKNavigationDelegatePrivate`.
- Implement the private `_webView:webContentProcessDidTerminateWithReason:`
  callback when available, and use the public
  `webViewWebContentProcessDidTerminate:` callback as a fallback only if needed.
- Map WebKit termination reasons into stable TermSurf strings:
  - `_WKProcessTerminationReasonExceededMemoryLimit` -> `memory`;
  - `_WKProcessTerminationReasonExceededCPULimit` -> `cpu`;
  - `_WKProcessTerminationReasonRequestedByClient` -> `requested`;
  - `_WKProcessTerminationReasonCrash` -> `crash`;
  - `_WKProcessTerminationReasonExceededSharedProcessCrashLimit` ->
    `crash-limit`;
  - unknown values -> `unknown`.
- Fire `g_callbacks.on_renderer_crashed` with:
  - the owning `WebContents`;
  - the mapped reason string;
  - exit code `0` unless WebKit exposes a reliable exit status for this path;
  - the current `webView.URL.absoluteString`;
  - `visible=true` when the hosting window is visible.
- Add a test-only C helper in `smoke-test/test_support.h` and
  `libtermsurf_webkit.mm`, for example
  `ts_webkit_test_kill_web_content_process(ts_web_contents_t wc)`, that calls
  private `_killWebContentProcessAndResetState` on the wrapped `WKWebView`.
  WebKit's own tests use this reset-state helper for the deterministic
  `_WKProcessTerminationReasonRequestedByClient` path; plain
  `_killWebContentProcess` terminates the process through a different path and
  does not match the expected `requested` reason.
- Extend the C smoke harness to register `ts_set_on_renderer_crashed`, invoke
  the test helper after prior callback coverage has passed, and fail unless:
  - exactly one renderer-crash callback is received;
  - the reason is `requested`;
  - the URL contains the deterministic smoke page URL;
  - `visible` is true;
  - the callback occurs through WebKit's delegate path, not through local fake
    callback invocation. The implementation should emit an observable smoke log
    marker only from `_webView:webContentProcessDidTerminateWithReason:`, and
    the smoke harness must assert that marker is observed before or alongside
    the C ABI callback.
- Keep Experiment 6-13 smoke coverage intact: lifecycle, navigation, resize,
  focus, mouse, scroll, keyboard, color scheme, JavaScript dialogs, HTTP auth,
  target URL hover, cursor callbacks, and console messages must still pass.
- Update `surfari/libtermsurf_webkit/README.md` so renderer crash reporting
  moves from unsupported to implemented only if the smoke proof passes.

## Verification

Start from a clean TermSurf repo root:

```bash
git status --short
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

Build and run the smoke test:

```bash
surfari/libtermsurf_webkit/build.sh

mkdir -p logs
DYLD_FRAMEWORK_PATH="$PWD/webkit/src/WebKitBuild/Debug" \
surfari/libtermsurf_webkit/build/smoke-test \
  "$PWD/surfari/libtermsurf_webkit/test-content/index.html" \
  "$PWD/surfari/libtermsurf_webkit/test-content/navigation.html" \
  > logs/issue756-exp14-renderer-crash.log 2>&1
rc=$?
echo "SMOKE_EXIT_STATUS=$rc" >> logs/issue756-exp14-renderer-crash.log
exit $rc
```

The smoke log must prove:

- Experiment 6-13 evidence still passes.
- The smoke test calls the test-only WebKit termination helper after previous
  callback coverage has completed.
- WebKit invokes the process-termination delegate callback.
- `ts_set_on_renderer_crashed` receives exactly one callback with:
  - `reason=requested`;
  - `exit_code=0`;
  - URL containing `navigation.html` or the last deterministic loaded URL;
  - `visible=1`.
- The smoke harness fails, rather than merely logging, if the callback is
  missing, duplicated, has the wrong reason, has an empty/wrong URL, or reports
  an unexpected visibility value.

Verify symbols/linkage and checkout state:

```bash
nm -gU surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg ' _ts_|_ts_webkit_test' | sort
otool -L surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg 'WebKit|JavaScriptCore|libtermsurf'
otool -L surfari/libtermsurf_webkit/build/smoke-test | rg 'WebKit|JavaScriptCore|libtermsurf'
git diff --check
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/14-webkit-renderer-crash.md
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

There is no project-configured formatter for Objective-C++ or C in
`surfari/libtermsurf_webkit`; keep those edits local-style consistent and use
`git diff --check` as the whitespace guard.

**Pass** = renderer crash callbacks work through WebKit's process termination
delegate path, the smoke test exits 0, all prior evidence still passes, the
README reflects support, and `webkit/src` remains unchanged.

**Partial** = the delegate callback works but WebKit does not expose enough
metadata to provide reason, URL, exit code, or visibility with the expected
strength. The result must record the exact limitation and whether renderer crash
reporting should stay listed as unsupported or partially supported.

**Fail** = the implementation regresses prior lifecycle/input/browser-state
coverage, fakes the callback without WebKit termination evidence, requires
WebKit source changes without prior design, or cannot identify a concrete next
step.

## Design Review

Adversarial review required one change: the original plan called
`_killWebContentProcess` while expecting the WebKit termination reason
`RequestedByClient`. The review found that WebKit's own tests use
`_killWebContentProcessAndResetState` for that reason-bearing path, while plain
`_killWebContentProcess` terminates through a different path. The design now
uses `_killWebContentProcessAndResetState` for the smoke helper.

The review also suggested making the delegate-path proof explicit. The design
now requires an observable marker emitted only from
`_webView:webContentProcessDidTerminateWithReason:` and asserted by the smoke
harness before or alongside the C ABI callback.

## Result

**Result:** Pass

Implemented renderer crash reporting in `libtermsurf_webkit` through WebKit's
process-termination delegate path. `TSNavigationDelegate` now adopts
`WKNavigationDelegatePrivate`, maps WebKit termination reasons to stable
TermSurf strings, suppresses duplicate fallback reports for the same renderer
termination, and forwards `ts_set_on_renderer_crashed` with reason, exit code,
URL, and visibility.

The smoke-only helper
`ts_webkit_test_kill_web_content_process(ts_web_contents_t wc)` calls WebKit's
private `_killWebContentProcessAndResetState`, matching WebKit's own
`RequestedByClient` test path. A second smoke-only helper,
`ts_webkit_test_renderer_crash_delegate_count`, exposes a counter incremented
only inside `_webView:webContentProcessDidTerminateWithReason:` so the smoke
harness can prove the C ABI callback came from WebKit's private delegate path
rather than a local fake callback.

The smoke test now runs renderer crash verification as the final stage after
Experiment 6-13 coverage has passed. The recorded log at
`logs/issue756-exp14-renderer-crash.log` includes:

```text
CALLBACK renderer_crash_trigger helper=_killWebContentProcessAndResetState
CALLBACK renderer_crash_delegate reason=requested
CALLBACK renderer_crashed reason=requested exit_code=0 url=http://127.0.0.1:52204/auth-accept visible=1
SMOKE_PASS initialized=1 tab_ready=1 ca_context=5 url=6 loading_started=4 loading_finished=4 title=3 navigations=4 resized=1 focus=1 input=1 target_url=1 cursor=1 console=1 js_dialogs=1 http_auth=1 renderer_crash=1
SMOKE_EXIT_STATUS=0
```

Verification completed:

```bash
surfari/libtermsurf_webkit/build.sh
DYLD_FRAMEWORK_PATH="$PWD/webkit/src/WebKitBuild/Debug" \
  surfari/libtermsurf_webkit/build/smoke-test \
  "$PWD/surfari/libtermsurf_webkit/test-content/index.html" \
  "$PWD/surfari/libtermsurf_webkit/test-content/navigation.html"
nm -gU surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg ' _ts_|_ts_webkit_test' | sort
otool -L surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg 'WebKit|JavaScriptCore|libtermsurf'
otool -L surfari/libtermsurf_webkit/build/smoke-test | rg 'WebKit|JavaScriptCore|libtermsurf'
git diff --check
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/14-webkit-renderer-crash.md \
  surfari/libtermsurf_webkit/README.md
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

The build completed successfully. It emitted the existing linker warning about
building for macOS 26.0 while linking the system WebKit framework built for
26.5, but produced both
`surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib` and
`surfari/libtermsurf_webkit/build/smoke-test`.

`webkit/src` remained unchanged:

```text
cdfb8cbf86f7c5e52cef0b2f14e8ab30ceeea91c
webkit-1452a439-issue-756-exp12
true
```

`surfari/libtermsurf_webkit/README.md` now lists renderer crash reporting as
implemented. DevTools remains unsupported.

## Completion Review

Adversarial result review approved the completed experiment with no findings.
The reviewer confirmed the working-tree diff was scoped to the six expected
files, the result commit had not yet been made, the implementation uses WebKit's
private process-termination delegate path with a public fallback, the smoke test
enforces exactly one delegate marker and one C callback with the expected
`requested` payload, the verification log contains `SMOKE_EXIT_STATUS=0`, and
`webkit/src` remained unchanged at `cdfb8cbf86f7c5e52cef0b2f14e8ab30ceeea91c` on
branch `webkit-1452a439-issue-756-exp12`.

## Conclusion

Renderer crash reporting is now implemented for the macOS WebKit C ABI and
covered by the smoke harness. The remaining unsupported browser-state callback
in `libtermsurf_webkit` is DevTools. The next experiment should decide whether
to implement DevTools next or pivot to the Surfari Rust binary/protocol audit
now that the core browser lifecycle, input, browser-state, auth, console, and
crash paths are covered at the C ABI layer.
