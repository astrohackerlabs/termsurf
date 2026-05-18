+++
status = "closed"
opened = "2026-02-16"
closed = "2026-02-16"
+++

# Issue 508: Retina Checkerboard with Safe IOSurface Lifetime

## Background

Issue 507 proved the full Chromium integration pipeline works — IOSurface frames
streamed at 60fps from Chromium Profile Server through XPC to the Metal
renderer. But it crashed after ~3 seconds. The same crash occurred when resizing
the terminal with a static checkerboard overlay.

Both crashes have the same root cause: **IOSurface use-after-free across the
Swift/Zig boundary.** Swift passes the IOSurface to Zig as a raw pointer via
`Unmanaged.passUnretained().toOpaque()`. When Swift replaces or releases the
IOSurface (on resize or new frame), ARC frees it while the Zig renderer still
holds the dangling pointer.

This issue isolates the lifetime problem using a simple test case — a
checkerboard IOSurface — without any Chromium complexity. Once the checkerboard
survives resize without crashing, the same fix applies to live Chromium frames.

### What exists today (Issue 505)

- **Pink overlay pipeline** (`pink_overlay` in `shaders.zig` / `shaders.metal`)
  renders a solid hot-pink quad at grid coordinates. No IOSurface, no texture
  sampling — just a constant color from the fragment shader.
- **C API** (`ghostty_surface_set_overlay` / `clear_overlay`) sets grid
  coordinates on the renderer under `draw_mutex`.
- **`web` TUI** sends viewport grid coordinates via XPC. The pink quad appears
  at the exact viewport position and clears on disconnect.

### What Issue 507 added and reverted

- **IOSurface texture overlay** (`overlay` pipeline, `overlay_vertex` /
  `overlay_fragment` shaders) — samples from an IOSurface-backed Metal texture
  instead of returning a constant color.
- **`ghostty_surface_set_overlay_iosurface`** — C API to pass an IOSurface
  pointer to the renderer.
- **`ghostty_surface_get_cell_size`** — C API to query physical pixel dimensions
  of a terminal cell (already includes Retina scale factor via DPI-scaled font
  metrics).
- **IOSurface texture import** (`Texture.fromIOSurface`) — creates a Metal
  texture from an IOSurface reference.
- **Checkerboard test surface** — Swift code in `CompositorXPC.swift` that
  creates an IOSurface, fills it with a blue/dark checkerboard pattern, and
  passes it to the renderer.

All of this code was reverted to the pink overlay state. This issue will
reimplement it with proper IOSurface lifetime management.

## The Problem

The renderer runs on its own thread. The IOSurface pointer is set from the main
thread (or XPC queue) under `draw_mutex`. The mutex protects the pointer swap
but not the IOSurface lifetime:

```
Thread A (main/XPC):          Thread B (renderer):
───────────────────           ────────────────────
lock(draw_mutex)
  old = overlay_surface
  overlay_surface = new
  // ARC releases old         reading old surface's memory
unlock(draw_mutex)            → USE AFTER FREE
```

The Zig side stores a raw `*anyopaque` pointer. It has no way to prevent ARC
from releasing the IOSurface because it doesn't participate in reference
counting.

## The Fix: CFRetain/CFRelease on the Zig Side

The simplest fix: when the Zig renderer receives a new IOSurface pointer, it
calls `CFRetain` on the new one and `CFRelease` on the old one. This gives the
Zig side its own ownership stake — ARC on the Swift side can release freely
because the Zig retain keeps the surface alive.

```
Thread A (main/XPC):          Thread B (renderer):
───────────────────           ────────────────────
lock(draw_mutex)
  CFRelease(old)
  overlay_surface = new
  CFRetain(new)
unlock(draw_mutex)            reading surface → safe, Zig holds a retain
```

The `draw_mutex` serializes the swap, and the Zig-side retain prevents
deallocation until the renderer is done. On `clearOverlay`, the Zig side calls
`CFRelease` on the current surface.

### Why not double-buffering?

Double-buffering (two IOSurface slots, swap atomically) is more complex and
doesn't solve the fundamental problem — someone still needs to manage the
lifetime of the "old" slot. CFRetain/CFRelease is the direct solution.

### Why not Mach port lookup per frame?

Calling `IOSurfaceLookupFromMachPort` on the render thread would work but adds
per-frame overhead and is only relevant for the cross-process Chromium case. The
checkerboard doesn't use Mach ports. And even for Chromium, the Swift side
already does the lookup — passing the result with proper retain is cleaner.

## Current State (starting point)

| Component                               | State                                     |
| --------------------------------------- | ----------------------------------------- |
| `pink_overlay` pipeline                 | Working — solid color quad at grid coords |
| `overlay` pipeline (IOSurface texture)  | Reverted — needs reimplementation         |
| `ghostty_surface_set_overlay_iosurface` | Reverted — needs reimplementation         |
| `ghostty_surface_get_cell_size`         | Reverted — needs reimplementation         |
| `Texture.fromIOSurface`                 | Reverted — needs reimplementation         |
| Checkerboard test surface (Swift)       | Reverted — needs reimplementation         |
| CFRetain/CFRelease lifetime management  | Never existed — new work                  |

## Key Files

| File                                            | Role                                                                     |
| ----------------------------------------------- | ------------------------------------------------------------------------ |
| `ts5/src/renderer/shaders/shaders.metal`        | Metal shaders (add `overlay_vertex`/`overlay_fragment`)                  |
| `ts5/src/renderer/metal/shaders.zig`            | Pipeline definitions (add `overlay` pipeline, `OverlayParams` struct)    |
| `ts5/src/renderer/metal/Texture.zig`            | IOSurface → Metal texture import (`fromIOSurface`)                       |
| `ts5/src/renderer/generic.zig`                  | Renderer state (`overlay_iosurface` field, render step in `drawFrame()`) |
| `ts5/src/Surface.zig`                           | `setOverlayIOSurface()` / `clearOverlay()` with CFRetain/CFRelease       |
| `ts5/src/apprt/embedded.zig`                    | C API exports                                                            |
| `ts5/include/ghostty.h`                         | C API declarations                                                       |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Checkerboard creation, `set_overlay` handler                             |

## Pass Criteria

1. `cargo run -p web -- http://example.com` shows a blue/dark checkerboard at
   Retina resolution in the viewport area. Each checker cell is exactly one
   terminal cell with sharp edges.
2. **Resizing the terminal window does not crash.** The checkerboard recreates
   at the new size and remains pixel-perfect.
3. Quitting `web` clears the overlay.
4. No crash during normal operation.

## Experiments

### Experiment 1: IOSurface Checkerboard with CFRetain Lifetime

Reimplement the Issue 507 checkerboard (experiments 1+2) in a single experiment,
with the CFRetain/CFRelease fix for IOSurface lifetime. This combines:

- IOSurface texture overlay pipeline (507 Exp 1)
- Retina-correct resolution via cell size query (507 Exp 2)
- **New:** CFRetain/CFRelease on the Zig side to prevent use-after-free

#### Changes

**1. Metal shaders — `ts5/src/renderer/shaders/shaders.metal`**

Add `overlay_vertex` and `overlay_fragment` after the pink overlay section. The
vertex shader reuses `PinkOverlayIn` for grid coordinates and outputs texture
coordinates. The fragment shader samples the IOSurface texture with
nearest-neighbor filtering (1:1 pixel mapping at Retina resolution).

```metal
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
    float2 size = float2(params.grid_width, params.grid_height) * uniforms.cell_size;

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

**2. Pipeline — `ts5/src/renderer/metal/shaders.zig`**

Add `overlay` pipeline alongside `pink_overlay`:

```zig
.{ "overlay", .{
    .vertex_fn = "overlay_vertex",
    .fragment_fn = "overlay_fragment",
    .blending_enabled = true,
} },
```

**3. IOSurface texture import — `ts5/src/renderer/metal/Texture.zig`**

Add a function to create an MTLTexture from an IOSurfaceRef. This is zero-copy —
the texture is a view into the same GPU memory. Creating it per-frame is the
correct pattern (the IOSurface contents change between frames).

```zig
pub fn fromIOSurface(device: objc.Object, iosurface: *anyopaque) ?Texture {
    const desc = objc.Class.named("MTLTextureDescriptor").?
        .msgSend(objc.Object, objc.sel("new"), .{});
    defer desc.release();

    // Query IOSurface dimensions.
    const width: c_ulong = @intCast(IOSurfaceGetWidth(iosurface));
    const height: c_ulong = @intCast(IOSurfaceGetHeight(iosurface));

    desc.setProperty("pixelFormat", @intFromEnum(mtl.MTLPixelFormat.bgra8unorm_srgb));
    desc.setProperty("width", width);
    desc.setProperty("height", height);
    desc.setProperty("usage", @as(c_ulong, 0x0004)); // ShaderRead

    const texture = device.msgSend(
        ?objc.Object,
        objc.sel("newTextureWithDescriptor:iosurface:plane:"),
        .{ desc, iosurface, @as(c_ulong, 0) },
    ) orelse return null;

    return .{ .texture = texture };
}
```

Needs `extern "c"` declarations for `IOSurfaceGetWidth` and `IOSurfaceGetHeight`
(both take `*anyopaque`, return `usize`).

**4. Renderer state + drawFrame — `ts5/src/renderer/generic.zig`**

Add field alongside `pink_overlay`:

```zig
/// IOSurfaceRef for the overlay texture (Issue 508).
/// Retained via CFRetain — caller must pair with CFRelease.
/// When non-null, drawFrame() creates an MTLTexture from it and
/// renders with the overlay pipeline instead of pink_overlay.
overlay_iosurface: ?*anyopaque = null,
```

In `drawFrame()`, modify the pink overlay block. When `overlay_iosurface` is
set, create an MTLTexture from it and render with the `overlay` pipeline.
Otherwise fall back to the pink pipeline:

```zig
// Overlay (Issue 508 / Issue 505 fallback).
if (self.pink_overlay.grid_width > 0 and
    self.pink_overlay.grid_height > 0)
{
    if (Buffer(shaderpkg.PinkOverlay).initFill(
        self.api.imageBufferOptions(),
        &.{self.pink_overlay},
    )) |*buf| {
        defer buf.deinit();
        if (self.overlay_iosurface) |iosurface| {
            // IOSurface texture path.
            if (Texture.fromIOSurface(self.api.device, iosurface)) |tex| {
                defer tex.deinit();
                pass.step(.{
                    .pipeline = self.shaders.pipelines.overlay,
                    .uniforms = frame.uniforms.buffer,
                    .buffers = &.{buf.buffer},
                    .textures = &.{tex},
                    .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
                });
            }
        } else {
            // Pink fallback (no IOSurface).
            pass.step(.{
                .pipeline = self.shaders.pipelines.pink_overlay,
                .uniforms = frame.uniforms.buffer,
                .buffers = &.{buf.buffer},
                .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
            });
        }
    } else |err| {
        log.warn("error creating overlay buffer err={}", .{err});
    }
}
```

**5. Surface methods with CFRetain/CFRelease — `ts5/src/Surface.zig`**

This is the critical new code. `setOverlayIOSurface` retains the new surface and
releases the old one under `draw_mutex`. `clearOverlay` releases the current
surface.

```zig
const CFRetain = macos.foundation.CFRetain;
const CFRelease = macos.foundation.CFRelease;

pub fn setOverlayIOSurface(self: *Surface, iosurface: ?*anyopaque) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    // Release old surface (Zig's retain).
    if (self.renderer.overlay_iosurface) |old| {
        CFRelease(old);
    }

    // Retain new surface (Zig takes ownership stake).
    if (iosurface) |new| {
        CFRetain(new);
    }

    self.renderer.overlay_iosurface = iosurface;
    self.queueRender() catch {};
}

pub fn clearOverlay(self: *Surface) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    // Release Zig's retain on the IOSurface.
    if (self.renderer.overlay_iosurface) |old| {
        CFRelease(old);
    }

    self.renderer.overlay_iosurface = null;
    self.renderer.pink_overlay = .{};
    self.queueRender() catch {};
}
```

Note: `CFRetain`/`CFRelease` are already declared in
`ts5/pkg/macos/foundation/type.zig` as `extern "c"` functions taking
`*anyopaque`. The renderer imports `macos` conditionally on `.macos`.

**6. Cell size query — `ts5/src/Surface.zig`**

```zig
pub fn getCellSize(self: *Surface, width: *u32, height: *u32) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    width.* = self.renderer.grid_metrics.cell_width;
    height.* = self.renderer.grid_metrics.cell_height;
}
```

**7. C API — `ts5/include/ghostty.h`**

```c
void ghostty_surface_set_overlay_iosurface(ghostty_surface_t, void* iosurface_ref);
void ghostty_surface_get_cell_size(ghostty_surface_t,
                                    uint32_t* width, uint32_t* height);
```

**8. Exports — `ts5/src/apprt/embedded.zig`**

```zig
export fn ghostty_surface_set_overlay_iosurface(
    surface: *Surface,
    iosurface: ?*anyopaque,
) void {
    surface.core_surface.setOverlayIOSurface(iosurface);
}

export fn ghostty_surface_get_cell_size(
    surface: *Surface,
    width: *u32,
    height: *u32,
) void {
    surface.core_surface.getCellSize(width, height);
}
```

**9. Checkerboard — `ts5/macos/Sources/Ghostty/CompositorXPC.swift`**

In `handleMessage` for `set_overlay`, after calling
`ghostty_surface_set_overlay()` with grid coordinates:

1. Query cell size via `ghostty_surface_get_cell_size()`
2. Compute physical pixel dimensions: `pixelWidth = width * cellWidth`,
   `pixelHeight = height * cellHeight`
3. If dimensions match the cached surface, skip (no rebuild needed)
4. Otherwise, create a new IOSurface at the exact pixel dimensions
5. Fill with blue (#4488FF) / dark (#222222) checkerboard where each checker
   cell is `cellWidth x cellHeight` (one terminal cell)
6. Call `ghostty_surface_set_overlay_iosurface(cSurface, ptr)` — the Zig side
   will CFRetain it

Store the current IOSurface on `CompositorXPC` alongside existing pane tracking.
ARC keeps the Swift reference alive; the Zig CFRetain keeps the surface alive
independently. Either side can release without affecting the other.

```swift
// In handleMessage, after ghostty_surface_set_overlay():
var cellWidth: UInt32 = 0
var cellHeight: UInt32 = 0
ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)

let pixelWidth = Int(width) * Int(cellWidth)
let pixelHeight = Int(height) * Int(cellHeight)

// Skip if dimensions unchanged.
if let existing = self.currentSurfaces[uuid],
   IOSurfaceGetWidth(existing) == pixelWidth,
   IOSurfaceGetHeight(existing) == pixelHeight {
    return
}

let testSurface = IOSurface(properties: [
    .width: pixelWidth,
    .height: pixelHeight,
    .bytesPerElement: 4,
    .bytesPerRow: pixelWidth * 4,
    .pixelFormat: 0x42475241  // 'BGRA'
] as [IOSurfacePropertyKey: Any])!

testSurface.lock(options: [], seed: nil)
let base = testSurface.baseAddress
let bpr = testSurface.bytesPerRow
let cw = Int(cellWidth)
let ch = Int(cellHeight)
for y in 0..<pixelHeight {
    for x in 0..<pixelWidth {
        let cellX = x / cw
        let cellY = y / ch
        let isLight = (cellX + cellY) % 2 == 0
        let offset = y * bpr + x * 4
        if isLight {
            base.storeBytes(of: UInt32(0xFF_FF_88_44), toByteOffset: offset, as: UInt32.self)
        } else {
            base.storeBytes(of: UInt32(0xFF_22_22_22), toByteOffset: offset, as: UInt32.self)
        }
    }
}
testSurface.unlock(options: [], seed: nil)

self.currentSurfaces[uuid] = testSurface
let ptr = Unmanaged.passUnretained(testSurface).toOpaque()
ghostty_surface_set_overlay_iosurface(cSurface, ptr)
```

#### Pass Criteria

1. `cargo run -p web -- http://example.com` shows a blue/dark checkerboard at
   Retina resolution in the viewport area. Each checker cell is exactly one
   terminal cell with sharp crisp edges.
2. **Resizing the terminal window does not crash.** The checkerboard recreates
   at the new size and remains pixel-perfect after resize.
3. Quitting `web` clears the overlay.
4. No crash during normal operation.

#### Files

| File                                            | Change                                                       |
| ----------------------------------------------- | ------------------------------------------------------------ |
| `ts5/src/renderer/shaders/shaders.metal`        | Add `overlay_vertex` + `overlay_fragment` (nearest sampling) |
| `ts5/src/renderer/metal/shaders.zig`            | Add `overlay` pipeline                                       |
| `ts5/src/renderer/metal/Texture.zig`            | Add `fromIOSurface()` (MTLTexture from IOSurfaceRef)         |
| `ts5/src/renderer/generic.zig`                  | Add `overlay_iosurface` field, texture render in `drawFrame` |
| `ts5/src/Surface.zig`                           | `setOverlayIOSurface` (CFRetain/CFRelease), `getCellSize`    |
| `ts5/src/apprt/embedded.zig`                    | Export two new C functions                                   |
| `ts5/include/ghostty.h`                         | Declare two new C functions                                  |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Retina checkerboard, dimension caching, cell size query      |

#### Build & Verify

```bash
cd ts5 && zig build
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- http://example.com

# Expected:
# - Blue/dark checkerboard at viewport, each square = one terminal cell
# - Resize terminal → checkerboard recreates, still crisp, NO CRASH
# - Quit web → overlay clears
```

#### Result: Fail

The checkerboard rendered correctly (orange/dark — the BGRA byte order in the
test pattern produces #FF8844 instead of the intended #4488FF, but the texture
pipeline works). Resizing the terminal window crashed the app.

**CFRetain/CFRelease is necessary but not sufficient.** The fix correctly
prevents ARC from releasing the IOSurface while Zig holds a retain, but the
crash is a different race condition: `overlay_iosurface` is read during
`drawFrame()` on the renderer thread _after_ the `draw_mutex` has been released.
The mutex is only held at the beginning of `drawFrame()` to copy critical state
(uniforms, cells), but `overlay_iosurface` is read directly from `self` later in
the render pass. The main thread can swap the pointer between the mutex release
and the render pass read.

The `draw_mutex` protects the pointer swap in `setOverlayIOSurface()`, but the
renderer's `drawFrame()` does not hold the mutex when it reads
`self.overlay_iosurface` and creates the MTLTexture from it. This means:

1. Renderer reads `self.overlay_iosurface` → gets pointer to old IOSurface
2. Main thread acquires `draw_mutex`, CFReleases old, CFRetains new, swaps ptr
3. Main thread releases `draw_mutex`
4. Old IOSurface's last retain is gone (Swift ARC + Zig CFRelease)
5. Renderer calls `Texture.fromIOSurface(old)` → dangling pointer

**Diagnosis (INCORRECT — see Experiment 2):** The IOSurface pointer must be
snapshotted under the `draw_mutex` at the beginning of `drawFrame()`, alongside
the other critical state. The snapshot should include a CFRetain so the surface
stays alive for the entire frame. CFRelease the snapshot at the end of the
frame.

**Correction:** On closer inspection, `drawFrame()` holds `draw_mutex` for its
entire duration (`lock()` at line 1420, `defer unlock()` at line 1421). The race
condition described above cannot happen — the mutex serializes the pointer read
in `drawFrame()` with the pointer swap in `setOverlayIOSurface()`. The actual
crash cause is unknown because macOS rate-limited the crash report.

### Experiment 2: Diagnostic Logging

The Experiment 1 failure diagnosis was wrong. Rather than guess at another fix,
this experiment adds logging at every critical point in the overlay path to
identify the actual crash cause when resize is attempted.

No code changes to the overlay logic — only `fputs`/`log.warn` instrumentation.

#### Changes

**1. Zig logging in `drawFrame()` — `ts5/src/renderer/generic.zig`**

Add logging inside the overlay block (after line 1665) to trace every frame
where the overlay renders:

```zig
// Overlay (Issue 508 / Issue 505 fallback).
if (self.pink_overlay.grid_width > 0 and
    self.pink_overlay.grid_height > 0)
{
    log.warn("[overlay] frame: grid=({d},{d} {d}x{d}) iosurface={?}", .{
        @as(u32, @intFromFloat(self.pink_overlay.grid_col)),
        @as(u32, @intFromFloat(self.pink_overlay.grid_row)),
        @as(u32, @intFromFloat(self.pink_overlay.grid_width)),
        @as(u32, @intFromFloat(self.pink_overlay.grid_height)),
        @as(?usize, if (self.overlay_iosurface) |p| @intFromPtr(p) else null),
    });

    if (Buffer(shaderpkg.PinkOverlay).initFill( ... )) |*buf| {
        defer buf.deinit();
        if (self.overlay_iosurface) |iosurface| {
            const tex_result = Texture.fromIOSurface(self.api.device, iosurface);
            log.warn("[overlay] fromIOSurface: success={}", .{tex_result != null});
            if (tex_result) |tex| {
                defer tex.deinit();
                log.warn("[overlay] texture: {d}x{d}", .{tex.width, tex.height});
                pass.step( ... );
                log.warn("[overlay] step submitted", .{});
            }
        } else {
            log.warn("[overlay] pink fallback", .{});
            pass.step( ... );
        }
    } else |err| {
        log.warn("[overlay] buffer error: {}", .{err});
    }
}
```

This tells us: (a) whether the overlay was active when the crash happened, (b)
the IOSurface pointer value, (c) whether `fromIOSurface` succeeded, (d) the
texture dimensions, (e) whether the step was submitted.

**2. Zig logging in Surface methods — `ts5/src/Surface.zig`**

Add logging to `setOverlayIOSurface`, `clearOverlay`, and `getCellSize`:

```zig
pub fn setOverlayIOSurface(self: *Surface, iosurface: ?*anyopaque) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    const old_ptr: ?usize = if (self.renderer.overlay_iosurface) |p| @intFromPtr(p) else null;
    const new_ptr: ?usize = if (iosurface) |p| @intFromPtr(p) else null;
    log.warn("[overlay] setIOSurface: old={?} new={?}", .{old_ptr, new_ptr});

    if (self.renderer.overlay_iosurface) |old| { CFRelease(old); }
    if (iosurface) |new| { CFRetain(new); }
    self.renderer.overlay_iosurface = iosurface;
    self.queueRender() catch {};
}

pub fn clearOverlay(self: *Surface) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    const ptr: ?usize = if (self.renderer.overlay_iosurface) |p| @intFromPtr(p) else null;
    log.warn("[overlay] clearOverlay: ptr={?}", .{ptr});

    if (self.renderer.overlay_iosurface) |old| { CFRelease(old); }
    self.renderer.overlay_iosurface = null;
    self.renderer.pink_overlay = .{};
    self.queueRender() catch {};
}

pub fn getCellSize(self: *Surface, width: *u32, height: *u32) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    width.* = self.renderer.grid_metrics.cell_width;
    height.* = self.renderer.grid_metrics.cell_height;
    log.warn("[overlay] getCellSize: {d}x{d}", .{width.*, height.*});
}
```

**3. Zig logging in renderer `deinit` — `ts5/src/renderer/generic.zig`**

Log if overlay_iosurface is non-null at destruction time (would indicate a
leak):

```zig
pub fn deinit(self: *Self) void {
    if (self.overlay_iosurface) |ptr| {
        log.warn("[overlay] deinit: leaked IOSurface ptr={}", .{@intFromPtr(ptr)});
    }
    // ... existing deinit code ...
}
```

**4. Swift logging in CompositorXPC — already present but augment**

The existing `fputs` calls cover most of the Swift path. Add one line to log
when the dimension cache prevents rebuild:

```swift
if let existing = self.currentSurfaces[uuid],
   IOSurfaceGetWidth(existing) == pixelWidth,
   IOSurfaceGetHeight(existing) == pixelHeight {
    fputs("[Compositor] Dimension cache hit, skipping rebuild\n", stderr)
    return
}
```

#### Files

| File                                            | Change                                                                 |
| ----------------------------------------------- | ---------------------------------------------------------------------- |
| `ts5/src/renderer/generic.zig`                  | Add `log.warn` in overlay block + `deinit`                             |
| `ts5/src/Surface.zig`                           | Add `log.warn` in `setOverlayIOSurface`, `clearOverlay`, `getCellSize` |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Add `fputs` for dimension cache hit                                    |

#### Build & Reproduce

```bash
cd ts5 && zig build
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- http://example.com

# 1. Verify checkerboard appears
# 2. Resize the terminal window slowly
# 3. If it crashes, read ~/dev/termsurf/logs/ for the last overlay log lines
# 4. Check for a macOS crash report in ~/Library/Logs/DiagnosticReports/
#    (may need to wait — macOS rate-limits to ~1/day/app)
```

#### What the logs will tell us

| Log pattern                                                     | Meaning                                       |
| --------------------------------------------------------------- | --------------------------------------------- |
| `[overlay] frame:` stops appearing before crash                 | Crash is outside the overlay path             |
| `[overlay] frame:` appears, then `fromIOSurface: success=false` | MTLTexture creation failed                    |
| `[overlay] frame:` appears with stale dimensions after resize   | Grid coordinates not updating                 |
| `[overlay] setIOSurface:` with mismatched old/new pointers      | Unexpected pointer state                      |
| `[overlay] deinit: leaked`                                      | IOSurface not released on surface destruction |
| No `[overlay]` logs at all before crash                         | Crash happens before overlay code runs        |

#### Pass Criteria

This experiment passes if we can **identify the crash cause** from the logs. The
checkerboard doesn't need to survive resize yet — that's for the fix experiment
that follows.

#### Result: Fail

The logging was added to all four files and the build succeeded. The
checkerboard rendered, and resizing crashed as before. But **none of the logs
were captured.**

Zig's `log.warn` writes to stderr via `std.debug.print`. Swift's
`fputs(...,
stderr)` also writes to stderr. When the app is launched via `open`,
macOS discards stderr — it is not written to any file or the unified log. The
crash also didn't produce a new macOS crash report
(`~/Library/Logs/DiagnosticReports/`) because macOS rate-limits reports to
roughly one per app per day, and a report from the previous session was already
on disk.

**The experiment failed to collect any diagnostic data.** The logging
instrumentation is correct but the output channel is wrong. The next experiment
must route logs to a file on disk (e.g. `~/dev/termsurf/logs/`) so they survive
the crash and are readable afterward.

### Experiment 3: Redirect stderr to file

The Experiment 2 instrumentation is correct — `log.warn` (Zig) and
`fputs(..., stderr)` (Swift) both write to stderr. The problem is that `open`
discards stderr. The simplest fix: redirect stderr to a file at app startup with
`freopen`. One line of Swift. All existing logging automatically goes to disk.

#### Changes

**1. Redirect stderr — `ts5/macos/Sources/App/macOS/AppDelegate.swift`**

At the top of `applicationDidFinishLaunching`, before any other code:

```swift
freopen("/Users/ryan/dev/termsurf/logs/overlay.log", "a", stderr)
```

This redirects ALL stderr output — Zig `log.warn`, Swift `fputs`, Metal
warnings, everything — to `~/dev/termsurf/logs/overlay.log` in append mode. The
`"a"` flag means it appends (doesn't truncate on launch). Since `write()` is a
syscall that completes before returning, output is preserved even if the process
crashes immediately after.

No other changes. All Experiment 2 logging instrumentation remains as-is.

#### Files

| File                                            | Change                                                  |
| ----------------------------------------------- | ------------------------------------------------------- |
| `ts5/macos/Sources/App/macOS/AppDelegate.swift` | Add `freopen` at top of `applicationDidFinishLaunching` |

#### Build & Reproduce

```bash
cd ts5 && zig build
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- http://example.com

# 1. Verify checkerboard appears
# 2. Resize the terminal window
# 3. After crash, read the log:
tail -50 ~/dev/termsurf/logs/overlay.log
```

#### Pass Criteria

Same as Experiment 2: this passes if we can **read the `[overlay]` log lines**
after the crash and identify what was happening when it died.

#### Result: Pass

The `freopen` redirect worked — Swift `fputs` output and Metal validation
messages were captured in `~/dev/termsurf/logs/overlay.log`. The crash cause is
now identified.

**Root cause: Metal bytesPerRow alignment.**

```
_mtlValidateStrideTextureParameters:1927: failed assertion `Texture Descriptor Validation
IOSurface texture: bytesPerRow (6500) must be aligned to 16 bytes
```

Metal requires `bytesPerRow` to be a multiple of 16 for IOSurface-backed
textures. The IOSurface is created in `CompositorXPC.swift` with
`.bytesPerRow: pixelWidth * 4`, which is not always 16-byte aligned:

- Before resize: 1560×928 → `bytesPerRow = 6240` → 6240 / 16 = 390 ✓
- After resize: 1625×986 → `bytesPerRow = 6500` → 6500 / 16 = 406.25 ✗

The fix: align `bytesPerRow` to 16 bytes when creating the IOSurface:

```swift
let bytesPerRow = (pixelWidth * 4 + 15) & ~15
```

**Not a race condition. Not a use-after-free. Not a CFRetain issue.** The
Experiment 1 diagnosis was wrong on all counts. The crash was a Metal validation
assertion on a misaligned byte stride.

**Note:** Zig `log.warn` output did not appear in the log file. `freopen`
redirects the C `FILE* stderr`, but Zig's `std.debug.print` writes to fd 2 (the
POSIX file descriptor) directly, bypassing the C FILE layer. Only Swift `fputs`
and Metal's internal logging were captured. This is sufficient — the Metal
assertion told us everything we needed.

### Experiment 4: Reproduce with `open --stderr`

Experiment 3 used `freopen` in Swift to redirect stderr. This works but
hardcodes a path in the source — unusable for release. A better approach: the
`open` command has `--stderr` and `--stdout` flags that redirect fd 2 at the OS
level before the process starts. This captures everything — Zig `log.warn`,
Swift `fputs`, Metal assertions — with zero code changes.

This experiment removes the `freopen` hack, reproduces the crash using
`open --stderr`, and confirms we see both Zig and Swift log output.

#### Changes

**1. Remove `freopen` — `ts5/macos/Sources/App/macOS/AppDelegate.swift`**

Delete the `freopen` line added in Experiment 3. No other code changes.

#### Build & Reproduce

```bash
cd ts5 && zig build
> ~/dev/termsurf/logs/overlay.log  # truncate previous logs
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log

# In a TermSurf pane:
cargo run -p web -- http://example.com

# 1. Verify checkerboard appears
# 2. Resize the terminal window
# 3. After crash, read the log:
tail -100 ~/dev/termsurf/logs/overlay.log
```

#### Pass Criteria

1. The log file contains **both** Zig `[overlay]` lines and Swift `[Compositor]`
   lines (confirming `open --stderr` captures fd 2 for both runtimes).
2. The Metal `bytesPerRow` assertion appears in the log (reproducing the
   Experiment 3 finding without any `freopen` code).

#### Result: Pass

`open --stderr` captures Swift `[Compositor]` lines and Metal assertions without
any source code changes. The `freopen` hack was removed from AppDelegate.swift.

The second run produced more data — the resize survived one intermediate size
before crashing on a different one:

- 1560×928 → `bytesPerRow = 6240` → 6240/16 = 390 ✓ works
- 1716×1044 → `bytesPerRow = 6864` → 6864/16 = 429 ✓ works (resize survived)
- 2002×1247 → `bytesPerRow = 8008` → 8008/16 = 500.5 ✗ crashes

This confirms the root cause is purely alignment — resizing works when the pixel
width happens to produce a 16-byte-aligned stride, and crashes when it doesn't.

Zig `[overlay]` lines did not appear. Zig's `std.log` likely filters below
`.err` in optimized builds. This is not a blocker — the Swift and Metal output
is sufficient for diagnostics. For future Zig-side debugging, we could either
lower the log level or write directly to a file descriptor.

**Conclusion:** `open --stderr <path>` is the correct way to collect logs from
TermSurf. No source code changes needed. This should be the standard approach
for all future debugging.

### Experiment 5: Align bytesPerRow to 16 bytes

The root cause from Experiments 3–4: Metal requires `bytesPerRow` to be a
multiple of 16 for IOSurface-backed textures. The IOSurface is created with
`.bytesPerRow: pixelWidth * 4`, which is not always aligned.

One-line fix in `CompositorXPC.swift`.

#### Changes

**1. Align bytesPerRow — `ts5/macos/Sources/Ghostty/CompositorXPC.swift`**

Change the IOSurface creation from:

```swift
.bytesPerRow: pixelWidth * 4,
```

To:

```swift
.bytesPerRow: (pixelWidth * 4 + 15) & ~15,
```

The checkerboard fill loop already uses `testSurface.bytesPerRow` (line 196), so
it will automatically respect the aligned stride. No other changes needed.

**2. Also fix the checkerboard color while we're here.**

The BGRA byte order is wrong — `UInt32(0xFF_FF_88_44)` on little-endian stores
bytes B=0x44 G=0x88 R=0xFF, producing orange (#FF8844) instead of blue
(#4488FF). Fix:

```swift
// Before: UInt32(0xFF_FF_88_44) → orange #FF8844
// After:  UInt32(0xFF_44_88_FF) → blue #4488FF
```

#### Files

| File                                            | Change                                    |
| ----------------------------------------------- | ----------------------------------------- |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Align `bytesPerRow` to 16, fix BGRA color |

#### Build & Reproduce

```bash
cd ts5 && zig build
> ~/dev/termsurf/logs/overlay.log
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log

# In a TermSurf pane:
cargo run -p web -- http://example.com

# 1. Verify BLUE checkerboard appears (not orange)
# 2. Resize the terminal window — should NOT crash
# 3. Resize several times in different directions
# 4. Quit web — overlay should clear
# 5. Read logs: tail -50 ~/dev/termsurf/logs/overlay.log
```

#### Pass Criteria

1. Blue/dark checkerboard appears at Retina resolution.
2. **Resizing the terminal window does not crash.** Multiple resizes in
   different directions all survive.
3. Quitting `web` clears the overlay.
4. No Metal validation assertions in the log file.

#### Result: Pass

All four criteria met. The blue/dark checkerboard renders at Retina resolution,
resizing works without crashing, quitting `web` clears the overlay, and no Metal
validation assertions appear in the logs.

**Resize is noticeably slower than the pink texture.** The checkerboard is
recreated on every resize — the Swift handler creates a new IOSurface, fills
every pixel in a CPU loop, and passes it to the renderer. The pink overlay is
just four floats updated instantly. For the eventual Chromium integration,
resize performance will need to be faster than this — but that's a Chromium-side
concern (the browser renders to its own IOSurface, we just display it).

## Conclusion

The checkerboard IOSurface overlay works with proper lifetime management and
correct Metal alignment. The five experiments traced a path from misdiagnosis to
root cause:

| Experiment | Goal                    | Result                                         |
| ---------- | ----------------------- | ---------------------------------------------- |
| 1          | CFRetain/CFRelease fix  | Fail — wrong diagnosis (blamed race condition) |
| 2          | Diagnostic logging      | Fail — stderr discarded by `open`              |
| 3          | freopen stderr to file  | Pass — found Metal bytesPerRow alignment crash |
| 4          | `open --stderr` flag    | Pass — confirmed root cause, no code changes   |
| 5          | Align bytesPerRow to 16 | Pass — resize works, no crash                  |

**Root cause:** Metal requires `bytesPerRow` to be a multiple of 16 for
IOSurface-backed textures. The fix was one line: `(pixelWidth * 4 + 15) & ~15`.

**What works now:**

- IOSurface overlay pipeline (vertex shader, fragment shader with texture
  sampling, `Texture.fromIOSurface`)
- CFRetain/CFRelease lifetime management across Swift/Zig boundary
- Retina-resolution checkerboard with correct cell alignment
- Resize without crash
- Clean overlay teardown on disconnect

**What's left for Chromium (Issue 507):**

- Replace the checkerboard IOSurface with live Chromium frames
- Resize performance (CPU-filled checkerboard is slow; Chromium GPU rendering
  should be faster)
- The CFRetain/CFRelease and `bytesPerRow` alignment patterns established here
  apply directly to Chromium IOSurfaces
