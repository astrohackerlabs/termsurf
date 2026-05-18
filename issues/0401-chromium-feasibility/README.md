+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 401: Chromium Feasibility Research

Phase 0 of the TermSurf 4.0 roadmap (Issue 400). The goal is to determine
whether direct Chromium embedding — without CEF — is feasible before writing any
application code.

## The Five Questions

From Issue 400, Phase 0 must answer:

1. What is the minimal Content API surface for: initialize browser process,
   create off-screen browser, receive rendered frames, send input?
2. How does CEF's `CefBrowserHost::CreateBrowser()` map to Content API calls?
3. How does CEF's `OnAcceleratedPaint` receive rendered frames from Chromium's
   compositor? Can we get frames without CEF's throttling?
4. What does Electron's `OffScreenRenderWidgetHostView` do differently to
   achieve 240fps?
5. What is the build system integration story? Can we build a minimal Chromium
   shared library with GN and link it from Rust via C FFI?

## Source Material

Three codebases are cloned into the repository (gitignored) for reference:

- `/cef/` — CEF source (Bitbucket mirror, 19MB)
- `/electron/` — Electron source (GitHub, ~100MB)
- `/chromium/` — Chromium source (googlesource, 6.5GB, depth 1)

## Question 1: Minimal Content API Surface

### Initialization

Chromium's embedding entry point is straightforward. The `content/shell/`
directory is the canonical minimal embedder (~105 files).

**Entry point** (`content/shell/app/shell_main.cc`):

```cpp
int main(int argc, const char** argv) {
    content::ShellMainDelegate delegate;
    content::ContentMainParams params(&delegate);
    params.argc = argc;
    params.argv = argv;
    return content::ContentMain(std::move(params));
}
```

**What the delegate must provide** (`content/public/app/content_main_delegate.h`):

| Method | Purpose |
| --- | --- |
| `BasicStartupComplete()` | Early init, singletons |
| `PreSandboxStartup()` | Pre-sandbox resource loading |
| `CreateContentClient()` | Resource/string provider |
| `CreateContentBrowserClient()` | Browser process customization |
| `CreateContentGpuClient()` | GPU process customization |
| `CreateContentRendererClient()` | Renderer process customization |
| `CreateContentUtilityClient()` | Utility process customization |

**Browser process lifecycle** (`content/shell/browser/shell_browser_main_parts.h`):

The `ContentBrowserClient::CreateBrowserMainParts()` override returns a
`BrowserMainParts` subclass with hooks:

```
PreEarlyInitialization → PreCreateThreads → PostCreateThreads
→ PostCreateMainMessageLoop → ToolkitInitialized
→ PreMainMessageLoopRun → [run loop] → PostMainMessageLoopRun
```

### Creating a Browser

```cpp
content::WebContents::CreateParams params(browser_context);
auto web_contents = content::WebContents::Create(params);
```

That's it. `WebContents` is the core abstraction — one per tab/webview. It
manages the renderer process, navigation, and frame delivery internally. The
`CreateParams` accepts:

- `browser_context` — Required. Owns cookies, storage, permissions. One per
  profile (maps directly to ts3's one-process-per-profile constraint).
- `initially_hidden` — Start hidden (for off-screen).
- `is_never_composited` — Hint that this WebContents won't display to a window.

### Receiving Rendered Frames

Chromium's Content API provides two frame capture mechanisms on
`RenderWidgetHostView`:

1. **`CopyFromSurface(src_rect, output_size, timeout, callback)`** — One-shot
   capture. Good for screenshots, not for streaming.

2. **`CreateVideoCapturer()`** — Returns a `viz::ClientFrameSinkVideoCapturer`.
   This is the streaming path. Both CEF and Electron use this for GPU-accelerated
   frame delivery.

**There is no built-in off-screen rendering API.** The Content API provides
hooks (`is_never_composited`, `CopyFromSurface`, `CreateVideoCapturer`), but each
embedder must implement its own `RenderWidgetHostView` subclass that:

- Manages a `ui::Compositor` and `content::DelegatedFrameHost`
- Creates a `viz::HostDisplayClient` for the software path
- Creates a `viz::ClientFrameSinkVideoCapturer` for the GPU path
- Routes frames to the application

### Sending Input

`RenderWidgetHostViewBase` (the base class both CEF and Electron extend)
provides input methods:

```cpp
void ProcessMouseEvent(const blink::WebMouseEvent&, ...);
void ProcessMouseWheelEvent(const blink::WebMouseWheelEvent&, ...);
void ProcessTouchEvent(const blink::WebTouchEvent&, ...);
void ProcessGestureEvent(const blink::WebGestureEvent&, ...);
```

Keyboard input goes through `RenderWidgetHost::ForwardKeyboardEvent()`.

### Summary: Minimal API Surface

| Task | Content API | Lines of glue |
| --- | --- | --- |
| Initialize | `ContentMainRunner::Create()` + `ContentMain()` | ~200 (delegate + main parts) |
| Create browser | `WebContents::Create(params)` | ~50 |
| Receive frames | Custom `RenderWidgetHostView` subclass | ~2000 (the hard part) |
| Send input | `ProcessMouseEvent()`, `ForwardKeyboardEvent()` | ~100 |

The `RenderWidgetHostView` subclass is the bulk of the work. The public
interface has **60 virtual methods** (`render_widget_host_view.h`, 343 lines).
The base class adds more (`render_widget_host_view_base.h`, 706 lines).

## Question 2: CEF's Browser Creation → Content API Mapping

CEF's `AlloyBrowserHostImpl::Create()` (`libcef/browser/alloy/alloy_browser_host_impl.cc`)
does the following:

```
CefBrowserHost::CreateBrowser(settings, client, url, ...)
  → CefBrowserPlatformDelegate::Create(params)      // Platform-specific delegate
  → platform_delegate->CreateWebContents(params)     // content::WebContents::Create()
  → AlloyBrowserHostImpl::CreateInternal(...)
      → platform_delegate->WebContentsCreated(web_contents)
      → new AlloyBrowserHostImpl(settings, client, web_contents, ...)
      → browser->InitializeBrowser()                 // WebContents observer hooks
      → browser->CreateHostWindow()                  // Native window (skipped for OSR)
      → OnAfterCreated callback
```

The critical chain is:

1. **`WebContents::Create(CreateParams(browser_context))`** — Standard Content API
2. **`WebContentsView` subclass** — CEF provides `CefWebContentsViewOSR`
   (`libcef/browser/osr/web_contents_view_osr.h`) which implements
   `content::WebContentsView` + `content::RenderViewHostDelegateView`
3. **`CreateViewForWidget(RenderWidgetHost*)`** — Called by Chromium when
   WebContents needs a view. CEF returns `CefRenderWidgetHostViewOSR`.
4. **`CefRenderWidgetHostViewOSR`** — The 2000+ line class that does all OSR
   work. Extends `content::RenderWidgetHostViewBase`.

### What CEF adds on top of Content API

CEF's `libcef/` directory is ~500 files. Most of it is API surface (C bindings,
ref counting, settings) and platform abstractions. The OSR-specific code is
concentrated in `libcef/browser/osr/` (~10 files, ~5000 lines total):

| File | Lines | Purpose |
| --- | --- | --- |
| `render_widget_host_view_osr.h/cc` | ~1500 | RenderWidgetHostViewBase subclass |
| `host_display_client_osr.h/cc` | ~150 | Software frame delivery (LayeredWindowUpdater) |
| `video_consumer_osr.h/cc` | ~250 | GPU frame delivery (FrameSinkVideoCapturer) |
| `web_contents_view_osr.h/cc` | ~150 | WebContentsView for off-screen |

**~2000 lines of core OSR logic.** The rest of CEF's 500 files are API
wrappers, platform code, and Chrome integration — not needed for direct
embedding.

## Question 3: Frame Delivery Without CEF's Throttling

### CEF's frame delivery pipeline

CEF has two paths:

**Software path** (CPU):
```
Chromium compositor → viz::HostDisplayClient
  → CreateLayeredWindowUpdater() → CefLayeredWindowUpdaterOSR
  → OnAllocatedSharedMemory() → shared memory mapping
  → Draw() → OnPaint(damage_rect, pixel_size, pixels)
```

**GPU path** (IOSurface/DXGI):
```
Chromium compositor → viz::ClientFrameSinkVideoCapturer
  → CefVideoConsumerOSR::OnFrameCaptured()
  → extract gpu_memory_buffer_handle (IOSurface on macOS)
  → OnAcceleratedPaint(damage_rect, pixel_size, paint_info)
```

### Where does CEF throttle?

CEF's `CefVideoConsumerOSR` (`libcef/browser/osr/video_consumer_osr.cc`)
configures the video capturer:

```cpp
video_capturer_->SetAutoThrottlingEnabled(false);   // Disabled
video_capturer_->SetAnimationFpsLockIn(false, 1);   // Disabled
```

CEF disables Chromium's auto-throttling. The frame rate is controlled by
`video_capturer_->SetMinCapturePeriod(frame_rate)`, which is set from the
embedder. The `CefCopyFrameGenerator` that we identified as a bottleneck in
Issue 350 is part of this pipeline — it manages frame scheduling and can
discard frames when one is already in-flight.

### Can we bypass the throttling?

**Yes.** The `viz::ClientFrameSinkVideoCapturer` is a Chromium API, not a CEF
invention. Both CEF and Electron use it directly. The throttling is in how the
capturer is configured (min capture period, auto-throttling, frame pool size)
and how the consumer processes frames. By implementing our own
`FrameSinkVideoConsumer`, we control all of this.

The key insight from the Chromium source: `FrameSinkVideoCapturerImpl` uses a
`GpuMemoryBufferVideoFramePool` with a configurable capacity (default 10
frames). As long as we release frames promptly, the pool won't block.

## Question 4: Electron's 240fps Advantage

### Electron's OSR architecture

Electron's OSR code lives in `shell/browser/osr/` and mirrors CEF's structure
closely:

| Electron | CEF | Purpose |
| --- | --- | --- |
| `OffScreenRenderWidgetHostView` | `CefRenderWidgetHostViewOSR` | RenderWidgetHostViewBase subclass |
| `OffScreenVideoConsumer` | `CefVideoConsumerOSR` | FrameSinkVideoCapturer wrapper |
| `OffScreenHostDisplayClient` | `CefHostDisplayClientOSR` | Software frame path |
| `OffScreenWebContentsView` | `CefWebContentsViewOSR` | WebContentsView for off-screen |

Both extend `content::RenderWidgetHostViewBase`. Both use
`viz::ClientFrameSinkVideoCapturer`. The architecture is nearly identical.

### What Electron does differently

1. **Frame rate cap** (`osr_render_widget_host_view.cc`, line 919):
   ```cpp
   if (!offscreen_use_shared_texture_ && frame_rate > 240)
       frame_rate = 240;
   ```
   Without shared textures: capped at 240fps. With shared textures: **no cap**.

2. **Buffer format preference** (`osr_video_consumer.cc`, line 82):
   ```cpp
   SetBufferFormatPreference(kPreferMappableSharedImage)
   ```
   When using shared textures, Electron requests `kPreferMappableSharedImage`,
   which gives direct GPU buffer handles (IOSurface on macOS, DXGI on Windows).

3. **Frame pool** — Chromium's internal `GpuMemoryBufferVideoFramePool` holds 10
   frames. Electron exposes a `release()` callback to JavaScript, letting the
   consumer control when frames return to the pool.

4. **Capturer configuration** — Same as CEF: auto-throttling disabled,
   animation FPS lock-in disabled, resolution constraints set to full range.

### Why Electron achieves higher FPS

The frame rate difference is **not** in the capturer configuration (both disable
throttling). It's in the **frame delivery path**:

- **CEF** delivers frames through its C API callback (`OnAcceleratedPaint`),
  which involves CEF's internal frame management and ref counting overhead.
- **Electron** delivers frames through a simpler C++ callback that goes directly
  to the JavaScript layer or shared texture consumer.

For our use case (rendering to a wgpu texture), neither path matters. We'll
implement our own `FrameSinkVideoConsumer` that receives `GpuMemoryBuffer`
handles and imports them as textures. No CEF overhead, no Electron callback
chain. The capturer itself supports whatever frame rate the GPU can produce.

### The real 240fps story

Electron's 240fps is the configured limit, not the achieved rate. The actual
frame rate depends on:

1. GPU compositor speed (how fast Chromium can render)
2. Frame pool availability (10 frames, must release promptly)
3. `SetMinCapturePeriod()` value
4. Content complexity and GPU load

For ts4, targeting 60fps with headroom is realistic. The capturer can deliver
that if we process frames fast enough.

## Question 5: Build System Integration

### Chromium's build system

Chromium uses GN (Generate Ninja) + Ninja. The build is massive (~40 minutes for
a full build on a fast machine). Key facts:

- GN generates Ninja build files from `.gn` and `BUILD.gn` files
- The build produces shared libraries and executables
- `content_shell` is the reference minimal embedder
- Dependencies are specified in `BUILD.gn` files

### Minimal embedder dependencies

From `content/shell/BUILD.gn`, a minimal embedder needs:

```
//content/public/app          — ContentMainRunner, ContentMainDelegate
//content/public/browser       — WebContents, BrowserContext, RenderWidgetHostView
//content/public/common        — Shared types
//base                         — Threading, message loops, memory
//ui/gfx                       — Graphics types
//mojo                         — IPC
//components/viz               — Frame compositing
```

### Rust integration strategy

Three possible approaches:

**Option A: Chromium as a shared library**

Build a custom GN target that produces `libchromium_embed.dylib` containing the
Content API surface. Link from Rust via `#[link(name = "chromium_embed")]` and a
thin C FFI shim.

Pros: Clean boundary. Chromium is a black box. Rust app links against one lib.
Cons: Defining the shared library boundary is hard. Chromium doesn't have a
stable ABI. Every Chromium update may break the boundary.

**Option B: C++ shim compiled within Chromium**

Write a C++ file that lives in the Chromium source tree (or alongside it) and
compiles as part of the Chromium build. This shim exposes a C API:

```c
// termsurf_chromium.h
typedef void (*frame_callback)(void* handle, int width, int height);

int ts_chromium_init(int argc, const char** argv);
void* ts_chromium_create_browser(const char* url, int width, int height);
void ts_chromium_set_frame_callback(void* browser, frame_callback cb);
void ts_chromium_send_mouse_event(void* browser, int x, int y, int type);
void ts_chromium_send_key_event(void* browser, int key, int modifiers);
void ts_chromium_shutdown();
```

Pros: Thin C ABI is stable. Rust calls C functions. Chromium internals hidden
behind the shim. This is essentially what CEF does, but minimal.
Cons: Must live in or near the Chromium source tree for GN to find dependencies.

**Option C: Fork content_shell**

Start with `content/shell/` and modify it to be our OSR embedder. Keep it in the
Chromium tree. Compile with GN + Ninja. The output is a binary or library that
our Rust application spawns or loads.

Pros: Proven build path. content_shell already builds. We modify known-working
code.
Cons: Tied to Chromium source tree. Harder to version separately.

### Recommended approach: Option B

A C++ shim with a C API is the most pragmatic path. It's what CEF does at its
core — just with 500 files of additional API surface we don't need. Our shim
would be ~2000-3000 lines of C++ (the OSR `RenderWidgetHostView` subclass, a
thin wrapper around `WebContents`, and initialization boilerplate) with a ~50
function C API.

The build story:

1. Clone Chromium (already done — 6.5GB, depth 1)
2. Add our shim as a GN target alongside `content/shell/`
3. Build with GN + Ninja → produces `libtermsurf_chromium.dylib`
4. Rust links against it via `build.rs` + `cc` or `bindgen`

## Feasibility Assessment

### What's feasible

1. **Initialization** — Straightforward. `ContentMain()` + delegate is
   well-documented and stable across Chromium versions.

2. **WebContents creation** — Trivial. `WebContents::Create()` is a stable
   public API.

3. **Frame capture** — `viz::ClientFrameSinkVideoCapturer` is a stable Chromium
   API used by both CEF and Electron. IOSurface handles on macOS give us direct
   GPU texture access.

4. **Input forwarding** — Standard `RenderWidgetHostViewBase` methods.

### What's hard

1. **`RenderWidgetHostView` subclass** — 60+ virtual methods. Most can be
   stubbed, but the compositor integration (`DelegatedFrameHost`,
   `ui::Compositor`, `viz::BeginFrameSource`) is non-trivial. Both CEF and
   Electron's implementations are ~1500 lines each. We'd need ~2000 lines.

2. **Build system** — GN + Ninja is alien to the Rust ecosystem. First build
   takes ~40 minutes. Incremental builds are fast. The main risk is keeping our
   shim compatible as Chromium evolves.

3. **Multi-process architecture** — Chromium spawns renderer, GPU, and utility
   processes internally. This is handled by the Content API and doesn't require
   our intervention. But our process (the browser process) must run Chromium's
   message loop or integrate with it.

4. **Message loop integration** — Chromium wants to own the message loop on the
   main thread. This conflicts with our desire to own the event loop (winit).
   CEF solves this with `external_message_pump`. We'd need a similar approach
   or run Chromium on a separate thread.

### What's risky

1. **API stability** — Chromium's Content API is `content/public/` but it's not
   versioned or guaranteed stable. Methods change, classes are renamed, entire
   subsystems are refactored. CEF and Electron employ engineers full-time to
   track these changes. We'd need to pin to a Chromium version and update
   deliberately.

2. **Platform differences** — macOS, Windows, and Linux have different
   `RenderWidgetHostView` implementations in Chromium. Our OSR approach avoids
   most platform differences (no native window), but GPU texture sharing differs
   (IOSurface vs DXGI vs DMA-BUF).

3. **Scope creep** — The "minimal" Content API surface is not that minimal.
   `BrowserContext` alone has dozens of required delegates (permissions, storage,
   downloads, SSL). content_shell stubs most of these, but some are required for
   basic web browsing (cookies, HTTPS, storage).

## Research Plan

### Step 1: Build content_shell

**Goal:** Prove we can build Chromium and produce a running binary.

1. Fetch Chromium dependencies (`gclient sync`)
2. Configure with GN for macOS
3. Build `content_shell` target
4. Run it, verify a webpage loads

**Time estimate:** 1-2 days (mostly waiting for builds).

### Step 2: Study content_shell's RenderWidgetHostView

**Goal:** Understand the platform view implementation that content_shell uses.

content_shell on macOS uses Chromium's standard `RenderWidgetHostViewMac` (the
same view Chrome uses). We need to understand what it does so we can write an
off-screen equivalent.

Key files to study:
- `content/browser/renderer_host/render_widget_host_view_mac.h/mm`
- `content/browser/renderer_host/render_widget_host_view_base.h/cc`
- `content/browser/renderer_host/delegated_frame_host.h/cc`

### Step 3: Write a minimal OSR RenderWidgetHostView

**Goal:** Create the smallest possible off-screen view that receives frames.

Start with CEF's `CefRenderWidgetHostViewOSR` as a reference. Strip it down to:
- Constructor that sets up `DelegatedFrameHost` + `ui::Compositor`
- `ShowWithVisibility()` that creates the `FrameSinkVideoCapturer`
- `OnFrameCaptured()` that prints frame dimensions (proof of life)
- Stubs for the remaining 57 virtual methods

**Reference:** CEF's implementation is ~1500 lines. Electron's is similar. A
stubbed version should be ~500 lines.

### Step 4: Build our shim as a GN target

**Goal:** Prove we can add custom code to the Chromium build.

Create a `BUILD.gn` file that:
1. Depends on `//content/public/browser`, `//content/public/app`
2. Compiles our OSR view + a simple `main()` that loads a URL
3. Produces a binary that opens a headless browser and prints frame info

### Step 5: Extract a shared library with C API

**Goal:** Create the FFI boundary between Chromium (C++) and TermSurf (Rust).

Write the C API shim:
- `ts_init()`, `ts_create_browser()`, `ts_set_frame_callback()`
- Build as `libtermsurf_chromium.dylib`
- Write a Rust test that loads the library and receives one frame

### Step 6: Integrate with wgpu

**Goal:** Render a Chromium frame as a wgpu texture.

The frame callback delivers an IOSurface handle. Use `wgpu`'s Metal backend to
import it as a texture (same approach ts3 uses, already proven).

### Go/No-Go Decision

After Step 4, we'll know if direct embedding is feasible:

- **Go:** content_shell builds, our custom view receives frames, GN integration
  works. Proceed to Steps 5-6 and then Phase 1 (window + compositor).
- **Pivot:** If building Chromium is blocked, the Content API has changed too
  much, or the `RenderWidgetHostView` subclass is intractable, fall back to:
  - CEF with workarounds (accept ~31fps and optimize elsewhere)
  - WebKit (via WebKitGTK or WebKit2)
  - Servo (Rust-native, but incomplete)

## Appendix: Class Hierarchy

```
content::RenderWidgetHostView              (60 virtual methods)
  └── content::RenderWidgetHostViewBase    (base implementation, 706 lines)
        ├── RenderWidgetHostViewMac         (Chrome/content_shell macOS)
        ├── RenderWidgetHostViewAura        (Chrome/content_shell Linux/Windows)
        ├── CefRenderWidgetHostViewOSR      (CEF off-screen)
        └── OffScreenRenderWidgetHostView   (Electron off-screen)
```

Both CEF and Electron's OSR views:
- Extend `RenderWidgetHostViewBase`
- Implement `RenderFrameMetadataProvider::Observer`
- Implement `ui::CompositorDelegate`
- Create their own `ui::Compositor` + `DelegatedFrameHost`
- Use `viz::ClientFrameSinkVideoCapturer` for GPU frame capture
- Use `viz::HostDisplayClient` for software frame fallback

## Appendix: Frame Delivery Comparison

```
                     CEF                          Electron
                      │                              │
        CefVideoConsumerOSR            OffScreenVideoConsumer
                      │                              │
    viz::ClientFrameSinkVideoCapturer  viz::ClientFrameSinkVideoCapturer
                      │                              │
    FrameSinkVideoCapturerImpl         FrameSinkVideoCapturerImpl
                      │                              │
    GpuMemoryBufferVideoFramePool      GpuMemoryBufferVideoFramePool
    (capacity: configurable)           (capacity: 10)
                      │                              │
    OnFrameCaptured()                  OnFrameCaptured()
                      │                              │
    GpuMemoryBuffer handle             GpuMemoryBuffer handle
    (IOSurface on macOS)               (IOSurface on macOS)
                      │                              │
    OnAcceleratedPaint()               OnPaint() + texture info
    → CEF C API callback               → JS callback / release()
                      │                              │
                  Application                    Application

Both paths use the same Chromium internals. The difference is in
the callback mechanism and frame lifecycle management.
```

## Appendix: Key Source Locations

### CEF (`/cef/`)

| File | Purpose |
| --- | --- |
| `libcef/browser/osr/render_widget_host_view_osr.h/cc` | OSR view (~1500 lines) |
| `libcef/browser/osr/host_display_client_osr.h/cc` | Software frame path |
| `libcef/browser/osr/video_consumer_osr.h/cc` | GPU frame capture |
| `libcef/browser/osr/web_contents_view_osr.h/cc` | WebContentsView for OSR |
| `libcef/browser/alloy/alloy_browser_host_impl.cc` | Browser creation |
| `libcef/browser/main_runner.cc` | Chromium initialization |

### Electron (`/electron/`)

| File | Purpose |
| --- | --- |
| `shell/browser/osr/osr_render_widget_host_view.h/cc` | OSR view |
| `shell/browser/osr/osr_video_consumer.h/cc` | GPU frame capture |
| `shell/browser/osr/osr_host_display_client.h/cc` | Software frame path |
| `shell/browser/osr/osr_web_contents_view.h/cc` | WebContentsView for OSR |
| `shell/browser/osr/osr_paint_event.h` | Frame metadata struct |
| `shell/app/electron_main_delegate.h` | ContentMainDelegate |

### Chromium (`/chromium/`)

| File | Purpose |
| --- | --- |
| `content/public/app/content_main.h` | Entry point |
| `content/public/app/content_main_delegate.h` | Embedder delegate interface |
| `content/public/browser/web_contents.h` | WebContents factory (1049 lines) |
| `content/public/browser/browser_context.h` | Profile/storage context |
| `content/public/browser/render_widget_host_view.h` | View interface (60 methods) |
| `content/browser/renderer_host/render_widget_host_view_base.h` | Base class (706 lines) |
| `content/browser/renderer_host/delegated_frame_host.h` | Frame compositing bridge |
| `content/shell/app/shell_main.cc` | Minimal embedder entry point |
| `content/shell/browser/shell.h` | Minimal browser window |
| `content/shell/browser/shell_content_browser_client.h` | Minimal browser client |
| `content/shell/browser/shell_browser_main_parts.h` | Minimal lifecycle hooks |
