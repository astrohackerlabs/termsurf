# Issue 624: Chromium IPC

## Goal

Understand how Chromium's processes communicate internally — what processes
exist, what IPC mechanisms they use, and specifically how input reaches the
renderer and how rendered frames reach the display. This knowledge will inform
how to replace TermSurf's current XPC message-passing with something faster.

## Background

### The latency problem

TermSurf runs Chromium out-of-process. The GUI (Ghostty fork) communicates with
a Chromium Profile Server over XPC. This works, but every interaction has
visible lag:

```
Mouse event → Zig Surface → XPC to Chromium → Chromium processes input →
renderer paints → compositor composites → capturer captures (timer) →
IOSurface → XPC to GUI → next CVDisplayLink vsync → Metal composites
```

[Issue 619](619-input-latency.md) measured this at 15–25ms average, 1–2 frames
of extra latency versus native Chrome. Three sources: the FrameSinkVideoCapturer
running on its own timer (0–8ms), async XPC dispatch (1–3ms each direction), and
a double-vsync penalty.

### What we tried and abandoned

[Issues 620](620-zig-content-shell.md)–[623](623-viz-display-serialization.md)
spent 25 experiments across four issues trying to run multiple browser profiles
in a single Chromium process. If multiple `BrowserContext`s could coexist at
60fps, there would be no IPC at all — the GUI would host Chromium in-process.

The attempt failed. Two BrowserContexts with JavaScript animations degrade to
2fps. [Issue 621](621-single-process.md) isolated the trigger to JavaScript on
the Blink main thread (CSS animations are immune).
[Issue 622](622-javascript-is-slow.md) proved both conditions are required —
multiple BrowserContexts AND JavaScript.
[Issue 623](623-viz-display-serialization.md) debunked the leading theory (Viz
Display serialization). After 25 experiments, the root cause remains unknown.

### The new direction

Rather than continue debugging the single-process 2fps mystery, we're pursuing
the multi-process architecture that TermSurf already uses — but making it
faster. The key insight from Issue 619's research: **Chrome itself is
multi-process, yet achieves 1-frame latency.** Chrome's browser process,
renderer processes, and GPU/Viz process are all separate — the same kind of
cross-process architecture TermSurf has. Chrome stays fast because its
performance-critical paths use shared memory, not message passing.

Issue 619 identified that Chromium uses shared memory ring buffers for GPU
commands and shared GPU textures (IOSurface) for frame data. Mojo on macOS uses
Mach ports — the same kernel mechanism as XPC. The transport is not the
bottleneck. What matters is what travels over it.

Before we can adopt these patterns, we need to deeply understand how they
actually work in Chromium's codebase.

### What we already know (from Issue 619)

Issue 619's research established:

- **GPU Command Buffer** — renderers write GL-equivalent commands into a shared
  memory ring buffer (`gpu/command_buffer/client/cmd_buffer_helper.h`). Hundreds
  of commands batch before a single IPC notification.
- **CompositorFrames are metadata, not pixels** — a `CompositorFrame` contains
  texture references and draw quads. Zero pixel data crosses the boundary.
- **Mojo uses Mach ports on macOS** — `MOJO_USE_APPLE_CHANNEL` buildflag,
  `channel_mac.cc` implements transport via `mach_msg`.
- **Compositor-thread input handling** — `cc/input/InputHandler` handles scroll
  on the compositor thread without touching the main thread.
- **CALayerParams** — Chrome's normal display path uses `ca_context_id` for
  zero-copy GPU compositing, or `io_surface_mach_port` as a fallback.

But this was a high-level survey. We need to trace the actual code paths.

## Research questions

### 1. What processes exist when viewing a web page?

We know the broad categories (browser, renderer, GPU/Viz) but need the precise
picture:

- Exactly how many processes does Content Shell spawn for one tab? For two tabs?
- Which process is the "browser process" — is it the one that calls
  `ContentMain()`, or does Chromium spawn a separate one?
- Where does the GPU/Viz process get created? Is it always a separate process,
  or can it run in-process?
- Are there other processes (utility, network, audio) relevant to rendering?

### 2. How do they communicate?

The IPC landscape in Chromium is layered and confusing. We need to understand
the stack:

- **Mojo** — Chromium's primary IPC framework. What exactly is it? Message
  pipes, data pipes, shared buffers — how do these map to OS primitives?
- **Legacy IPC** — does any of it remain, or is everything Mojo now?
- **Shared memory** — how does Chromium create and share memory regions across
  processes? What API (`base::SharedMemory`, `base::WritableSharedMemoryRegion`,
  platform-specific)?
- **Mach ports** — how are they used beyond Mojo channels? IOSurface transfer,
  task ports, etc.

### 3. What IPC protocols exist?

- What Mojo interfaces carry rendering-critical messages?
- What is the `viz.mojom.CompositorFrameSink` interface?
- What is the `viz.mojom.DisplayClient` / `viz.mojom.DisplayPrivate` interface?
- What carries input events from browser to renderer?

### 4. Where is shared memory used?

The GPU Command Buffer uses shared memory. What else does?

- **Bitmaps / raster buffers** — are software-rasterized tiles shared via shared
  memory?
- **Input events** — are they sent as Mojo messages or through shared memory?
- **Frame metadata** — is the CompositorFrame itself in shared memory, or
  serialized over a Mojo message pipe?
- **Sync tokens / fences** — are these in shared memory or IPC messages?

### 5. How does user input reach the renderer?

Trace the complete path for a mouse click:

- Where does the browser process receive the OS event?
- How does it decide which renderer gets it?
- What Mojo interface carries the event?
- Does the event go directly to the renderer, or through the GPU/Viz process?
- How does the compositor thread receive it for scroll/selection?
- What is the latency of this path?

### 6. How does the rendered frame reach the display?

Trace the complete path for a rendered pixel:

- Renderer rasterizes into... what? GPU textures? Shared memory bitmaps?
- The CompositorFrame is submitted to... where? The GPU process? The browser
  process?
- How does the GPU/Viz process aggregate frames from multiple renderers?
- How does the final composited result reach the screen on macOS?
- What is `CALayerParams`? Where is it produced and consumed?
- What is a `ca_context_id`? How does `CALayerHost` work?

## Approach

Source code research only — no code changes, no builds. Read the Chromium source
in `chromium/src/` to trace the actual code paths. The goal is a detailed map of
the IPC architecture that we can use to design TermSurf's replacement for XPC
message-passing.

## Experiments

### Experiment 1: Map Chromium's IPC architecture

A source code research experiment — no code changes, no builds. Read the
Chromium source in `chromium/src/` to answer all six research questions. The
goal is a concrete, code-referenced map of every process, IPC mechanism, and
data path involved in rendering a web page.

#### Q1: What processes exist?

Trace how Content Shell spawns its process tree.

**Where to look:**

- `content/browser/browser_main_loop.cc` — browser process initialization. What
  child processes does it launch?
- `content/browser/gpu/gpu_process_host.cc` — GPU/Viz process launch. Is it
  always out-of-process? What flags control in-process GPU?
- `content/browser/renderer_host/render_process_host_impl.cc` — renderer process
  creation. How does `GetProcessHostForSiteInstance()` decide whether to create
  a new process or reuse one?
- `content/browser/utility_process_host.cc` — utility processes. Is the network
  service a utility process?
- `content/public/common/content_switches.h` — flags like `--single-process`,
  `--in-process-gpu`, `--no-sandbox`. What do they control?

**Deliverable:** A process tree diagram showing exactly what processes exist for
a Content Shell instance with one tab loading a page with JavaScript.

#### Q2: How do they communicate?

Map the IPC stack from OS primitives up to application-level interfaces.

**Where to look:**

- `mojo/public/cpp/system/` — Mojo primitives. What are message pipes, data
  pipes, shared buffers, and platform handles? How do they map to kernel
  objects?
- `mojo/core/` — Mojo core implementation. How does a Mojo message pipe become
  an actual OS-level transport?
- `mojo/public/cpp/platform/platform_channel.cc` — how channels are created.
  What OS primitive is used on macOS?
- `mojo/core/channel_mac.cc` — macOS channel implementation. How does it use
  `mach_msg`? How are Mach ports bootstrapped between processes?
- `ipc/ipc_channel_mojo.cc` — the legacy IPC layer on top of Mojo. Is this still
  used for anything rendering-critical?
- `content/browser/child_process_launcher.cc` — how the browser process creates
  a child and establishes the initial Mojo connection.

**Deliverable:** A layered diagram: OS primitives (Mach ports, shared memory) →
Mojo transport → Mojo interfaces → application-level calls.

#### Q3: What Mojo interfaces carry rendering traffic?

Identify the specific `.mojom` interfaces on the rendering-critical path.

**Where to look:**

- `services/viz/public/mojom/compositing/compositor_frame_sink.mojom` — the
  interface between renderer and Viz. What methods does it have? How are
  CompositorFrames submitted?
- `third_party/blink/public/mojom/widget/platform_widget.mojom` — or whatever
  carries input events from browser to renderer.
- `content/common/renderer.mojom` — renderer-side Mojo interface. What
  rendering-relevant methods exist?
- `services/viz/privileged/mojom/compositing/` — privileged Viz interfaces used
  by the browser process.
- `content/browser/renderer_host/input/input_router_impl.cc` — how input events
  are routed. What Mojo interface do they travel on?

**Deliverable:** A list of the Mojo interfaces on the hot path for input and
frame submission, with their method signatures.

#### Q4: Where is shared memory used?

Find every place shared memory is used in the rendering pipeline.

**Where to look:**

- `gpu/command_buffer/common/cmd_buffer_common.h` — the GPU command buffer ring.
  How is the shared memory region created and mapped?
- `gpu/command_buffer/client/cmd_buffer_helper.h` — client-side command buffer.
  How does the renderer write commands without IPC per call?
- `gpu/command_buffer/service/command_buffer_service.cc` — GPU-side command
  buffer. How does the GPU process consume commands?
- `base/memory/shared_memory_region.h` — Chromium's shared memory abstraction.
  How are regions created, duplicated across processes, and mapped?
- `base/memory/platform_shared_memory_region.h` — platform-specific
  implementation. What macOS API does it use? (`mach_vm_allocate`? `shm_open`?
  `mmap`?)
- `components/viz/common/resources/transferable_resource.h` — how GPU textures
  are referenced across processes. Are they shared memory or GPU handles?
- `gpu/ipc/common/gpu_memory_buffer_impl_io_surface.cc` — IOSurface as shared
  GPU memory. How is this created and shared?

**Deliverable:** A catalog of shared memory uses in the rendering pipeline: what
data lives in shared memory, how regions are created, and how they're shared
between processes.

#### Q5: How does user input reach the renderer?

Trace a mouse click from the OS event to the renderer's compositor thread.

**Where to look:**

- `content/browser/renderer_host/render_widget_host_view_mac.mm` — where macOS
  delivers NSEvents. How does `mouseDown:` get processed?
- `content/browser/renderer_host/render_widget_host_input_event_router.cc` — how
  the browser process routes events to the correct renderer.
- `content/browser/renderer_host/input/input_router_impl.cc` — the input router.
  What Mojo interface sends events to the renderer?
- `content/renderer/input/widget_input_handler_impl.cc` — renderer-side input
  handling. How does the event reach the compositor thread?
- `cc/input/input_handler.cc` — compositor-thread input handling. How does
  scroll get handled without the main thread?
- `third_party/blink/renderer/platform/widget/input/widget_input_handler_manager.cc`
  — how input is dispatched between compositor and main threads in the renderer.

**Deliverable:** A sequence diagram from `NSEvent` to compositor thread action,
with every process boundary and IPC hop labeled.

#### Q6: How does the rendered frame reach the display?

Trace a rendered pixel from rasterization to the screen on macOS.

**Where to look:**

- `cc/trees/layer_tree_host_impl.cc` — how the compositor produces a
  CompositorFrame. What does `SubmitCompositorFrame()` do?
- `services/viz/public/mojom/compositing/compositor_frame_sink.mojom` — the Mojo
  interface for frame submission. Is the CompositorFrame serialized or
  referenced?
- `components/viz/service/display/display.cc` — how the Display aggregates
  frames and draws. What is the output?
- `components/viz/service/display_embedder/output_surface_provider_impl.cc` —
  how the output surface is created on macOS.
- `ui/accelerated_widget_mac/accelerated_widget_mac.mm` — how `CALayerParams`
  are produced and delivered.
- `ui/accelerated_widget_mac/display_ca_layer_tree.mm` — how `CALayerHost` is
  created from a `ca_context_id`.
- `ui/gfx/ca_layer_params.h` — the struct that carries the display result. What
  fields does it have?
- `gpu/ipc/service/gpu_memory_buffer_factory_io_surface.cc` — how IOSurface
  buffers are created in the GPU process.

**Deliverable:** A sequence diagram from `SubmitCompositorFrame()` to pixels on
screen, with every process boundary, GPU operation, and macOS Window Server
interaction labeled.

#### Verification

Research is complete when we can draw two end-to-end diagrams:

1. **Input path:** OS event → browser process → renderer process → compositor
   thread, with every IPC mechanism (Mojo message pipe, shared memory, Mach
   port) labeled at each hop.
2. **Frame path:** Renderer rasterization → CompositorFrame submission → Viz
   aggregation → display output → macOS screen, with every IPC mechanism and GPU
   memory sharing technique labeled.

Both diagrams should reference specific source files and line numbers. The
diagrams should make it clear which steps use message passing (and could be
replaced with shared memory) and which already use shared memory or zero-copy
GPU textures.

#### Results

##### A1: Process tree

The process that calls `ContentMain()` IS the browser process. It spawns all
children.

```
Content Shell (Browser Process)
├── GPU/Viz Process (out-of-process by default)
│   └── All GPU calls, display compositing, rasterization
├── Renderer Process (one per site/BrowserContext)
│   ├── Main thread (Blink, JavaScript, DOM)
│   ├── Compositor thread (layer tree, animations, scroll)
│   └── Worker threads (WebWorkers)
├── Network Service (utility process)
└── Storage Service (utility process)
```

**Process creation:**

| Process  | Created by                           | File                                                        | Line |
| -------- | ------------------------------------ | ----------------------------------------------------------- | ---- |
| GPU/Viz  | `GpuProcessHost::LaunchGpuProcess()` | `content/browser/gpu/gpu_process_host.cc`                   | 1261 |
| Renderer | `RenderProcessHostImpl::Init()`      | `content/browser/renderer_host/render_process_host_impl.cc` | 1780 |
| Utility  | `UtilityProcessHost::StartProcess()` | `content/browser/service_host/utility_process_host.cc`      | 310  |

**Flags:**

- `--single-process` — runs browser, renderer, and GPU in one process
- `--in-process-gpu` — runs GPU in-process with browser, renderer stays separate
- `--no-sandbox` — disables sandboxing for child processes

For Content Shell with one tab: browser process + GPU process + 1 renderer
process + network service = **4 processes minimum**.

##### A2: IPC stack on macOS

Four layers from kernel to application:

```
Application Layer
  OutgoingInvitation / IncomingInvitation (process bootstrap)
  AttachMessagePipe() / ExtractMessagePipe() (named pipe exchange)
      │
Mojo System Layer
  ScopedMessagePipeHandle (two-ended pipe)
  MojoWriteMessage() / MojoReadMessage()
  DataPipe (streaming), SharedBuffer (shared memory)
      │
Mojo Core / Transport Layer
  ChannelMac (channel_mac.cc)
  mach_msg_header_t construction
  mach_msg_port_descriptor_t (Mach port transfer in messages)
  Handshake protocol (kChannelMacHandshakeMsgId = 'mjhs')
      │
macOS Kernel Primitives
  mach_msg() system call (MACH_SEND_MSG / MACH_RCV_MSG)
  mach_port_t (send rights, receive rights)
  vm_allocate() for message buffers
```

**Bootstrap sequence:**

1. Parent creates `PlatformChannel` — creates Mach port pair (send + receive)
2. Parent creates `OutgoingInvitation`, attaches named message pipes
3. Parent launches child via `ChildProcessLauncher`, passes remote endpoint on
   command line
4. Child calls `IncomingInvitation::Accept()` with the endpoint
5. Child extracts named pipes — direct IPC connection established

**Key files:**

| Component        | File                                          | What it does                         |
| ---------------- | --------------------------------------------- | ------------------------------------ |
| macOS channel    | `mojo/core/channel_mac.cc`                    | `mach_msg()` send/receive, handshake |
| Platform handle  | `mojo/public/cpp/platform/platform_handle.h`  | Wraps `mach_port_t` send/receive     |
| Platform channel | `mojo/public/cpp/platform/platform_channel.h` | Creates entangled endpoint pair      |
| Message pipes    | `mojo/public/cpp/system/message_pipe.h`       | Two-ended pipe abstraction           |
| Invitations      | `mojo/public/cpp/system/invitation.h`         | Process bootstrap (attach/extract)   |
| Child launcher   | `content/browser/child_process_launcher.cc`   | Spawns child, sends Mojo invitation  |

##### A3: Mojo interfaces on the rendering hot path

**Frame submission (renderer → Viz):**

`viz.mojom.CompositorFrameSink` — `compositor_frame_sink.mojom:61-116`

- `SubmitCompositorFrame(LocalSurfaceId, CompositorFrame, ...)` — primary hot
  path. CompositorFrame contains metadata + resource references (mailboxes), not
  pixels. Marked `[UnlimitedSize]`.
- `SetNeedsBeginFrame(bool)` — signal need for frames
- `DidNotProduceFrame(BeginFrameAck)` — acknowledge without producing

`viz.mojom.CompositorFrameSinkClient` — `compositor_frame_sink.mojom:119-153`

- `OnBeginFrame(BeginFrameArgs, ...)` — vsync signal from Viz to renderer
- `DidReceiveCompositorFrameAck(array<ReturnedResource>)` — backpressure ack
- `ReclaimResources(array<ReturnedResource>)` — resource lifecycle

**Input dispatch (browser → renderer):**

`blink.mojom.WidgetInputHandler` — `input_handler.mojom:464-571`

- `DispatchEvent(Event, Event?) => (...)` — primary input hot path. Blocking
  call with response callback. Carries full event structure (mouse, key, touch,
  gesture).
- `DispatchNonBlockingEvent(Event)` — one-way, no callback

`blink.mojom.WidgetInputHandlerHost` — `input_handler.mojom:246-301`

- `DidOverscroll(DidOverscrollParams)` — overscroll feedback
- `SetMouseCapture(bool)` — mouse capture state

**Widget lifecycle (browser ↔ renderer):**

`blink.mojom.WidgetHost` — `platform_widget.mojom:30-89`

- `CreateFrameSink(...)` — creates the CompositorFrameSink and input channels

`blink.mojom.Widget` — `platform_widget.mojom:93-133`

- `UpdateVisualProperties(VisualProperties)` — size, scale, surface ID

##### A4: Shared memory catalog

| What                    | How created                          | Transfer method               | Access pattern                               |
| ----------------------- | ------------------------------------ | ----------------------------- | -------------------------------------------- |
| GPU command ring buffer | `CreateTransferBuffer()`             | Shared memory mapping         | Renderer writes put ptr, GPU reads get ptr   |
| GPU transfer buffers    | `CreateTransferBuffer()`             | Shared memory mapping         | Renderer writes data, GPU reads; IPC for ID  |
| IOSurface (macOS)       | Native IOSurface API                 | Mach port via Mojo            | GPU renders, Mach port enables cross-process |
| SharedImage / Mailbox   | `gpu::Mailbox` registry              | Mailbox ID in CompositorFrame | GPU resolves mailbox → texture handle        |
| CPU staging buffers     | `UnsafeSharedMemoryRegion::Create()` | Shared region handle via Mojo | Renderer CPU writes, GPU copies to VRAM      |
| Foreground time         | `ReadOnlySharedMemoryRegion`         | Shared region via Mojo (once) | Atomic TimeTicks, no per-update IPC          |

**GPU command buffer** — the core shared memory hot path:

- `cmd_buffer_helper.cc:83-104` — `AllocateRingBuffer()` creates shared memory
  via `CreateTransferBuffer()`
- `cmd_buffer_common.h:43-98` — `CommandBufferEntry` is a 4-byte union (header,
  uint32, int32, float)
- Renderer writes hundreds of GPU commands into the ring. A single lightweight
  IPC notification tells the GPU process to consume them. No per-command kernel
  transition.
- Synchronization: put/get pointer offsets. `InsertToken()` for sync points.
  `WaitForAvailableEntries()` for backpressure.

**IOSurface on macOS:**

- `iosurface_image_backing.h:131-150` — `IOSurfaceImageBacking` wraps native
  IOSurface
- Cross-process: `IOSurfaceCreateMachPort()` → Mach port via Mojo →
  `IOSurfaceLookupFromMachPort()` in receiver
- Zero-copy: GPU renders directly to IOSurface, receiver maps same GPU memory

**Platform shared memory on macOS:**

- `platform_shared_memory_region.h:109-128` — `CreateWritable()`,
  `CreateUnsafe()`
- Uses `mach_vm_allocate` on macOS (referenced via error enum at line 97)

**Key insight: input events do NOT use shared memory.** They are serialized in
Mojo messages. Only GPU commands and textures use shared memory.

##### A5: Input path — mouse click

```
macOS kernel delivers NSEvent
  │
  ▼ [Browser Process, Main Thread]
RenderWidgetHostViewCocoa::mouseEvent()
  render_widget_host_view_cocoa.mm:975
  │
  ├─ WebMouseEventBuilder::Build(theEvent) → WebMouseEvent
  │    line 1079
  │
  └─ _hostHelper->RouteOrProcessMouseEvent(event)
       line 1118
       │
       ▼
  RenderWidgetHostViewMac::RouteOrProcessMouseEvent()
    render_widget_host_view_mac.mm:1884-1895
       │
       └─ GetInputEventRouter()->RouteMouseEvent()
            line 1890
            │
            ▼
  InputRouterImpl::SendMouseEvent()
    input_router_impl.cc:107
       │
       └─ FilterAndSendWebInputEvent()
            line 622
            │
            └─ client_->GetWidgetInputHandler()->DispatchEvent()
                 line 696
                 │
  ═══════════════╪══════════════════════════════════════════
  MOJO IPC       │  blink.mojom.WidgetInputHandler::DispatchEvent()
  (message pipe) │  input_handler.mojom:526-531
  ═══════════════╪══════════════════════════════════════════
                 │
                 ▼ [Renderer Process, Mojo/IO Thread]
  WidgetInputHandlerImpl::DispatchEvent()
    widget_input_handler_impl.cc:178
       │
       └─ input_handler_manager_->DispatchEvent()
            line 188
            │
            ▼ [Renderer Process, Compositor Thread]
  InputHandlerProxy::RouteToTypeSpecificHandler()
    input_handler_proxy.cc:873-985
       │
       ├─ kMouseDown  → DID_NOT_HANDLE (line 943)
       ├─ kMouseUp    → DID_NOT_HANDLE (line 952)
       ├─ kMouseMove  → DID_NOT_HANDLE (line 968)
       └─ kMouseLeave → DID_NOT_HANDLE (line 973)
            │
            ▼ [Renderer Process, Main Thread]
  MainThreadEventQueue → Blink EventHandler → JavaScript
```

**Key findings:**

- **Single IPC hop** — browser → renderer via Mojo message pipe
- **No shared memory** for input — event data serialized in Mojo message
- **Mouse events skip the compositor** — always forwarded to main thread
  (`DID_NOT_HANDLE`). Only scroll/pinch/gesture events get compositor handling.
- **Latency:** ~2-20ms (Mojo IPC ~0.5-2ms, rest is thread scheduling)

##### A6: Frame path — rasterization to screen

```
[Renderer Process, Compositor Thread]
LayerTreeHostImpl produces CompositorFrame
  cc/trees/layer_tree_frame_sink.h:125-126
     │
     ├─ CompositorFrame contains:
     │    metadata + TransferableResource[] (mailbox refs) + render passes
     │    NO pixel data — only GPU texture references
     │
═════╪═══════════════════════════════════════════════════
MOJO │  viz.mojom.CompositorFrameSink::SubmitCompositorFrame()
IPC  │  compositor_frame_sink.mojom:88-92
═════╪═══════════════════════════════════════════════════
     │
     ▼ [GPU/Viz Process, Viz Thread]
CompositorFrameSinkImpl::SubmitCompositorFrame()
  compositor_frame_sink_impl.cc:156-180
     │
     └─ Surface stores frame
          │
          ▼
SurfaceAggregator::Aggregate()
  surface_aggregator.cc
  Merges frames from all surfaces, resolves Mailbox → GPU texture
     │
     ▼ [GPU/Viz Process, GPU Thread]
SkiaRenderer::DrawFrame()
  skia_renderer.cc
  Renders aggregated quads into output IOSurface (Metal/GL)
     │
     ▼
ImageTransportSurfaceOverlayMacEGL::Present()
  image_transport_surface_overlay_mac.h:61-63
     │
     └─ CALayerTreeCoordinator::CommitPresentedFrameToCA()
          ca_layer_tree_coordinator.mm:171-250
          │
          ├─ Creates CAContext (ca_layer_tree_coordinator.mm:54-57)
          │    CAContext contextWithCGSConnection:CGSMainConnectionID()
          │
          └─ Populates CALayerParams (lines 206-221):
               ├─ ca_context_id (uint32) — OR —
               ├─ io_surface_mach_port (Mach port)
               ├─ pixel_size
               └─ scale_factor
                    │
═══════════════════════╪═════════════════════════════════
MOJO IPC               │  CALayerParams (tiny struct)
(ca_context_id: uint32 │  ca_layer_params_mojom_traits.cc
or IOSurface Mach port)│
═══════════════════════╪═════════════════════════════════
                       │
                       ▼ [Browser Process, UI Thread]
AcceleratedWidgetMac::UpdateCALayerTree()
  accelerated_widget_mac.mm:82-93
     │
     └─ DisplayCALayerTree::UpdateCALayerTree()
          display_ca_layer_tree.mm:66-121
          │
          ├─ PATH A: Remote CAContext (preferred, line 84-86)
          │    GotCALayerFrame(ca_context_id)
          │    display_ca_layer_tree.mm:123-153
          │    Creates CALayerHost with contextId
          │    Window Server composites GPU process's CAContext directly
          │    ZERO COPY — GPU VRAM → screen
          │
          └─ PATH B: IOSurface direct (fallback, line 91-98)
               IOSurfaceLookupFromMachPort(mach_port)
               GotIOSurfaceFrame(io_surface, dip_size, scale_factor)
               display_ca_layer_tree.mm:155-188
               CALayer.contents = (__bridge id)io_surface
               Window Server reads IOSurface GPU memory
               ZERO COPY — GPU VRAM → screen
     │
     ▼ [macOS Window Server]
CATransaction flush → GPU composites → display scanout
```

**Key findings:**

- **No pixel data crosses any process boundary.** CompositorFrames carry mailbox
  references. CALayerParams carry a uint32 context ID or a Mach port. Everything
  is GPU memory references.
- **Two display paths on macOS:**
  - **Remote CAContext** (preferred): GPU process creates CAContext, sends
    `contextId` (uint32). Browser creates `CALayerHost`. Window Server
    composites GPU process's layers directly from VRAM.
  - **IOSurface direct** (fallback): GPU process creates IOSurface, sends Mach
    port. Browser creates CALayer with IOSurface as contents.
- **Both paths are zero-copy.** The Window Server reads directly from GPU VRAM.
- **Two IPC hops:** Renderer → GPU/Viz (CompositorFrame via Mojo), GPU/Viz →
  Browser (CALayerParams via Mojo). Neither carries pixel data.

#### Conclusion

The full IPC architecture is now mapped. The critical insight for TermSurf:

**Chrome's input path uses Mojo message passing, not shared memory.** Input
events are serialized in Mojo messages with a single hop from browser to
renderer. There is no shared memory ring buffer for input. Chrome achieves low
input latency not through shared memory but through a short path: one Mojo
message, compositor thread receives it, and for scroll/gesture events the
compositor handles it directly without touching the main thread. Mouse events
always go to the main thread.

**Chrome's frame path uses zero-copy GPU memory references.** No pixel data
crosses any process boundary. The renderer sends mailbox IDs, the GPU process
resolves them and renders into IOSurface, and the browser receives either a
`ca_context_id` (4 bytes) or an IOSurface Mach port. The Window Server reads
directly from GPU VRAM.

**What TermSurf currently does differently:**

1. **Input:** XPC message per event (similar to Chrome's Mojo message, but with
   an extra process hop: GUI → Chromium server → Chromium's internal browser →
   renderer). Chrome's browser process IS the one receiving OS events.
2. **Frames:** FrameSinkVideoCapturer with GPU readback + IOSurface Mach port
   per frame via XPC. Chrome uses `ca_context_id` (sent once) or IOSurface Mach
   port (sent per frame but from the normal display path, no capturer).

**The architectural gap is not shared memory vs message passing for input.** It
is:

1. **Extra process hop for input** — TermSurf has GUI → Chromium server →
   internal renderer. Chrome has browser → renderer (one hop).
2. **Capturer vs display path for frames** — TermSurf uses a recording API.
   Chrome uses the native display compositor output (`CALayerParams`).
3. **Per-frame Mach port transfer** — TermSurf sends a new IOSurface Mach port
   every frame. Chrome sends `ca_context_id` once and the Window Server handles
   the rest.
