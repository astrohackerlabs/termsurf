# Experiment 10: Implement WebKit target URL hover callbacks

## Description

Experiments 8 and 9 implemented JavaScript dialogs and HTTP Basic auth, leaving
target URL changes, cursor updates, renderer crash reporting, and console
messages unsupported. Target URL reporting is the next narrow browser-state gap:
Roamium sends `TargetUrlChanged` when the mouse hovers a link, and WebKit
exposes the needed information through the macOS private `WKUIDelegatePrivate`
`_webView:mouseDidMoveOverElement:withFlags:userInfo:` callback and
`_WKHitTestResult.absoluteLinkURL`.

This experiment should implement `ts_set_on_target_url_changed` by wiring the
WebKit hover-hit-test callback in `libtermsurf_webkit`, then prove it with real
mouse movement over a deterministic local link in the C smoke harness.

This experiment should not create the Surfari Rust binary, modify Ghostboard,
modify `termsurf.proto`, implement cursor updates, implement console messages,
implement renderer crash reporting, or edit `webkit/src`.

## Changes

- Study local WebKit target-hover references:
  - `Source/WebKit/UIProcess/API/Cocoa/WKUIDelegatePrivate.h`;
  - `Source/WebKit/Shared/API/Cocoa/_WKHitTestResult.h`;
  - `Tools/TestWebKitAPI/Tests/WebKit/WKWebView/UIDelegate.mm`;
  - `Tools/TestWebKitAPI/Helpers/cocoa/MouseSupportUIDelegate.mm`.
- Extend `TSUIDelegate` to adopt the private target-hover callback on macOS.
- Use `_WKHitTestResult.absoluteLinkURL.absoluteString` as the target URL. The
  implementation should import the private headers from the built WebKit
  framework when possible, using `<WebKit/WKUIDelegatePrivate.h>` and
  `<WebKit/_WKHitTestResult.h>`.
- Fire `g_callbacks.on_target_url_changed` when the hovered target URL changes.
  The implementation should avoid duplicate callbacks for the same URL.
- Fire an empty string when hover leaves a link if WebKit provides a nil/empty
  hit-test URL for the new hover target.
- Add a deterministic link to the local smoke page at known pixel coordinates.
- Extend the smoke harness to move the pointer over that link and then away from
  it using the existing real Cocoa mouse-event path.
- Keep Experiment 6/7/8/9 smoke coverage intact: lifecycle, navigation, resize,
  focus, mouse, scroll, keyboard, color scheme, JavaScript dialogs, and HTTP
  auth must still pass.
- Update `surfari/libtermsurf_webkit/README.md` so target URL updates move from
  unsupported to implemented only if the real WebKit hover callback is proven.

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
  > logs/issue756-exp10-target-url.log 2>&1
rc=$?
echo "SMOKE_EXIT_STATUS=$rc" >> logs/issue756-exp10-target-url.log
```

The smoke log must prove:

- Experiment 6/7/8/9 evidence still passes.
- WebKit invokes the private hover-hit-test callback after forwarded mouse
  movement over the local link.
- `ts_set_on_target_url_changed` receives the expected link URL exactly once
  when the pointer repeatedly moves over the same link target.
- Moving away from the link clears the target URL with exactly one empty-string
  callback when WebKit emits a nil/empty hover target.
- Duplicate moves over the same link do not emit any duplicate URL callbacks.
- The smoke harness fails, rather than merely logging, if the callback sequence
  is not the expected link URL followed by the expected empty clear.

Verify symbols/linkage and checkout state:

```bash
nm -gU surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg ' _ts_|_ts_webkit_test' | sort
otool -L surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg 'WebKit|JavaScriptCore|libtermsurf'
otool -L surfari/libtermsurf_webkit/build/smoke-test | rg 'WebKit|JavaScriptCore|libtermsurf'
git diff --check
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/10-webkit-target-url-hover.md
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

There is no project-configured formatter for Objective-C++ or C in
`surfari/libtermsurf_webkit`; keep those edits local-style consistent and use
`git diff --check` as the whitespace guard.

## Design Review

Adversarial subagent review, fresh context, read-only.

Initial verdict: **Changes Required**.

Findings:

- Required: verification did not require an exact target URL callback
  sequence/count, so it could fail to prove duplicate suppression or clear
  behavior.
- Required: the `_WKHitTestResult.h` source path was wrong. The correct local
  source path is `Source/WebKit/Shared/API/Cocoa/_WKHitTestResult.h`; the built
  framework exposes the private header as `<WebKit/_WKHitTestResult.h>`.
- Required: formatting hygiene was incomplete because the verification listed
  `git diff --check` but no Markdown formatting check or Objective-C++/C
  formatter note.

Fixes:

- Tightened verification to require exactly one link URL callback, exactly one
  empty clear callback when WebKit emits a nil/empty hover target, no duplicate
  URL callbacks, and smoke-test failure on sequence mismatch.
- Corrected the `_WKHitTestResult.h` source path and documented the intended
  private framework imports.
- Added Prettier verification for the issue docs and documented that no
  project-configured Objective-C++/C formatter exists for
  `surfari/libtermsurf_webkit`.

Re-review verdict: **Approved**. No Required findings remained.

**Pass** = target URL hover callbacks work through WebKit's real hover-hit-test
path, the smoke test exits 0, all prior evidence still passes, the README
reflects support, and `webkit/src` remains unchanged.

**Partial** = target URL extraction requires private headers unavailable from
the built framework, or WebKit does not emit hover callbacks from the current
forwarded mouse-event path. The result must record the exact API/build/input
blocker and the next experiment.

**Fail** = the implementation regresses prior lifecycle/input/focus/dialog/auth
coverage, fakes target URL changes without WebKit hover evidence, or cannot
identify a concrete next step.

## Result

**Result:** Pass

Implemented target URL callbacks in `libtermsurf_webkit` by adopting WebKit's
private macOS `WKUIDelegatePrivate` hover-hit-test callback on `TSUIDelegate`.
The implementation reads `_WKHitTestResult.absoluteLinkURL.absoluteString`,
suppresses duplicate target URL values, suppresses the initial empty non-link
state, and sends an empty string after leaving a previously hovered link.

The smoke harness now places a deterministic link on the navigation test page,
moves over it twice, moves away, and fails unless the callback sequence is
exactly:

1. `https://example.test/surfari-target`
2. empty string

Verification was run with:

```bash
surfari/libtermsurf_webkit/build.sh

mkdir -p logs
DYLD_FRAMEWORK_PATH="$PWD/webkit/src/WebKitBuild/Debug" \
surfari/libtermsurf_webkit/build/smoke-test \
  "$PWD/surfari/libtermsurf_webkit/test-content/index.html" \
  "$PWD/surfari/libtermsurf_webkit/test-content/navigation.html" \
  > logs/issue756-exp10-target-url.log 2>&1
rc=$?
echo "SMOKE_EXIT_STATUS=$rc" >> logs/issue756-exp10-target-url.log
```

Key evidence from `logs/issue756-exp10-target-url.log`:

```text
CALLBACK input_state {"focus":true,"focusIn":false,"blur":true,"move":"120,130","click":"140,150,0","scroll":-120,"key":"a","colorScheme":"dark"}
CALLBACK target_url url=https://example.test/surfari-target
CALLBACK target_url url=
CALLBACK javascript_dialog_state {"alert":"done","confirm":true,"prompt":"surfari-prompt-reply"}
CALLBACK http_auth request_id=4 url=http://127.0.0.1:50382/auth-accept scheme=basic challenger=http://127.0.0.1:50382 realm=surfari proxy=0 first=1 primary=1 navigation=1
CALLBACK http_auth_accept_state Surfari Auth OK:auth-ok
CALLBACK http_auth request_id=5 url=http://127.0.0.1:50382/auth-reject scheme=basic challenger=http://127.0.0.1:50382 realm=surfari proxy=0 first=1 primary=1 navigation=1
SMOKE_PASS initialized=1 tab_ready=1 ca_context=5 url=6 loading_started=4 loading_finished=4 title=3 navigations=4 resized=1 focus=1 input=1 target_url=1 js_dialogs=1 http_auth=1
SMOKE_EXIT_STATUS=0
```

Additional verification:

- `nm -gU surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg ' _ts_|_ts_webkit_test' | sort`
  listed the expected exported `ts_*` and `ts_webkit_test_*` symbols, including
  `_ts_set_on_target_url_changed`.
- `otool -L` showed the library links to
  `@rpath/WebKit.framework/Versions/A/WebKit` and the smoke test links to
  `@rpath/libtermsurf_webkit.dylib`.
- `git diff --check` passed.
- `prettier --check --prose-wrap always --print-width 80 issues/0756-surfari/README.md issues/0756-surfari/10-webkit-target-url-hover.md`
  passed.
- `webkit/src` remained clean at `1452a43959523449099b2616793fd2c5b6a6487e` on
  `webkit-1452a439-issue-756`, with shallow repository state `true`.

## Conclusion

WebKit's real hover-hit-test path works through forwarded Cocoa mouse movement,
so Surfari can now report target URL changes without JavaScript injection or
local synthetic hit testing. The remaining unsupported browser-state gaps are
DevTools, renderer crash reporting, cursor updates, and console messages.

## Completion Review

Adversarial subagent review, fresh context, read-only.

Verdict: **Approved**. No Required findings.
