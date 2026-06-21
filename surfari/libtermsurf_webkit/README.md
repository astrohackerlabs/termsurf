# libtermsurf_webkit

`libtermsurf_webkit` is the macOS WebKit C ABI for Surfari.

This directory contains the macOS `libtermsurf_webkit` scaffold: a buildable
dynamic library, public C header, and C smoke test that exercise the initial
WebKit browser-view lifecycle through `ts_*` functions compatible with Roamium's
Rust FFI shape.

## Build

Build WebKit first:

```bash
webkit/src/Tools/Scripts/build-webkit --debug
```

Then build the library and smoke test:

```bash
surfari/libtermsurf_webkit/build.sh
```

Outputs:

```text
surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib
surfari/libtermsurf_webkit/build/smoke-test
```

## Smoke Test

```bash
DYLD_FRAMEWORK_PATH="$(pwd)/webkit/src/WebKitBuild/Debug" \
  surfari/libtermsurf_webkit/build/smoke-test \
  "$(pwd)/surfari/libtermsurf_webkit/test-content/index.html" \
  "$(pwd)/surfari/libtermsurf_webkit/test-content/navigation.html"
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
- target URL updates through WebKit hover hit testing;
- JavaScript alert, confirm, and prompt requests through `WKUIDelegate`, with
  pending request IDs and `ts_reply_javascript_dialog`;
- HTTP Basic auth requests through `WKNavigationDelegate`, with
  Roamium/Chromium-compatible field normalization and `ts_reply_http_auth`.

Still unsupported:

- DevTools;
- renderer crash reporting;
- cursor updates;
- console messages.
