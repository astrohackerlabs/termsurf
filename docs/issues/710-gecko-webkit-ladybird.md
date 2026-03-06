# Issue 710: Gecko, WebKit & Ladybird engine research

## Goal

Determine what it takes to build Roamium equivalents for Gecko (Firefox), WebKit
(Safari), and Ladybird. Each engine gets a C shared library wrapping its
embedding API, plus a Rust binary that links the library and speaks the TermSurf
protocol.

The end state is four browser backends — one per engine — all compatible with
the same board (GUI, Wezboard, etc.):

| Engine   | C library              | Rust binary        | Code name |
| -------- | ---------------------- | ------------------ | --------- |
| Chromium | `libtermsurf_chromium` | Roamium            | (done)    |
| Gecko    | `libtermsurf_gecko`    | TBD (e.g., Recko)  | TBD       |
| WebKit   | `libtermsurf_webkit`   | TBD (e.g., Rebkit) | TBD       |
| Ladybird | `libtermsurf_ladybird` | TBD                | TBD       |

## Background

### Roamium as the template

Roamium (Issue 707) proved the pattern: a ~400-line Rust binary linking a C
shared library (`libtermsurf_chromium`, Issue 708) that wraps the browser
engine's embedding API. The C library exports ~23 functions with C types only
(`ts_init`, `ts_create_tab`, `ts_navigate`, `ts_send_mouse_event`, etc.). The
Rust binary handles Unix socket IPC, protobuf parsing, and process lifecycle.

The same pattern should work for Gecko, WebKit, and Ladybird:

1. **C shared library** — Wraps the engine's embedding/content API. Exports
   `ts_*` functions with identical signatures (or as close as possible) to
   `libtermsurf_chromium`. Lives inside the engine's source tree, built with the
   engine's build system.

2. **Rust binary** — Links the C library. Connects to the board via Unix socket.
   Handles the TermSurf protocol (30 messages). Built with Cargo outside the
   engine tree.

### What the C library must do

Based on `libtermsurf_chromium`'s 23 exported functions, the C library for each
engine must support:

- **Initialization** — Start the engine's event loop, create a browser context
  (profile).
- **Tab lifecycle** — Create tabs (web contents), close tabs, resize viewports.
- **Navigation** — Load URLs, handle redirects.
- **Input forwarding** — Mouse events (click, move, scroll), keyboard events
  (keydown, keyup, repeat).
- **Focus management** — Notify the engine when a tab gains/loses focus.
- **GPU compositing** — Provide a `CAContext` ID (macOS) or equivalent surface
  handle for zero-copy compositing via `CALayerHost`.
- **State callbacks** — Notify the Rust binary when URL changes, loading state
  changes, page title changes, cursor type changes, and tab readiness.
- **DevTools** — Open DevTools for a given tab.
- **Color scheme** — Set dark/light mode preference.

### Key research questions

For each engine:

1. **Embedding API** — What is the official embedding API? How mature and stable
   is it? (Chromium has the Content API; Gecko has GeckoView; WebKit has
   WKWebView / WebKitGTK; Ladybird has LibWeb.)

2. **Headless/hidden rendering** — Can the engine render offscreen or to a
   hidden window while still producing GPU output for compositing? (Chromium
   uses `--hidden` flag with `setAlphaValue:0`.)

3. **CAContext / GPU surface** — Can we get a `CAContext` ID or equivalent
   handle for `CALayerHost` compositing? This is the critical path for 60fps
   zero-copy rendering.

4. **Input injection** — Can we programmatically inject mouse and keyboard
   events? At what level? (Chromium uses `RenderWidgetHost::ForwardMouseEvent`
   and `ForwardKeyboardEvent`.)

5. **Callback hooks** — Can we register callbacks for URL changes, loading
   state, title changes, cursor changes? (Chromium uses `WebContentsObserver`
   subclass.)

6. **DevTools** — How does the engine expose DevTools? Can we open DevTools
   programmatically for a specific tab?

7. **Build system** — What build system does the engine use? How do we add a
   shared library target? (Chromium uses GN/Ninja; Gecko uses moz.build/Make;
   WebKit uses CMake; Ladybird uses CMake.)

8. **Multi-profile** — Can we run multiple browser profiles (separate cookie
   jars, storage) in the same process? (Chromium uses `BrowserContext`; relevant
   for shared session.)

9. **Fork size** — How many files do we need to modify in the engine tree? Can
   we keep the footprint as small as Chromium's (24 files, 8 stock patches)?

10. **Cross-platform** — What platforms does the embedding API support? Does
    compositing work on Linux/Windows?

## Approach

1. Clone all three repos into `vendor/`:
   - `vendor/firefox` — Full clone of `mozilla-firefox/firefox`
   - `vendor/webkit` — Full clone of `WebKit/WebKit`
   - `vendor/ladybird` — Full clone of `LadybirdBrowser/ladybird`

2. Research Gecko's embedding API (GeckoView, libxul, Servo components).

3. Research WebKit's embedding API (WebKitGTK, WKWebView, MiniBrowser).

4. Research Ladybird's embedding API (LibWeb, headless browser).

5. For each engine, map the 10 research questions above to specific source
   locations and answer them.

6. Assess feasibility and estimate the C library surface area for each engine.

## Repos

| Engine          | GitHub                     | Size (full) |
| --------------- | -------------------------- | ----------- |
| Firefox (Gecko) | `mozilla-firefox/firefox`  | ~4.5 GB     |
| WebKit          | `WebKit/WebKit`            | ~11.9 GB    |
| Ladybird        | `LadybirdBrowser/ladybird` | ~418 MB     |

## Experiment 1: Clone all three repos

### Goal

Clone Firefox, WebKit, and Ladybird into `vendor/` so we have local copies to
research. Full clones (not shallow) so we can inspect history if needed.

### Steps

1. Clone Firefox:

```bash
cd ~/dev/termsurf
git clone https://github.com/mozilla-firefox/firefox.git vendor/firefox
```

2. Clone WebKit:

```bash
cd ~/dev/termsurf
git clone https://github.com/WebKit/WebKit.git vendor/webkit
```

3. Clone Ladybird:

```bash
cd ~/dev/termsurf
git clone https://github.com/LadybirdBrowser/ladybird.git vendor/ladybird
```

4. Add all three to `.gitignore` (they're vendor dependencies, not part of the
   TermSurf repo):

```
vendor/firefox/
vendor/webkit/
vendor/ladybird/
```

5. Verify all repos are intact:

```bash
ls vendor/firefox/layout/        # Gecko's layout engine
ls vendor/webkit/Source/WebKit/  # WebKit's main source
ls vendor/ladybird/Libraries/LibWeb/  # Ladybird's web engine
```

### Success criteria

- `vendor/firefox/` exists with Gecko source (look for `layout/`, `dom/`,
  `gfx/`, `parser/`)
- `vendor/webkit/` exists with WebKit source (look for `Source/WebKit/`,
  `Source/WebCore/`, `Source/JavaScriptCore/`)
- `vendor/ladybird/` exists with Ladybird source (look for `Libraries/LibWeb/`,
  `Libraries/LibJS/`)
- All three are gitignored from the main repo

### Result

**Success.** All three repos cloned into `vendor/`:

| Engine   | Size on disk | Files   | Clone type |
| -------- | ------------ | ------- | ---------- |
| Firefox  | 9.1 GB       | 402,481 | Full       |
| WebKit   | 7.4 GB       | 457,677 | Shallow    |
| Ladybird | 495 MB       | 20,808  | Full       |

WebKit required a shallow clone (`--depth=1`) — the full clone (~12 GB pack
file) failed twice with pack file corruption during transfer. Firefox also
failed once but succeeded on retry. Ladybird cloned instantly at 418 MB.

Key directories verified:

- `vendor/firefox/layout/` — Gecko layout engine
- `vendor/webkit/Source/WebKit/` — WebKit framework
- `vendor/ladybird/Libraries/LibWeb/` — Ladybird web engine

## Experiment 2: WebKit architecture audit

### Goal

Answer the 10 research questions for WebKit. Map each question to specific
source locations in `vendor/webkit/`. Determine what it would take to build
`libtermsurf_webkit` — a C shared library wrapping WebKit's embedding API,
following the same pattern as `libtermsurf_chromium`.

### Research plan

Answer these questions by reading source code in `vendor/webkit/`:

**Q1. Embedding API** — WebKit has two embedding APIs on macOS:

- **WKWebView** (modern, `Source/WebKit/`) — The public macOS/iOS API. Runs the
  web engine in a separate process (`WebContent` process). Apple's Safari uses
  this.
- **WebKitLegacy** (`Source/WebKitLegacy/`) — The old in-process API
  (`WebView`). Deprecated but still in the tree.

Research: Can we use WKWebView programmatically from C/C++? Or do we need to go
below the public API and use the internal `WebPage`/`WebPageProxy` layer
directly? Look at `Source/WebKit/UIProcess/` (the "UI process" side that hosts
the web view) and `Source/WebKit/WebProcess/` (the renderer).

Also examine `MiniBrowser` (`Tools/MiniBrowser/`) — WebKit's test browser app.
This is the equivalent of Chromium's Content Shell and will show the minimal
embedding surface.

**Q2. Headless/hidden rendering** — Look for headless or offscreen rendering
modes. Check `WebKitTestRunner` (`Tools/WebKitTestRunner/`) and `MiniBrowser`
for headless flags. Can we create a `WKWebView` without showing a window?

**Q3. CAContext / GPU surface** — This is the critical question. WebKit's
multi-process architecture means the `WebContent` process does the rendering.
Look for:

- `CAContext` or `CARemoteLayerServer` in `Source/WebKit/WebProcess/`
- `CALayerHost` or `CARemoteLayerClient` in `Source/WebKit/UIProcess/`
- The `RemoteLayerTree` infrastructure — WebKit already uses remote layer trees
  for cross-process compositing between `UIProcess` and `WebContent` process.

If WebKit already uses `CAContext` / `CALayerHost` internally, we may be able to
intercept or reuse those layer IDs.

**Q4. Input injection** — How does WKWebView handle input? Look for:

- `simulateMouseDown` / `simulateKeyDown` in test infrastructure
- `WebPageProxy::handleMouseEvent()` / `handleKeyboardEvent()` in UIProcess
- `NativeWebMouseEvent` / `NativeWebKeyboardEvent` construction

**Q5. Callback hooks** — WKWebView has delegate protocols:

- `WKNavigationDelegate` — URL changes, loading state
- `WKUIDelegate` — UI events, alerts, context menus
- Look for C++ equivalents: `WebPageProxy::Observer`, `PageLoadState`,
  `WebFrameProxy`

**Q6. DevTools** — WebKit uses Web Inspector (not Chrome DevTools). Look for:

- `WebInspector` / `WebInspectorProxy` in `Source/WebKit/UIProcess/`
- `Source/WebInspectorUI/` — the inspector frontend
- Remote inspection protocol (`_WKRemoteWebInspectorViewController`)

**Q7. Build system** — WebKit uses CMake. Look at:

- `Source/CMakeLists.txt` — top-level build
- `Source/WebKit/CMakeLists.txt` — WebKit framework target
- `Source/PlatformMac.cmake` — macOS-specific build

Can we add a `libtermsurf_webkit` shared library target alongside the existing
WebKit framework?

**Q8. Multi-profile** — Look for `WKWebsiteDataStore` (the WebKit equivalent of
Chromium's `BrowserContext`). Can we create multiple data stores in one process?

**Q9. Fork size** — Based on the other answers, estimate how many files we'd
need to modify. The ideal is a small library inside the tree with minimal
patches to stock files.

**Q10. Cross-platform** — WebKit builds on macOS (native), Linux (WebKitGTK,
WPE), and iOS. Check which compositing path each platform uses.

### Key source directories to examine

- `Source/WebKit/UIProcess/` — UI process (embedding host)
- `Source/WebKit/WebProcess/` — Web content process (renderer)
- `Source/WebKit/Shared/` — Shared types between processes
- `Source/WebKit/UIProcess/RemoteLayerTree/` — Remote layer compositing
- `Source/WebKit/WebProcess/WebPage/` — WebPage (renderer side)
- `Source/WebCore/` — Core engine (DOM, layout, rendering)
- `Source/WebInspectorUI/` — Web Inspector frontend
- `Tools/MiniBrowser/` — Test browser app
- `Tools/WebKitTestRunner/` — Test runner (headless patterns)

### Success criteria

All 10 research questions answered with specific file paths and code references.
A clear assessment of whether `libtermsurf_webkit` is feasible, and if so, what
the C library surface area would look like.
