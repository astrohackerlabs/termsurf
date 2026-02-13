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

## Key challenge: IOSurface output from Content API

CEF provided IOSurface output directly via `on_accelerated_paint` with
`shared_texture_enabled`. The Content API has no equivalent callback. We need to
find a way to capture the composited output as an IOSurface.

### Approaches (in order of complexity)

**1. Offscreen `NSWindow` + `CALayer` IOSurface capture**

Each profile server creates a real `NSWindow` (positioned off-screen or hidden).
The Content API compositor renders normally to the window's view, which is
backed by a `CALayer` whose backing store is an IOSurface. We access this
IOSurface directly from the layer and create a Mach port.

- Pros: No Chromium modifications. Uses the normal windowed rendering path that
  we already know works at 60fps.
- Cons: Requires accessing the `CALayer` backing IOSurface, which uses private
  CoreAnimation APIs. May require the window to be on-screen for the compositor
  to produce frames.
- Risk: Hidden windows may not receive compositor frames (the visibility issue
  from earlier experiments).

**2. `CopyFromSurface()` to shared IOSurface**

Use `WebContents::CopyFromSurface()` to asynchronously copy each composited
frame to an IOSurface we own. This goes through Chromium's
`viz::CopyOutputRequest` pipeline.

- Pros: Public Content API. Well-documented.
- Cons: Involves a GPU-to-GPU copy (not zero-copy). May add latency. Need to
  call it every frame at 60Hz.
- Risk: `CopyFromSurface()` may be designed for occasional screenshots, not
  continuous 60fps capture. Latency may accumulate.

**3. Custom `viz::OutputSurface` that writes to a shared IOSurface**

Replace the compositor's output surface with a custom implementation that
renders directly to an IOSurface we control. This is the zero-copy approach —
the compositor writes to our IOSurface, we send the Mach port, done.

- Pros: True zero-copy. Highest possible performance.
- Cons: Deep Chromium modification. Requires understanding the viz compositor
  pipeline. Fragile across Chromium versions.
- Risk: Significant engineering effort. May require forking compositor code.

**4. `CAContext` / `CALayerHost` cross-process layer hosting**

macOS has a native mechanism for cross-process layer compositing. The profile
server creates a `CAContext` containing the WebContents view's layer tree. The
GUI creates a `CALayerHost` with the remote context ID. WindowServer composites
the remote layers into the GUI's window automatically.

- Pros: Zero-copy. No frame capture needed — macOS handles the compositing. This
  is how Chromium's own GPU process works internally.
- Cons: Uses private Apple APIs (`CAContext`, `CALayerHost`). Compositing is
  handled by WindowServer, not us — less control.
- Risk: Private APIs may change. Behavior with hidden windows unknown.

### Recommended approach

Start with **Approach 1** (off-screen window + CALayer IOSurface capture). It
requires the least Chromium modification and builds directly on the One Profile
app. If the hidden window doesn't receive compositor frames, keep the window
visible but off-screen (positioned at e.g. -10000, -10000). If CALayer IOSurface
access proves impractical, fall back to **Approach 2** (`CopyFromSurface`).

**Approach 4** (CAContext/CALayerHost) is the most elegant long-term solution
but requires investigation of the private APIs and their interaction with
Chromium's compositor. Worth exploring in a later experiment.

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

## Ideas for Experiments

### Idea 1: IOSurface capture from Content API

**Goal:** Prove we can extract the composited output of a Content API
`WebContents` as an IOSurface at 60fps.

Modify the One Profile app to:

1. Create a `WebContents` and navigate to the test page (as today)
2. On every composited frame, capture the output as an IOSurface
3. Log the IOSurface dimensions and frame rate

No XPC yet — this experiment is purely about capture. The question is: can we
get an IOSurface from the Content API compositor at 60fps?

Start with the CALayer backing store approach. If that doesn't work, try
`CopyFromSurface()`.

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

### Experiment 1: Intercept the compositor's IOSurface

#### Hypothesis

Chromium's macOS compositor already produces IOSurfaces as part of its normal
rendering pipeline. We don't need `CopyFromSurface()` or any capture mechanism —
we just need to intercept the existing IOSurface at the right point. If we can
grab it at 60fps, the entire frame delivery pipeline (IOSurface → Mach port →
XPC → GUI) is a solved problem.

#### Background: how Chromium presents frames on macOS

Research into Chromium's macOS rendering pipeline reveals a clean architecture:

```
GPU process (or GPU thread if in-process)
  ↓ Compositor produces frames
CALayerTreeCoordinator
  ↓ GetContentIOSurface() → raw IOSurface
  ↓ IOSurfaceCreateMachPort() → Mach port
  ↓ Packages into CALayerParams { ca_context_id OR io_surface_mach_port }
Browser process
  ↓ DisplayCALayerTree::UpdateCALayerTree(ca_layer_params)
  ↓ IOSurfaceLookupFromMachPort() → IOSurface
  ↓ Sets CALayer.contents = io_surface
CoreAnimation
  ↓ Presents to screen
```

Two rendering modes exist:

1. **Remote layers (`ca_context_id`):** The GPU process creates a `CAContext`
   with a layer tree. The browser process creates a `CALayerHost` with the
   context ID. WindowServer composites remotely. Used when GPU is in-process.

2. **IOSurface Mach port:** The GPU process extracts the IOSurface from the
   layer tree via `GetContentIOSurface()`, creates a Mach port via
   `IOSurfaceCreateMachPort()`, and sends it in `CALayerParams`. The browser
   process imports it via `IOSurfaceLookupFromMachPort()`. Used when GPU is
   out-of-process.

The key files:

- `ui/accelerated_widget_mac/ca_layer_tree_coordinator.{h,mm}` — GPU-side.
  Manages the layer tree, calls `GetContentIOSurface()`, creates Mach ports.
- `ui/accelerated_widget_mac/ca_renderer_layer_tree.{h,mm}` — Builds the
  hierarchy of CALayers from compositor output. Each `ContentLayer` holds an
  `io_surface_` member.
- `ui/accelerated_widget_mac/display_ca_layer_tree.{h,mm}` — Browser-side.
  Receives `CALayerParams`, imports IOSurfaces, assigns to CALayers.
- `ui/gfx/ca_layer_params.h` — The IPC structure: `ca_context_id` (uint32) or
  `io_surface_mach_port` (scoped Mach port), plus `pixel_size` and
  `scale_factor`.

The critical insight: **Chromium already creates IOSurface Mach ports.** We
don't need to invent a capture mechanism. We need to intercept what Chromium
already produces.

#### Design

Modify the One Profile app to intercept frames at the
`DisplayCALayerTree::UpdateCALayerTree()` call site and log what arrives.

**Step 1: Determine which rendering path content_shell uses.**

Add logging to `DisplayCALayerTree::UpdateCALayerTree()` (or its equivalent in
the One Profile app's rendering path) to check which `CALayerParams` field is
populated:

- If `ca_layer_params.ca_context_id != 0` → remote layer path (in-process GPU)
- If `ca_layer_params.io_surface_mach_port` → IOSurface Mach port path

This tells us which intercept strategy to use.

**Step 2: Grab the IOSurface.**

- **If IOSurface Mach port path:** Call
  `IOSurfaceLookupFromMachPort(ca_layer_params.io_surface_mach_port.get())` to
  get the IOSurface. Log its dimensions (`IOSurfaceGetWidth`,
  `IOSurfaceGetHeight`). Increment a frame counter.

- **If remote layer path:** We need to either:
  - (a) Force the IOSurface Mach port path by disabling remote layers (e.g.,
    `--disable-gpu-compositing` or similar flag), or
  - (b) Intercept earlier at `CALayerTreeCoordinator` where
    `GetContentIOSurface()` is called before the CA context ID path is chosen,
    or
  - (c) Intercept at the `CARendererLayerTree::ContentLayer` level where each
    layer holds an `io_surface_` member directly.

**Step 3: Measure the rate.**

Add a frame counter and a periodic log (once per second) that reports:
- Frames in the last second
- IOSurface dimensions
- Which rendering path was used

#### What we're modifying

The One Profile app in `content/one_profile/`. The exact file depends on which
intercept point works best:

- **Minimal change:** Add logging to `display_ca_layer_tree.mm` in the
  `ui/accelerated_widget_mac/` directory. This is Chromium code (not One Profile
  code), so the change is a Chromium fork modification.
- **Cleaner change:** Override the relevant method in the One Profile app's
  `ShellBrowserMainParts` or platform delegate, if the rendering path is
  hookable from the embedder level.

The experiment will reveal which approach is practical.

#### Expected result

60 frames per second with IOSurface dimensions matching the window size (e.g.,
1600×1200 physical pixels for an 800×600 logical window at 2x Retina). This
proves that we can intercept Chromium's compositor output at full framerate
without any additional capture mechanism.

#### What a failure would mean

- **0 frames (wrong path):** Content_shell uses the remote layer path and we
  can't easily intercept IOSurfaces. Fix: force the IOSurface path or intercept
  earlier in the pipeline.
- **< 60fps:** The intercept itself is adding overhead (unlikely for a simple
  log). Investigate.
- **IOSurface is null or wrong size:** The compositor isn't producing
  single-surface frames (multi-layer tree). Need to understand the layer
  structure and potentially composite multiple IOSurfaces ourselves.
