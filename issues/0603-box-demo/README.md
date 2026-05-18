+++
status = "closed"
opened = "2026-02-19"
closed = "2026-02-19"
+++

# Issue 603: Box Demo in Ghost

## Goal

Render live Chromium frames in Ghost. The `web` TUI opens a URL, Ghost spawns a
Chromium Profile Server, receives IOSurface Mach ports at 60fps, and renders
them as a textured overlay at the correct grid coordinates. The box demo
(spinning blue square) is the test page.

## Background

Issue 602 proved the pink overlay pipeline works — a GPU quad renders at grid
coordinates specified by `web`, survives resize, and clears on disconnect. This
issue replaces the pink quad with live Chromium frames.

### What we have

**Ghost (from Issues 601–602):**

- XPC gateway connection, anonymous listener, endpoint registration
- Message parsing (`set_overlay`, `mode_changed`)
- Pane ID on Surface, propagated as `TERMSURF_PANE_ID`
- Surface lookup by pane ID
- Pink overlay shader, pipeline, renderer state, render step in `drawFrame()`
- `setOverlay()` / `clearOverlay()` with `draw_mutex` thread safety
- XPC handler wired to surface methods

**Chromium Profile Server (from ts5 Issues 503–515):**

- Full XPC protocol: gateway connect, `server_register`, `create_tab`,
  `tab_ready`, `display_surface`, `resize`, mouse/scroll/focus forwarding
- 120fps IOSurface capture via `FrameSinkVideoCapturer`
- Mach port transfer: `IOSurfaceCreateMachPort` → `xpc_dictionary_set_mach_send`
- Per-tab pane routing, auto-exit on last tab close
- Current branch: `146.0.7650.0-issue-515` (latest, all features)

**`web` TUI (already sends URL):**

- `send_set_overlay` includes `url`, `profile`, `browsing` fields
- No changes needed to `web`

### What we need to build

1. **Copy box demo** — Move `ts4/box-demo/` to top-level `box-demo/`
2. **Fork Chromium branch** — Create `146.0.7650.0-issue-603` from the latest
   working branch. No Chromium source changes expected.
3. **IOSurface overlay shader** — Textured overlay vertex/fragment in
   `shaders.metal` (samples IOSurface texture instead of returning pink)
4. **IOSurface overlay pipeline** — `overlay` pipeline in `shaders.zig`
5. **IOSurface texture creation** — `Texture.fromIOSurface()` in Ghost's
   `Texture.zig` using `MTLDevice.newTextureWithDescriptor:iosurface:plane:`
6. **IOSurface state on renderer** — `overlay_iosurface` pointer field,
   `overlay_surface_changed` flag
7. **`setOverlayIOSurface()` on Surface** — Thread-safe IOSurface update with
   `CFRetain` / `CFRelease` under `draw_mutex`
8. **Render path in `drawFrame()`** — If IOSurface present, use textured overlay
   pipeline; otherwise fall back to pink
9. **Chromium server lifecycle in XPC** — Handle `server_register`, send
   `create_tab`, handle `display_surface` (Mach port → IOSurface → renderer)
10. **Server spawning** — Launch `Chromium Profile Server.app` with the right
    flags when `set_overlay` arrives with a URL

### Chromium server XPC protocol

Messages Ghost must handle from the Chromium server:

| Message           | Fields                          | Frequency   |
| ----------------- | ------------------------------- | ----------- |
| `server_register` | action, profile                 | Once        |
| `tab_ready`       | action, tab_id                  | Once/tab    |
| `display_surface` | action, pane_id, iosurface_port | 60fps       |
| `url_changed`     | action, pane_id, url            | On navigate |
| `cursor_changed`  | action, pane_id, cursor_type    | On change   |

Messages Ghost must send to the Chromium server:

| Message      | Fields                                          | When           |
| ------------ | ----------------------------------------------- | -------------- |
| `create_tab` | action, url, pane_id, pixel_width, pixel_height | After register |
| `resize`     | action, pane_id, pixel_width, pixel_height      | On resize      |

The server connects to the xpc-gateway, sends `{ action: "connect" }`, receives
Ghost's endpoint, connects directly, and sends `server_register`. Ghost replies
with `create_tab`. The server then streams `display_surface` at 60fps.

### Mach port transfer in Zig

The `display_surface` message carries an IOSurface Mach port. In Zig:

```zig
extern "c" fn xpc_dictionary_copy_mach_send(xdict: xpc_object_t, key: [*:0]const u8) u32;
extern "c" fn IOSurfaceLookupFromMachPort(port: u32) ?*anyopaque;
extern "c" fn mach_port_deallocate(task: u32, name: u32) i32;
extern "c" fn mach_task_self() u32;
```

Flow:

1. `xpc_dictionary_copy_mach_send(msg, "iosurface_port")` → Mach port
2. `IOSurfaceLookupFromMachPort(port)` → IOSurfaceRef
3. `mach_port_deallocate(mach_task_self(), port)` — clean up kernel reference
4. Pass IOSurfaceRef to `surface.setOverlayIOSurface()`

### IOSurface texture creation

`MTLDevice.newTextureWithDescriptor:iosurface:plane:` creates a zero-copy
MTLTexture view into the IOSurface's GPU memory. From ts5's Texture.zig:

```zig
pub fn fromIOSurface(device: objc.Object, iosurface: *anyopaque) ?Self {
    const width: usize = IOSurfaceGetWidth(iosurface);
    const height: usize = IOSurfaceGetHeight(iosurface);
    // Create MTLTextureDescriptor with bgra8unorm, shader-read usage
    // Call device.newTextureWithDescriptor:iosurface:plane:
}
```

### Server spawning

The Chromium Profile Server binary lives at:

```
chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server
```

Launch arguments:

```
--xpc-service=com.termsurf.xpc-gateway
--user-data-dir=~/.config/termsurf/chromium-profiles/{profile}
--hidden
```

Ghost spawns it via `std.process.Child` (Zig's process API). One server per
profile — multiple panes with the same profile share one server.

### Key technical details

**Pixel dimensions for `create_tab`:** The server needs physical pixel
dimensions, not grid cells. Ghost computes them:
`pixel_width = grid_width * cell_width`,
`pixel_height = grid_height * cell_height`. Cell size comes from
`renderer.grid_metrics.cell_width/cell_height`.

**Thread safety:** `display_surface` arrives at 60fps on the XPC queue
(background thread). `setOverlayIOSurface()` locks `draw_mutex`, swaps the
IOSurface pointer with `CFRetain`/`CFRelease`, sets
`overlay_surface_changed = true`, and queues a render. `drawFrame()` holds
`draw_mutex` and creates an MTLTexture from the current IOSurface each frame.

**Server peer vs web peer:** Ghost's XPC listener now accepts two kinds of
peers: `web` processes (send `set_overlay`) and Chromium servers (send
`server_register`). The listener handler must distinguish them by the first
message received.

## Experiment 1: IOSurface texture pipeline

### Goal

Prove the IOSurface → Metal texture path works in Zig. A programmatically
created blue checkerboard IOSurface renders at the correct grid coordinates,
replacing the pink quad. No Chromium needed — isolates the texture pipeline from
server lifecycle.

### Changes

**1. `ghost/src/renderer/shaders/shaders.metal` — Textured overlay shaders**

Add `pixel_width` and `pixel_height` to `PinkOverlayIn` (the pink overlay shader
ignores them, but both shaders share the same buffer layout). Add
`OverlayVertexOut`, `overlay_vertex`, and `overlay_fragment`:

```metal
struct PinkOverlayIn {
  float grid_col;
  float grid_row;
  float grid_width;
  float grid_height;
  float pixel_width;   // NEW
  float pixel_height;  // NEW
};

struct OverlayVertexOut {
  float4 position [[position]];
  float2 texcoord;
};

vertex OverlayVertexOut overlay_vertex(
  uint vid [[vertex_id]],
  constant PinkOverlayIn& params [[buffer(0)]],
  constant Uniforms& uniforms [[buffer(1)]]
) {
  float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size;
  float2 size = float2(params.pixel_width, params.pixel_height);

  float2 corner;
  corner.x = float(vid == 1 || vid == 3);
  corner.y = float(vid == 2 || vid == 3);

  OverlayVertexOut out;
  out.position = uniforms.projection_matrix * float4(origin + size * corner, 0.0, 1.0);
  out.texcoord = corner;
  return out;
}

fragment float4 overlay_fragment(
  OverlayVertexOut in [[stage_in]],
  texture2d<float> tex [[texture(0)]]
) {
  constexpr sampler s(mag_filter::nearest, min_filter::nearest);
  return tex.sample(s, in.texcoord);
}
```

The vertex shader sizes the quad using `pixel_width`/`pixel_height` from the
IOSurface dimensions (not `grid_width * cell_size`), so the texture renders at
its native resolution. The fragment shader samples the bound texture at UV
coordinates.

**2. `ghost/src/renderer/metal/shaders.zig` — Overlay pipeline and params**

Extend `PinkOverlay` with pixel dimensions:

```zig
pub const PinkOverlay = extern struct {
    grid_col: f32 = 0,
    grid_row: f32 = 0,
    grid_width: f32 = 0,
    grid_height: f32 = 0,
    pixel_width: f32 = 0,   // NEW
    pixel_height: f32 = 0,  // NEW
};
```

Add `overlay` pipeline entry (after `pink_overlay`):

```zig
.{ "overlay", .{
    .vertex_fn = "overlay_vertex",
    .fragment_fn = "overlay_fragment",
    .blending_enabled = true,
} },
```

**3. `ghost/src/renderer/metal/Texture.zig` — `fromIOSurface()`**

Add a method that creates a zero-copy MTLTexture view into IOSurface GPU memory:

```zig
pub fn fromIOSurface(device: objc.Object, iosurface: *anyopaque) ?Self {
    const width: usize = IOSurfaceGetWidth(iosurface);
    const height: usize = IOSurfaceGetHeight(iosurface);

    const desc = init: {
        const Class = objc.getClass("MTLTextureDescriptor").?;
        const id_alloc = Class.msgSend(objc.Object, objc.sel("alloc"), .{});
        const id_init = id_alloc.msgSend(objc.Object, objc.sel("init"), .{});
        break :init id_init;
    };
    defer desc.release();

    desc.setProperty("pixelFormat", @intFromEnum(mtl.MTLPixelFormat.bgra8unorm));
    desc.setProperty("width", @as(c_ulong, width));
    desc.setProperty("height", @as(c_ulong, height));
    desc.setProperty("usage", @as(c_ulong, 0x0004)); // MTLTextureUsageShaderRead

    const id = device.msgSend(
        ?*anyopaque,
        objc.sel("newTextureWithDescriptor:iosurface:plane:"),
        .{ desc, iosurface, @as(c_ulong, 0) },
    ) orelse return null;

    return .{
        .texture = objc.Object.fromId(id),
        .width = width,
        .height = height,
        .bpp = 4,
    };
}

extern "c" fn IOSurfaceGetWidth(iosurface: *anyopaque) usize;
extern "c" fn IOSurfaceGetHeight(iosurface: *anyopaque) usize;
```

**4. `ghost/src/renderer/generic.zig` — IOSurface state and render path**

Add fields (after `pink_overlay`):

```zig
overlay_iosurface: ?*anyopaque = null,
overlay_surface_changed: bool = false,
```

In `drawFrame()`, replace the pink overlay render step with a branch:

```zig
if (self.pink_overlay.grid_width > 0 and
    self.pink_overlay.grid_height > 0)
{
    if (self.overlay_iosurface) |iosurface| {
        // IOSurface texture path.
        if (Texture.fromIOSurface(self.api.device, iosurface)) |tex| {
            defer tex.deinit();
            var overlay_params = self.pink_overlay;
            overlay_params.pixel_width = @floatFromInt(tex.width);
            overlay_params.pixel_height = @floatFromInt(tex.height);
            if (Buffer(shaderpkg.PinkOverlay).initFill(
                self.api.imageBufferOptions(),
                &.{overlay_params},
            )) |*buf| {
                defer buf.deinit();
                pass.step(.{
                    .pipeline = self.shaders.pipelines.overlay,
                    .uniforms = frame.uniforms.buffer,
                    .buffers = &.{buf.buffer},
                    .textures = &.{tex},
                    .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
                });
            } else |_| {}
        }
    } else {
        // Pink fallback (no IOSurface).
        if (Buffer(shaderpkg.PinkOverlay).initFill(
            self.api.imageBufferOptions(),
            &.{self.pink_overlay},
        )) |*buf| {
            defer buf.deinit();
            pass.step(.{
                .pipeline = self.shaders.pipelines.pink_overlay,
                .uniforms = frame.uniforms.buffer,
                .buffers = &.{buf.buffer},
                .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
            });
        } else |_| {}
    }
}
```

**5. `ghost/src/Surface.zig` — `setOverlayIOSurface()`, update
`clearOverlay()`**

Add `setOverlayIOSurface()` with `CFRetain`/`CFRelease` under `draw_mutex`:

```zig
extern "c" fn CFRetain(*anyopaque) void;
extern "c" fn CFRelease(*anyopaque) void;

pub fn setOverlayIOSurface(self: *Surface, iosurface: ?*anyopaque) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    if (self.renderer.overlay_iosurface) |old| CFRelease(old);
    if (iosurface) |new| CFRetain(new);

    self.renderer.overlay_iosurface = iosurface;
    self.renderer.overlay_surface_changed = true;
    self.queueRender() catch {};
}
```

Update `clearOverlay()` to release IOSurface:

```zig
pub fn clearOverlay(self: *Surface) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    if (self.renderer.overlay_iosurface) |old| CFRelease(old);
    self.renderer.overlay_iosurface = null;
    self.renderer.pink_overlay = .{};
    self.queueRender() catch {};
}
```

**6. `ghost/src/apprt/xpc.zig` — Test IOSurface**

In `handleSetOverlay()`, create a 200×200 blue checkerboard IOSurface
programmatically and pass it to the renderer. Uses raw CoreFoundation extern
declarations (temporary test code, replaced by Chromium frames in Experiment 2):

```zig
fn createTestIOSurface() ?*anyopaque {
    // Create CFDictionary with width=200, height=200, BGRA, 4 bpe
    // IOSurfaceCreate → lock → fill blue checkerboard → unlock
    // Return IOSurfaceRef
}
```

After calling `surface.setOverlay()`, call
`surface.setOverlayIOSurface(createTestIOSurface())`.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/overlay.log
# In a Ghost pane:
cargo run -p web -- http://example.com
```

Pass: Blue checkerboard renders at correct grid coordinates. Pink quad replaced.

### Result

**Pass.** Blue checkerboard renders at the correct grid origin, replacing the
pink quad. The 200×200 test IOSurface is intentionally smaller than the viewport
— real Chromium frames will arrive at viewport pixel dimensions.

Initial build had a bug: the `kIOSurface*` extern constants are `CFStringRef`
values (pointers), but were declared as `anyopaque` and passed with `&`, giving
the address OF the pointer instead of the pointer itself. Fixed by declaring as
`*const anyopaque` and passing directly. `IOSurfaceCreate` silently returned
null with the wrong keys.

Files changed:

- `ghost/src/renderer/shaders/shaders.metal` — `pixel_width`/`pixel_height` in
  `PinkOverlayIn`, textured `overlay_vertex`/`overlay_fragment`
- `ghost/src/renderer/metal/shaders.zig` — `overlay` pipeline, extended
  `PinkOverlay` struct
- `ghost/src/renderer/metal/Texture.zig` — `fromIOSurface()` zero-copy texture
- `ghost/src/renderer/generic.zig` —
  `overlay_iosurface`/`overlay_surface_changed` fields, branched render path
- `ghost/src/Surface.zig` — `setOverlayIOSurface()` with `CFRetain`/`CFRelease`,
  updated `clearOverlay()`
- `ghost/src/apprt/xpc.zig` — `createTestIOSurface()` (200×200 blue
  checkerboard), wired into `handleSetOverlay`

## Experiment 2: Chromium server lifecycle

### Goal

Box demo renders live in the terminal at 60fps. Ghost spawns the Chromium
Profile Server when `set_overlay` arrives with a URL, handles the full XPC
protocol (`server_register` → `create_tab` → `display_surface`), and streams
IOSurface frames to the renderer. The texture pipeline from Experiment 1 is used
as-is.

### Prerequisites

1. **Copy box demo** — `cp -r ts4/box-demo box-demo`
2. **Fork Chromium branch** — `146.0.7650.0-issue-603` from
   `146.0.7650.0-issue-515`. No source changes expected.
3. **Build server** — `autoninja -C out/Default chromium_profile_server`

### Changes

Only `ghost/src/apprt/xpc.zig` and `ghost/src/Surface.zig`. Everything else from
Experiment 1 is used unchanged.

**1. `ghost/src/Surface.zig` — Add `getCellSize()`**

Thread-safe accessor for pixel dimension computation:

```zig
pub fn getCellSize(self: *Surface) struct { width: u32, height: u32 } {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    return .{
        .width = self.renderer.grid_metrics.cell_width,
        .height = self.renderer.grid_metrics.cell_height,
    };
}
```

**2. `ghost/src/apprt/xpc.zig` — Full rewrite of server lifecycle**

**Remove:** `createTestIOSurface()` and all CF/IOSurface-creation externs (test
code from Experiment 1).

**New extern declarations:**

```zig
// Mach port transfer (display_surface handler).
extern "c" fn xpc_dictionary_copy_mach_send(
    xdict: xpc_object_t, key: [*:0]const u8) u32;
extern "c" fn IOSurfaceLookupFromMachPort(port: u32) ?*anyopaque;
extern "c" fn mach_port_deallocate(task: u32, name: u32) i32;
extern const mach_task_self_: u32;

// Sending messages to server.
extern "c" fn xpc_dictionary_set_uint64(
    xdict: xpc_object_t, key: [*:0]const u8, value: u64) void;

// Peer identification.
extern "c" fn xpc_dictionary_get_remote_connection(
    msg: xpc_object_t) xpc_object_t;
```

**New module state:**

```zig
var server_peer: xpc_object_t = null;
var server_process: ?std.process.Child = null;

// Pending state between set_overlay and server_register.
var pending_url_buf: [2048]u8 = undefined;
var pending_url_len: usize = 0;
var pending_pane_id: [36]u8 = undefined;
var pending_pixel_w: u64 = 0;
var pending_pixel_h: u64 = 0;
```

**Modified `listenerHandler`:** Don't assign to `web_peer` — peers are
identified by their first message via `xpc_dictionary_get_remote_connection`.

**Modified `handleMessage`:** Add `server_register`, `display_surface`,
`tab_ready` actions. On `set_overlay`, retain connection as `web_peer`. On
`server_register`, retain connection as `server_peer`.

**Modified `handleSetOverlay`:** Store URL and pane ID in pending buffers.
Compute pixel dimensions via `surface.getCellSize()`. Spawn the Chromium Profile
Server. Remove test IOSurface code.

**New `spawnServer`:** Launch via `std.process.Child.init` (following the
pattern in `ghost/src/os/open.zig`):

```zig
fn spawnServer(profile: []const u8) void {
    const home = std.posix.getenv("HOME") orelse return;
    // Build argv with server path, --xpc-service, --user-data-dir, --hidden
    // Spawn and store in server_process
}
```

Server binary path:

```
{HOME}/dev/termsurf/chromium/src/out/Default/
  Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server
```

Arguments: `--xpc-service=com.termsurf.xpc-gateway`,
`--user-data-dir={HOME}/.config/termsurf/chromium-profiles/{profile}`,
`--hidden`

**New `handleServerRegister`:** Send `create_tab` to the server peer:

```zig
fn handleServerRegister(msg: xpc_object_t) void {
    const reply = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(reply, "action", "create_tab");
    xpc_dictionary_set_string(reply, "url", pending_url_buf[0..pending_url_len]);
    xpc_dictionary_set_string(reply, "pane_id", &pending_pane_id);
    xpc_dictionary_set_uint64(reply, "pixel_width", pending_pixel_w);
    xpc_dictionary_set_uint64(reply, "pixel_height", pending_pixel_h);
    xpc_connection_send_message(server_peer, reply);
}
```

**New `handleDisplaySurface`:** Called at 60fps. Extract Mach port, import
IOSurface, pass to renderer:

```zig
fn handleDisplaySurface(msg: xpc_object_t) void {
    const port = xpc_dictionary_copy_mach_send(msg, "iosurface_port");
    if (port == 0) return;
    const iosurface = IOSurfaceLookupFromMachPort(port) orelse {
        _ = mach_port_deallocate(mach_task_self_, port);
        return;
    };
    _ = mach_port_deallocate(mach_task_self_, port);

    if (overlay_surface) |surface| {
        surface.setOverlayIOSurface(iosurface);
    }
    // IOSurfaceLookupFromMachPort returns +1; setOverlayIOSurface
    // CFRetains, so we CFRelease our lookup reference.
    CFRelease(iosurface);
}
```

**Modified disconnect handler:** On any peer disconnect, kill server process,
release both peers, clear overlay.

### Verification

```bash
cd box-demo && bun run server.ts &
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/overlay.log
# In a Ghost pane:
cargo run -p web -- http://localhost:9407
```

Pass: Box demo (spinning blue square) renders in the terminal at 60fps for >30s.
Clean exit (`ctrl+c` in `web`) kills the server process.

### Result

**Pass.** Box demo renders live in the terminal at ~60fps. The Chromium Profile
Server streams IOSurface Mach ports over XPC, Ghost imports them as zero-copy
Metal textures, and the overlay shader composites them at the correct grid
coordinates. Google.com also renders correctly.

A disconnect crash was discovered and fixed: when `web` exits, both the web peer
and server peer disconnect simultaneously on different XPC dispatch queue
threads. Both called `handleDisconnect()` concurrently, racing on shared
module-level state (`server_process`, `web_peer`, `server_peer`). Thread A's
`proc.wait()` invalidated the process handle, then Thread B's `proc.kill()` read
the undefined `id` field → `panic: reached unreachable code` on two threads.

Fixed by introducing a `Pane` struct with a per-pane `std.Thread.Mutex`. All
handlers lock the pane's mutex before touching state. `handleDisconnect` is
idempotent — the second thread sees null peers and no-ops. This design scales to
multi-webview: replace `var pane: Pane` with a `HashMap(UUID, *Pane)`.

Files changed:

- `ghost/src/apprt/xpc.zig` — Full Chromium server lifecycle (`server_register`,
  `create_tab`, `display_surface`), Mach port transfer, server spawning via
  `std.process.Child`, per-pane mutex for thread safety
- `ghost/src/Surface.zig` — `getCellSize()` for pixel dimension computation
- `docs/chromium.md` — Added `146.0.7650.0-issue-603` branch
- `box-demo/` — Copied from `ts4/box-demo/` (spinning blue square test page)

## Experiment 3: Dynamic resize

### Goal

Resize the terminal window while Chromium is streaming. Ghost sends `resize` to
the server, the server adjusts capture resolution, and frames continue at the
new size. The overlay quad resizes automatically because the vertex shader
already reads pixel dimensions from each IOSurface.

### Background

`web` already sends updated `set_overlay` messages on resize — the logs from
Experiment 2 show the same pane receiving progressively larger grid dimensions
as the window is dragged. Ghost already recomputes pixel dimensions in
`handleSetOverlay`. The Chromium server already handles `resize` messages
(`shell_browser_main_parts.cc:211`), calling `ResizeCapture` with new pixel
dimensions.

The only missing piece: Ghost never sends the `resize` message to the server.

### Changes

**One file: `ghost/src/apprt/xpc.zig`**

In `handleSetOverlay`, after recomputing `pending_pixel_w` and
`pending_pixel_h`, check if the server is already running and the pixel
dimensions changed. If so, send a `resize` message:

```zig
// If the server is already running and dimensions changed, send resize.
if (pane.server_peer != null) {
    if (new_pixel_w != old_pixel_w or new_pixel_h != old_pixel_h) {
        sendResize(&pane);
    }
}
```

New helper (caller holds `p.mutex`):

```zig
fn sendResize(p: *Pane) void {
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "resize");

    var pane_z: [37]u8 = undefined;
    @memcpy(pane_z[0..36], &p.pending_pane_id);
    pane_z[36] = 0;
    xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));

    xpc_dictionary_set_uint64(msg, "pixel_width", p.pending_pixel_w);
    xpc_dictionary_set_uint64(msg, "pixel_height", p.pending_pixel_h);

    xpc_connection_send_message(p.server_peer, msg);
    log.info("sent resize pixel={d}x{d}", .{ p.pending_pixel_w, p.pending_pixel_h });
}
```

No other files change. The overlay vertex shader already reads
`pixel_width`/`pixel_height` from each IOSurface via `Texture.fromIOSurface`, so
the quad automatically resizes when new frames arrive at the new resolution.

### Verification

```bash
cd box-demo && bun run server.ts &
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/overlay.log
# In a Ghost pane:
cargo run -p web -- http://localhost:9407
# Drag the window edge to resize
```

Pass: Box demo continues rendering during and after resize. The overlay fills
the new viewport dimensions. Logs show `sent resize` messages.

### Result

**Pass.** Resize works end-to-end. Dragging the window edge sends updated
`set_overlay` messages from `web`, Ghost detects the pixel dimension change,
sends `resize` to the Chromium server, and the server adjusts capture
resolution. New IOSurface frames arrive at the new size and the overlay quad
scales automatically.

Files changed:

- `ghost/src/apprt/xpc.zig` — `sendResize()` helper, dimension change detection
  in `handleSetOverlay`

## Conclusion

Issue 603 is complete. Ghost renders live Chromium frames in the terminal at
60fps with dynamic resize — the full pipeline from `web` TUI to IOSurface
overlay, built entirely in Zig.

Three experiments, each building on the last:

1. **IOSurface texture pipeline** — Proved zero-copy Metal textures from
   IOSurface work in Zig. A programmatic blue checkerboard rendered at the
   correct grid coordinates, replacing the pink quad from Issue 602.
2. **Chromium server lifecycle** — Full end-to-end streaming. Ghost spawns the
   Chromium Profile Server, handles the XPC protocol (`server_register` →
   `create_tab` → `display_surface`), and composites live frames. A disconnect
   race condition (two XPC threads calling `handleDisconnect` concurrently) was
   fixed with a per-pane mutex — a design that scales to multi-webview.
3. **Dynamic resize** — One `sendResize` helper. The server adjusts capture
   resolution, new frames arrive at the new size, and the overlay shader scales
   automatically.

### What's next

Mouse forwarding, keyboard forwarding, and navigation are natural next steps
(Issue 604+). The XPC protocol and Chromium server already support all three —
Ghost just needs to send the messages.
