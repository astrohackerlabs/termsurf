# libtermsurf_webkit

`libtermsurf_webkit` is the macOS WebKit C ABI for WebKit, the WebKit-backed
Astrohacker Terminal engine.

This directory contains the macOS `libtermsurf_webkit` scaffold: a buildable
dynamic library, public C header, and C smoke test that exercise the initial
WebKit browser-view lifecycle through `ts_*` functions compatible with Chromium's
Rust FFI shape.

## Build

Build WebKit first with the repository helper:

```bash
scripts/build.sh webkit
```

Then build the library and smoke test:

```bash
scripts/build.sh webkit-lib
```

To build the wrapper and Rust WebKit binary together:

```bash
scripts/build.sh webkit
```

Release builds use the matching Release WebKit output:

```bash
scripts/build.sh webkit --release
scripts/build.sh webkit --release
```

The direct wrapper command remains available when needed:

```bash
webkit/libtermsurf_webkit/build.sh --configuration Debug
```

Outputs:

```text
webkit/libtermsurf_webkit/build/libtermsurf_webkit.dylib
webkit/libtermsurf_webkit/build/smoke-test
```

## Smoke Test

```bash
DYLD_FRAMEWORK_PATH="$(pwd)/webkit/src/WebKitBuild/Debug" \
  webkit/libtermsurf_webkit/build/smoke-test \
  "$(pwd)/webkit/libtermsurf_webkit/test-content/index.html" \
  "$(pwd)/webkit/libtermsurf_webkit/test-content/navigation.html"
```

The smoke test initializes the library, creates persistent and incognito browser
contexts, creates a WebKit-backed web contents, receives lifecycle callbacks,
navigates between deterministic local pages, resizes the view, forwards
mouse/scroll/keyboard input, verifies page-visible WebKit focus and inactive
state, destroys the objects, and quits.

`DYLD_FRAMEWORK_PATH` is required because WebKit's debug framework has
source-built transitive framework dependencies such as `JavaScriptCore`.

## Current Limitations

Implemented:

- lifecycle entry, task posting, and quit;
- persistent and incognito browser contexts;
- WebKit-backed web contents creation/destruction;
- navigation and resize;
- AppKit first-responder assignment, page-visible focus, and GUI active/inactive
  state;
- mouse move, mouse click, wheel scroll, and keyboard forwarding through Cocoa
  events;
- dark/light appearance assignment through `NSAppearance`;
- tab ready, CA context ID, URL, loading, and title callbacks;
- pointer, hand, and i-beam cursor updates through the TermSurf WebKit cursor
  hook;
- target URL updates through WebKit hover hit testing;
- console message callbacks through a document-start WebKit script-message
  bridge;
- JavaScript alert, confirm, and prompt requests through `WKUIDelegate`, with
  pending request IDs and `ts_reply_javascript_dialog`;
- HTTP Basic auth requests through `WKNavigationDelegate`, with
  Chromium/Chromium-compatible field normalization and `ts_reply_http_auth`;
- renderer crash reporting through WebKit process-termination delegate
  callbacks;
- semantic Back and Forward through `WKWebView.goBack` and
  `WKWebView.goForward`, with authoritative per-view `canGoBack` and
  `canGoForward` callbacks and fail-closed crash state;
- DevTools tab creation through WebKit Inspector's frontend `WKWebView`, exposed
  as a normal Astrohacker Terminal/TermSurf protocol CA context surface.

No unsupported C ABI entry points are currently known in the smoke-tested
WebKit surface. Ghostboard integration remains unproven.

The combined Back/Forward native contract has its own two-view Forward smoke:

```bash
rust/ah-webkitd/libtermsurf_webkit/smoke-test/run-forward-action-smoke.sh
```

It builds the Release wrapper, serves a WebKit-local deterministic fixture, and
proves a Back/Forward history round trip, independent enabled and disabled
state, fresh-navigation clearing, same-document history, view isolation, and
content-process crash/recovery without synthesizing a key event.
