# Issue 414: Two Profiles via XPC

## Goal

Two browser profiles rendering side by side in one window at an uncompromising
60fps, each profile running in its own process, communicating via XPC with
IOSurface Mach port transfer. This is the architecture that TermSurf will ship.

## Background

### The multi-profile problem

Issue 413 proved the core constraint: two `BrowserContext` instances in one
Chromium process drop rendering to 2fps (Experiment 4), while two `WebContents`
sharing one `BrowserContext` render at 60fps side by side (Experiment 6). The
boundary is clear — one profile per process.

### Multi-process rendering is proven

Three prior efforts proved that cross-process rendering via IOSurface Mach port
transfer works on macOS:

| Effort                           | Result                        | Bottleneck                                         |
| -------------------------------- | ----------------------------- | -------------------------------------------------- |
| **Issue 403** (Swift+Rust+C++)   | 60fps, <0.12ms composite time | None — architecture proven                         |
| **cef-test** (two CEF profiles)  | 50fps per profile             | CEF internal scheduling jitter (~15% vsync misses) |
| **ts3** (WezTerm + CEF profiles) | 38fps                         | Input pipeline + GUI rendering overhead            |

The cef-test result is the most relevant. Two independent CEF processes
rendering simultaneously achieved 50fps each with p50 = 16.7ms (exactly on
vsync). The ~10fps gap from 60fps is entirely due to CEF's
`do_message_loop_work()` jitter — not XPC, not IOSurface transfer, not
compositing. The Content API should eliminate this ceiling entirely.

### What we're building on

- **One Profile app** (Issue 412–413) — A Content Shell clone that renders at
  60fps. This becomes the basis for each profile server process.
- **cef-test** — Multi-process architecture with proven XPC protocol and
  IOSurface compositing. Port the frame delivery and compositing code, replace
  CEF with Content API, simplify bootstrap by eliminating the launcher.
- **termsurf-xpc** — Rust XPC bindings used by cef-test and ts3. Wraps
  `xpc_connection`, `xpc_dictionary`, Mach port transfer, IOSurface
  create/lookup.

## Branch

Create a new branch `146.0.7650.0-issue-414` in the `termsurf-chromium`
submodule, forked from `146.0.7650.0-issue-412` (the One Profile app). This
starts with a working Content Shell clone that renders at 60fps with a single
profile — the baseline from Issue 412. Each experiment is a commit on top.

```bash
cd ts4/termsurf-chromium/src
git checkout -b 146.0.7650.0-issue-414 146.0.7650.0-issue-412
```

We fork from Issue 412 (not Issue 413) because Issue 413 added multi-profile
experiments (second `BrowserContext`, second `WebContents`, side-by-side layout)
that we don't need here. Issue 414's experiments build new capture and XPC
machinery on top of a clean single-profile app.

## Architecture

```
Two Profiles GUI (Cocoa/Metal window + XPC Mach service)
├── Listens on com.termsurf.two-profiles
├── Spawns profile-a server → connects back to GUI
│   └── Left pane ◀── IOSurface Mach port ── Profile A server (Content API)
├── Spawns profile-b server → connects back to GUI
│   └── Right pane ◀── IOSurface Mach port ── Profile B server (Content API)
└── Composites both IOSurfaces into one window
```

Two process types (no launcher):

1. **GUI process** — Creates a single window with two Metal quads. Registers as
   a named XPC Mach service (`com.termsurf.two-profiles`). Spawns profile server
   processes as children, passing the service name and a session ID as CLI args.
   Receives IOSurface Mach ports from both profile servers via XPC. Imports each
   as a Metal texture and composites them side by side. No browser code runs
   here.

2. **Profile server process** (one per profile) — Runs the Content API with a
   single `BrowserContext`. Navigates a `WebContents` to the test page. Captures
   the composited output as an IOSurface. Connects to the GUI's Mach service by
   name and sends IOSurface Mach ports every frame.

### Why no launcher?

In cef-test and ts3, a separate launcher process acted as a middleman: the GUI
sent an anonymous XPC endpoint to the launcher, the launcher stored it, and the
profile server claimed it. This relay was necessary because XPC endpoints can
only be transferred over existing XPC connections — two processes with no shared
channel have no way to exchange endpoints.

The launcher solved the bootstrap problem, but it was a third process (~220
lines) that existed solely to relay one message per profile. A simpler
alternative: **make the GUI itself the named Mach service.** The GUI registers a
hard-coded service name (e.g., `com.termsurf.two-profiles`) via a launchd plist.
Profile servers receive this name as a CLI argument and connect directly. No
endpoint relay, no session claiming, no middleman.

The connection is bidirectional — once established, the profile server sends
IOSurface frames to the GUI, and the GUI sends input events (keyboard, mouse,
resize) back to the profile server over the same connection. This is the same
communication pattern as cef-test, just with one fewer process.

If the GUI-as-service approach hits problems (e.g., multiple GUI instances
conflicting on the service name), we can fall back to the launcher pattern. For
the PoC with a single window, the simpler approach should work.

### XPC protocol

**Bootstrap (simplified from cef-test):**

1. GUI registers as Mach service `com.termsurf.two-profiles` (via launchd plist)
2. GUI spawns profile-a server with args:
   `--service com.termsurf.two-profiles
   --session-id profile-a --profile profile-a --url <url>`
3. Profile-a server connects to `com.termsurf.two-profiles` by name
4. Profile-a server sends `register` message with its session ID
5. GUI maps the connection to the left pane
6. Repeat for profile-b (right pane)

No anonymous listeners, no endpoint relay, no claim handshake. Each profile
server connects directly to the GUI.

**Frame delivery (fast path, every frame):**

```
Profile server → GUI:
{
  action: "display_surface",
  iosurface_port: <mach_port_t>,  // set_mach_send()
  width: i64,                      // physical pixels
  height: i64,                     // physical pixels
}
```

**Input forwarding (GUI → profile server, same connection):**

```
GUI → Profile server:
{
  action: "key_event" | "mouse_click" | "mouse_move" | "resize" | ...,
  ... event-specific fields ...
}
```

**GUI import pipeline:**

1. `copy_mach_send("iosurface_port")` — extract Mach port from XPC message
2. `IOSurfaceLookupFromMachPort(port)` — reconstruct IOSurface in GUI process
3. Import as Metal texture
4. Composite into window
5. `mach_port_deallocate(port)` — release kernel resource

## Prior art: what to reuse

### From cef-test

cef-test used a three-process architecture (GUI, launcher, profile server) where
the launcher relayed XPC endpoints between the GUI and profile servers. We're
simplifying to two process types (GUI + profile servers) by making the GUI the
named Mach service, but the frame delivery and compositing code is directly
reusable:

- **Frame delivery protocol:** `display_surface` message with `iosurface_port`.
  One message per frame, ~100 bytes + Mach port. Identical to what we need.
- **GUI compositing:** wgpu render pipeline with two quads (left/right),
  IOSurface import via `IOSurfaceLookupFromMachPort`, sRGB texture views.
- **Background dispatch queue for XPC callbacks:** Critical discovery — XPC
  handlers must dispatch on a background queue, not the main queue, to avoid
  conflicts with the GUI event loop.
- **Benchmark harness:** 60-second automated run with frame interval statistics
  (avg fps, % at 60fps, p50/p95/p99, max consecutive streak).

### From termsurf-xpc

Reference implementation for the XPC patterns we need. The Rust code won't be
reused directly, but the patterns translate 1:1 to Apple's C API
(`<xpc/xpc.h>`):

- **Connection management:** `xpc_connection_create_mach_service()` for named
  services, `xpc_connection_set_event_handler()` for message dispatch.
- **Mach port transfer:** `xpc_dictionary_set_mach_send()` (sender) /
  `xpc_dictionary_copy_mach_send()` (receiver).
- **IOSurface sharing:** `IOSurfaceCreateMachPort()` (sender) /
  `IOSurfaceLookupFromMachPort()` (receiver) / `mach_port_deallocate()`
  (cleanup).

### From the One Profile app

- **Content API embedder:** Complete, buildable, 60fps Content Shell clone. This
  becomes the profile server with the addition of IOSurface capture and XPC
  frame delivery.
- **Profile path management:** `SHELL_DIR_USER_DATA` override for isolated
  profile storage. Each profile server process overrides to its own path.

## Language choice for the PoC

C++ for everything. Both the GUI and profile server are C++/Objective-C++.

- **Profile server:** C++. Links against Chromium. XPC calls use Apple's C API
  directly (`<xpc/xpc.h>`). Modified from the One Profile app.
- **GUI:** C++/Objective-C++. Metal rendering via Objective-C++
  (`<Metal/Metal.h>`, `<QuartzCore/QuartzCore.h>`). XPC Mach service
  registration via Apple's C API. IOSurface import via
  `<IOSurface/IOSurface.h>`.

This keeps the entire PoC in one language, avoids cross-language build
complexity, and matches Chromium's own codebase. The cef-test Rust code and
termsurf-xpc crate are useful as reference for the XPC protocol and IOSurface
transfer patterns, but the implementation will be native C++.

## How Electron captures GPU textures

Electron's off-screen rendering (OSR) solves the same problem we need to solve:
capture the composited output of a `WebContents` as a GPU texture, without
displaying it in a window. Studying Electron's approach reveals that Chromium
already has a built-in API for this.

### Two capture paths (GPU vs. software)

Electron has two capture paths, selected by whether GPU acceleration is enabled:

**GPU-accelerated (FrameSinkVideoCapturer):** When
`HardwareAccelerationEnabled()` returns true (the normal case), Electron creates
an `OffScreenVideoConsumer` backed by `ClientFrameSinkVideoCapturer`. This is
Chromium's built-in video capture API — the same mechanism Chrome uses for tab
capture, WebRTC screen sharing, and remote display. It issues
`CopyOutputRequest`s at the compositor level and delivers frames as
`GpuMemoryBufferHandle`s. On macOS, these handles are IOSurfaces.

**Software rasterization (HostDisplayClient):** When GPU acceleration is
disabled, Electron falls back to `OffScreenHostDisplayClient`. On macOS, this
receives `OnDisplayReceivedCALayerParams()` callbacks from Chromium's
compositor, which include `io_surface_mach_port`. This is the older, legacy
path.

The selection logic is straightforward (`osr_render_widget_host_view.cc`):

```cpp
if (content::GpuDataManager::GetInstance()->HardwareAccelerationEnabled()) {
  video_consumer_ = std::make_unique<OffScreenVideoConsumer>(...);
  video_consumer_->SetActive(is_painting());
} else {
  // Falls through to HostDisplayClient path
}
```

Only one path is active at a time. They are never used simultaneously.

### FrameSinkVideoCapturer: the GPU-accelerated path

This is the path that matters for TermSurf. GPU acceleration is not optional —
we need it for 60fps rendering.

How it works:

1. `CreateVideoCapturer()` on the `RenderWidgetHostView` creates a
   `ClientFrameSinkVideoCapturer` (host side) linked to a
   `FrameSinkVideoCapturerImpl` (renderer side)
2. Chromium's viz layer monitors frame damage and issues `CopyOutputRequest`s
3. Frames arrive in `OnFrameCaptured()` as `GpuMemoryBufferHandle`s
4. On macOS, the handle contains an IOSurface pointer
   (`OffscreenSharedTextureValue.shared_texture_handle`)
5. A buffer pool of 10 pre-allocated GPU textures eliminates per-frame
   allocation

Key properties:

- **Supported API.** Designed for continuous frame capture, not a hook into
  compositor internals.
- **Buffer pooling.** 10-frame ring buffer (`kFramePoolCapacity = 10`), no
  allocation per frame.
- **Frame rate control.** Built-in `SetMinCapturePeriod()`.
- **Damage tracking.** Only dirty regions flagged via `content_rect`.
- **Cross-platform.** IOSurface on macOS, D3D11 on Windows, DMA-BUF on Linux.

The tradeoff is that `CopyOutputRequest` involves a GPU-to-GPU copy — not true
zero-copy. But it's a GPU-side copy, fast enough for Chrome's real-time tab
capture.

### The `useSharedTexture` option

Within the FrameSinkVideoCapturer path, a separate `useSharedTexture` preference
controls the capture format:

- `true` → GPU shared texture (IOSurface on macOS). This is what we want.
- `false` → Shared memory bitmap (CPU-accessible pixels).

This preference does NOT select between the two capture paths — it only controls
the buffer format within the GPU-accelerated path.

### Why the CALayerParams path is irrelevant

The `OffScreenHostDisplayClient` / `OnDisplayReceivedCALayerParams()` path on
macOS is only active when GPU acceleration is disabled. Since TermSurf requires
GPU acceleration for 60fps rendering, this path is irrelevant to us. Early
research (before studying Electron's source) considered intercepting at
`DisplayCALayerTree::UpdateCALayerTree()` to grab
`CALayerParams.io_surface_mach_port`, but this is the wrong approach — it's the
software fallback, not the GPU-accelerated path.

### What this means for TermSurf

The profile server should use `FrameSinkVideoCapturer` to capture composited
frames as IOSurfaces, then create Mach ports from those IOSurfaces and send them
to the GUI via XPC. This is exactly what Electron does for its `paint` event
with `useSharedTexture = true`, except instead of delivering the texture to
JavaScript, we deliver the Mach port to a separate GUI process.

Key reference files:

- `electron/shell/browser/osr/osr_video_consumer.{h,cc}` — capture logic
- `electron/shell/browser/osr/osr_render_widget_host_view.{h,cc}` — OSR widget
- `electron/shell/browser/osr/osr_paint_event.h` — frame data structures
- `electron/shell/browser/osr/osr_host_display_client_mac.mm` — legacy macOS
  path (irrelevant but useful as reference)

## Ideas for Experiments

### Idea 1: FrameSinkVideoCapturer

**Goal:** Capture composited frames as IOSurfaces at 60fps using Chromium's
built-in video capture API.

Electron's off-screen rendering uses `ClientFrameSinkVideoCapturer`, which
implements `viz::mojom::FrameSinkVideoConsumer`. This is a Chromium API designed
for exactly this use case — capturing compositor output for headless rendering,
screen sharing, and remote display. Chrome uses it for tab capture and WebRTC.

How it works:

1. Call `CreateVideoCapturer()` on the `RenderWidgetHostView`
2. Chromium's viz layer issues `CopyOutputRequest`s at the compositor level
3. Frames arrive in `OnFrameCaptured()` as `GpuMemoryBufferHandle`s — which on
   macOS are IOSurfaces
4. A buffer pool of 10 pre-allocated GPU textures eliminates per-frame
   allocation

Advantages:

- **Supported API.** Designed for continuous capture, not a hook into internals.
- **Buffer pooling.** 10-frame ring buffer, no allocation per frame.
- **Frame rate control.** Built-in `SetMinCapturePeriod()`.
- **Damage tracking.** Only dirty regions flagged.
- **Cross-platform.** IOSurface on macOS, D3D11 on Windows, DMA-BUF on Linux.

Tradeoff: involves a `CopyOutputRequest` (GPU-to-GPU copy), so not true
zero-copy. But it's a GPU-side copy — fast enough for Chrome's real-time tab
capture at 60fps.

Reference: `electron/shell/browser/osr/osr_video_consumer.{h,cc}`.

### Idea 2: Single profile server with XPC frame delivery

**Goal:** Prove IOSurface Mach port transfer from a Content API process to a
separate GUI process works at 60fps.

Two components:

1. **GUI** (C++/ObjC++) — registers as Mach service `com.termsurf.two-profiles`,
   spawns profile server, receives Mach ports, imports as Metal textures,
   renders to window
2. **Profile server** (modified One Profile app, C++) — captures frames as
   IOSurfaces, connects to GUI's Mach service, sends Mach ports via XPC

This proves the full pipeline: Content API → IOSurface → Mach port → XPC → GPU
texture → window. If this hits 60fps, the architecture is validated.

### Idea 3: Two profile servers, one window

**Goal:** Two profiles, two processes, one window, both at 60fps.

Run two profile server instances (profile-a and profile-b) with the GUI
displaying both side by side. This is the target architecture — identical to
cef-test but with Content API instead of CEF.

Success criteria: both panes rendering the spinning blue square at 60fps with
different localStorage identities (proving profile isolation).

### Idea 4: Stress test and benchmarking

**Goal:** Sustained 60fps under load, matching or exceeding cef-test's 50fps.

Run the two-profile setup for 60+ seconds with continuous animation. Measure:

- Average FPS per profile
- Percentage of frames at 60fps (within one vsync interval)
- p50, p95, p99 frame intervals
- CPU usage (must not be 100%)
- Max consecutive frames at 60fps

Compare against cef-test's benchmark (50fps, 80.8% at 60fps, p50=16.7ms,
p95=33.6ms). The Content API should beat these numbers since CEF's internal
scheduling jitter was the bottleneck.

## Success criteria

- Two panes in one window, each showing the spinning blue square
- Different localStorage identity in each pane (profile isolation)
- Both at 60fps sustained for 60+ seconds
- CPU usage well below 100% (no busy-wait loops)
- IOSurface transfer via XPC (not shared memory, not window capture)

## What this unlocks

Once this PoC works, the path to TermSurf is clear:

1. **Ghostty integration:** Replace the Rust/Swift GUI with Ghostty's Metal
   renderer. Ghostty composites IOSurfaces from profile servers alongside
   terminal panes.
2. **Input forwarding:** GUI sends keyboard and mouse events to profile servers
   via XPC (reverse direction of the frame pipeline).
3. **Process lifecycle:** Ghostty manages profile server processes. Multiple
   `web` commands for the same profile reuse the existing process.
4. **Multiple WebContents per profile:** Each profile server handles multiple
   WebContents (tabs). Issue 413 Experiment 6 proved this works at 60fps.

## Experiments

### Experiment 1: Capture frames with FrameSinkVideoCapturer

#### Hypothesis

Chromium's `FrameSinkVideoCapturer` — the same API Electron uses for off-screen
rendering — can capture composited frames from a `WebContents` as IOSurfaces at
60fps. If this works, the profile server's capture mechanism is solved: each
frame is already an IOSurface, ready for Mach port transfer via XPC.

#### Background

Electron's GPU-accelerated OSR path uses `ClientFrameSinkVideoCapturer`, which
implements `viz::mojom::FrameSinkVideoConsumer`. The capturer attaches to a
`FrameSinkId` (identifying which compositor output to capture), issues
`CopyOutputRequest`s at the viz layer, and delivers frames as
`GpuMemoryBufferHandle`s. On macOS, these handles contain IOSurfaces.

The API is designed for continuous capture — Chrome uses it for tab capture,
WebRTC screen sharing, and remote display. It has a 10-frame buffer pool,
built-in frame rate control, and damage tracking.

Key reference: `electron/shell/browser/osr/osr_video_consumer.{h,cc}`.

#### Design

Working on the `146.0.7650.0-issue-414` branch (forked from Issue 412's One
Profile app), modify the app to attach a `FrameSinkVideoCapturer` to the first
`WebContents` and log every captured frame. The WebContents still renders
normally in its window — we're just tapping the compositor output.

##### Step 1: Create a video consumer class

Add a new file `shell_video_consumer.{h,cc}` in `content/one_profile/browser/`.
It implements `viz::mojom::FrameSinkVideoConsumer`:

```cpp
#include "components/viz/host/client_frame_sink_video_capturer.h"
#include "content/browser/compositor/surface_utils.h"
#include "content/public/browser/render_widget_host.h"
#include "content/public/browser/render_widget_host_view.h"
#include "content/public/browser/web_contents.h"
#include "services/viz/privileged/mojom/compositing/frame_sink_video_capture.mojom.h"

class ShellVideoConsumer : public viz::mojom::FrameSinkVideoConsumer {
 public:
  void Attach(content::WebContents* web_contents);

  // viz::mojom::FrameSinkVideoConsumer:
  void OnFrameCaptured(
      media::mojom::VideoBufferHandlePtr data,
      media::mojom::VideoFrameInfoPtr info,
      const gfx::Rect& content_rect,
      mojo::PendingRemote<viz::mojom::FrameSinkVideoConsumerFrameCallbacks>
          callbacks) override;
  void OnNewCaptureVersion(
      const media::CaptureVersion& capture_version) override {}
  void OnFrameWithEmptyRegionCapture() override {}
  void OnStopped() override {}
  void OnLog(const std::string& message) override {}

 private:
  std::unique_ptr<viz::ClientFrameSinkVideoCapturer> capturer_;
  int frame_count_ = 0;
  base::TimeTicks last_log_time_;
};
```

##### Step 2: Configure and start capture

In `Attach()`:

1. Get the `HostFrameSinkManager` via `content::GetHostFrameSinkManager()`
2. Create a capturer via `manager->CreateVideoCapturer()`
3. Configure: `SetFormat(media::PIXEL_FORMAT_ARGB)`,
   `SetMinCapturePeriod(base::Milliseconds(16))` (60fps),
   `SetAutoThrottlingEnabled(false)`
4. Get the `FrameSinkId` from the WebContents:
   `web_contents->GetRenderWidgetHostView()->GetRenderWidgetHost()->GetFrameSinkId()`
5. Set target:
   `capturer_->ChangeTarget(viz::VideoCaptureTarget(frame_sink_id), 0)`
6. Start:
   `capturer_->Start(this, viz::mojom::BufferFormatPreference::kPreferMappableSharedImage)`

The `kPreferMappableSharedImage` preference is what makes Chromium deliver
IOSurfaces (GPU memory buffers) instead of shared memory bitmaps.

##### Step 3: Handle captured frames

In `OnFrameCaptured()`:

1. Check `data->is_gpu_memory_buffer_handle()` — if false, log a warning (means
   we got shared memory instead of an IOSurface)
2. Extract the handle: `data->get_gpu_memory_buffer_handle()`
3. On macOS, the handle contains an IOSurface: `gmb_handle.io_surface().get()`
   returns an `IOSurfaceRef`
4. Log the IOSurface dimensions via `IOSurfaceGetWidth()` /
   `IOSurfaceGetHeight()`
5. Increment frame counter, log fps once per second
6. **Signal done immediately:** create a `mojo::Remote` from the `callbacks`
   pending remote and call `Done()`. This returns the buffer to the pool. If we
   don't call `Done()`, the 10-frame pool depletes and capture stalls.

##### Step 4: Wire it up

In `ShellBrowserMainParts::InitializeMessageLoopContext()`, after creating the
Shell and WebContents, create a `ShellVideoConsumer` and call
`Attach(web_contents)`. The consumer must outlive the WebContents — store it as
a member of `ShellBrowserMainParts`.

Note: the `RenderWidgetHostView` may not exist immediately after
`Shell::CreateNewWindow()`. If `GetRenderWidgetHostView()` returns null, defer
attachment. The simplest approach: post a delayed task (e.g., 1 second) to allow
navigation to complete. A production implementation would use
`WebContentsObserver::RenderViewReady()`, but for the PoC a delay is fine.

#### What we're modifying

- **New files:** `content/one_profile/browser/shell_video_consumer.{h,cc}`
- **Modified:** `content/one_profile/browser/shell_browser_main_parts.{h,cc}` —
  add `ShellVideoConsumer` member, wire up in `InitializeMessageLoopContext()`
- **Modified:** `content/one_profile/BUILD.gn` — add new source files

No changes to Chromium's own code. Everything uses public Content API +
`content/browser/compositor/surface_utils.h` (internal but stable).

#### Expected result

Log output showing 60 captured frames per second, each with an IOSurface of the
correct dimensions (e.g., 1200×1200 physical pixels for a 600×600 logical view
at 2x Retina). The window still displays normally — we're capturing alongside
the regular rendering path.

```
[INFO] Frame 1: 1200x1200 IOSurface (gpu_memory_buffer)
[INFO] Frame 2: 1200x1200 IOSurface (gpu_memory_buffer)
...
[INFO] 60 frames in last 1.00s (60.0 fps)
```

#### What a failure would mean

- **`is_gpu_memory_buffer_handle()` returns false:** We got shared memory
  instead of an IOSurface. Check that `kPreferMappableSharedImage` was passed to
  `Start()`, and that GPU acceleration is enabled (no `--disable-gpu` flag).
- **0 frames:** The capturer didn't attach to the right `FrameSinkId`, or the
  `RenderWidgetHostView` wasn't ready. Check timing and frame sink resolution.
- **< 60fps:** The `CopyOutputRequest` overhead is significant, or
  auto-throttling is still enabled. Try `SetAutoThrottlingEnabled(false)` and
  `SetMinCapturePeriod(base::TimeDelta())` (unlimited).
- **Crash in `io_surface()`:** The `GpuMemoryBufferHandle` type on macOS doesn't
  use IOSurface in this build configuration. Investigate the handle type and
  platform-specific extraction.

#### Result: PASSED

`FrameSinkVideoCapturer` delivers IOSurfaces at a rock-solid 60fps. Every frame
arrives as a `gpu_memory_buffer_handle` containing a valid IOSurface.

```
[ShellVideoConsumer] Attached to FrameSinkId FrameSinkId(5, 3), starting capture
[ShellVideoConsumer] 61 frames in 1.0047s (60.7144 fps) | IOSurface 640x360
[ShellVideoConsumer] 60 frames in 1.00019s (59.9889 fps) | IOSurface 640x360
[ShellVideoConsumer] 61 frames in 1.0165s (60.0097 fps) | IOSurface 640x360
[ShellVideoConsumer] 61 frames in 1.0167s (59.9982 fps) | IOSurface 640x360
[ShellVideoConsumer] 60 frames in 1.00249s (59.8507 fps) | IOSurface 640x360
[ShellVideoConsumer] 61 frames in 1.01447s (60.1299 fps) | IOSurface 640x360
[ShellVideoConsumer] 61 frames in 1.01616s (60.0299 fps) | IOSurface 640x360
[ShellVideoConsumer] 60 frames in 1.00022s (59.9868 fps) | IOSurface 640x360
[ShellVideoConsumer] 60 frames in 1.00013s (59.992 fps) | IOSurface 640x360
[ShellVideoConsumer] 61 frames in 1.0166s (60.0041 fps) | IOSurface 640x360
[ShellVideoConsumer] 60 frames in 1.00004s (59.9974 fps) | IOSurface 640x360
[ShellVideoConsumer] 61 frames in 1.0164s (60.0155 fps) | IOSurface 640x360
```

Key observations:

- **60fps sustained.** Every 1-second interval reports exactly 60 or 61 frames.
  No drops, no jitter.
- **IOSurfaces, not shared memory.** Every frame arrived as a
  `gpu_memory_buffer_handle`. The `kPreferMappableSharedImage` preference
  worked as expected.
- **640x360 IOSurface.** This matches the window's content view size. The
  capturer respects the actual rendered resolution.
- **No impact on windowed rendering.** The page renders normally in its window
  while the capturer taps the compositor output in parallel.
- **2-second delayed attach worked.** The `RenderWidgetHostView` was available
  by the time the delayed task fired.

This proves the capture mechanism for the profile server. Each frame is already
an IOSurface — ready for `IOSurfaceCreateMachPort()` and XPC transfer to the
GUI process.

### Experiment 2: XPC frame delivery to a receiver process

#### Hypothesis

IOSurface Mach ports created from `FrameSinkVideoCapturer` frames can be
transferred via XPC to a separate process and reconstructed at 60fps. This
proves the critical link in the architecture: the profile server's captured
IOSurfaces can cross process boundaries without GPU-to-CPU readback.

#### Background

Experiment 1 proved that `FrameSinkVideoCapturer` delivers IOSurfaces at 60fps.
The next step is to verify that these IOSurfaces — which are allocated by
Chromium's GPU process — support `IOSurfaceCreateMachPort()` for cross-process
transfer. This is not guaranteed; the IOSurface must be backed by a kernel
object accessible to other processes.

The XPC transfer pattern is well-established from cef-test and ts3: the sender
calls `IOSurfaceCreateMachPort()` on the IOSurface, embeds the port in an XPC
dictionary via `xpc_dictionary_set_mach_send()`, and sends it. The receiver
calls `xpc_dictionary_copy_mach_send()` to extract the port, then
`IOSurfaceLookupFromMachPort()` to reconstruct the IOSurface. Both sides call
`mach_port_deallocate()` to avoid leaking kernel resources.

Key reference: `ts3/cef-test-profile/src/main.rs` (Mach port creation and send),
`ts3/cef-test-gui/src/main.rs` (Mach port receive and IOSurface import),
`ts3/termsurf-xpc/src/iosurface.rs` (IOSurface Mach port API wrappers).

#### Design

Two binaries, communicating via XPC:

1. **Profile server** — The One Profile app from Experiment 1, modified to
   create Mach ports from captured IOSurfaces and send them via XPC. Keeps its
   window — useful for debugging, and headless mode is a separate concern.
2. **Receiver** — A minimal standalone Objective-C program that receives
   IOSurface Mach ports via XPC and logs their dimensions and frame rate. No
   Metal rendering — this just proves the transfer works. Rendering is a
   separate experiment.

##### Step 1: Build the receiver

Create `ts4/two-profiles-receiver/main.m`, a standalone Objective-C program
(~100 lines). It runs as an XPC Mach service listener:

1. Call
   `xpc_connection_create_mach_service("com.termsurf.two-profiles", queue, XPC_CONNECTION_MACH_SERVICE_LISTENER)`
   to register as a listener on the Mach service name.
2. Set a new-connection handler. For each incoming connection, set an event
   handler that processes `display_surface` messages:
   ```c
   mach_port_t port = xpc_dictionary_copy_mach_send(msg, "iosurface_port");
   IOSurfaceRef surface = IOSurfaceLookupFromMachPort(port);
   size_t w = IOSurfaceGetWidth(surface);
   size_t h = IOSurfaceGetHeight(surface);
   // Log dimensions, increment frame counter, log FPS once per second
   CFRelease(surface);
   mach_port_deallocate(mach_task_self(), port);
   ```
3. Run the dispatch loop with `dispatch_main()`.

Build with:
```bash
clang -framework Foundation -framework IOSurface -o receiver main.m
```

##### Step 2: Register the Mach service

Create a launchd agent plist (`ts4/two-profiles-receiver/com.termsurf.two-profiles.plist`)
that registers the `com.termsurf.two-profiles` Mach service name. This is
required — `xpc_connection_create_mach_service` with the listener flag only
works for launchd-registered services.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.two-profiles</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.two-profiles</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/ryan/dev/termsurf/ts4/two-profiles-receiver/receiver</string>
    </array>
    <key>StandardOutPath</key>
    <string>/Users/ryan/dev/termsurf/logs/two-profiles-receiver.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/ryan/dev/termsurf/logs/two-profiles-receiver.log</string>
</dict>
</plist>
```

Load with:
```bash
launchctl load ~/dev/termsurf/ts4/two-profiles-receiver/com.termsurf.two-profiles.plist
```

Unload with:
```bash
launchctl unload ~/dev/termsurf/ts4/two-profiles-receiver/com.termsurf.two-profiles.plist
```

When the profile server connects to the service name, launchd starts the
receiver on demand. Output goes to `/Users/ryan/dev/termsurf/logs/two-profiles-receiver.log`.

For interactive debugging, skip the plist and run:
```bash
launchctl debug gui/com.termsurf.two-profiles -- $(pwd)/ts4/two-profiles-receiver/receiver
```

##### Step 3: Modify the profile server

Extend `ShellVideoConsumer` to send captured IOSurfaces via XPC:

**XPC connection.** Add a `ConnectToService(const std::string& name)` method:

```cpp
#include <xpc/xpc.h>

void ShellVideoConsumer::ConnectToService(const std::string& name) {
  xpc_connection_ = xpc_connection_create_mach_service(
      name.c_str(), nullptr, 0);  // Client mode (no listener flag)
  xpc_connection_set_event_handler(xpc_connection_, ^(xpc_object_t event) {
    if (xpc_get_type(event) == XPC_TYPE_ERROR) {
      LOG(ERROR) << "[ShellVideoConsumer] XPC error";
    }
  });
  xpc_connection_resume(xpc_connection_);
  LOG(INFO) << "[ShellVideoConsumer] Connected to XPC service: " << name;
}
```

**Mach port send.** In `OnFrameCaptured()`, after extracting the IOSurface:

```cpp
if (xpc_connection_) {
  mach_port_t port = IOSurfaceCreateMachPort(io_surface);
  if (port != MACH_PORT_NULL) {
    xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(msg, "action", "display_surface");
    xpc_dictionary_set_mach_send(msg, "iosurface_port", port);
    xpc_dictionary_set_int64(msg, "width", (int64_t)width);
    xpc_dictionary_set_int64(msg, "height", (int64_t)height);
    xpc_connection_send_message(xpc_connection_, msg);  // Async, no reply
    xpc_release(msg);
    mach_port_deallocate(mach_task_self(), port);
  }
}
```

`xpc_connection_send_message` is fire-and-forget — it returns immediately after
queuing the message. The Mach port send right is copied into the XPC message, so
we deallocate our copy after sending.

**CLI flag.** Add `--xpc-service <name>` to the One Profile app. Parse it in
`ShellBrowserMainParts::InitializeMessageLoopContext()` and pass to the video
consumer's `ConnectToService()` before calling `Attach()`.

##### Step 4: Run and verify

1. `cd ts4/box-demo && bun run server.ts` — Start the test page
2. The receiver starts automatically via launchd when the profile server
   connects, or use `launchctl debug` for interactive mode
3. Start the profile server:
   ```bash
   out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
     --xpc-service com.termsurf.two-profiles \
     http://localhost:9407 2>&1
   ```
4. Check receiver output: `tail -f /Users/ryan/dev/termsurf/logs/two-profiles-receiver.log`

#### What we're creating

- `ts4/two-profiles-receiver/main.m` — Receiver program (standalone ObjC)
- `ts4/two-profiles-receiver/com.termsurf.two-profiles.plist` — Launchd agent
- `ts4/two-profiles-receiver/Makefile` — Build script

#### What we're modifying

- `content/one_profile/browser/shell_video_consumer.{h,cc}` — Add XPC client
  connection, Mach port creation/send in `OnFrameCaptured()`
- `content/one_profile/browser/shell_browser_main_parts.cc` — Parse
  `--xpc-service` flag, call `ConnectToService()` before `Attach()`
- `content/one_profile/BUILD.gn` — May need additional framework deps
  (XPC is part of libSystem so likely no changes)

#### Expected result

Receiver logs showing 60 IOSurface reconstructions per second:

```
[Receiver] Profile server connected
[Receiver] 60 frames in 1.00s (60.0 fps) | IOSurface 640x360
[Receiver] 61 frames in 1.02s (59.8 fps) | IOSurface 640x360
[Receiver] 60 frames in 1.00s (60.0 fps) | IOSurface 640x360
```

Profile server logs showing Mach port sends alongside the existing capture logs
from Experiment 1.

#### What a failure would mean

- **`IOSurfaceCreateMachPort()` returns `MACH_PORT_NULL`:** The IOSurfaces from
  `FrameSinkVideoCapturer` don't support Mach port creation. This would mean
  Chromium allocates them in a way that prevents cross-process sharing. Check
  whether the `GpuMemoryBufferHandle` carries an IOSurface that's backed by a
  real kernel object (not a process-local mapping).
- **`IOSurfaceLookupFromMachPort()` returns NULL:** The Mach port arrived but
  the IOSurface can't be reconstructed. Verify that `copy_mach_send` (not
  `get_mach_send`) is used on the receive side. Check that
  `mach_port_deallocate` isn't called before lookup.
- **< 60fps in receiver:** XPC message overhead is significant. Measure
  per-message latency. The cef-test benchmark showed XPC overhead was
  negligible (~0.1ms per message), so this would be surprising.
- **Launchd won't start the receiver:** Plist syntax error or wrong binary
  path. Check `launchctl list | grep termsurf` and
  `launchctl print gui/com.termsurf.two-profiles`.
- **Profile server can't connect:** The Mach service name isn't registered yet.
  Add retry with exponential backoff (100ms initial, 10 attempts) to
  `ConnectToService()`.
