+++
status = "closed"
opened = "2026-03-05"
closed = "2026-03-05"
+++

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

### Result

#### Q1. Embedding API

WebKit has two embedding APIs on macOS:

- **WKWebView** (modern) — Objective-C class inheriting from `NSView`. This is
  the only supported embedding API. There is no C++ or C wrapper for it. Any use
  requires Objective-C or Swift.
- **Legacy C API** (`Source/WebKit/UIProcess/API/C/WKPage.h`) — Exports C
  functions like `WKPageLoadURL()`, `WKPageCopyTitle()`, etc. However, this API
  is deprecated, not feature-complete, and diverged from the modern Objective-C
  API. Not suitable for new code.

MiniBrowser (`Tools/MiniBrowser/mac/`) shows the minimal surface:

```objc
WKWebViewConfiguration *config = ...;
WKWebView *webView = [[WKWebView alloc] initWithFrame:bounds
                                        configuration:config];
webView.navigationDelegate = self;
webView.UIDelegate = self;
[containerView addSubview:webView];
```

Internally, `WKWebView` wraps `WebViewImpl` (C++, macOS-specific) which creates
`WebPageProxy` (the real engine interface). `WebPageProxy` handles all IPC with
the WebContent process.

**For libtermsurf_webkit:** Write an Objective-C wrapper that creates WKWebView
and exposes `ts_*` C functions. This is the same pattern Chromium uses — the C
library is C++ internally but exports a C interface.

#### Q2. Headless/hidden rendering

WKWebView can render without a visible window. Create an `NSWindow` with
`setAlphaValue:0` and `orderWindow:NSWindowBelow` (identical to what we do with
Chromium's Content Shell via the `--hidden` flag). The WebContent process
continues GPU rendering regardless of window visibility. The `WebKitTestRunner`
in `Tools/WebKitTestRunner/` runs headless tests this way.

**Verdict:** Same approach as Chromium. No special handling needed.

#### Q3. CAContext / GPU surface (CRITICAL)

WebKit's cross-process compositing uses **RemoteLayerTree** — but it does NOT
work like Chromium's direct CAContext sharing.

**How WebKit composites:**

1. WebContent process creates graphics layers
   (`GraphicsLayerCARemote`/`PlatformCALayerRemote`)
2. Layer **properties** are serialized into `RemoteLayerTreeTransaction`
   messages (position, bounds, opacity, transform, etc.)
3. UIProcess receives the transaction and **reconstructs** the layer tree as
   local `CALayer` objects (`RemoteLayerTreeHost::makeNode()`)
4. The root `CALayer` is inserted into the `WKWebView`'s layer hierarchy

WebKit does use `CAContext`/`CALayerHost` internally — but only for hosting
**external content** (AVPlayer video layers, AR models):

```objc
// Source/WebKit/Platform/cocoa/LayerHostingContext.mm
m_context = [CAContext remoteContextWithOptions:contextOptions];

// Source/WebCore/platform/graphics/cocoa/WebCoreCALayerExtras.mm
+ (CALayer *)_web_renderLayerWithContextID:(uint32_t)contextID {
    CALayerHost *layerHost = [CALayerHost layer];
    layerHost.contextId = contextID;
    return layerHost;
}
```

**Key insight:** The `WKWebView` itself contains the reconstructed layer tree.
We don't need to extract a `CAContext` ID from WebKit — we can take the
`WKWebView`'s layer (or its backing layer) and host it via `CAContext` +
`CALayerHost`, or simply position the `WKWebView` at the overlay's pixel
coordinates.

**Two approaches:**

1. **WKWebView as overlay** — Create a `WKWebView`, add it as a subview of the
   terminal window at the overlay's pixel coordinates. WebKit handles all GPU
   compositing internally. No `CAContext` interception needed.

2. **Extract CAContext** — Create a `CAContext` from the `WKWebView`'s root
   layer, export its `contextId`, and use `CALayerHost` in the board. This would
   require forking WebKit to create the `CAContext` wrapper, since WKWebView
   doesn't expose one.

Approach 1 is simpler and requires zero WebKit modifications. The `WKWebView` is
just an `NSView` — position it inside the terminal window and let Window Server
composite it.

#### Q4. Input injection

`WebPageProxy` has direct methods for injecting events:

- `handleMouseEvent(const NativeWebMouseEvent&)` — mouse clicks, moves
- `handleWheelEvent(const WebWheelEvent&)` — scroll
- `handleKeyboardEvent(const NativeWebKeyboardEvent&)` — keyboard

`NativeWebMouseEvent` wraps `NSEvent` with fields: button, position, click
count, modifiers, delta. `NativeWebKeyboardEvent` wraps `NSEvent` with fields:
text, key code, `windowsVirtualKeyCode`, modifiers, auto-repeat.

However, since approach 1 (WKWebView as overlay) places a real `NSView` in the
window, we may not need to inject events at all — macOS will route events to the
`WKWebView` naturally when it's the first responder. We only need to manage
focus (make `WKWebView` first responder in browse mode, resign in control mode).

For programmatic injection (e.g., forwarding events from the terminal view),
`EventSenderProxy` in `Tools/WebKitTestRunner/` shows synthetic event creation.

#### Q5. Callback hooks

**Navigation/loading** — `WKNavigationDelegate` protocol:

- `didStartProvisionalNavigation:` — navigation started
- `didCommitNavigation:` — content arriving
- `didFinishNavigation:` — load complete
- `didFailNavigation:withError:` — load failed

**UI events** — `WKUIDelegate` protocol:

- `createWebViewWithConfiguration:forNavigationAction:windowFeatures:` — new
  window/tab
- `webViewDidClose:` — page closed itself
- `runJavaScriptAlertPanel...` / `runJavaScriptConfirmPanel...` — JS dialogs

**Internal observer** — `PageLoadState::Observer`:

- `didChangeTitle()` — page title changed
- `didChangeActiveURL()` — URL changed
- `didChangeEstimatedProgress()` — loading progress (0.0–1.0)
- `didChangeIsLoading()` — loading state changed

**Cursor changes** — `WebPageProxy` receives `SetCursor(WebCore::Cursor)` IPC
messages from WebContent process, dispatched to `PageClient::setCursor()`.

**KVO properties** on `WKWebView`: `title`, `URL`, `estimatedProgress`,
`loading`, `canGoBack`, `canGoForward` — all observable via KVO, which is the
simplest callback mechanism for Objective-C code.

#### Q6. DevTools

WebKit uses **Web Inspector** (not Chrome DevTools). Every `WKWebView` has a
`_inspector` property returning a `_WKInspector` instance:

```objc
_WKInspector *inspector = webView._inspector;
[inspector connect];     // establish backend connection
[inspector show];        // open inspector UI
[inspector attachRight]; // dock to right side
[inspector detach];      // floating window
```

Configurable via `_WKInspectorConfiguration` (custom process pool, group ID).
The inspector frontend lives in `Source/WebInspectorUI/`.

**For TermSurf:** Open inspector with `[webView._inspector show]`, attach it to
a split pane with `attachRight`/`attachBottom`, or detach for a separate window.

#### Q7. Build system

WebKit uses **CMake**. On macOS it builds as a `.framework` bundle
(`WebKit.framework`). On Linux it builds as a shared library (`.so`).

Top-level structure (`Source/CMakeLists.txt`):

```
bmalloc → WTF → JavaScriptCore → ANGLE → libwebrtc
  → WebInspectorUI → WebCore → WebKit → WebDriver → WebGPU
```

The `WEBKIT_FRAMEWORK()` macro creates the main target. Additional subprocess
targets: `WebKitWebProcess`, `WebKitNetworkProcess`, `WebKitGPUProcess`.

**For libtermsurf_webkit:** Add a new `CMakeLists.txt` alongside
`Source/WebKit/` that creates a shared library target linking `WebKit` and
`WebCore`. Write Objective-C source files that wrap `WKWebView` and export
`ts_*` C functions. Build with `cmake --build`.

#### Q8. Multi-profile

**Yes.** `WKWebsiteDataStore` is WebKit's equivalent of Chromium's
`BrowserContext`:

```objc
// Persistent store by UUID (macOS 14+)
WKWebsiteDataStore *store = [WKWebsiteDataStore
    dataStoreForIdentifier:[[NSUUID alloc] init]];

// Non-persistent (incognito)
WKWebsiteDataStore *ephemeral = [WKWebsiteDataStore nonPersistentDataStore];

// Apply to configuration
config.websiteDataStore = store;
WKWebView *view = [[WKWebView alloc] initWithFrame:frame configuration:config];
```

Multiple `WKWebView` instances can use different data stores in the same
process. Each data store has isolated cookies, cache, localStorage, IndexedDB.
Backed by `WebsiteDataStore` (C++) with a unique `SessionID`.

#### Q9. Fork size

**Potentially zero fork modifications.** Unlike Chromium, where we had to:

- Add `--hidden` flag (patch `shell_platform_delegate_mac.mm`)
- Make DevTools constructor public (patch `shell_devtools_frontend.h`)
- Add CALayerParams callback (patch `render_widget_host_view_mac.h/.mm`)
- Add cursor callback (patch `render_widget_host_impl.h/.cc`)

WebKit's public API already provides everything we need:

- Hidden window: standard `NSWindow` API (no WebKit patch)
- DevTools: `[webView._inspector show]` (public API)
- CALayerHost: not needed (WKWebView is an NSView, position it directly)
- Callbacks: KVO on `title`, `URL`, `estimatedProgress`, `loading`
- Input: `NSView` first responder (natural event routing)
- Multi-profile: `WKWebsiteDataStore` (public API)

The `libtermsurf_webkit` library could live entirely outside the WebKit tree —
just an Objective-C wrapper that links the system `WebKit.framework`. No fork
needed at all.

**However:** If we want to use the open-source WebKit build (not Apple's system
framework), we'd add a `libtermsurf_webkit/` directory to the source tree and
build it with CMake. Still minimal patches — the C library wraps public APIs.

#### Q10. Cross-platform

| Platform    | API                | Compositing          | Build output |
| ----------- | ------------------ | -------------------- | ------------ |
| macOS       | WKWebView (Cocoa)  | CALayer tree         | `.framework` |
| Linux (GTK) | WebKitGTK          | DMA-BUF + Skia       | `.so`        |
| Linux (WPE) | WPE WebKit         | Shared memory + Skia | `.so`        |
| Windows     | HWND-based WebView | CoordinatedGraphics  | `.dll`       |
| iOS         | WKWebView (UIKit)  | CALayer tree         | `.framework` |

Linux uses DMA-BUF for GPU buffer sharing (similar concept to CAContext but for
Linux DRM). Windows uses CoordinatedGraphics with named pipes for IPC.

### Assessment

**WebKit is the easiest engine to embed.** Compared to Chromium:

| Aspect               | Chromium                              | WebKit                               |
| -------------------- | ------------------------------------- | ------------------------------------ |
| Fork modifications   | 8 stock patches, 24 files             | Potentially zero                     |
| C library complexity | ~2,000 lines, 16 files                | ~500 lines estimated                 |
| GPU compositing      | Custom CAContext + CALayerHost        | WKWebView is an NSView (native)      |
| Input handling       | Manual injection via RenderWidgetHost | NSView first responder (automatic)   |
| DevTools             | Had to make constructor public        | `[webView._inspector show]` (public) |
| Multi-profile        | BrowserContext (internal API)         | WKWebsiteDataStore (public API)      |
| Build dependency     | Must fork Chromium source             | Can link system WebKit.framework     |

**The WKWebView-as-overlay approach eliminates most complexity.** Instead of
extracting a CAContext ID and compositing via CALayerHost (like Chromium),
simply create a WKWebView and position it as a subview at the overlay's pixel
coordinates. macOS handles compositing natively.

**Remaining work for libtermsurf_webkit:**

1. Objective-C wrapper (~500 lines) that creates WKWebView, implements
   delegates, exposes `ts_*` C functions
2. Rust binary (could share most of Roamium's code) linking the wrapper
3. No Chromium-style fork — either link system WebKit.framework or add a small
   CMake target to the open-source build

## Experiment 3: Gecko architecture audit

### Goal

Answer the 10 research questions for Gecko (Firefox). Map each question to
specific source locations in `vendor/firefox/`. Determine what it would take to
build `libtermsurf_gecko` — a C shared library wrapping Gecko's embedding API.

### Background

Gecko is fundamentally different from Chromium and WebKit in its embedding
story. Chromium has the Content API, WebKit has WKWebView — both are designed
for third-party embedding. Gecko was historically embeddable (via
`libxul`/`nsIWebBrowser`), but Mozilla dropped embedding support years ago. The
modern embedding API is **GeckoView** — but it's Android-only (Java/Kotlin).

This means the research needs to find what embedding surfaces exist on desktop,
even if they're not officially supported.

### Research plan

**Q1. Embedding API** — Gecko has no official desktop embedding API. Research
the options:

- **libxul** — The monolithic shared library containing all of Gecko. Firefox
  itself links it. Can we load `libxul.so`/`XUL.framework` and call into it?
  Look at `toolkit/xre/` for the XRE (XUL Runtime Environment) bootstrap.
- **GeckoView** — Android-only (`mobile/android/geckoview/`). Uses JNI to
  communicate with Gecko. Not usable on desktop.
- **nsIWebBrowser** — The old XPCOM embedding interface. Deprecated but may
  still exist in the tree. Search for `nsIWebBrowser`, `nsWebBrowser`.
- **Servo components** — Mozilla has been replacing parts of Gecko with Servo
  (Rust). Look at `servo/` for Servo components integrated into Gecko.
- **Content Shell equivalent** — Does Firefox have a minimal browser app like
  Chromium's Content Shell? Check `browser/` vs `toolkit/`.

**Q2. Headless/hidden rendering** — Firefox has headless mode (`--headless`
flag). Look at `widget/headless/` for the headless widget backend. Can we render
to a hidden window while keeping GPU compositing active?

**Q3. CAContext / GPU surface** — This is the critical question. Gecko has its
own compositor:

- Look at `gfx/layers/` — Gecko's layer system (Layers, WebRender)
- `gfx/webrender_bindings/` — Rust bindings for WebRender
- `widget/cocoa/` — macOS widget implementation. Search for `CAContext`,
  `CALayer`, `IOSurface`, compositor setup
- `gfx/layers/composite/` — compositing infrastructure
- Does Gecko use `CAContext` for cross-process compositing on macOS?

**Q4. Input injection** — How does Firefox receive input?

- `widget/cocoa/nsChildView.mm` — macOS event handling
- `widget/InputData.h` — input event data structures
- `dom/events/` — DOM event dispatch
- `widget/nsIWidget.h` — the widget interface that receives platform events
- Can we call `DispatchEvent()` or similar on a widget to inject events?

**Q5. Callback hooks** — How does Firefox expose navigation/loading state?

- `docshell/` — the navigation engine (nsIDocShell, nsIWebNavigation)
- `uriloader/` — URI loading infrastructure
- `dom/base/Document.h` — document state
- `toolkit/components/browser/nsIWebBrowserChrome.idl` — browser chrome
  interface
- Look for observer patterns: `nsIWebProgressListener`, `nsIObserverService`

**Q6. DevTools** — Firefox has its own DevTools:

- `devtools/` — the DevTools frontend and server
- `devtools/server/` — the DevTools server (Remote Debugging Protocol)
- Can we connect to DevTools programmatically via the protocol?
- Firefox supports remote debugging — is there a socket-based protocol?

**Q7. Build system** — Gecko uses `moz.build` + `mach`:

- `moz.build` files throughout the tree
- `build/` — build system infrastructure
- `toolkit/library/moz.build` — how libxul is built
- Can we add a shared library target alongside libxul?

**Q8. Multi-profile** — Firefox supports profiles:

- `toolkit/profile/` — profile management
- `browser/components/profiles/` — profile switching
- Can we run multiple profiles in one process? (Firefox normally uses one
  profile per process.)

**Q9. Fork size** — Based on the other answers, estimate the modification
footprint.

**Q10. Cross-platform** — Gecko runs on macOS, Linux, Windows, Android.

- `widget/cocoa/` — macOS backend
- `widget/gtk/` — Linux/GTK backend
- `widget/windows/` — Windows backend
- `widget/android/` — Android backend
- What compositing does each platform use?

### Key source directories to examine

- `toolkit/xre/` — XUL Runtime Environment (bootstrap, startup)
- `toolkit/library/` — libxul build definition
- `widget/cocoa/` — macOS widget (NSView, CALayer, input events)
- `widget/headless/` — Headless widget backend
- `gfx/layers/` — Layer system and compositing
- `gfx/webrender_bindings/` — WebRender Rust bindings
- `docshell/` — Navigation engine
- `dom/events/` — DOM event dispatch
- `devtools/server/` — DevTools server
- `mobile/android/geckoview/` — GeckoView (Android, for reference)
- `xpcom/` — XPCOM component system

### Success criteria

All 10 research questions answered with specific file paths and code references.
A clear assessment of whether `libtermsurf_gecko` is feasible, and if so, what
the C library surface area would look like. Comparison with the WebKit findings
from Experiment 2.

### Result

#### Q1. Embedding API

Gecko has three embedding options on desktop — two functional, one dead:

- **libxul + nsIWindowlessBrowser (RECOMMENDED)** — `nsIWindowlessBrowser`
  creates a headless browser context (no OS window) backed by `nsWebBrowser`,
  `BrowsingContext`, and a `PuppetWidget`. Created via
  `nsIAppShellService::CreateWindowlessBrowser()`. Used in Firefox's own test
  suite. This is the cleanest path for `libtermsurf_gecko`.

- **libxul + nsIWebBrowser** — The older XPCOM embedding interface. Contrary to
  reports that it's "deprecated", `nsIWebBrowser` is still **fully functional**
  in the tree. Implementation in `toolkit/components/browser/nsWebBrowser.cpp` —
  creates a `BrowsingContext`, attaches a widget, and provides
  `nsIWebNavigation` for URL loading. Requires more setup than
  `nsIWindowlessBrowser` but gives full windowed rendering.

- **GeckoView** — Android-only (`mobile/android/geckoview/`). Uses JNI. Not
  usable on desktop.

**Initialization flow:**

1. Link against `XUL.framework` (macOS) or `libxul.so` (Linux)
2. Call `XRE_GetBootstrap()` (`toolkit/xre/Bootstrap.h`) to get a Bootstrap
   instance
3. Initialize XPCOM via `NS_InitXPCOM()` (`xpcom/build/nsXPCOM.h`)
4. Get `nsIAppShellService` from the XPCOM service manager
5. Call `CreateWindowlessBrowser(false, 0)` to get an `nsIWindowlessBrowser`
6. Access `nsIWindowlessBrowser::docShell` for content manipulation

**Key files:**

- `toolkit/xre/Bootstrap.h` — XRE bootstrap entry point
- `xpcom/build/nsXPCOM.h` — XPCOM initialization
- `xpfe/appshell/nsIWindowlessBrowser.idl` — headless browser interface
- `xpfe/appshell/nsAppShellService.cpp:297-461` — `WindowlessBrowser`
  implementation
- `toolkit/components/browser/nsIWebBrowser.idl` — browser interface
- `toolkit/components/browser/nsWebBrowser.cpp` — browser implementation

**Servo components** (`servo/components/`) are integrated into Gecko's style
system (CSS selector engine, style computation) — not a standalone embedding
target.

#### Q2. Headless/hidden rendering

**Headless mode disables GPU compositing entirely.** When `--headless` or
`MOZ_HEADLESS` is set, Firefox explicitly blocks the GPU process and hardware
acceleration (`gfx/thebes/gfxPlatform.cpp:2504-2550`).

The headless widget backend (`widget/headless/HeadlessWidget.cpp`) uses
`HeadlessCompositorWidget` — purely software rendering, returns `nullptr` for
native data.

**For libtermsurf_gecko:** Do NOT use headless mode. Instead, use a real macOS
widget (or `PuppetWidget` from `nsIWindowlessBrowser`) with a hidden `NSWindow`
(same approach as Chromium: `setAlphaValue:0`, `orderWindow:NSWindowBelow`).
This keeps GPU compositing active.

#### Q3. CAContext / GPU surface (CRITICAL)

**Gecko does NOT use CAContext for cross-process compositing.** This is a
fundamental architectural difference from Chromium.

Gecko's modern compositing path on macOS:

1. **WebRender** (Rust) renders content into **IOSurface** GPU buffers
2. Each `NativeLayerCA` (`gfx/layers/NativeLayerCA.h/.mm`) wraps one IOSurface
   with a double/triple-buffered swap chain
3. `NativeLayerRootCA` manages the `CALayer` tree on the NSView
4. Cross-process compositing uses **IOSurface mach ports** — child process sends
   `IOSurfacePort` (Mach send right) to parent, parent unpacks the
   `IOSurfaceRef` (`gfx/layers/ipc/IOSurfacePort.h`,
   `gfx/layers/NativeLayerRootRemoteMacParent.h/.mm`)
5. Window Server composites CALayers directly from GPU VRAM (zero per-frame
   copy)

**No CAContext IDs are exchanged between processes** — only IOSurface mach
ports.

**For libtermsurf_gecko:** Two approaches:

1. **Custom NativeLayerRoot** — Create a subclass that doesn't render to an
   NSView's CALayer tree but instead exposes the underlying IOSurfaceRefs. Wrap
   those IOSurfaces in a `CAContext` + `CALayerHost` for display in the terminal
   pane. This requires forking Gecko.

2. **NSView overlay** — Like the WebKit approach, create a real Gecko widget
   with an NSView and position it as a subview at the overlay's pixel
   coordinates. This avoids the IOSurface extraction complexity.

**Key files:**

- `gfx/layers/NativeLayerCA.h/.mm` — NativeLayer abstraction
- `gfx/layers/NativeLayerRootRemoteMacParent.h/.mm` — cross-process receiver
- `gfx/layers/ipc/IOSurfacePort.h` — IOSurface IPC serialization
- `gfx/2d/MacIOSurface.h` — IOSurface wrapper
- `gfx/webrender_bindings/RenderCompositorNative.h` — WebRender integration
- `gfx/layers/SurfacePoolCA.h/.mm` — IOSurface pool

#### Q4. Input injection

Gecko has excellent programmatic input injection. The `nsIWidget` interface
(`widget/nsIWidget.h`) provides synthesis methods that don't require real OS
events:

```cpp
// Mouse events (nsIWidget.h:1681)
nsresult SynthesizeNativeMouseEvent(
    LayoutDeviceIntPoint aPoint,
    NativeMouseMessage aNativeMessage,  // ButtonDown, ButtonUp, Move
    MouseButton aButton,
    Modifiers aModifierFlags,
    nsISynthesizedEventCallback* aCallback);

// Keyboard events (nsIWidget.h:1648)
nsresult SynthesizeNativeKeyEvent(
    int32_t aNativeKeyboardLayout,
    int32_t aNativeKeyCode,
    uint32_t aModifierFlags,
    const nsAString& aCharacters,
    const nsAString& aUnmodifiedCharacters,
    nsISynthesizedEventCallback* aCallback);
```

Input data structures (`widget/InputData.h`):

- `MouseInput` — type (MOUSE_MOVE/DOWN/UP), button (PRIMARY/SECONDARY/MIDDLE),
  origin, modifiers
- `ScrollWheelInput` — delta type (LINE/PAGE/PIXEL), mode (INSTANT/SMOOTH),
  deltaX/Y
- `KeyboardInput` — type (KEY_DOWN/PRESS/UP), keyCode, charCode

Lower-level dispatch: `nsIWidget::DispatchInputEvent()` sends events through APZ
(Async Pan/Zoom) then to content.

On macOS, the `ChildView` NSView subclass (`widget/cocoa/nsChildView.h`)
receives native `NSEvent`s and converts them to Gecko's internal format.

**For libtermsurf_gecko:** Call `SynthesizeNativeMouseEvent()` and
`SynthesizeNativeKeyEvent()` on the widget. These accept completion callbacks
via `nsISynthesizedEventCallback*`.

#### Q5. Callback hooks

Gecko has a rich observer system through XPCOM interfaces:

**Navigation/loading — `nsIWebProgressListener`**
(`uriloader/base/nsIWebProgressListener.idl`):

- `onStateChange()` — STATE_START, STATE_REDIRECTING, STATE_TRANSFERRING,
  STATE_STOP (with STATE_IS_DOCUMENT/NETWORK/WINDOW flags)
- `onProgressChange()` — bytes loaded (curSelfProgress/maxSelfProgress)
- `onLocationChange()` — URL changed (with flags: SAME_DOCUMENT, ERROR_PAGE,
  RELOAD, HASHCHANGE)
- `onStatusChange()` — status messages
- `onSecurityChange()` — HTTP/HTTPS transitions

**Registration** — via `nsIWebProgress` (`uriloader/base/nsIWebProgress.idl`):

```cpp
webProgress->AddProgressListener(listener,
    NOTIFY_STATE_DOCUMENT | NOTIFY_PROGRESS | NOTIFY_LOCATION);
```

**Navigation control** — `nsIWebNavigation`
(`docshell/base/nsIWebNavigation.idl`):

- `loadURI(aURI, aLoadURIOptions)` — load a URL
- `currentURI` — current URL
- `document` — DOM document (access `document.title` for page title)
- `canGoBack`, `canGoForward`, `goBack()`, `goForward()`, `reload()`, `stop()`

**Cursor changes** — `nsIWidget::SetCursor()` is called by the rendering engine.
Hook into widget event processing to monitor cursor state.

**For libtermsurf_gecko:** Implement `nsIWebProgressListener` and register it
with `nsIWebProgress::addProgressListener()`. Get URL/title from
`nsIWebNavigation`.

#### Q6. DevTools

Firefox DevTools uses the **Remote Debugging Protocol (RDP)** — a JSON-based
bidirectional protocol:

- **Packet format:** `{"to": actor, "type": type, ...}` (client → server),
  `{"from": actor, ...}` (server → client)
- **Server:** `devtools/server/devtools-server.js` — manages initialization,
  actor registration
- **Connection:** `devtools/server/devtools-server-connection.js` — manages
  connections, request ordering

**Actor hierarchy:**

- Root actor (`devtools/server/actors/root.js`) — entry point
- TabDescriptorActor (`devtools/server/actors/descriptors/tab.js`) — per-tab
- WatcherActor (`devtools/server/actors/watcher.js`) — observes targets
- InspectorActor (`devtools/server/actors/inspector/inspector.js`) — DOM
  inspection

**Remote debugging:** WebSocket transport
(`devtools/server/socket/websocket-server.js`). Connect via TCP, upgrade to
WebSocket, access root actor, call `listTabs`, get tab descriptors.

**For libtermsurf_gecko:** Connect to the DevTools server via WebSocket. The RDP
protocol is well-documented (`devtools/docs/contributor/backend/protocol.md`).
This is more complex than WebKit's `[webView._inspector show]` but fully
programmable.

#### Q7. Build system

Gecko uses **moz.build** + **mach**.

**libxul construction** (`toolkit/library/moz.build`):

```python
@template
def Libxul(name, output_category=None):
    if CONFIG["MOZ_WIDGET_TOOLKIT"] in ("cocoa", "uikit"):
        GeckoFramework(name, output_category=output_category)
        SHARED_LIBRARY_NAME = "XUL"
    else:
        GeckoSharedLibrary(name, output_category=output_category)
        SHARED_LIBRARY_NAME = "xul"
```

On macOS: builds as `XUL.framework`. On Linux: builds as `libxul.so`.

**Adding libtermsurf_gecko:** Create a new directory (e.g., `toolkit/termsurf/`)
with a `moz.build`:

```python
GeckoSharedLibrary("termsurf_gecko")
SHARED_LIBRARY_NAME = "termsurf_gecko"
UNIFIED_SOURCES += ["exports.cpp"]
```

Register in parent `moz.build`: `DIRS += ["termsurf"]`.

**Build commands:** `./mach build` (full), `./mach build faster`
(frontend-only).

#### Q8. Multi-profile

**One profile per process** — same as Chromium. The profile service
(`toolkit/profile/nsToolkitProfileService.cpp`) binds to **one profile directory
per launch**:

```idl
interface nsIToolkitProfileService {
    nsIToolkitProfile getProfileByName(AUTF8String aName);
    nsIToolkitProfile createProfile(nsIFile aRootDir, AUTF8String aName);
    readonly nsIToolkitProfile currentProfile;
};
```

Each profile has a directory lock (`nsProfileLock`). `BrowsingContext`
(`docshell/base/BrowsingContext.h`) manages DOM window/frame hierarchy, not
profile isolation.

**For libtermsurf_gecko:** Each profile requires a separate Gecko process. This
is exactly how TermSurf already works — one Roamium process per profile — so
it's a perfect fit.

#### Q9. Fork size

**Moderate fork — significantly larger than WebKit, comparable to Chromium.**

Estimated modifications:

1. **New directory** — `toolkit/termsurf/` with `libtermsurf_gecko` C library
   (~1,000–1,500 lines). Larger than WebKit's ~500-line estimate because:
   - Must initialize XPCOM and XRE bootstrap (WebKit: just create WKWebView)
   - Must implement `nsIWebProgressListener` (WebKit: KVO observing)
   - Must call `SynthesizeNative*Event()` (WebKit: natural NSView events)

2. **GPU compositing** — If using the NSView overlay approach, zero
   modifications. If extracting IOSurfaces for CALayerHost compositing, moderate
   patches to `gfx/layers/NativeLayerCA.mm` and compositor setup.

3. **Build integration** — New `moz.build` files, register in parent directory.
   Minimal.

4. **No XPCOM interface changes needed** — all required interfaces
   (`nsIWebProgressListener`, `nsIWebNavigation`, `nsIWidget`) are public.

**Estimate:** \~5–10 modified stock files (if NSView overlay), or \~15–20 (if
IOSurface extraction). New code: \~1,000–1,500 lines in C library + existing
Roamium Rust binary (\~400 lines reusable).

#### Q10. Cross-platform

Gecko has highly portable widget backends:

| Platform | Backend   | Compositing                               | Key files          |
| -------- | --------- | ----------------------------------------- | ------------------ |
| macOS    | Cocoa     | CALayer + IOSurface via NativeLayerCA     | `widget/cocoa/`    |
| Linux    | GTK       | Wayland EGL + DMA-BUF, or X11 + Cairo/EGL | `widget/gtk/`      |
| Windows  | Win32     | Direct3D 11 + DXGI swapchain              | `widget/windows/`  |
| Android  | GeckoView | SurfaceTexture (Java/JNI)                 | `widget/android/`  |
| Headless | Software  | BasicCompositor (CPU only)                | `widget/headless/` |

The `CompositorWidget` abstraction (`widget/CompositorWidget.h`) provides a
platform-neutral interface:

- `GetNativeLayerRoot()` — native compositing backend
- `PreRender()` / `PostRender()` — per-frame lifecycle hooks
- `StartRemoteDrawing()` / `EndRemoteDrawing()` — software fallback

**For libtermsurf_gecko:** Target the `CompositorWidget` abstraction for
cross-platform code. Platform-specific code only needed for GPU zero-copy output
(CALayerHost on macOS, DMA-BUF on Linux Wayland, DXGI on Windows).

The NSView overlay approach (Q3 approach 2) would need platform-specific overlay
code per platform (NSView on macOS, GtkWidget on Linux GTK, HWND child on
Windows).

### Assessment

**Gecko is significantly harder to embed than WebKit, and moderately harder than
Chromium.**

| Aspect               | Chromium                  | WebKit                  | Gecko                                  |
| -------------------- | ------------------------- | ----------------------- | -------------------------------------- |
| Embedding API        | Content API (C++)         | WKWebView (Obj-C)       | libxul + XPCOM (C++)                   |
| Official support     | Yes (Content API)         | Yes (WKWebView)         | No (dropped years ago)                 |
| Fork modifications   | 8 patches, 24 files       | Potentially zero        | 5-20 files                             |
| C library complexity | ~2,000 lines              | ~500 lines est.         | ~1,000-1,500 lines est.                |
| GPU compositing      | CAContext + CALayerHost   | NSView (native)         | IOSurface mach ports or NSView overlay |
| Input handling       | Manual injection          | NSView first responder  | SynthesizeNative*Event()               |
| DevTools             | Had to expose constructor | `_inspector` (public)   | RDP over WebSocket                     |
| Multi-profile        | One per process           | Multiple in one process | One per process                        |
| Build system         | GN/Ninja                  | CMake                   | moz.build/mach                         |
| Init complexity      | Low (Content API)         | Very low (WKWebView)    | High (XPCOM bootstrap)                 |

**Key concerns:**

1. **No official embedding support** — Mozilla dropped desktop embedding years
   ago. We'd be using internal APIs that could change between versions. This is
   the biggest risk.

2. **XPCOM complexity** — Initializing Gecko requires bootstrapping XPCOM,
   loading libxul, getting service managers. Much heavier than WebKit's
   `[[WKWebView alloc] init]`.

3. **No direct CAContext** — GPU compositing requires either the NSView overlay
   approach (simpler, like WebKit) or IOSurface extraction (complex, requires
   forking the compositor).

4. **WebRender (Rust) in the rendering path** — Gecko's renderer is written in
   Rust. Any compositor modifications require understanding both C++ and Rust
   codebases.

**Recommendation:** If building `libtermsurf_gecko`, use the NSView overlay
approach (like WebKit) to avoid the IOSurface extraction complexity. Initialize
via `nsIWindowlessBrowser` but with a real widget for GPU compositing. This
gives a moderate-effort integration path comparable to the WebKit approach,
though with higher initialization complexity and no official API stability
guarantees.

## Experiment 4: Ladybird architecture audit

### Goal

Answer the 10 research questions for Ladybird. Map each question to specific
source locations in `vendor/ladybird/`. Determine what it would take to build
`libtermsurf_ladybird` — a C shared library wrapping Ladybird's embedding API.

### Background

Ladybird is fundamentally different from the three established engines. It's a
new browser built from scratch — no legacy embedding API, no decades of
accumulated abstraction layers. The codebase is relatively small (~495 MB vs
Firefox's 9.1 GB or WebKit's 7.4 GB) and written in modern C++ with some Rust.

Ladybird uses a multi-process architecture:

- **UI process** — `UI/AppKit/` (macOS), `UI/Qt/` (Linux/cross-platform)
- **WebContent process** — Renders pages, runs JavaScript (LibWeb + LibJS)
- **WebDriver process** — Browser automation
- **RequestServer** — Network requests
- **ImageDecoder** — Image decoding

The key library is **LibWebView** (`Libraries/LibWebView/`) — this is Ladybird's
embedding layer, analogous to Chromium's Content API or WebKit's WKWebView.

### Research plan

**Q1. Embedding API** — Ladybird's embedding surface is `LibWebView`:

- `Libraries/LibWebView/ViewImplementation.h` — The base class for all web
  views. This is the primary embedding interface.
- `Libraries/LibWebView/WebContentClient.h` — IPC client that talks to the
  WebContent process.
- `UI/AppKit/Application/` — macOS application code showing how AppKit uses
  LibWebView.
- `UI/Qt/` — Qt frontend showing the cross-platform embedding path.
- How does a host application create a web view, load a URL, and receive
  callbacks?

**Q2. Headless/hidden rendering** — Can Ladybird render offscreen?

- `Libraries/LibWebView/` — Look for headless or offscreen view implementations
- `Utilities/` — Check for headless browser utilities
- `Tests/` — Test infrastructure may use headless rendering
- Does Ladybird support rendering without a visible window?

**Q3. CAContext / GPU surface** — How does Ladybird composite on macOS?

- `UI/AppKit/Interface/` — Look for NSView subclasses, CALayer usage
- `Libraries/LibGfx/` — Graphics primitives, painting, GPU surfaces
- `Libraries/LibWebView/` — How does the WebContent process send rendered output
  to the UI process?
- Does Ladybird use GPU compositing at all, or is it CPU/Skia-only?
- Search for `CAContext`, `CALayerHost`, `IOSurface`, `Metal`, `Skia`
- Look at `Libraries/LibWeb/Painting/` — how does painting work?

**Q4. Input injection** — How does Ladybird receive input?

- `Libraries/LibWebView/ViewImplementation.h` — Look for mouse/keyboard event
  methods
- `Libraries/LibWeb/Page/EventHandler.h` — DOM-level event handling
- `UI/AppKit/Interface/` — How does AppKit forward events to LibWebView?
- Can we call input methods directly on ViewImplementation?

**Q5. Callback hooks** — How does Ladybird notify the host of state changes?

- `Libraries/LibWebView/ViewImplementation.h` — Look for callback members,
  virtual methods, or observer patterns
- Search for `on_title_change`, `on_url_change`, `on_load_start`,
  `on_load_finish`, `on_cursor_change` or similar
- `Libraries/LibWebView/WebContentClient.h` — IPC messages from WebContent to UI

**Q6. DevTools** — Ladybird has its own DevTools:

- `Libraries/LibDevTools/` — DevTools library
- `Libraries/LibWebView/` — Look for inspector/devtools integration
- Does Ladybird use Chrome DevTools Protocol (CDP), or its own protocol?
- Can we open DevTools programmatically for a specific tab?

**Q7. Build system** — Ladybird uses CMake:

- `CMakeLists.txt` — Root build configuration
- `Libraries/CMakeLists.txt` — Library targets
- `Libraries/LibWebView/CMakeLists.txt` — LibWebView build
- Can we add a `libtermsurf_ladybird` shared library target?

**Q8. Multi-profile** — Does Ladybird support multiple browser profiles?

- Search for profile, data store, cookie jar, or session concepts
- `Libraries/LibWebView/` — Look for per-session or per-context isolation
- Can we create multiple isolated browser contexts in one process?

**Q9. Fork size** — Based on the other answers, estimate the modification
footprint. Ladybird's smaller codebase may make forking easier.

**Q10. Cross-platform** — Ladybird targets macOS, Linux, and potentially
Windows:

- `UI/AppKit/` — macOS frontend
- `UI/Qt/` — Qt frontend (Linux, cross-platform)
- `UI/Android/` — Android frontend
- What compositing does each platform use?
- How portable is `LibWebView`?

### Key source directories to examine

- `Libraries/LibWebView/` — Embedding layer (ViewImplementation,
  WebContentClient)
- `Libraries/LibWeb/` — Core web engine (DOM, layout, painting)
- `Libraries/LibWeb/Painting/` — Painting/rendering pipeline
- `Libraries/LibGfx/` — Graphics primitives
- `Libraries/LibDevTools/` — DevTools
- `Libraries/LibIPC/` — IPC infrastructure
- `UI/AppKit/` — macOS frontend
- `UI/Qt/` — Qt frontend
- `Utilities/` — Utility programs (headless browser?)

### Success criteria

All 10 research questions answered with specific file paths and code references.
A clear assessment of whether `libtermsurf_ladybird` is feasible, and if so,
what the C library surface area would look like. Comparison with the Chromium,
WebKit, and Gecko findings from Experiments 2–3.

### Result

#### Q1. Embedding API

Ladybird has a clean, modern embedding API built around **`ViewImplementation`**
(`Libraries/LibWebView/ViewImplementation.h`) — an abstract C++ base class that
any frontend implements. This is the primary embedding surface.

**Key methods:**

- `load(URL::URL const&)` — Load a URL
- `load_html(StringView)` — Load raw HTML
- `reload()` — Reload current page
- `traverse_the_history_by_delta(int)` — Navigate back/forward
- `enqueue_input_event(Web::InputEvent)` — Send mouse/keyboard/drag events
- `set_window_position(Gfx::IntPoint)` / `set_window_size(Gfx::IntSize)`
- `set_zoom(double)` — Zoom level
- `initialize_client()` — Spawn WebContent process and connect IPC

**Platform bridges:**

- **macOS:** `WebViewBridge` (`UI/AppKit/Interface/LadybirdWebViewBridge.h`) —
  extends `ViewImplementation` with Metal rendering, IOSurface access, DPI
  handling. Created via `WebViewBridge::create(screen_rects, dpi, max_fps)`.
- **Qt:** `WebContentView` (`UI/Qt/WebContentView.h`) — extends both `QWidget`
  and `ViewImplementation`.
- **Android:** `WebViewImplementationNative`
  (`UI/Android/src/main/cpp/WebViewImplementationNative.h`) — JNI bridge.

**Embedding pattern (macOS):**

```cpp
auto bridge = WebViewBridge::create(screen_rects, dpi, max_fps);
bridge->initialize_client();  // spawn WebContent process
bridge->set_viewport_rect(visible_area);
bridge->load(url);
// When on_ready_to_paint fires:
auto paintable = bridge->paintable();  // {bitmap, size, iosurface_ref}
```

**IPC:** `WebContentClient` (`Libraries/LibWebView/WebContentClient.h`) manages
communication with the WebContent renderer process. Each ViewImplementation
holds one. Messages: `async_load_url`, `async_handle_input_event`,
`async_take_screenshot`, etc.

**For libtermsurf_ladybird:** Subclass `ViewImplementation` or use
`WebViewBridge` directly. The API is clean C++ — wrap it with `ts_*` C
functions. Much simpler than Chromium's Content API or Gecko's XPCOM.

#### Q2. Headless/hidden rendering

**Yes.** Ladybird supports headless mode via `HeadlessMode` enum
(`Libraries/LibWebView/Options.h`):

```cpp
enum class HeadlessMode {
    Screenshot,  // Render once, save, exit
    LayoutTree,  // Output layout tree
    Text,        // Output rendered text
    Manual,      // Headless but stay running
    Test,        // Automated testing
};
```

`ViewImplementation` is abstract and doesn't require a window. Its only abstract
methods are `viewport_size()`, `to_content_position()`, `to_widget_position()`,
`update_zoom()`, and `initialize_client()`. All can be implemented for headless
use.

The WebContent process accepts `--headless` flag
(`Services/WebContent/main.cpp`). WebDriver (`Services/WebDriver/`) runs a full
headless browser for automation.

**For libtermsurf_ladybird:** Same approach as Chromium/Gecko — use a hidden
`NSWindow` to keep GPU compositing active while rendering offscreen.

#### Q3. CAContext / GPU surface (CRITICAL)

**Ladybird already renders to IOSurfaces with Metal on macOS.** This is the most
GPU-friendly architecture of all four engines.

**Rendering pipeline:**

1. WebContent process creates IOSurface backing stores
   (`Libraries/LibWeb/Painting/BackingStoreManager.cpp`) — two surfaces
   (front/back) for double buffering
2. Skia renders with Metal backend directly into IOSurface GPU memory
   (`Libraries/LibGfx/MetalContext.mm`) — zero-copy CPU-GPU handoff
3. IOSurface Mach ports sent to UI process via Mach message IPC
4. UI process reconstructs IOSurfaceRef and creates Metal textures from it
5. `WebViewBridge::paintable()` returns `{bitmap, size, iosurface_ref}` — the
   `iosurface_ref` is the raw `IOSurfaceRef` pointer

**macOS rendering paths** (`UI/AppKit/Interface/LadybirdWebView.mm`):

- **Metal path (GPU):** IOSurface → Metal texture →
  `[blitEncoder copyFromTexture:]` → CAMetalLayer drawable
- **CPU fallback:** IOSurface → CGImage → CALayer.contents

**No CAContext export yet** — Ladybird uses Metal blit to CAMetalLayer, not
CAContext/CALayerHost. But the IOSurface is already available via `paintable()`,
so we have two integration options:

1. **IOSurface → CAContext** — Take the IOSurfaceRef from `paintable()`, create
   a `CAContext` wrapping it, export the context ID for CALayerHost in the
   board. Small fork modification.
2. **NSView overlay** — Position the LadybirdWebView as a subview in the
   terminal window at overlay coordinates. Zero fork modifications.

**Key files:**

- `Libraries/LibWeb/Painting/BackingStoreManager.cpp` — IOSurface allocation
- `Libraries/LibCore/IOSurface.cpp` — IOSurface wrapper (BGRA8888, Mach ports)
- `Libraries/LibGfx/MetalContext.mm` — Metal texture from IOSurface
- `UI/AppKit/Interface/LadybirdWebViewBridge.h:43-48` — `Paintable` struct
- `UI/AppKit/Interface/LadybirdWebView.mm:968-1061` — Metal/CPU render paths

#### Q4. Input injection

**Direct programmatic injection — no OS events needed.** Call
`enqueue_input_event()` on ViewImplementation with constructed event objects:

```cpp
// Mouse event
Web::MouseEvent event {
    .type = Web::MouseEvent::Type::MouseDown,
    .position = { 100, 200 },
    .screen_position = { 500, 600 },
    .button = Web::UIEvents::MouseButton::Primary,
    .buttons = Web::UIEvents::MouseButton::Primary,
    .modifiers = Web::UIEvents::KeyModifier::Mod_None,
    .wheel_delta_x = 0, .wheel_delta_y = 0,
};
view->enqueue_input_event(move(event));

// Keyboard event
Web::KeyEvent key {
    .type = Web::KeyEvent::Type::KeyDown,
    .key = Web::UIEvents::KeyCode::Key_A,
    .modifiers = Web::UIEvents::KeyModifier::Mod_None,
    .code_point = 'A',
    .repeat = false,
};
view->enqueue_input_event(move(key));
```

**Event types** (`Libraries/LibWeb/Page/InputEvent.h`):

- `MouseEvent` — MouseDown, MouseUp, MouseMove, MouseLeave, MouseWheel,
  DoubleClick, TripleClick
- `KeyEvent` — KeyDown, KeyUp
- `DragEvent` — DragStart, DragMove, DragEnd, Drop
- `PinchEvent` — position + scale_delta

Events are queued in `m_pending_input_events` and dispatched asynchronously to
WebContent via IPC (`async_mouse_event`, `async_key_event`, etc.).

**For libtermsurf_ladybird:** Construct `Web::MouseEvent`/`Web::KeyEvent`
structs and call `enqueue_input_event()`. Cleanest input API of all four
engines.

#### Q5. Callback hooks

Ladybird uses **`Function<>` callbacks** (AK's equivalent of `std::function`) as
public members on `ViewImplementation`. Over 40 named hooks. The ones relevant
to TermSurf:

**Navigation/loading:**

- `on_load_start` — `Function<void(URL::URL const&, bool)>` (URL + redirect
  flag)
- `on_load_finish` — `Function<void(URL::URL const&)>`
- `on_url_change` — `Function<void(URL::URL const&)>`
- `on_title_change` — `Function<void(Utf16String const&)>`

**Rendering:**

- `on_ready_to_paint` — `Function<void()>` (frame ready for display)
- `on_favicon_change` — `Function<void(Gfx::Bitmap const&)>`

**Cursor/UI:**

- `on_cursor_change` — `Function<void(Gfx::Cursor const&)>`
- `on_link_hover` / `on_link_unhover` — link hover state

**Tab/window:**

- `on_new_web_view` —
  `Function<String(ActivateTab, WebViewHints, Optional<u64>)>` (target=\_blank)
- `on_activate_tab` / `on_close`

**Input completion:**

- `on_finish_handling_key_event` — `Function<void(Web::KeyEvent const&)>`
  (unhandled key events bubble back to host)

**Dispatch pattern:**

```cpp
// In WebContentClient (receives IPC from WebContent process)
void WebContentClient::did_change_title(u64 page_id, Utf16String title) {
    if (auto view = view_for_page_id(page_id); view.has_value()) {
        if (view->on_title_change)
            view->on_title_change(title);
    }
}
```

**For libtermsurf_ladybird:** Assign lambdas to the callback members. This is
the simplest callback system of all four engines — no delegates, no XPCOM
observers, no protocol registrations.

#### Q6. DevTools

Ladybird uses the **Firefox DevTools Protocol** (not Chrome DevTools Protocol):

- TCP server on configurable port (default 6000,
  `Libraries/LibWebView/Options.h:70`)
- Wire format: `{length}:{json_payload}` (length-prefixed JSON)
- Only one DevTools client at a time
- Actor-based: 27 actors in `Libraries/LibDevTools/Actors/`

**Key actors:** RootActor, TabActor, InspectorActor, WalkerActor, NodeActor,
ConsoleActor, PageStyleActor, NetworkParentActor, AccessibilityActor

**Opening DevTools programmatically:**

- `Application::toggle_devtools_enabled()`
  (`Libraries/LibWebView/Application.h:133`)
- `ViewImplementation::did_connect_devtools_client()` notifies when connected
- Connect to `localhost:6000`, get root actor, call `listTabs`, inspect

**For libtermsurf_ladybird:** Connect to the DevTools TCP server. Similar
complexity to Gecko's RDP (both JSON over TCP). Different from WebKit's simple
`[webView._inspector show]`.

#### Q7. Build system

Ladybird uses **CMake** (3.25+). Clean macro system:

```cmake
# Libraries/LibWebView/CMakeLists.txt
ladybird_lib(LibWebView webview EXPLICIT_SYMBOL_EXPORT)
```

The `ladybird_lib()` macro (`Meta/CMake/targets.cmake:189`) creates shared or
static library targets with automatic export header generation.

**Adding libtermsurf_ladybird:**

```cmake
# Libraries/LibTermsurfLadybird/CMakeLists.txt
set(SOURCES TermSurfBrowser.cpp)
ladybird_lib(LibTermsurfLadybird termsurfladybird EXPLICIT_SYMBOL_EXPORT)
target_link_libraries(LibTermsurfLadybird PRIVATE LibCore LibWebView)
```

Register in `Libraries/CMakeLists.txt`: `add_subdirectory(LibTermsurfLadybird)`.

Output: `liblagom-termsurfladybird.dylib`.

**For libtermsurf_ladybird:** Easiest build integration of all four engines.
CMake is familiar, the macro system handles symbol export automatically.

#### Q8. Multi-profile

**One profile per process** — same as Chromium and Gecko. Ladybird uses global
singletons for storage:

- `Application::cookie_jar()` — single global `CookieJar` per process
  (`Libraries/LibWebView/CookieJar.h`)
- `Application::storage_jar()` — single global `StorageJar` per process
  (`Libraries/LibWebView/StorageJar.h`)

Storage is keyed by origin (domain), not by profile. No multi-profile API
exists.

**For libtermsurf_ladybird:** Each profile requires a separate Ladybird process.
This is exactly how TermSurf already works — one process per profile — so it's a
perfect fit.

#### Q9. Fork size

**Minimal fork — smallest of all four engines.**

Estimated modifications:

1. **New library** — `Libraries/LibTermsurfLadybird/` with C wrapper (~300–500
   lines). Smaller than all others because:
   - `ViewImplementation` is already a clean C++ embedding API
   - `Function<>` callbacks need only thin C wrappers
   - `enqueue_input_event()` accepts constructed structs directly
   - No XPCOM bootstrap, no Content API ceremony

2. **GPU compositing** — If using NSView overlay, zero modifications. If
   extracting IOSurface for CAContext/CALayerHost, small patch to
   `LadybirdWebView.mm` to create a CAContext from the existing IOSurfaceRef
   (~20 lines).

3. **Build integration** — One new `CMakeLists.txt`, one line in parent
   CMakeLists. Trivial.

**Estimate:** 0–3 modified stock files. New code: ~300–500 lines in C library +
existing Roamium Rust binary (~400 lines reusable).

#### Q10. Cross-platform

| Platform | Frontend | Compositing                      | GPU | Key files     |
| -------- | -------- | -------------------------------- | --- | ------------- |
| macOS    | AppKit   | Metal + IOSurface → CAMetalLayer | Yes | `UI/AppKit/`  |
| Linux    | Qt       | QPainter software blit           | No  | `UI/Qt/`      |
| Android  | Kotlin   | Canvas software blit             | No  | `UI/Android/` |

`LibWebView` (`Libraries/LibWebView/`) is highly portable — only two
`#ifdef AK_OS_MACOS` gates in `ViewImplementation`:

1. `did_allocate_iosurface_backing_stores()` — IOSurface handoff (macOS only)
2. `iosurface_ref` field in `SharedBitmap` struct

All other code is platform-agnostic. The abstract `ViewImplementation` handles
IPC, bitmap management, event queuing, and callbacks identically across
platforms.

**For libtermsurf_ladybird:** On macOS, use the Metal/IOSurface path for GPU
compositing. On Linux/Qt, use shared-memory bitmaps (software rendering). The C
library would need minimal platform ifdefs.

### Assessment

**Ladybird is the most natural fit for TermSurf's architecture.** It has the
cleanest embedding API, the smallest codebase, and already exposes IOSurfaces on
macOS.

| Aspect               | Chromium                | WebKit                  | Gecko                    | Ladybird                  |
| -------------------- | ----------------------- | ----------------------- | ------------------------ | ------------------------- |
| Embedding API        | Content API (C++)       | WKWebView (Obj-C)       | libxul + XPCOM (C++)     | ViewImplementation (C++)  |
| Official support     | Yes                     | Yes                     | No                       | Yes (it's the API)        |
| Fork modifications   | 8 patches, 24 files     | Potentially zero        | 5-20 files               | 0-3 files                 |
| C library complexity | ~2,000 lines            | ~500 lines est.         | ~1,000-1,500 lines est.  | ~300-500 lines est.       |
| GPU compositing      | CAContext + CALayerHost | NSView (native)         | IOSurface mach ports     | IOSurface + Metal         |
| Input handling       | Manual injection        | NSView first responder  | SynthesizeNative*Event() | enqueue_input_event()     |
| DevTools             | Custom (forked)         | `_inspector` (public)   | RDP over WebSocket       | Firefox DevTools Protocol |
| Multi-profile        | One per process         | Multiple in one process | One per process          | One per process           |
| Build system         | GN/Ninja                | CMake                   | moz.build/mach           | CMake                     |
| Init complexity      | Low                     | Very low                | High (XPCOM)             | Very low                  |
| Codebase size        | ~40 GB                  | ~7.4 GB                 | ~9.1 GB                  | ~495 MB                   |
| Web compat           | Production-grade        | Production-grade        | Production-grade         | Early stage               |

**Key advantages:**

1. **Cleanest C++ API** — `ViewImplementation` is a modern, well-designed
   abstract class. No Objective-C wrappers, no XPCOM, no Content API ceremony.
   Direct `enqueue_input_event()`, `Function<>` callbacks, `load(url)`.

2. **IOSurface already exposed** — `paintable()` returns the raw IOSurfaceRef.
   Creating a CAContext wrapper is trivial (~20 lines). No compositor forking.

3. **Smallest fork footprint** — 0-3 modified stock files, ~300-500 line C
   library. The smallest of all four engines by far.

4. **CMake build** — Standard CMake with clean macros. Adding a library target
   is one macro call.

5. **Small, readable codebase** — 495 MB vs Chromium's 40+ GB. Entire engine is
   comprehensible. Contributions upstream are realistic.

**Key concern:**

1. **Web compatibility** — Ladybird is a young engine still in active
   development. It does not yet pass all Web Platform Tests. Many sites will
   render incorrectly or not work at all. This is the fundamental trade-off:
   easiest to embed, but furthest from production-ready web compatibility.

**Recommendation:** Ladybird is the best engine for a proof-of-concept and for
long-term investment. The embedding API is a near-perfect match for TermSurf's
`libtermsurf_*` pattern. However, for production use today, Chromium (Roamium)
remains necessary due to web compatibility. WebKit is the best middle ground —
production-grade compatibility with minimal embedding effort.

## Conclusion

We audited three browser engines — WebKit, Gecko, and Ladybird — to determine
what it would take to build Roamium equivalents (C shared library + Rust binary)
for each. The audits examined embedding APIs, GPU compositing, input injection,
callbacks, DevTools, build systems, multi-profile support, fork size, and
cross-platform portability. Here's what we learned.

### The libtermsurf\_\* pattern works for all four engines

Every engine can be wrapped in a C shared library exporting `ts_*` functions,
linked by a Rust binary that speaks the TermSurf protocol over Unix sockets.
The pattern proven by Roamium (Issue 707) and libtermsurf_chromium (Issue 708)
generalizes cleanly. The Rust binary (~400 lines) is almost entirely reusable
across engines — only the C library differs.

### Engine ranking by embedding effort

1. **Ladybird** (~300-500 line C lib, 0-3 stock patches) — Modern C++ API with
   `ViewImplementation` base class, `Function<>` callbacks, direct
   `enqueue_input_event()`. IOSurface already exposed via `paintable()`. CMake
   build with one-line library target. The cleanest integration by far.

2. **WebKit** (~500 line C lib, potentially zero patches) — WKWebView is an
   NSView. Position it as a subview and macOS handles compositing natively. KVO
   for callbacks, `_inspector` for DevTools, `WKWebsiteDataStore` for profiles.
   Can link system WebKit.framework with no fork at all.

3. **Chromium** (~2,000 line C lib, 8 patches across 24 files) — Content API
   works but requires forking for CAContext extraction, DevTools access, and
   hidden window support. GN/Ninja build. Already done (Roamium).

4. **Gecko** (~1,000-1,500 line C lib, 5-20 patches) — No official desktop
   embedding API. Must bootstrap XPCOM, use `nsIWindowlessBrowser`, implement
   `nsIWebProgressListener`. IOSurface mach ports instead of CAContext. The
   riskiest option due to Mozilla dropping embedding support years ago.

### GPU compositing varies by engine

Each engine takes a different approach to cross-process compositing on macOS:

- **Chromium:** CAContext + CALayerHost (Window Server routes layer IDs)
- **WebKit:** RemoteLayerTree serialization (reconstructs CALayers in UI
  process). WKWebView contains the final layer tree as a native NSView.
- **Gecko:** IOSurface mach ports (sends GPU buffer handles, parent composites
  into NativeLayerCA). No CAContext IDs exchanged.
- **Ladybird:** IOSurface + Metal (Skia renders into IOSurface, Metal blits to
  CAMetalLayer). IOSurfaceRef exposed via `paintable()`.

For WebKit and Ladybird, the **NSView overlay approach** avoids all compositor
complexity — just position the engine's view as a subview at the overlay's
pixel coordinates. For Chromium, we already use CAContext/CALayerHost. For
Gecko, the NSView overlay approach is also the recommended path.

### Multi-profile is consistent

Chromium, Gecko, and Ladybird all support one profile per process. WebKit is
the exception — `WKWebsiteDataStore` allows multiple isolated data stores in a
single process. TermSurf's architecture (one browser process per profile) was
designed around this constraint and works with all four engines.

### Web compatibility is the real differentiator

The technical embedding effort matters less than whether the engine actually
renders the web correctly:

- **Chromium** and **Gecko** — Production-grade. Decades of web compatibility
  work. Pass virtually all Web Platform Tests.
- **WebKit** — Production-grade. Powers Safari. Excellent standards compliance,
  though some sites target Chrome specifically.
- **Ladybird** — Early stage. Active development, but many sites will render
  incorrectly or not work. Not yet suitable for daily use.

### What this means for TermSurf

**Roamium (Chromium) remains the production browser.** It works today, renders
everything, and is already shipping.

**WebKit (Rebkit) is the highest-value next engine.** Production-grade web
compatibility with the smallest embedding effort. Potentially zero fork
modifications. The WKWebView-as-overlay approach eliminates GPU compositing
complexity entirely. On macOS, it could link system WebKit.framework — no
engine build required.

**Ladybird is the long-term bet.** The cleanest API, smallest codebase, and
most natural fit for TermSurf's architecture. As Ladybird matures toward web
compatibility, it becomes increasingly attractive. Worth tracking and
contributing to.

**Gecko (Recko) is the hardest path for the least gain.** No official embedding
API, XPCOM complexity, and high maintenance risk. Firefox users who want a
terminal browser can use Roamium (Chromium renders everything Firefox does).
Not recommended unless there's a specific need for Gecko's rendering.
