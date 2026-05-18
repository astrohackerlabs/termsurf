+++
status = "closed"
opened = "2026-01-24"
closed = "2026-01-24"
+++

# TermSurf 3.0 Webview Rendering

## Background

This document continues the work started in
[301-architecture.md](./301-architecture.md), which established the core
process model for TermSurf 3.0.

### What We Accomplished

TermSurf 3.0 uses a multi-process architecture for browser integration:

- **Profile servers**: Separate CEF processes, one per profile, providing true
  session isolation (different cookies, storage, login sessions)
- **Socket communication**: Unix domain sockets with JSON protocol for
  coordinator-to-profile-server and coordinator-to-GUI communication
- **Connection-based lifecycle**: Webview lifetime tied to coordinator
  connection, ensuring crash-proof cleanup

Through 8 experiments, we validated:

1. CEF initializes correctly with profile-specific cache paths
2. CEF enforces single-process-per-profile via `SingletonLock`
3. Socket communication works (ping/pong, open, get_status)
4. Multiple coordinators share one profile server
5. Webviews close automatically when coordinators disconnect
6. GUI socket server receives texture handle messages

**What failed**: Cross-process IOSurface sharing via global IDs.
`IOSurfaceLookup()` returns NULL for IOSurfaces created by CEF's GPU process.
The texture handle passes successfully between processes, but the receiving
process cannot access the actual texture data.

## Goal

**Display a webview texture rendered by the profile server in the GUI.**

The profile server (CEF) renders web content to a texture. The GUI (wezterm-gui)
must display that texture in a terminal pane. The challenge is sharing GPU
texture data between two separate processes on macOS.

### Requirements

1. Profile server renders webpage to a texture (already working)
2. Texture data becomes accessible to GUI process
3. GUI imports texture and renders it in the correct pane location
4. No visible latency or tearing during normal browsing

### Approaches to Explore

Since `IOSurfaceLookup()` failed, we need alternative approaches:

1. **Mach port-based IOSurface transfer**: Pass IOSurface references between
   processes using Mach ports instead of global IDs
2. **Shared memory with pixel copy**: Profile server copies pixel data to shared
   memory, GUI reads and uploads to GPU
3. **XPC services**: Use macOS XPC for structured cross-process communication
   with IOSurface support
4. **CALayerHost/CARemoteLayer**: Use Core Animation's built-in cross-process
   layer sharing

## Future Plans

Once texture display works, we will address (in rough order):

1. **Resize handling**: When the pane resizes, tell CEF to re-render at the new
   size (not just stretch the texture)
2. **Keyboard input**: Route keystrokes to CEF when webview pane is focused
3. **Mouse input**: Route clicks, scrolls, and hover events to CEF
4. **Keybindings**: Implement browse mode vs control mode (like ts1/ts2)
5. **Console output**: Stream `console.log` from browser to terminal
6. **Navigation controls**: Back, forward, reload, URL bar
7. **Multiple webviews**: Support multiple browser panes simultaneously
8. **Focus management**: Track which pane has focus, route input accordingly

These features are deferred until we solve the fundamental texture sharing
problem.

## Hypotheses

### Hypothesis 1: IOSurface Global ID Deprecation

Investigation into the cross-process IOSurface sharing failure revealed a
potential root cause: **the IOSurface is created by CEF's GPU subprocess, and
global ID lookup is deprecated**.

#### CEF's Multi-Process Architecture

CEF (Chromium Embedded Framework) uses multiple processes:

```
wezterm-gui (separate process tree - CANNOT lookup IOSurface by global ID)

termsurf-web (profile server / CEF browser process)
    └── WezTerm Helper (CEF GPU process - CREATES the IOSurface)
        └── IOSurface lives here
```

When `on_accelerated_paint` is called in the profile server, the IOSurface
handle is valid within that process tree (profile server + GPU subprocess).
However, wezterm-gui is a completely separate process with no relationship to
the CEF process hierarchy.

#### The kIOSurfaceIsGlobal Deprecation

Apple deprecated `kIOSurfaceIsGlobal` in macOS 10.11 (2015). This flag was
required for `IOSurfaceGetID`/`IOSurfaceLookup` to work across arbitrary
processes. Without it:

- IOSurfaces are not globally registered
- `IOSurfaceLookup(id)` only works within the same process hierarchy
- Apple considers globally accessible screen buffers a security hole

CEF/Chromium does not set this flag because:

1. Their internal processes use Mach port IPC, not global IDs
2. Global accessibility is a security concern
3. The flag has been deprecated for nearly a decade

#### Apple-Recommended Solutions

**Mach Port Transfer**: Apple's recommended approach for cross-process IOSurface
sharing:

```
Profile server:  IOSurfaceCreateMachPort(handle) → send port to GUI
GUI:             IOSurfaceLookupFromMachPort(port) → get valid handle
```

Challenge: Mach ports cannot be sent over regular Unix domain sockets. They
require XPC or direct Mach IPC (`mach_msg`).

**XPC Service**: Apple provides `IOSurfaceCreateXPCObject()` and
`IOSurfaceLookupFromXPCObject()` for this exact use case. Would require
restructuring the profile server as an XPC service.

**Shared Memory Fallback**: Profile server reads pixels from IOSurface, writes
to shared memory (`shm_open`), GUI reads and uploads to GPU. Loses zero-copy GPU
performance but guaranteed to work.

#### How Others Solve This

- **OBS Browser**: Runs CEF in the same process as the main application. No
  cross-process transfer needed.
- **Chromium internally**: Uses Mach ports between browser and GPU processes.
- **Syphon Framework**: Struggled with the same `kIOSurfaceIsGlobal` deprecation
  issue.

#### Sources

- [Chromium IOSurface Meeting Notes](https://www.chromium.org/developers/design-documents/iosurface-meeting-notes/)
- [Apple Developer Forums - kIOSurfaceIsGlobal deprecated](https://developer.apple.com/forums/thread/18958)
- [IOSurfaceCreateMachPort Example](https://fdiv.net/2011/01/27/example-iosurfacecreatemachport-and-iosurfacelookupfrommachport)
- [OBS Browser CEF Integration](https://github.com/obsproject/obs-browser/pull/252)
- [Syphon Framework - kIOSurfaceIsGlobal Deprecation](https://github.com/Syphon/Syphon-Framework/issues/47)

### Hypothesis 2: Process Tree Ancestry Enables IOSurface Lookup

**Status:** DISPROVEN (see Experiment 1)

An alternative explanation: `IOSurfaceLookup()` may work when the calling
process is an **ancestor** of the process that created the IOSurface.

#### Evidence from Chromium

Chromium's internal architecture uses the same `IOSurfaceGetID` /
`IOSurfaceLookup` pattern we attempted:

> The GPU process uses `IOSurfaceGetID()` to get a global identifier. The ID is
> sent to the browser process via IPC. The browser process uses
> `IOSurfaceLookup()` to get an IOSurfaceRef.

This works because the browser process **launches** the GPU process. The browser
is the parent/ancestor of the GPU process in the process tree.

#### How ts2 Works

In ts2, wezterm-gui runs CEF directly. The `on_accelerated_paint` callback
receives the IOSurface handle, and it's used **directly** without any
`IOSurfaceLookup()` call:

```rust
// ts2: Handle used directly in the same process
let shared_handle = SharedTextureHandle::new(info);  // info.shared_texture_io_surface
let src_texture = shared_handle.import_texture(&self.handler.device);
```

ts2 never needs `IOSurfaceLookup()` because everything runs in the same process.
But this confirms CEF's GPU subprocess (WezTerm Helper) creates IOSurfaces that
its parent process can access.

#### Current ts3 Process Tree (Broken)

```
wezterm-gui (CANNOT lookup - not an ancestor of GPU process)

coordinator (web CLI)
    └── termsurf-web (profile server)
        └── WezTerm Helper (CEF GPU - creates IOSurface)
```

wezterm-gui is completely separate from the process tree containing the GPU
process. It has no ancestral relationship.

#### Proposed ts3 Process Tree (May Work)

```
wezterm-gui (ancestor of GPU process - may be able to lookup)
    └── termsurf-web (profile server)
        └── WezTerm Helper (CEF GPU - creates IOSurface)
```

If wezterm-gui launches the profile server instead of the coordinator, then
wezterm-gui becomes an ancestor of the CEF GPU process. This mirrors Chromium's
architecture where the browser process can lookup IOSurfaces from its GPU
subprocess.

#### Architectural Change Required

Instead of:

1. Coordinator spawns profile server
2. Coordinator tells GUI about IOSurface ID
3. GUI fails to lookup IOSurface (not in same process tree)

Change to:

1. Coordinator tells GUI to spawn profile server
2. GUI spawns profile server (becomes ancestor)
3. Profile server sends IOSurface ID to GUI
4. GUI lookups IOSurface (now in same process tree - may work)

#### Why This Might Work

The `kIOSurfaceIsGlobal` deprecation may not be the full story. The deprecation
notes say IOSurfaces are no longer **globally** accessible, but they may still
be accessible within the same process tree/session. Chromium successfully uses
`IOSurfaceLookup()` between its browser and GPU processes, which suggests
parent-child relationships still enable lookup.

#### What We Need to Test

1. Modify ts3 so wezterm-gui launches profile servers
2. Keep the existing socket communication for commands
3. Test if `IOSurfaceLookup()` succeeds when GUI is the ancestor

If this works, it's a simpler solution than Mach ports or XPC, requiring only an
architectural change to process spawning.

## Experiments

### Experiment 1: GUI-Spawned Profile Servers

**Status:** COMPLETED (Hypothesis 2 Disproven)

**Goal:** Re-architect ts3 so that wezterm-gui spawns profile servers instead of
the coordinator. This is a prerequisite for testing Hypothesis 2 (process
ancestry enables IOSurface lookup).

#### Current Architecture

```
coordinator (web CLI)
    ├── spawns profile server (termsurf-web)
    │       └── spawns WezTerm Helper (CEF GPU)
    ├── connects to profile server socket
    ├── sends "open" to profile server
    ├── receives IOSurface ID from profile server
    ├── connects to GUI socket
    └── sends "display_webview" with IOSurface ID to GUI
            └── GUI tries IOSurfaceLookup() → FAILS (not ancestor)
```

#### Target Architecture

```
coordinator (web CLI)
    └── connects to GUI socket
        └── sends "open_webview" (profile, URL) to GUI
                │
                GUI
                ├── spawns profile server if not running
                │       └── spawns WezTerm Helper (CEF GPU)
                ├── connects to profile server socket
                ├── sends "open" to profile server
                ├── receives IOSurface ID
                └── tries IOSurfaceLookup() → may work (is ancestor)
```

#### Protocol Changes

**Coordinator → GUI message (new):**

```json
{
  "action": "open_webview",
  "data": {
    "engine": "/path/to/termsurf-web",
    "profile": "default",
    "url": "https://google.com",
    "pane_id": 0,
    "width": 800,
    "height": 600
  }
}
```

Note: `pane_id` specifies which terminal pane to render the webview in.

The `engine` field specifies which browser engine executable to run. This makes
the protocol generic - any browser engine that implements the profile server
socket protocol can be used. Future possibilities:

- CEF/Chromium (current): `termsurf-web`
- WebKit: hypothetical `termsurf-webkit`
- Firefox/Gecko: hypothetical `termsurf-gecko`
- Servo: hypothetical `termsurf-servo`

The GUI doesn't care which engine it spawns - it just needs the engine to
implement the standard socket protocol (open, close, get_status, etc.) and
return IOSurface IDs.

**GUI → Coordinator response:**

```json
{
  "status": "ok",
  "data": {
    "webview_id": 1
  }
}
```

The coordinator no longer needs to know about IOSurface IDs - that's now
internal to the GUI.

#### Implementation Steps

1. **Modify GUI socket server** (`webview_socket.rs`)
   - Add `open_webview` action handler
   - Implement profile server spawning logic
   - Implement profile server connection management
   - Track which profile servers are running by (engine, profile) tuple

2. **Add profile server management to GUI**
   - Spawn profile server process when needed
   - **Socket path convention**: `~/.config/termsurf/sockets/{profile}.sock`
   - **Retry logic**: Wait for socket to exist with exponential backoff (up to
     5s)
   - Connect to profile server socket
   - Send `open` request, receive IOSurface ID
   - Store IOSurface ID in overlay state for the specified `pane_id`

3. **Modify profile server to wait for first paint**
   (`termsurf-web/src/main.rs`)
   - After creating browser, wait for first `on_accelerated_paint` callback
   - Only then return the IOSurface ID in the `open` response
   - This ensures the IOSurface ID is valid, not 0 or stale

4. **Simplify coordinator** (`termsurf-web/src/main.rs`)
   - Remove profile server spawning code
   - Remove profile server socket connection
   - Only connect to GUI socket
   - Send `open_webview` instead of `display_webview`
   - Include `pane_id` from `WEZTERM_PANE` environment variable

5. **Test IOSurface lookup**
   - Re-enable the `IOSurfaceLookup()` code in `draw.rs`
   - Verify if lookup succeeds now that GUI is ancestor

#### Files to Modify

| File                                           | Changes                                                          |
| ---------------------------------------------- | ---------------------------------------------------------------- |
| `wezterm-gui/src/termwindow/webview_socket.rs` | Add `open_webview` handler, profile server spawning, retry logic |
| `termsurf-web/src/main.rs`                     | Wait for first paint in `open`; simplify coordinator to GUI-only |
| `wezterm-gui/src/termwindow/render/draw.rs`    | Re-enable IOSurface lookup code                                  |
| `wezterm-gui/src/termwindow/webgpu.rs`         | Re-add webview render pipeline                                   |

#### Success Criteria

- [x] Coordinator only communicates with GUI (not profile server directly)
- [x] GUI spawns profile server on first `open_webview` for a profile
- [x] GUI connects to profile server and receives IOSurface ID
- [ ] `IOSurfaceLookup()` returns a valid handle (not NULL) — **FAILED**
- [ ] Texture renders correctly in the pane — not attempted

#### Results

**Date:** 2026-01-25

The architectural change was implemented successfully. The coordinator now sends
`open_webview` to the GUI, which spawns the profile server, connects to it, and
receives the IOSurface ID. However, `IOSurfaceLookup()` still returned NULL:

```
[GUI Socket] Profile server returned: iosurface_id=188, size=800x600
[GUI Socket] FAILED: IOSurfaceLookup returned NULL for id=188
[GUI Socket] Hypothesis 2 DISPROVEN: process ancestry does NOT enable IOSurface sharing
```

**Conclusion:** Process ancestry (grandparent → grandchild) does NOT enable
cross-process IOSurface sharing via global IDs. The `kIOSurfaceIsGlobal`
deprecation is absolute — IOSurfaces cannot be looked up by ID across any
process boundary, regardless of parent/child relationships.

**Implication:** We must use one of the alternative approaches:

1. Mach port-based IOSurface transfer
2. Shared memory with pixel copy
3. XPC services
4. CALayerHost/CARemoteLayer

#### Key Insight from Failure

We tested whether **grandparent** process ancestry enables IOSurface lookup:

```
Chromium internal:  Browser → GPU (parent → child, works without IOSurfaceLookup)
Our architecture:   GUI → Profile Server → GPU (grandparent → grandchild, tested)
```

The experiment revealed that Chromium likely does NOT use `IOSurfaceLookup()` at
all — the browser process receives the IOSurface handle directly via Mach IPC,
not via global ID lookup. The "global ID" approach is fundamentally broken for
cross-process sharing since macOS 10.11.

#### Deferred Items

- **Close/cleanup handling**: When coordinator disconnects or sends
  `close_webview`, need to close browser and potentially shut down idle profile
  servers. Defer to later experiment.
- **Error handling**: Engine not found, profile server crash, CEF init failure.
  Add basic error responses but defer comprehensive handling.

#### Notes

- Profile server code changes slightly (wait for first paint before responding)
- Socket protocol between GUI and profile server remains the same
- Only the spawning relationship changes
- Coordinator's role becomes minimal (just a CLI that talks to GUI)
- The `engine` field enables future browser engine diversity - any engine that
  implements the socket protocol can be used
- GUI manages engine processes by (engine, profile) tuple - one process per
  engine/profile combination

#### Research: How Chromium/CEF/cef-rs Handle GPU Texture Sharing

After the experiment failed, we researched how texture sharing actually works at
each layer:

**cef-rs (this repo):**

- No Mach ports used — IOSurface handles passed directly via
  `on_accelerated_paint`
- The `iosurface_ipc.rs` module explicitly notes: "Cross-process IOSurface
  sharing via global IDs does not work for IOSurfaces created by CEF's GPU
  process"
- In-process usage (like ts2): Handle used directly to create Metal texture, no
  lookup needed

**CEF:**

- `on_accelerated_paint` provides the IOSurface handle directly in
  `cef_accelerated_paint_info_t.shared_texture_io_surface`
- CEF handles GPU→browser process sharing internally before invoking your
  callback
- The handle is valid within the CEF browser process but not in external
  processes

**Chromium:**

The
[IOSurface meeting notes](https://www.chromium.org/developers/design-documents/iosurface-meeting-notes/)
(from 2010) described using `IOSurfaceGetID()`/`IOSurfaceLookup()`. However,
this predates the `kIOSurfaceIsGlobal` deprecation (macOS 10.11, 2015).

Critically,
[Chromium code review 1532813002](https://codereview.chromium.org/1532813002) is
titled: **"Replace IOSurfaceManager by directly passing IOSurface Mach ports
over Chrome IPC"** — confirming Chromium switched from global IDs to Mach ports.

**Summary:**

| Layer             | Mechanism                                                |
| ----------------- | -------------------------------------------------------- |
| Chromium (modern) | Mach ports via Chrome IPC                                |
| CEF               | Inherits from Chromium; handle valid in browser process  |
| cef-rs            | Direct handle usage in-process; no cross-process support |

**Sources:**

- [Chromium IOSurface Meeting Notes](https://www.chromium.org/developers/design-documents/iosurface-meeting-notes/)
- [Chromium Code Review: Mach Port IOSurface](https://codereview.chromium.org/1532813002)
- [Cross-process Rendering (Russ Bishop)](http://www.russbishop.net/cross-process-rendering)
- [CEF accelerated_paint_info_t](https://cef-builds.spotifycdn.com/docs/132.3/structcef__accelerated__paint__info__t.html)
- [OBS Browser ARM64/Apple Silicon PR](https://github.com/obsproject/obs-browser/pull/310)

#### Research: Cross-Platform Texture Transfer Mechanisms

After understanding how Chromium/CEF handles texture sharing internally, we
researched how to make our cross-process transfer mechanism work on all
platforms (macOS, Linux, Windows).

**Why Mach Ports Are Unavoidable on macOS:**

In ts2, CEF runs in-process with wezterm-gui. The `on_accelerated_paint`
callback receives an IOSurface handle that is _already valid_ because
Chromium/CEF internally transferred it from the GPU helper process via Mach
ports. The code then creates a Metal texture backed by that IOSurface:

```rust
let texture: metal::Texture = objc::msg_send![
    device_ref,
    newTextureWithDescriptor:desc_ref
    iosurface:self.handle  // Handle already valid in this process
    plane:0usize
];
```

The Metal texture creation is NOT a transfer mechanism — it's what you do AFTER
you have a valid handle. In ts3, we need to perform the same Mach port transfer
that Chromium does internally, but between our profile server and GUI processes.

**Platform-Specific Transfer Mechanisms:**

| Platform    | Handle Type               | Transfer Mechanism         | Complexity |
| ----------- | ------------------------- | -------------------------- | ---------- |
| **macOS**   | IOSurface (`*mut c_void`) | Mach ports                 | High       |
| **Linux**   | DMA-BUF (file descriptor) | Unix socket + `SCM_RIGHTS` | Low        |
| **Windows** | DXGI handle (`HANDLE`)    | `DuplicateHandle`          | Medium     |

**Key Insight: Linux Is Easier**

On Linux, DMA-BUF uses file descriptors which can be sent over Unix domain
sockets using `SCM_RIGHTS` ancillary data. This means our existing Unix socket
infrastructure can carry texture handles directly — no new IPC mechanism needed!

From cef-rs `dmabuf.rs`:

```rust
pub struct DmaBufImporter {
    fds: Vec<std::os::fd::RawFd>,  // File descriptors - can use SCM_RIGHTS
    ...
}
```

**No Existing Cross-Platform Abstraction:**

There's no single library that abstracts all three mechanisms because they're
fundamentally different:

- macOS: Kernel objects transferred via Mach IPC
- Linux: File descriptors transferred via socket ancillary data
- Windows: Handles duplicated via Win32 API

cef-rs already abstracts the **import side** (`TextureImporter` trait). We need
to add a **transfer abstraction**:

```rust
// Proposed new abstraction
pub trait TextureTransfer {
    fn send(&self, info: &AcceleratedPaintInfo, dest: &Destination) -> Result<()>;
    fn receive(&self) -> Result<ReceivedTextureInfo>;
}

// Platform implementations:
// - macOS: MachPortTransfer
// - Linux: ScmRightsTransfer (uses existing Unix sockets)
// - Windows: HandleDuplicateTransfer
```

**Implementation Strategy:**

1. **macOS first** — Mach ports are the hardest; get this working first
2. **Linux second** — Add `SCM_RIGHTS` support to Unix socket code (simpler)
3. **Windows third** — `DuplicateHandle` is straightforward

The overall architecture (profile server sends → GUI receives → GUI imports)
will be identical across platforms. Only the transfer mechanism differs.

**Sources:**

- [Inter-Process Texture Sharing with DMA-BUF](https://blaztinn.gitlab.io/post/dmabuf-texture-sharing/)
- [Linux Kernel DMA-BUF Documentation](https://docs.kernel.org/driver-api/dma-buf.html)
- [Cross-process rendering using CALayer](https://teamdev.com/jxbrowser/blog/cross-process-rendering-using-calayer/)
