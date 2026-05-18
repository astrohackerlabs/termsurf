+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 619: Input latency

## Goal

Reduce the visible lag between user input (mouse movement, text selection,
scrolling) and the browser's visual response. The goal is to close the
perceptible gap between TermSurf and native Chrome.

## Background

Issue 512 solved vsync micro-stutter (uneven frame cadence) with 120fps
oversampling. The frame cadence is now smooth. But there is a separate problem:
**input-to-display latency**. When selecting text, the selection visibly trails
the cursor. When scrolling, the page visibly lags behind the scroll gesture.
Bounce effects at the top and bottom of the page feel sluggish. The whole
experience feels less refined than native Chrome.

### The round-trip in native Chrome

```
Mouse event → compositor thread → render → display (same vsync)
Total: 0–16ms (one frame)
```

Input received before the vsync deadline appears on that vsync. The compositor
thread can respond to scroll and selection immediately — often within the same
frame — because everything is in-process with a single clock.

### The round-trip in TermSurf

```
Mouse event → Zig Surface → XPC to Chromium → Chromium processes input →
renderer paints → compositor composites → capturer captures (timer) →
IOSurface → XPC to GUI → next CVDisplayLink vsync → Metal composites
```

| Stage                       | Latency    | Notes                                   |
| --------------------------- | ---------- | --------------------------------------- |
| Input → XPC to Chromium     | 1–3ms      | Async dispatch queue scheduling         |
| Chromium processes input    | 2–5ms      | Layout, paint, composite                |
| Wait for next capture cycle | **0–8ms**  | Capturer on 120fps timer, not on-demand |
| Captured frame → XPC to GUI | 1–3ms      | Another async dispatch queue hop        |
| Wait for next vsync         | **0–16ms** | CVDisplayLink tick                      |

Worst case: 35ms. Average: 15–25ms. That's 1–2 frames of extra latency versus
native Chrome.

### Three sources of lag

**1. FrameSinkVideoCapturer is a recording API, not a display API.**

The capturer runs on its own 120fps timer and issues `CopyOutputRequest`s
periodically. It does not know that input just arrived and a fresh frame is
urgently needed. After Chromium renders the new frame in response to input, you
wait up to 8ms for the next capture cycle to notice it. In Chrome, input
directly triggers compositor work within the same BeginFrame — no capture delay.

**2. XPC is asynchronous.**

Messages are enqueued on dispatch queues and delivered when the OS scheduler
gets around to it. This cost is paid twice — once for input going to Chromium,
once for the frame coming back. There is no way to make XPC synchronous without
blocking the caller, which would be worse.

**3. The double-vsync penalty.**

In Chrome, input received before the vsync deadline appears on that vsync. In
TermSurf, input has to travel to Chromium, get rendered, get captured, travel
back, and then wait for the _next_ vsync. You effectively always lose at least
one frame compared to Chrome. This is inherent to any out-of-process streaming
architecture.

## How Chrome stays fast across process boundaries

Chrome uses separate processes for rendering and GPU compositing — the same kind
of cross-process architecture that TermSurf has. But Chrome feels responsive
because its performance-critical path does not use message-passing IPC. It uses
shared memory.

### Chrome's process model

| Process           | Role                                                               |
| ----------------- | ------------------------------------------------------------------ |
| **Browser**       | UI chrome, input dispatch, coordination                            |
| **Renderer** (1+) | Blink (DOM, layout, paint) + compositor thread (scroll, animation) |
| **GPU/Viz** (1)   | All GPU calls, display compositing, rasterization                  |

Renderers never touch the GPU directly. Every graphics call crosses a process
boundary to the GPU/Viz process. Yet Chrome still achieves 1–2 frame latency.

### Shared memory, not message passing

The critical difference from TermSurf's architecture (verified from
`chromium/src/`):

**GPU Command Buffer** — Renderers write GL-equivalent commands into a shared
memory ring buffer (`gpu/command_buffer/client/cmd_buffer_helper.h`). The
`CommandBufferHelper` manages put/get pointers over `SharedMemoryBufferBacking`
(actual cross-process shared memory). Hundreds of commands batch up before a
single lightweight IPC notification tells the GPU process to consume them. No
per-call kernel transition. No serialization overhead.

**CompositorFrames are metadata, not pixels** — A `CompositorFrame`
(`components/viz/common/quads/compositor_frame.h`) has three fields: `metadata`,
`resource_list` (GPU mailbox texture references via `TransferableResource`), and
`render_pass_list` (draw quads). Zero pixel data crosses the process boundary —
textures are already in GPU memory (IOSurface on macOS).

**Sync tokens** — Instead of blocking to wait for raster to complete, the
compositor submits frames with non-blocking sync tokens. The GPU resolves them
before drawing. The pipeline never stalls.

**Compositor-thread input handling** — The `InputHandler` (`cc/input/`) runs on
the compositor thread. `ScrollBegin` and `ScrollUpdate` in `input_handler.cc`
default to `kScrollOnImplThread` — no main thread needed. Scroll offsets are
applied directly to the layer tree, and the main thread is notified later via
commit. This is why scrolling stays smooth even when JS is blocked.

### Mojo uses Mach ports on macOS

Chromium's Mojo IPC uses **Mach ports** on macOS, not Unix domain sockets (which
are Linux/Android only). The `MOJO_USE_APPLE_CHANNEL` buildflag in
`mojo/features.gni` gates this at compile time. `platform_channel.cc` creates
Mach ports via `base::apple::CreateMachPort()`, and `channel_mac.cc` implements
the transport using `mach_msg`. This means Mojo's macOS transport uses the same
kernel mechanism as XPC — the difference is not the transport layer but what
travels over it (shared memory references vs full message payloads).

### TermSurf vs Chrome: the architectural gap

| Aspect                | Chrome                                       | TermSurf                                        |
| --------------------- | -------------------------------------------- | ----------------------------------------------- |
| Graphics commands     | Shared memory ring buffer (zero copy)        | N/A (capturer does the rendering)               |
| Frame submission      | Small metadata struct (quads + texture refs) | Full IOSurface Mach port transfer via XPC       |
| Input → compositor    | Mojo/Mach ports to compositor thread         | XPC to Chromium process (same kernel mechanism) |
| Frame synchronization | BeginFrame from single vsync clock           | Two independent clocks (120fps oversampling)    |
| Scroll/selection      | Compositor thread handles directly           | Full Chromium render + capture round-trip       |

The fundamental gap: TermSurf uses a recording API (`FrameSinkVideoCapturer`) on
top of message-passing IPC (XPC), whereas Chrome uses shared memory command
buffers with zero-copy GPU textures and compositor-driven input. Every input
event in TermSurf requires a full round-trip: XPC out, Chromium render, capture,
XPC back. In Chrome, the compositor thread handles scroll and selection within
the same process, often within the same frame.

### What is fixable (short-term)

The capturer timer is the most actionable target. Our `ShellVideoConsumer` holds
a `ClientFrameSinkVideoCapturer` (`client_frame_sink_video_capturer.h:97`) which
exposes `RequestRefreshFrame()`. We never call it — the capturer relies entirely
on its 120fps timer. Calling `RequestRefreshFrame()` after input events would
force an immediate capture instead of waiting up to 8ms for the next timer tick.
Combined with XPC delivery jitter reduction (high-priority dispatch queues), we
could shave 5–10ms off the average round-trip.

### What is fixable (long-term)

Chromium proves that cross-process rendering can be fast — its renderer and
GPU/Viz processes are separate, yet latency stays at 1–2 frames. The key is
shared memory, not message passing. Chromium uses shared memory ring buffers for
graphics commands and shared GPU textures (IOSurface) for frame data. Mojo on
macOS uses Mach ports — the same kernel mechanism as XPC. The transport is not
the bottleneck. What matters is what travels over it.

TermSurf can adopt the same approach without in-process embedding:

- **Shared memory for frame data.** Instead of transferring IOSurface Mach ports
  per frame via XPC, allocate a shared IOSurface pool up front and share it
  once. The Chromium server renders into the shared surfaces, and the GUI reads
  from them — no per-frame XPC message needed. Synchronization via atomics or
  lightweight signals.
- **Shared memory for input events.** Instead of sending each mouse/key event as
  an XPC message, write events into a shared ring buffer. The Chromium server
  polls or gets a lightweight wakeup signal. Eliminates per-event kernel hops.
- **Single vsync clock.** The GUI's CVDisplayLink could signal the Chromium
  server (via shared memory flag or Mach port notification) at each vsync,
  giving it a BeginFrame-like deadline to produce the next frame.

These are the same patterns Chrome uses across its own process boundaries. The
out-of-process architecture is not the problem — the message-passing pattern is.

## Investigation plan

1. **Measure** — Instrument the pipeline to measure actual input-to-display
   latency. Timestamp mouse events when sent, timestamp when the corresponding
   frame arrives. Identify which stage dominates.
2. **Request-driven capture** — After sending input events to Chromium, send a
   `RequestRefreshFrame()` call to make the capturer produce a frame immediately
   instead of waiting for its timer.
3. **Dispatch queue priority** — Ensure XPC connections on both sides use
   high-priority dispatch queues to minimize scheduling latency.
4. **Evaluate** — Compare TermSurf vs Chrome after optimizations. Determine how
   much of the remaining gap is inherent to out-of-process streaming.

## Research

### Research 1: Content Shell's native rendering path

Content Shell uses the exact same rendering pipeline as Chrome when running as a
normal windowed app. It does NOT use `FrameSinkVideoCapturer` for display — that
API is only used when explicitly created for tab capture or screen recording.

#### The normal display path (verified from `chromium/src/`)

1. **Renderer process** — Blink renders into a `cc::LayerTreeHost`, which
   commits CompositorFrames to the Viz display compositor (GPU process).
2. **GPU/Viz process** — Aggregates frames and produces output as
   `gfx::CALayerParams` (`ui/gfx/ca_layer_params.h`). On macOS this contains
   either a `ca_context_id` (remote CoreAnimation, the normal GPU path) or an
   `io_surface_mach_port` (software fallback).
3. **Browser process** — `BrowserCompositorMac` receives the params.
   `DisplayCALayerTree::UpdateCALayerTree()` creates a `CALayerHost` with the
   `ca_context_id`, or sets the IOSurface as a CALayer's contents.
4. **macOS Window Server** — Composites the CALayer tree onto the screen.

Zero pixel readback. Zero GPU-to-CPU copy. Display-vsync locked.
Compositor-thread scroll handling. The full Chrome experience.

#### How Content Shell sets this up

- `shell_platform_delegate_mac.mm:222-238` — `SetContents()` gets the
  WebContents' native NSView and adds it as a subview of the window.
- `web_contents_view_mac.mm:389-435` — Creates `RenderWidgetHostViewMac`. For
  Content Shell, `SetParentUiLayer()` is never called (no `ui::Views`).
- `render_widget_host_view_mac.mm:228` — Creates `BrowserCompositorMac`.
- `browser_compositor_view_mac.mm:191-206` — Enters `HasOwnCompositor` state
  (confirmed by the comment at line 159: "This is used by content shell").
- `browser_compositor_view_mac.mm:251-263` — Creates a `RecyclableCompositorMac`
  with its own `AcceleratedWidgetMac`.
- `accelerated_widget_mac.mm:82-93` — Receives `CALayerParams` from the GPU
  process.
- `display_ca_layer_tree.mm:66-121` — Creates a `CALayerHost` with the
  `ca_context_id` for zero-copy GPU compositing.

Content Shell gets all of this for free: compositor-thread input handling,
BeginFrame synchronization, and the full Viz display compositor pipeline. It is
part of the Content API, not the Chrome browser UI.

#### What our Chromium Profile Server does differently

Our `ShellVideoConsumer` bypasses the normal display path entirely. Instead of
using the `CALayerParams` output, it attaches a `FrameSinkVideoCapturer` to the
compositor's frame sink. The capturer:

1. Issues `CopyOutputRequest`s to read pixels back from GPU memory
2. Produces frames on its own 120fps timer (not display-vsync locked)
3. Delivers IOSurface Mach ports to our XPC consumer

Every frame pays a GPU readback cost that the normal path avoids. The capturer
is a recording API bolted onto the side of the normal compositor — not the
display path.

#### Options to eliminate the capturer

**Option A: Use `ca_context_id` directly.**

Let the display compositor produce its normal `ca_context_id` output. Send the
ID (a `uint32_t`) over XPC to the GUI. The GUI creates a `CALayerHost` with that
context ID. This is how Chrome's own multi-process architecture works — the GPU
process owns the CAContext, the browser process hosts it.

Caveat: `CALayerHost` normally needs to be in an NSView hierarchy for the Window
Server to composite it. Whether it can be used with our Metal renderer (which
composites into its own drawable) needs investigation. We may need to overlay
the `CALayerHost` as a sublayer of our Metal view's backing layer.

**Option B: Force the IOSurface path.**

Disable remote CoreAnimation in the Chromium Profile Server so the display
compositor produces `io_surface_mach_port` instead of `ca_context_id`. This
gives us IOSurface Mach ports from the normal compositor output — no capturer,
no GPU readback. The IOSurface is the same format we already handle in our Metal
overlay pipeline.

Caveat: need to verify that disabling remote CALayers doesn't degrade Chromium's
internal rendering performance.

**Option C: Intercept at `AcceleratedWidgetCALayerParamsUpdated`.**

Override the callback in `RenderWidgetHostViewMac` (line 156) to forward
`CALayerParams` over XPC instead of setting them on an NSView. This would let us
choose between the `ca_context_id` and `io_surface_mach_port` paths dynamically.

All three options eliminate the `FrameSinkVideoCapturer` entirely, removing the
GPU readback cost, the capture timer latency, and the decoupled timing. The
Chromium Profile Server would produce frames on the display compositor's vsync
clock, not on a recording timer.

### Research 2: Thought experiment — in-process vs out-of-process

The goal is performance equivalent to (or better than) native Chrome. There are
two architectural paths to get there.

#### Option 1: In-process Content API embedding

Write a thin Zig embedder that calls the Content API via C bindings — analogous
to Content Shell's 2000 lines of C++. The GUI process becomes the "browser
process." Chromium still spawns its own renderer and GPU sub-processes
internally, but the coordinator — the part that receives input, hosts the
compositor, and displays output — lives inside the GUI.

This is what ts4 proved: in-process Content API, multiple profiles, 60fps (Issue
406–413). The GUI would receive `CALayerParams` directly from its own GPU
sub-process, create a `CALayerHost`, and the Window Server composites it. Input
goes directly to the compositor thread. Zero IPC for either direction. The
absolute lowest possible latency — identical to Chrome.

Downsides:

- Tightly coupled to Chromium's C++ API surface.
- Chromium's threading model lives in the GUI process (main thread, IO thread,
  compositor threads).
- A Chromium crash takes down the terminal.
- Every rendering engine (Gecko, WebKit) would need its own in-process
  integration.

#### Option 2: Out-of-process with shared memory

Keep Chromium in a separate process, but replace the recording API and
message-passing with Chrome's own cross-process patterns. Chrome's GPU process
is already separate from the browser process, yet Chrome achieves 1-frame
latency. The transport (Mach ports) is the same as XPC. What matters is what
travels over it.

Concretely: the Chromium server produces a `ca_context_id` (as it normally
does), sends the ID once over XPC, and the GUI creates a `CALayerHost`. The
Window Server composites it at vsync — zero pixel copying, zero GPU readback.
Input uses a shared memory ring buffer with a lightweight Mach port signal. One
extra process boundary versus Option 1, but it's the same boundary Chrome has
internally between its browser and GPU processes.

Downsides:

- One extra hop for input dispatch (GUI → Chromium).
- More architectural complexity for shared memory plumbing.

But these are the same costs Chrome pays internally and still achieves 1-frame
latency.

#### Option 3: CALayerHost overlay without shared memory

A simpler variant of Option 2. The Chromium server runs normally, produces its
`ca_context_id`, sends it to the GUI once per tab. The GUI overlays a
`CALayerHost` as a sublayer of its Metal view's backing layer. Input forwarding
stays over XPC (not shared memory) but with high-priority dispatch queues. This
is the lowest-effort path that eliminates the capturer entirely. It won't match
Chrome exactly — the XPC input hop adds 2–3ms — but it eliminates the dominant
source of latency (the capturer's 0–8ms timer + GPU readback).

#### Recommendation: Option 2

Three reasons:

1. **Performance parity with Chrome is achievable.** Chrome's own architecture
   crosses the same process boundary (browser → GPU) and achieves 1-frame
   latency. If TermSurf mirrors that pattern — `CALayerHost` for display, shared
   memory for input — the latency ceiling is the same. The extra input hop (GUI
   → Chromium) with shared memory is microseconds, not milliseconds.
2. **Engine flexibility.** Supporting Gecko or WebKit as alternative engines
   becomes possible. Each engine runs in its own process with a shared protocol
   for frame delivery and input. In-process embedding means writing and
   maintaining a separate C++ integration layer for each engine's API surface.
3. **Process isolation.** A Chromium crash kills a browser tab, not the entire
   application. This matters for a terminal emulator where people have work
   open.

The one thing in-process gives that out-of-process doesn't: compositor-thread
input handling with zero dispatch latency. But Chrome's own numbers show that
the process boundary cost for input (with shared memory) is sub-millisecond. The
difference is imperceptible.

The current latency problem isn't the process boundary — it's that we bolted a
recording API onto the side of a display pipeline and used message-passing where
Chrome uses shared memory. Fix those two things and the out-of-process
architecture matches Chrome's own internal architecture.

### Research 3: Feasibility of a Zig Content Shell

Can the Content Shell embedder be rewritten in Zig? What would it take? What do
we actually use from Content Shell's 13,000 lines?

#### What we modify

Of the 13,000 lines in Content Shell, the Chromium Profile Server modifies
exactly 4 files and adds 2 new files:

| File                          | Lines   | What it does                                          |
| ----------------------------- | ------- | ----------------------------------------------------- |
| `shell_browser_main_parts.h`  | +40     | TabState struct, XPC methods, event handlers          |
| `shell_browser_main_parts.cc` | +590    | XPC gateway, tab lifecycle, input routing, navigation |
| `shell_video_consumer.h`      | 113 new | Capturer + observer class declaration                 |
| `shell_video_consumer.cc`     | 347 new | Frame capture, IOSurface transfer, loading/URL sync   |

Total TermSurf-specific code: about 1,050 lines. Everything else — the other
100+ files — is copied verbatim from Content Shell.

#### What we actually use

**Used now:**

- `ContentMain()` — entry point (1 call)
- `ContentMainDelegate` — app init (subclass with 5 overrides)
- `ContentBrowserClient` — browser config (subclass with 55 overrides, most are
  defaults)
- `BrowserMainParts` — init pipeline (8 overrides)
- `BrowserContext` / `ShellBrowserContext` — profile storage
- `Shell` — WebContents lifecycle (`CreateNewWindow`, `Close`, `Shutdown`)
- `WebContents` — `GetRenderWidgetHostView()`, `GetController()`, `LoadURL()`
- `RenderWidgetHost` — `ForwardMouseEvent()`, `ForwardWheelEvent()`,
  `ForwardKeyboardEventWithCommands()`
- `NavigationController` — `GoBack()`, `GoForward()`, `Reload()`
- `WebContentsObserver` — `DidFinishNavigation`, `DidStartLoading`,
  `LoadProgressChanged`, etc.
- `ClientFrameSinkVideoCapturer` — frame capture (will be eliminated by this
  issue)
- `ShellPlatformDelegate` — native window management

**Needed for TODO items:**

- `WebContentsDelegate::AddNewContents()` — target="_blank"
- `JavaScriptDialogManager` — alert/confirm/prompt
- `DownloadManager` / `DownloadManagerDelegate` — downloads
- `FileSelectHelper` / `RunFileChooser()` — file uploads
- `HostZoomMap` — page zoom
- `LoginHandler` — HTTP Basic Auth
- `PermissionController` — camera/mic
- `DevToolsManagerDelegate` — Web Inspector (already partially set up)

**Not needed (can strip):**

- Web test infrastructure (`IsRunWebTestsSwitchPresent()` paths) — roughly 30%
  of Content Shell
- Android, iOS, Fuchsia, ChromeOS platform code — macOS only
- Aura/Ozone UI — macOS doesn't use these

#### The C++ problem

The Content API is C++. Every interface is a C++ abstract class with virtual
methods:

```cpp
class ContentMainDelegate { virtual ... };     // 20 virtual methods
class ContentBrowserClient { virtual ... };    // 356 virtual methods (override 10)
class BrowserMainParts { virtual ... };        // 10 virtual methods
class WebContentsDelegate { virtual ... };     // 30 virtual methods
class WebContentsObserver { virtual ... };     // 40 virtual methods
```

You cannot subclass a C++ virtual class from Zig. The Content API uses virtual
dispatch, templates, `std::string`, `std::unique_ptr`, `base::OnceCallback`,
etc. Zig's `@cImport` handles C headers, not C++.

#### The simplest path: thin C++ shim + Zig logic

The same pattern Ghostty uses for Objective-C: a thin wrapper that bridges
between C++ and Zig.

```
content_api_shim.h    — C header (Zig-callable)
content_api_shim.cc   — C++ implementation (800–1200 lines)
├── Subclasses ContentMainDelegate, forwards to C callbacks
├── Subclasses ContentBrowserClient, forwards to C callbacks
├── Subclasses BrowserMainParts, forwards to C callbacks
├── Subclasses WebContentsDelegate/Observer, forwards to C callbacks
├── Exposes C functions: ts_create_web_contents(), ts_load_url(),
│   ts_forward_mouse_event(), ts_go_back(), ts_go_forward(), etc.
└── Mechanical glue — no logic

embedder.zig          — Zig implementation (500–800 lines)
├── Tab lifecycle management
├── XPC or shared memory communication
├── Input event routing
├── Navigation commands
└── Profile management
```

The C++ shim is mechanical forwarding. All logic lives in Zig. When a new
Content API feature is needed (downloads, file picker, permissions), add one C
function to the shim and implement the logic in Zig.

This replaces all 100+ Content Shell files with two files (shim + Zig) plus
`ShellBrowserContext` (150 lines, or rewritten via the shim).

#### In-process vs out-of-process implications

**Out-of-process:** The Zig Content Shell runs as a separate binary. The C++
shim links against `libcontent` and exposes a C API. Zig calls it. The binary
communicates with the GUI via shared memory. Straightforward — cleaner version
of what we have now.

**In-process:** The C++ shim becomes a library linked into the GUI. Zig calls
`ts_content_main()` on a dedicated thread. The Content API's browser process
runs inside the GUI process. No IPC for frames or input — direct function calls.
This is where the Zig rewrite pays off: the embedder logic integrates with the
GUI's existing Zig code (Surface, Metal renderer, XPC gateway).

#### Assessment

The Chromium Profile Server currently carries 14,000 lines: 13,000 of unmodified
Content Shell boilerplate + 1,050 of TermSurf logic. The Zig approach replaces
all of that with about 1,400 lines: an 800-line C++ shim (mechanical forwarding,
no logic) + 600 lines of Zig (all the actual decisions). The C++ shim is
unavoidable — the Content API is C++ and will always be C++ — but it contains
zero logic.

The value:

1. **In-process becomes possible.** The Zig embedder lives inside the GUI binary
   and shares state directly.
2. **Cleaner than forking.** No more carrying 100+ unmodified Content Shell
   files. Just the shim + Zig logic.
3. **Incremental.** Each TODO item (downloads, dialogs, permissions) adds one C
   function to the shim + Zig logic.

### Research 4: Multi-profile in one process

The multi-process architecture (one Chromium server per browser profile) was
inherited from the CEF era. CEF's `SingletonLock` file prevents two processes
from opening the same `root_cache_path`, and CEF Chrome runtime (post-M128)
ignores custom `cache_path` — the `root_cache_path` IS the profile. One process
= one profile. This was the defining constraint of ts3 (Issue 303, 325–350).

But CEF is gone. TermSurf uses the Content API directly. And the Content API
does not have this limitation.

#### Already proven in ts4

Multiple profiles in one process was proven across four experiments:

- **Issue 406** — `content::BrowserContext` supports multiple instances with
  different storage paths. Each gets isolated cookies, localStorage, and cache.
  The one-profile-per-process constraint was a CEF limitation, not a Chromium
  limitation.
- **Issue 407** — In-process Chromium PoC: two profiles, side by side, high
  framerate.
- **Issue 408** — Two profiles side by side at 60fps in content_shell.
- **Issue 413** — Converted a one-profile app into a two-profile app.

The Content API supports any number of `BrowserContext` instances in one
process. Each has its own storage path, cookies, localStorage, and cache. Ten
profiles in one process is fine — just ten `BrowserContext` instances with
different paths.

#### What this means for the architecture

The entire current multi-process architecture exists to work around a CEF
limitation that no longer applies:

- xpc-gateway daemon (rendezvous service)
- Profile server spawning (one process per profile)
- Per-server XPC connections
- IOSurface Mach port transfer (frame streaming)
- FrameSinkVideoCapturer (recording API for cross-process capture)
- 120fps oversampling (compensating for capture timer jitter)

With in-process multi-profile, this collapses to: Zig calls C shim, C shim calls
Content API, Content API renders into CALayerHost. No IPC. No process
management. No frame capture. No Mach port transfer.

Chromium still spawns its own renderer and GPU sub-processes internally — that's
Chromium's business, not ours. From the GUI's perspective, it's a library call.

#### Out-of-process is still an option

Even with multi-profile working in-process, there are reasons to keep
out-of-process as an option:

- **Crash isolation.** A Chromium crash in-process kills the terminal. Out-of-
  process, it kills a browser tab.
- **Engine flexibility.** Supporting Gecko or WebKit alongside Chromium is
  easier when each engine is a separate process.

But these would be choices, not constraints. The multi-process architecture
would exist because we want it, not because CEF forces it.

## Conclusion

Input latency in TermSurf comes from three sources: the FrameSinkVideoCapturer
(a recording API running on its own timer, not the display path), asynchronous
XPC message-passing (paid twice per frame), and a double-vsync penalty inherent
to out-of-process streaming. Together these add 15–25ms of latency versus native
Chrome.

Research into Chromium's internals revealed that Chrome achieves low latency
across its own process boundaries using shared memory and the native display
path — not message-passing. Content Shell uses the exact same pipeline as
Chrome: CALayerHost, zero-copy GPU compositing, compositor-thread input
handling. Our FrameSinkVideoCapturer bypasses all of this. It is a recording API
bolted onto the side of the display compositor — not the display path itself.

Further research showed that the entire multi-process architecture (xpc-gateway,
profile server spawning, per-server XPC connections, IOSurface Mach port
transfer, frame capture, 120fps oversampling) exists to work around a CEF
limitation that no longer applies. CEF required one process per profile. The
Content API does not — ts4 proved that multiple `BrowserContext` instances
coexist in one process with full isolation (Issues 406–413).

The path forward is to build a Zig Content Shell: a minimal Chromium embedder
using a thin C++ shim (forwarding calls to the Content API) and Zig logic (tab
management, input routing, profile lifecycle). This replaces the 14,000-line
Content Shell fork with about 1,400 lines. The critical experiment is whether
two different browser profiles can run in the same Zig process. If they can, the
in-process architecture is the answer — zero IPC, zero frame capture, native
Chrome performance. If they can't, the Zig Content Shell still serves as a
cleaner out-of-process server, replacing the current hodgepodge with a minimal,
understandable codebase.

Either way, the Zig Content Shell is the next step. This will be tracked in a
new issue.
