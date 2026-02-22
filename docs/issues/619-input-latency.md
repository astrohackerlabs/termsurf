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
| Input → XPC to Chromium     | ~1–3ms     | Async dispatch queue scheduling         |
| Chromium processes input    | ~2–5ms     | Layout, paint, composite                |
| Wait for next capture cycle | **0–8ms**  | Capturer on 120fps timer, not on-demand |
| Captured frame → XPC to GUI | ~1–3ms     | Another async dispatch queue hop        |
| Wait for next vsync         | **0–16ms** | CVDisplayLink tick                      |

Worst case: ~35ms. Average: ~15–25ms. That's 1–2 frames of extra latency versus
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
boundary to the GPU/Viz process. Yet Chrome still achieves ~1–2 frame latency.

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
could shave ~5–10ms off the average round-trip.

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
