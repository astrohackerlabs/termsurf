+++
status = "closed"
opened = "2026-02-19"
closed = "2026-02-19"
+++

# Issue 602: Pink Texture Overlay

## Goal

Render a pink GPU quad at the grid coordinates specified by `web`, entirely in
Zig. When the user runs `web https://example.com` in a Ghost pane, a pink
rectangle appears at the viewport coordinates. Resize updates the rectangle.
Disconnect clears it.

## Background

Issue 601 proved XPC works from Zig — Ghost can receive `set_overlay` messages
from `web` with grid coordinates, URL, and profile. But it doesn't do anything
with them yet. This issue makes the overlay visible.

In ts5, the pink texture was Issue 505. The overlay pipeline, Surface methods,
and C API were all built in that series (Issues 505–512). But ts5 built
everything in a mix of Swift and Zig. Ghost starts fresh from upstream Ghostty
and builds it all in Zig.

### What Ghost has (from upstream Ghostty)

Ghost inherited upstream Ghostty's renderer, which has no overlay support:

**Shader pipelines** (`ghost/src/renderer/metal/shaders.zig`):

- `bg_color` — full-screen background
- `cell_bg` — cell backgrounds
- `cell_text` — terminal text
- `image` — Kitty image protocol
- `bg_image` — background image

No `pink_overlay` or `overlay` pipeline.

**Render loop** (`ghost/src/renderer/generic.zig`, `drawFrame()`):

1. Background (bg_color or bg_image)
2. Kitty images below backgrounds
3. Cell backgrounds
4. Kitty images below text
5. Cell text
6. Kitty images above text
7. Debug overlay (hyperlink highlights, semantic prompts — not content)
8. Post-processing (custom shaders)

No overlay render step for external content.

**Surface** (`ghost/src/Surface.zig`):

- No pane ID or UUID field
- No overlay state (coordinates, IOSurface)
- No `setOverlay()` / `clearOverlay()` methods
- Identified only by memory address

**Surface management** (`ghost/src/App.zig`):

- `surfaces: ArrayListUnmanaged` — flat list
- Lookup by pointer comparison only (no ID-based lookup)
- `draw_mutex` exists on the renderer for thread-safe state updates

**C API** (`ghost/src/apprt/embedded.zig`):

- No overlay-related exports
- No `ghostty_surface_set_overlay` or similar

**Debug overlay** (`ghost/src/renderer/Overlay.zig`):

- CPU-rendered debug visualization (hyperlink highlights, semantic prompts)
- Renders via z2d to a pixel buffer, displayed as an image layer
- Not suitable for GPU-composited content overlays

### What ts5 built (for reference, not to copy verbatim)

ts5 added these TermSurf-specific pieces across Issues 505–512:

**Metal shaders** (`ts5/src/renderer/shaders/shaders.metal`):

- `pink_overlay_vertex` / `pink_overlay_fragment` — solid hot pink quad
- `overlay_vertex` / `overlay_fragment` — IOSurface texture quad

The pink vertex shader converts grid coordinates to pixel coordinates:

```metal
float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size;
float2 size = float2(params.grid_width, params.grid_height) * uniforms.cell_size;
```

The projection matrix already includes padding, so the shader doesn't add it.

**Pipeline definition** (`ts5/src/renderer/metal/shaders.zig`):

```zig
.{ "pink_overlay", .{
    .vertex_fn = "pink_overlay_vertex",
    .fragment_fn = "pink_overlay_fragment",
    .blending_enabled = false,
} },
```

**Params struct** (`ts5/src/renderer/metal/shaders.zig`):

```zig
pub const PinkOverlay = extern struct {
    grid_col: f32 = 0,
    grid_row: f32 = 0,
    grid_width: f32 = 0,
    grid_height: f32 = 0,
    pixel_width: f32 = 0,
    pixel_height: f32 = 0,
};
```

**Renderer state** (`ts5/src/renderer/generic.zig`):

```zig
pink_overlay: shaderpkg.PinkOverlay = .{},
```

**Surface methods** (`ts5/src/Surface.zig`):

- `setOverlay(col, row, width, height)` — sets grid coordinates under
  `draw_mutex`, queues render
- `clearOverlay()` — zeros coordinates, releases IOSurface, queues render

**C API exports** (`ts5/src/apprt/embedded.zig`):

- `ghostty_surface_set_overlay(surface, col, row, width, height)`
- `ghostty_surface_clear_overlay(surface)`

**Pane ID propagation**: Each surface sets `TERMSURF_PANE_ID` as a UUID in the
shell environment, inherited by child processes including `web`.

### What we need to build

1. **Pane ID on Surface** — UUID field, set during creation, propagated as
   `TERMSURF_PANE_ID` env var to child processes
2. **Surface lookup by pane ID** — find a Surface from a UUID string
3. **Pink overlay shader** — vertex + fragment in `shaders.metal`
4. **Pipeline definition** — add `pink_overlay` to `shaders.zig`
5. **Overlay params struct** — grid coordinates in `shaders.zig`
6. **Overlay state on renderer** — params field in `generic.zig`
7. **Render step in drawFrame()** — draw the pink quad after text/images
8. **Surface methods** — `setOverlay()` / `clearOverlay()` with `draw_mutex`
9. **Wire XPC to Surface** — `handleSetOverlay` looks up surface, calls
   `setOverlay()`; disconnect calls `clearOverlay()`

### Key technical details from ts5

**Grid-to-pixel conversion**: The projection matrix includes padding. The vertex
shader multiplies grid coordinates by `uniforms.cell_size` to get pixel
position. No padding adjustment needed in the shader.

**Thread safety**: XPC callbacks arrive on a background queue. `setOverlay()`
locks `draw_mutex` before writing coordinates. `drawFrame()` holds `draw_mutex`
during rendering. This serializes access.

**Resize**: Cell size is determined by font metrics and doesn't change on
terminal resize. Grid dimensions and padding change. The `web` TUI sends a new
`set_overlay` message with updated coordinates on resize. The overlay position
stays correct because it's derived from cell size (stable) and grid position
(updated by `web`).

## Ideas for experiments

1. **Pane ID and surface lookup** — Add UUID to Surface, propagate as env var,
   implement lookup by pane ID. Proves the XPC handler can find the right
   surface.

2. **Pink overlay rendering** — Add the shader, pipeline, renderer state, and
   render step. Wire `handleSetOverlay` to call `setOverlay()` on the looked-up
   surface. Pink rectangle appears at the correct grid coordinates.

3. **Resize and cleanup** — Verify resize updates the rectangle dimensions and
   disconnect clears it.

## Experiments

### Experiment 1: Pane ID and surface lookup

#### Goal

Each Surface gets a UUID pane ID. The shell inherits it as `TERMSURF_PANE_ID`.
When `web` sends `set_overlay` with a `pane_id`, Ghost looks up the matching
surface and logs success. Proves the full lookup path works end-to-end before
adding any rendering.

#### Changes

##### `ghost/src/Surface.zig`

Add a `pane_id` field — a 36-byte null-terminated UUID string (e.g.
`"9F96D529-1234-5678-ABCD-EF0123456789"`).

macOS ships `uuid_generate` and `uuid_unparse_upper` in `<uuid/uuid.h>`. Declare
them as `extern "c"`:

```zig
const uuid_t = [16]u8;
extern "c" fn uuid_generate(out: *uuid_t) void;
extern "c" fn uuid_unparse_upper(uu: *const uuid_t, out: *[37]u8) void;
```

Add the field to the Surface struct:

```zig
pane_id: [36:0]u8 = undefined,
```

In `init()`, generate the UUID early (before the env block at line 616):

```zig
var uuid: uuid_t = undefined;
uuid_generate(&uuid);
uuid_unparse_upper(&uuid, &self.pane_id);
```

Then inside the env block (after line 626, `env.remove("GHOSTTY_LOG")`), inject
the pane ID into the environment so the shell inherits it:

```zig
env.put("TERMSURF_PANE_ID", &self.pane_id);
```

`env` is a `std.process.EnvMap`. The `put` method copies the value, so the stack
reference is fine.

##### `ghost/src/App.zig`

Add a public lookup method:

```zig
pub fn findSurfaceByPaneId(
    self: *App,
    pane_id: []const u8,
) ?*apprt.Surface {
    for (self.surfaces.items) |surface| {
        if (std.mem.eql(u8, &surface.core().pane_id, pane_id))
            return surface;
    }
    return null;
}
```

This iterates the flat `surfaces` list and compares the `pane_id` field. With a
handful of surfaces this is fine — no hash map needed.

##### `ghost/src/apprt/xpc.zig`

Accept a `*CoreApp` in `init()` and store it as module-level state:

```zig
const CoreApp = @import("../App.zig");
var app: *CoreApp = undefined;

pub fn init(core_app: *CoreApp) void {
    app = core_app;
    // ... rest of init
}
```

In `handleSetOverlay`, after logging, look up the surface:

```zig
if (app.findSurfaceByPaneId(pane_id)) |surface| {
    _ = surface;
    log.info("surface found for pane={s}", .{pane_id});
} else {
    log.warn("no surface found for pane={s}", .{pane_id});
}
```

##### `ghost/src/apprt/embedded.zig`

Update the `xpc.init()` call in `App.init()` to pass `core_app`:

```zig
xpc.init(core_app);
```

#### Key unknowns

1. Does `uuid_generate` / `uuid_unparse_upper` link without explicit framework
   flags? These are in libSystem on macOS, so they should be available
   automatically.
2. Does `env.put` accept a `*[36:0]u8`? It expects `[]const u8` — the sentinel
   array should coerce. If not, use `std.mem.span(&self.pane_id)`.

#### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
```

In a Ghost pane:

```bash
echo $TERMSURF_PANE_ID   # Should print a UUID
cargo run -p web -- https://example.com
```

Pass: Ghost logs show `surface found for pane=<UUID>` where the UUID matches
`$TERMSURF_PANE_ID` from the shell. `echo $TERMSURF_PANE_ID` prints a valid UUID
in every new pane.

#### Result

Pass. Surface lookup works end-to-end:

```
info(xpc): set_overlay pane=83CA54D2-BBA2-4B7B-A703-12FAE6A59888 col=1 row=4 width=120 height=32 url=https://example.com profile=default browsing=true
info(xpc): surface found for pane=83CA54D2-BBA2-4B7B-A703-12FAE6A59888
```

Both key unknowns resolved:

1. **`uuid_generate` / `uuid_unparse_upper` link automatically** — no framework
   flags needed. They're in libSystem on macOS.
2. **`env.put` needs explicit coercion** — `*[36:0]u8` doesn't coerce directly
   to `[]const u8`. Used `std.mem.span(@as([*:0]const u8, &self.pane_id))` to
   convert the sentinel-terminated array to a slice.

#### Files changed

| File                           | Change                                        |
| ------------------------------ | --------------------------------------------- |
| `ghost/src/Surface.zig`        | UUID field, generation, env propagation       |
| `ghost/src/App.zig`            | `findSurfaceByPaneId()` lookup method         |
| `ghost/src/apprt/xpc.zig`      | Accept `*CoreApp`, look up surface on overlay |
| `ghost/src/apprt/embedded.zig` | Pass `core_app` to `xpc.init()`               |

### Experiment 2: Pink overlay rendering

#### Goal

When `web` sends `set_overlay`, a hot pink rectangle appears at the correct grid
coordinates in the Ghost pane. Proves the full GPU pipeline from XPC message to
rendered quad.

#### Changes

##### `ghost/src/renderer/shaders/shaders.metal`

Add a params struct and two shader functions at the end of the file:

```metal
#pragma mark - Pink Overlay

struct PinkOverlayIn {
  float grid_col;
  float grid_row;
  float grid_width;
  float grid_height;
};

vertex float4 pink_overlay_vertex(
  uint vid [[vertex_id]],
  constant PinkOverlayIn& params [[buffer(0)]],
  constant Uniforms& uniforms [[buffer(1)]]
) {
  float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size;
  float2 size = float2(params.grid_width, params.grid_height) * uniforms.cell_size;

  float2 corner;
  corner.x = float(vid == 1 || vid == 3);
  corner.y = float(vid == 2 || vid == 3);

  float2 pos = origin + size * corner;
  return uniforms.projection_matrix * float4(pos, 0.0f, 1.0f);
}

fragment float4 pink_overlay_fragment() {
  return float4(1.0, 0.41, 0.71, 1.0);
}
```

The vertex shader converts grid coordinates to physical pixels by multiplying by
`uniforms.cell_size`. The projection matrix (which includes padding) transforms
to NDC. Four vertices form a triangle strip quad. The fragment shader returns
hot pink.

##### `ghost/src/renderer/metal/shaders.zig`

Add the pipeline to `pipeline_descs` (after bg_image):

```zig
.{ "pink_overlay", .{
    .vertex_fn = "pink_overlay_vertex",
    .fragment_fn = "pink_overlay_fragment",
    .blending_enabled = false,
} },
```

No `vertex_attributes` — the params are passed as a raw buffer at index 0, not
as per-instance vertex attributes.

Add the params struct (after the existing `BgImage` struct):

```zig
pub const PinkOverlay = extern struct {
    grid_col: f32 = 0,
    grid_row: f32 = 0,
    grid_width: f32 = 0,
    grid_height: f32 = 0,
};
```

##### `ghost/src/renderer/generic.zig`

Add a field on the renderer struct (after the existing config/state fields):

```zig
pink_overlay: shaderpkg.PinkOverlay = .{},
```

Add a render step in `drawFrame()` after kitty images above text and before the
debug overlay. The step creates a temporary buffer each frame, fills it with the
params, and draws a triangle strip quad:

```zig
// Pink overlay (Issue 602).
if (self.pink_overlay.grid_width > 0 and
    self.pink_overlay.grid_height > 0)
{
    if (Buffer(shaderpkg.PinkOverlay).initFill(
        self.api.imageBufferOptions(),
        &.{self.pink_overlay},
    )) |*buf| {
        defer buf.deinit();
        pass.step(.{
            .pipeline = self.shaders.pipelines.pink_overlay,
            .uniforms = frame.uniforms.buffer,
            .buffers = &.{buf.buffer},
            .draw = .{
                .type = .triangle_strip,
                .vertex_count = 4,
            },
        });
    } else |_| {}
}
```

`Buffer(PinkOverlay).initFill` creates a Metal buffer from the struct on the CPU
side. `imageBufferOptions()` returns the device and storage mode. The buffer is
released after the draw call via `defer`.

##### `ghost/src/Surface.zig`

Add two public methods:

```zig
pub fn setOverlay(self: *Surface, col: u32, row: u32, width: u32, height: u32) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    self.renderer.pink_overlay = .{
        .grid_col = @floatFromInt(col),
        .grid_row = @floatFromInt(row),
        .grid_width = @floatFromInt(width),
        .grid_height = @floatFromInt(height),
    };
    self.queueRender() catch {};
}

pub fn clearOverlay(self: *Surface) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    self.renderer.pink_overlay = .{};
    self.queueRender() catch {};
}
```

Both lock `draw_mutex` to serialize with `drawFrame()`. XPC callbacks arrive on
a background queue, so thread safety is critical.

##### `ghost/src/apprt/xpc.zig`

Store the matched surface alongside the peer connection:

```zig
var overlay_surface: ?*CoreSurface = null;
```

In `handleSetOverlay`, after finding the surface, call `setOverlay()`:

```zig
if (app.findSurfaceByPaneId(pane_id)) |surface| {
    overlay_surface = surface.core();
    surface.core().setOverlay(
        @intCast(col), @intCast(row),
        @intCast(width), @intCast(height),
    );
    log.info("surface found for pane={s}", .{pane_id});
} else {
    log.warn("no surface found for pane={s}", .{pane_id});
}
```

In `peerHandler` on disconnect, clear the overlay:

```zig
if (overlay_surface) |surface| {
    surface.clearOverlay();
    overlay_surface = null;
}
```

`CoreSurface` is `@import("../Surface.zig")` — the core Surface type that has
`setOverlay` / `clearOverlay`.

#### Key unknowns

1. Does `Buffer(PinkOverlay).initFill` work with a non-vertex-attribute struct?
   The buffer is just raw bytes — the shader reads it as
   `constant PinkOverlayIn&` at buffer index 0. Should work since it's the same
   as how bg_image passes its params.
2. Does the pipeline auto-initialize without `vertex_attributes`? The bg_color
   pipeline also has no vertex attributes and works fine.

#### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
```

In a Ghost pane:

```bash
cargo run -p web -- https://example.com
```

Pass: A hot pink rectangle appears at the viewport coordinates (col=1, row=4,
filling the viewport area). The rectangle position matches the `web` TUI's
viewport border. Exiting `web` clears the rectangle. Resizing the terminal
causes `web` to send updated coordinates and the rectangle adjusts.

#### Result

Pass. Pink overlay renders at correct grid coordinates, tracks resize, and
clears on disconnect:

```
info(xpc): overlay set for pane=AE187CA0-A0AD-497F-A883-0F5999C475D5
info(xpc): set_overlay pane=AE187CA0-... col=1 row=4 width=168 height=42 ...
info(xpc): overlay set for pane=AE187CA0-...
info(xpc): set_overlay pane=AE187CA0-... col=1 row=4 width=120 height=35 ...
info(xpc): overlay set for pane=AE187CA0-...
info(xpc): mode_changed pane=AE187CA0-... browsing=false
info(xpc): peer disconnected
```

Resize produced six `set_overlay` messages as the terminal width changed
(168→201→198→137→121→120). Each update repositioned the pink quad correctly.
Disconnect cleared the overlay.

Both key unknowns resolved:

1. **`Buffer(PinkOverlay).initFill` works with a non-vertex-attribute struct** —
   the shader reads it as `constant PinkOverlayIn&` at buffer index 0. Same
   pattern as bg_image.
2. **Pipeline auto-initializes without `vertex_attributes`** — same as bg_color.

#### Files changed

| File                                       | Change                                    |
| ------------------------------------------ | ----------------------------------------- |
| `ghost/src/renderer/shaders/shaders.metal` | Pink overlay vertex + fragment shaders    |
| `ghost/src/renderer/metal/shaders.zig`     | Pipeline definition + PinkOverlay struct  |
| `ghost/src/renderer/generic.zig`           | Renderer field + render step in drawFrame |
| `ghost/src/Surface.zig`                    | `setOverlay()` / `clearOverlay()` methods |
| `ghost/src/apprt/xpc.zig`                  | Wire XPC to surface, clear on disconnect  |

## Conclusion

Pink overlay works end-to-end in Zig. A hot pink GPU quad renders at the grid
coordinates specified by `web`, survives terminal resize, and clears on
disconnect — all without any Swift involvement.

The implementation required two experiments across nine files:

1. **Pane ID and surface lookup** (Experiment 1) — UUID on each Surface,
   propagated as `TERMSURF_PANE_ID` to the shell, looked up by the XPC handler
   when `web` sends `set_overlay`.

2. **Pink overlay rendering** (Experiment 2) — Metal vertex/fragment shaders,
   pipeline definition, renderer state, `drawFrame()` render step, and
   thread-safe `setOverlay()` / `clearOverlay()` methods on Surface.

Key patterns established for future overlay work:

- **Grid-to-pixel conversion** happens in the vertex shader:
  `grid_coord * uniforms.cell_size`. The projection matrix handles padding.
- **Per-frame buffer creation** via `Buffer(T).initFill` passes params to the
  shader without vertex attributes. Created and released each frame.
- **Thread safety** via `draw_mutex` — XPC callbacks lock it before writing
  overlay state, `drawFrame()` holds it during rendering.
- **Resize** is automatic — `web` sends new grid coordinates on terminal resize,
  and the vertex shader recomputes pixel positions from the updated values.

### Files changed

| File                                       | Change                                    |
| ------------------------------------------ | ----------------------------------------- |
| `ghost/src/Surface.zig`                    | UUID field, `setOverlay`, `clearOverlay`  |
| `ghost/src/App.zig`                        | `findSurfaceByPaneId()` lookup            |
| `ghost/src/apprt/xpc.zig`                  | App ref, surface lookup, overlay wiring   |
| `ghost/src/apprt/embedded.zig`             | Pass `core_app` to `xpc.init()`           |
| `ghost/src/renderer/shaders/shaders.metal` | Pink overlay vertex + fragment shaders    |
| `ghost/src/renderer/metal/shaders.zig`     | Pipeline definition + PinkOverlay struct  |
| `ghost/src/renderer/generic.zig`           | Renderer field + render step in drawFrame |

### What's next

Issue 603+ will add the IOSurface overlay pipeline — replacing the pink quad
with live Chromium frames streamed via Mach port transfer over XPC.
