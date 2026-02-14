# Issue 416: Rust Receiver

## Goal

Reimplement the Issue 414 receiver in Rust. Two hidden Chromium profile servers
send IOSurface frames via XPC to a Rust receiver that composites them side by
side in one wgpu window at 60fps. The result must be identical to Issue 414
Experiment 7 and Issue 415 — same sender binary, same XPC protocol, same visual
output — but with the receiver written entirely in Rust.

## Motivation

WezTerm is written in Rust. If we integrate browser panes into WezTerm instead
of Ghostty, the IOSurface import, wgpu compositing, and XPC handling will live
in Rust code. Issue 415 proved the pipeline works in Swift (for Ghostty). This
issue proves it works in Rust (for WezTerm), so we can make an informed choice
between the two terminal emulators.

Issue 414 proved the architecture in Objective-C++. Issue 415 proved it in
Swift. This issue ports the receiver to Rust, validating that:

1. Rust can receive XPC Mach ports via FFI to the C XPC API
2. Rust can reconstruct IOSurfaces from Mach ports
3. Rust can create wgpu textures from IOSurfaces via the Metal HAL
4. The full pipeline sustains 60fps with Retina quality in Rust

The `termsurf-xpc` crate (already copied to `ts4/termsurf-xpc/`) provides
working Rust XPC bindings, and `cef-rs` has IOSurface-to-wgpu import code. This
issue reuses that proven code in a standalone receiver, separate from WezTerm.

## Chromium branch

Create a new branch `146.0.7650.0-issue-416` in the `termsurf-chromium`
submodule, forked from `146.0.7650.0-issue-414`. Every issue gets its own branch
of Chromium, even when no Chromium changes are expected.

```bash
cd ts4/termsurf-chromium/src
git checkout -b 146.0.7650.0-issue-416 146.0.7650.0-issue-414
```

The sender is unchanged from Issue 414. No Chromium code changes are expected.

## Existing Rust code to reuse

### `ts4/termsurf-xpc/` — XPC crate

Complete XPC FFI bindings with safe wrappers:

- `src/ffi.rs` — Raw `extern "C"` declarations for the C XPC API, IOSurface
  framework, Mach ports, and dispatch queues
- `src/connection.rs` — Safe `XpcConnection` wrapper
- `src/listener.rs` — Safe `XpcListener` wrapper
- `src/dictionary.rs` — Safe `XpcDictionary` wrapper
- `src/iosurface.rs` — `IOSurfaceLookupFromMachPort`, `IOSurfaceCreateMachPort`,
  port deallocation
- `src/block.rs` — Objective-C block helpers via `block2` crate

### `cef-rs/cef/src/osr_texture_import/iosurface.rs` — IOSurface to wgpu

Zero-copy texture import:

- `IOSurfaceImporter::from_mach_port()` — Reconstructs IOSurface from Mach port
- `import_to_wgpu()` — Creates Metal texture via `objc::msg_send!`, wraps as
  wgpu texture via `device.create_texture_from_hal::<Metal>()`
- sRGB format handling (prevents double gamma correction)

### Crate fitness

The `termsurf-xpc` crate's core abstractions (listener, connection, dictionary,
blocks) are architecture-agnostic. ts3 used anonymous endpoints relayed through
a launcher, but the crate supports direct Mach service listeners equally well:
`XpcListener::new_mach_service()` creates a listener with the
`XPC_CONNECTION_MACH_SERVICE_LISTENER` flag, exactly what the receiver needs.

The crate is essential because XPC event handlers are Objective-C blocks. The
`block2` crate integration in `block.rs` handles block creation, memory
management, and lifetime correctness. Writing inline FFI without it would mean
reimplementing ~100 lines of block handling. The IOSurface import code from
cef-rs can be used almost unchanged.

## Project structure

**Folder:** `ts4/two-profiles-rust/`

```
ts4/two-profiles-rust/
├── Cargo.toml                — Rust project manifest
├── build.rs                  — Metal shader compilation (xcrun metal/metallib)
├── src/
│   ├── main.rs               — Entry point, winit event loop
│   ├── xpc.rs                — XPC Mach service listener, message handling
│   ├── renderer.rs           — wgpu render pipeline, viewport compositing
│   └── shaders/
│       └── shaders.metal     — Vertex + fragment shaders (same as Issue 414)
└── com.termsurf.two-profiles-rust.plist  — Launchd agent
```

Cargo manages dependencies. A `build.rs` script compiles the Metal shaders
during build, solving the problem that SPM couldn't (Issue 415 finding #5).

## Launchd service

A new Mach service name: `com.termsurf.two-profiles-rust`. This avoids
conflicting with the existing services:

- `com.termsurf.two-profiles` — Objective-C++ receiver (Issue 414)
- `com.termsurf.two-profiles-swift` — Swift receiver (Issue 415)

All three can coexist for side-by-side comparison.

The plist (`ts4/two-profiles-rust/com.termsurf.two-profiles-rust.plist`):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.two-profiles-rust</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.two-profiles-rust</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/ryan/dev/termsurf/ts4/two-profiles-rust/target/debug/receiver</string>
    </array>
    <key>StandardOutPath</key>
    <string>/Users/ryan/dev/termsurf/logs/two-profiles-rust.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/ryan/dev/termsurf/logs/two-profiles-rust.log</string>
</dict>
</plist>
```

## XPC in Rust

The C XPC API is accessed via FFI. The `termsurf-xpc` crate provides safe
wrappers, but the underlying calls are all `unsafe`. Key mappings:

| C API                                  | Rust usage                                          |
| -------------------------------------- | --------------------------------------------------- |
| `xpc_connection_create_mach_service()` | `unsafe` FFI, returns raw pointer                   |
| `xpc_connection_set_event_handler()`   | Requires `block2` crate for Objective-C blocks      |
| `xpc_connection_resume()`              | `unsafe` FFI                                        |
| `xpc_dictionary_get_string()`          | Returns `*const c_char`, wrap with `CStr::from_ptr` |
| `xpc_dictionary_copy_mach_send()`      | Returns `mach_port_t`                               |
| `xpc_get_type()`                       | Compare with `XPC_TYPE_CONNECTION` etc. via FFI     |

The main difference from Swift: every XPC call is an `unsafe` block. The
`termsurf-xpc` crate's safe wrappers hide most of this, but the wrappers
themselves contain the `unsafe` code.

Connections must be stored in strong references (`Arc<XpcConnection>` or a
`Vec`) to prevent them from being dropped and canceled.

## IOSurface in Rust

```rust
use std::ffi::c_void;

// Raw FFI (from termsurf-xpc ffi.rs)
extern "C" {
    fn IOSurfaceLookupFromMachPort(port: u32) -> *mut c_void;
    fn IOSurfaceGetWidth(surface: *mut c_void) -> usize;
    fn IOSurfaceGetHeight(surface: *mut c_void) -> usize;
    fn CFRelease(cf: *mut c_void);
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
}

unsafe {
    let surface = IOSurfaceLookupFromMachPort(port);
    let width = IOSurfaceGetWidth(surface);
    let height = IOSurfaceGetHeight(surface);
    // When done:
    CFRelease(surface);
    mach_port_deallocate(mach_task_self_, port);
}
```

Unlike Swift, `CFRelease` is required — Rust has no ARC for CoreFoundation
objects.

## Metal texture from IOSurface via wgpu

This is the most complex part. The path is:

```
IOSurface (C object)
    → Metal texture (via objc::msg_send!)
        → wgpu texture (via device.create_texture_from_hal::<Metal>())
```

From cef-rs (`osr_texture_import/iosurface.rs`):

```rust
use objc::runtime::Object;
use objc::msg_send;
use metal::MTLPixelFormat;

// Get raw Metal device from wgpu
let metal_device = unsafe {
    device.as_hal::<wgpu::hal::api::Metal, _, _>(|device| {
        device.unwrap().raw_device().lock()
    })
};

// Create Metal texture descriptor
let descriptor: *mut Object = unsafe {
    msg_send![class!(MTLTextureDescriptor),
        texture2DDescriptorWithPixelFormat: MTLPixelFormat::BGRA8Unorm_sRGB
        width: width as u64
        height: height as u64
        mipmapped: false]
};

// Create Metal texture from IOSurface
let metal_texture: *mut Object = unsafe {
    msg_send![metal_device,
        newTextureWithDescriptor: descriptor
        iosurface: surface
        plane: 0u64]
};

// Wrap as wgpu texture
let wgpu_texture = unsafe {
    device.create_texture_from_hal::<wgpu::hal::api::Metal>(
        hal_texture, &texture_descriptor)
};
```

This is ~50 lines of unsafe Rust vs. 5 lines of safe Swift.

## Window management

The Swift and Objective-C++ receivers use `NSApplication` + `NSWindow` directly.
For Rust, use `winit` for window creation and event loop — this is what WezTerm
uses. `winit` handles the macOS run loop, window creation, and input events.

```rust
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

let event_loop = EventLoop::new();
let window = WindowBuilder::new()
    .with_title("Rust Receiver")
    .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 600.0))
    .build(&event_loop)?;
```

For vsync-driven rendering, `winit` provides `ControlFlow::WaitUntil` or
`RedrawRequested` events. No `CADisplayLink` equivalent needed — just request
redraws when new IOSurfaces arrive.

## Design

### Architecture

Identical to Issues 414 and 415:

```
Profile Server A (hidden)     Profile Server B (hidden)
--session-id=profile-a        --session-id=profile-b
IOSurface 1600x1200 @ 60fps   IOSurface 1600x1200 @ 60fps
        |                             |
        | XPC Mach port               | XPC Mach port
        v                             v
    +--------------------------------------+
    |       Rust Receiver (wgpu)           |
    |  +----------------+----------------+ |
    |  |  Left viewport | Right viewport | |
    |  |  profile-a     | profile-b      | |
    |  |  60fps         | 60fps          | |
    |  +----------------+----------------+ |
    +--------------------------------------+
```

### Module breakdown

**`main.rs`** — Entry point. Starts the XPC listener on a background thread,
creates the winit event loop and window, initializes wgpu, runs the render loop.

**`xpc.rs`** — XPC Mach service listener. Uses `termsurf-xpc` crate or inline
FFI. Accepts multiple peer connections, dispatches `display_surface` messages.
Maps `session_id` to pane index. Sends IOSurfaces to the renderer via a
`Mutex<Option<IOSurfaceRef>>` per pane.

**`renderer.rs`** — wgpu rendering. Creates the device, surface, render
pipeline. Manages two texture slots. Imports IOSurfaces as wgpu textures via the
Metal HAL. Draws two viewports with the fullscreen quad shader.

**`build.rs`** — Compiles `shaders.metal` to `shaders.metallib` using
`xcrun metal` / `xcrun metallib`. Places the output in `OUT_DIR`.

## Dependencies

```toml
[dependencies]
wgpu = "28"
winit = "0.29"
objc = "0.2"
metal = "0.33"
block2 = "0.5"
libc = "0.2"
```

Or, depend on `termsurf-xpc` as a path dependency:

```toml
[dependencies]
termsurf-xpc = { path = "../termsurf-xpc" }
```

## Build and run

```bash
# Build
cd ts4/two-profiles-rust
cargo build

# Load launchd plist
launchctl load ~/dev/termsurf/ts4/two-profiles-rust/com.termsurf.two-profiles-rust.plist

# Start profile servers (same binary as Issue 414)
cd ~/dev/termsurf/ts4/termsurf-chromium/src

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-rust \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-rust \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407 2>&1 &
```

## Success criteria

Identical to Issues 414 and 415:

- Two panes in one window, each showing the spinning blue square
- Different localStorage identity in each pane (profile isolation)
- Both at 60fps sustained for 60+ seconds
- Pixel-perfect Retina quality (1600x1200 IOSurface, sRGB, 1:1 mapping)
- IOSurface transfer via XPC

Plus:

- Receiver written entirely in Rust
- Builds with `cargo build`

## Challenges

### 1. Unsafe FFI surface area

Every XPC call, every IOSurface call, and the Metal texture creation all require
`unsafe` blocks. The Swift receiver does the same work in ~330 lines of safe
code. The Rust receiver will be ~500+ lines, most of it `unsafe`. Bugs in FFI
code manifest as segfaults, not compiler errors.

**Mitigation:** Reuse the `termsurf-xpc` crate's safe wrappers wherever
possible. Isolate new `unsafe` code in `xpc.rs` and `renderer.rs`. Test
thoroughly with one pane before adding the second.

### 2. IOSurface → wgpu texture indirection

Swift calls `device.makeTexture(descriptor:iosurface:plane:)` — one safe method
call. Rust goes through three layers: IOSurface (C FFI) → Metal texture
(`objc::msg_send!`) → wgpu texture (`create_texture_from_hal`). Each layer
crossing is `unsafe` and each has its own error modes.

**Mitigation:** Copy the proven pattern from
`cef-rs/osr_texture_import/
iosurface.rs`. It already handles sRGB formats,
texture usage flags, and the `msg_send!` typing correctly. The original
`transmute` crash has been fixed.

### 3. Objective-C block creation for XPC handlers

XPC event handlers are Objective-C blocks. Rust needs the `block2` crate to
create these. The blocks capture Rust closures, which must be `Send` and have
correct lifetimes. Getting this wrong causes use-after-free or data races.

**Mitigation:** The `termsurf-xpc` crate already handles this correctly. If
writing inline FFI instead, follow the patterns in
`ts3/wezterm-gui/src/termwindow/webview_xpc.rs` (ts3 reference, for the patterns
only).

### 4. CoreFoundation memory management

Unlike Swift (ARC) and Objective-C++ (ARC), Rust has no automatic reference
counting for CF objects. IOSurfaces returned by `IOSurfaceLookupFromMachPort`
follow the Create Rule — the caller owns them and must call `CFRelease`. Missing
a release leaks kernel resources. Double-releasing causes a crash.

**Mitigation:** Use RAII wrappers that call `CFRelease` in `Drop`. The cef-rs
code already has this pattern.

### 5. wgpu version compatibility

The cef-rs code uses wgpu 28, and the `create_texture_from_hal` API has changed
across wgpu versions. If we need a different wgpu version, the HAL code may need
updating.

**Mitigation:** Pin the same wgpu version as the existing codebase (28).

### 6. winit + wgpu event loop

Creating a window with `winit` and rendering with `wgpu` requires coordinating
the event loop, surface configuration, and redraw requests. This is standard
Rust GPU boilerplate but adds ~100 lines that the Swift/ObjC receivers don't
need (they use `NSApplication` directly).

**Mitigation:** Standard pattern, well-documented. Follow winit + wgpu examples.

## Experiments

### Experiment 1: Single-pane Rust receiver

#### Hypothesis

Rust can receive an IOSurface Mach port via the `termsurf-xpc` crate,
reconstruct the IOSurface, create a wgpu texture from it via the Metal HAL
(following the cef-rs pattern), and render it in a winit window at 60fps. If
this works, every Rust-specific unknown is resolved and Experiment 2 just adds
the second pane.

#### Why single-pane first

The architecture is proven from Issues 414 and 415. The question is whether Rust
can do the same work with its unsafe FFI constraints. There are seven
independent unknowns:

1. **Cargo workspace integration** — The `termsurf-xpc` crate uses workspace
   dependencies (`libc.workspace = true`, `block2.workspace = true`). The
   receiver must be added to the existing `ts4/Cargo.toml` workspace, and GPU
   dependencies (`wgpu`, `winit`, `objc`, `metal`) must be added as workspace
   dependencies.

2. **XPC Mach service listener via `termsurf-xpc`** —
   `XpcListener::new_mach_service()` + `block::set_new_connection_handler()` +
   `block::set_event_handler()`. These are proven in ts3 but never used for
   receiving IOSurface frames from a Chromium sender.

3. **IOSurface reconstruction** — `iosurface::lookup_from_mach_port()` from
   `termsurf-xpc`. Must return a valid handle, and manual `CFRelease` must not
   double-free or leak.

4. **IOSurface → Metal → wgpu texture** — The three-layer unsafe pipeline from
   cef-rs: `device.as_hal::<wgpu::wgc::api::Metal>()` →
   `objc::msg_send![device, newTextureWithDescriptor:iosurface:plane:]` →
   `Device::texture_from_raw()` → `device.create_texture_from_hal()`. This is
   the most complex unknown.

5. **winit + wgpu window and render pipeline** — Standard boilerplate. Surface
   creation, adapter/device, bind group layout, render pipeline with WGSL
   shader.

6. **WGSL fullscreen quad shader** — Translate the Metal shader from Issue 414
   to WGSL. Same logic (vertex_id triangle strip, texture sampling) but
   different syntax and bind group model.

7. **Cross-thread IOSurface handoff** — XPC events arrive on a dispatch queue
   thread. Rendering happens on the winit event loop thread. Need a
   `Mutex<Option<*mut c_void>>` for thread-safe handoff, plus `EventLoopProxy`
   to wake the event loop.

If any of these fail, a single-pane setup makes debugging straightforward. Once
all seven work together, adding the second pane is mechanical.

#### Design

##### Design divergences from issue-level plan

During experiment design, three corrections to the issue-level project structure
emerged:

1. **WGSL replaces Metal shaders.** The issue design proposed `shaders.metal`
   with a `build.rs` compiler, but wgpu uses WGSL natively. No `build.rs` needed
   — the shader is loaded at compile time with `include_str!`. This is simpler
   than both the Metal approach (no xcrun) and the SPM approach (which couldn't
   compile .metal files at all).

2. **Workspace integration required.** The `termsurf-xpc` crate uses
   `libc.workspace = true` and `block2.workspace = true`, so it can only be used
   within the `ts4/Cargo.toml` workspace. The receiver must be added as a
   workspace member, and GPU dependencies added as workspace dependencies.

3. **`block2 = "0.6"`** not `"0.5"` — the workspace already pins version 0.6.

##### Workspace setup

Add `two-profiles-rust` to the existing `ts4/Cargo.toml` workspace:

```toml
[workspace]
members = [
    "termsurf-terminal",
    "termsurf-xpc",
    "two-profiles-rust",
]
resolver = "2"

[workspace.dependencies]
libc = "0.2"
block2 = "0.6"
clap = { version = "4.0", features = ["derive"] }
wgpu = "28"
winit = "0.30"
objc = "0.2"
metal = "0.33"
```

The receiver's `Cargo.toml`:

```toml
[package]
name = "two-profiles-rust"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "receiver"
path = "src/main.rs"

[dependencies]
termsurf-xpc = { path = "../termsurf-xpc" }
wgpu.workspace = true
winit.workspace = true
objc.workspace = true
metal.workspace = true
libc.workspace = true
```

##### Project structure (single-pane)

```
ts4/two-profiles-rust/
├── Cargo.toml
├── src/
│   ├── main.rs          — Everything: XPC, IOSurface import, wgpu rendering
│   └── shaders.wgsl     — Fullscreen quad shader (loaded via include_str!)
└── com.termsurf.two-profiles-rust.plist
```

For Experiment 1, all Rust code lives in `main.rs`. If this works, Experiment 2
may split into the multi-file layout (`xpc.rs`, `renderer.rs`).

##### WGSL shader

Translate the Metal shader from Issue 414 to WGSL:

```wgsl
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texcoord: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    var positions = array<vec2f, 4>(
        vec2f(-1.0, -1.0), vec2f(1.0, -1.0),
        vec2f(-1.0,  1.0), vec2f(1.0,  1.0),
    );
    var texcoords = array<vec2f, 4>(
        vec2f(0.0, 1.0), vec2f(1.0, 1.0),
        vec2f(0.0, 0.0), vec2f(1.0, 0.0),
    );
    var out: VertexOutput;
    out.position = vec4f(positions[vid], 0.0, 1.0);
    out.texcoord = texcoords[vid];
    return out;
}

@group(0) @binding(0)
var tex: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(tex, samp, in.texcoord);
}
```

Same logic as the Metal shader: fullscreen quad from `vertex_index`, texture
sampling. The key difference is bind groups (`@group(0) @binding(0/1)`) instead
of Metal's `[[texture(0)]]` / `[[sampler(0)]]`.

##### XPC listener

Using `termsurf-xpc` crate's safe wrappers:

```rust
use termsurf_xpc::{XpcListener, XpcConnection, block};

let listener = XpcListener::new_mach_service(
    "com.termsurf.two-profiles-rust")?;

block::set_new_connection_handler(&listener, move |peer: XpcConnection| {
    eprintln!("[Receiver] Profile server connected");

    block::set_event_handler(&peer, move |result| {
        match result {
            Ok(dict) => handle_message(dict),
            Err(e) => eprintln!("[Receiver] XPC error: {:?}", e),
        }
    });
    peer.resume();

    // Store peer to prevent Drop from canceling the connection.
    PEERS.lock().unwrap().push(peer);
});
listener.resume();
```

The `set_new_connection_handler` callback receives an `XpcConnection` that must
be stored in a strong reference (`Vec`) to prevent `Drop` from canceling it.
Same pattern as the Swift receiver's `gPeers` array.

##### IOSurface message handling

```rust
use termsurf_xpc::{XpcDictionary, iosurface};

fn handle_message(dict: XpcDictionary) {
    let action = dict.get_string("action");
    if action.as_deref() != Some("display_surface") { return; }

    let port = dict.copy_mach_send("iosurface_port");
    if port == 0 { return; }  // MACH_PORT_NULL

    let surface = iosurface::lookup_from_mach_port(port);
    iosurface::deallocate_mach_port(port);

    if let Some(surface) = surface {
        // Release the previous IOSurface before replacing.
        let mut lock = PENDING_SURFACE.lock().unwrap();
        if let Some(old) = lock.take() {
            unsafe { CFRelease(old); }
        }
        *lock = Some(surface);
        drop(lock);

        // Wake the event loop to trigger a redraw.
        if let Some(proxy) = EVENT_PROXY.lock().unwrap().as_ref() {
            proxy.send_event(()).ok();
        }
    }
}
```

Key differences from Swift:

- `CFRelease` is required for the old IOSurface (Rust has no ARC for CF objects)
- Mach port is deallocated via `iosurface::deallocate_mach_port()` (wraps
  `mach_port_deallocate(mach_task_self_, port)`)
- Thread-safe handoff via `Mutex`, event loop wake via `EventLoopProxy`

##### IOSurface → wgpu texture

Adapted from `cef-rs/cef/src/osr_texture_import/iosurface.rs`:

```rust
use std::ffi::c_void;
use metal::{MTLPixelFormat, MTLTextureType, MTLTextureUsage};

extern "C" {
    fn IOSurfaceGetWidth(surface: *const c_void) -> usize;
    fn IOSurfaceGetHeight(surface: *const c_void) -> usize;
}

fn import_iosurface(
    device: &wgpu::Device,
    surface: *mut c_void,
) -> Option<wgpu::Texture> {
    let width = unsafe { IOSurfaceGetWidth(surface) } as u32;
    let height = unsafe { IOSurfaceGetHeight(surface) } as u32;
    if width == 0 || height == 0 { return None; }

    let texture_desc = wgpu::TextureDescriptor {
        label: Some("IOSurface"),
        size: wgpu::Extent3d {
            width, height, depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };

    unsafe {
        // 1. Get Metal device from wgpu HAL
        let hal_device_guard =
            device.as_hal::<wgpu::wgc::api::Metal>();
        let hal_device = hal_device_guard?;
        let raw_device = hal_device.raw_device();

        // 2. Create Metal texture descriptor
        let metal_desc = metal::TextureDescriptor::new();
        metal_desc.set_width(width as u64);
        metal_desc.set_height(height as u64);
        metal_desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
        metal_desc.set_texture_type(MTLTextureType::D2);
        metal_desc.set_usage(MTLTextureUsage::ShaderRead);

        // 3. Create Metal texture from IOSurface
        let device_ref: &metal::DeviceRef = raw_device;
        let desc_ref: &metal::TextureDescriptorRef =
            metal_desc.as_ref();
        let metal_tex: metal::Texture = objc::msg_send![
            device_ref,
            newTextureWithDescriptor:desc_ref
            iosurface:surface
            plane:0usize
        ];

        // 4. Wrap as wgpu HAL texture
        let hal_tex =
            <wgpu::wgc::api::Metal as wgpu::hal::Api>
            ::Device::texture_from_raw(
                metal_tex,
                texture_desc.format,
                MTLTextureType::D2,
                1, // array layers
                1, // mip levels
                wgpu::hal::CopyExtent {
                    width, height, depth: 1,
                },
            );

        // 5. Wrap as wgpu texture
        Some(device.create_texture_from_hal::<
            wgpu::wgc::api::Metal
        >(hal_tex, &texture_desc))
    }
}
```

This is the critical path — five steps of unsafe FFI. The pattern is proven in
cef-rs but has never been tested with `Bgra8UnormSrgb` format (cef-rs uses
`Bgra8Unorm` with sRGB view formats). If colors are wrong, the first thing to
try is `Bgra8Unorm` + `Bgra8UnormSrgb` in `view_formats`.

##### Cross-thread handoff

```rust
use std::sync::Mutex;
use std::sync::OnceLock;
use winit::event_loop::EventLoopProxy;

static PENDING_SURFACE: Mutex<Option<*mut c_void>> = Mutex::new(None);
static EVENT_PROXY: OnceLock<Mutex<EventLoopProxy<()>>> = OnceLock::new();
static PEERS: Mutex<Vec<XpcConnection>> = Mutex::new(Vec::new());
```

When XPC receives a new IOSurface, it stores the raw pointer in
`PENDING_SURFACE` and wakes the event loop via `EVENT_PROXY`. The render
callback grabs the pointer and imports it.

**`Send` safety:** `*mut c_void` is not `Send`. The pointer needs a newtype
wrapper with an unsafe `Send` impl, since IOSurface handles are kernel objects
safe to access from any thread.

**CFRelease responsibility:** The old IOSurface must be released when replaced.
The `handle_message` function releases the previous surface before storing the
new one.

##### winit event loop

```rust
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

let event_loop = EventLoop::<()>::with_user_event().build()?;

let window = WindowBuilder::new()
    .with_title("Rust Receiver")
    .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
    .build(&event_loop)?;

// Store proxy for XPC thread to wake the event loop.
EVENT_PROXY.set(Mutex::new(event_loop.create_proxy())).ok();

// ... wgpu setup (adapter, device, surface, pipeline) ...

event_loop.run(move |event, control_flow| {
    match event {
        Event::UserEvent(()) => {
            window.request_redraw();
        }
        Event::WindowEvent {
            event: WindowEvent::RedrawRequested, ..
        } => {
            render(&device, &wgpu_surface, &queue);
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested, ..
        } => {
            control_flow.exit();
        }
        _ => {}
    }
});
```

The event loop waits until a `UserEvent` arrives (sent by the XPC handler via
`EventLoopProxy`), requests a redraw, and renders on `RedrawRequested`. No
busy-wait — the XPC handler drives the framerate.

**winit API version note:** winit 0.30 uses `EventLoop::run()` with a closure
that receives `(Event, &ActiveEventLoop)`. If winit 0.30 has switched to the
`ApplicationHandler` trait, the code structure changes but the logic is the
same. This will be resolved during implementation.

##### wgpu render pipeline setup

```rust
let bind_group_layout = device.create_bind_group_layout(
    &wgpu::BindGroupLayoutDescriptor {
        label: Some("texture_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float {
                        filterable: true,
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(
                    wgpu::SamplerBindingType::Filtering,
                ),
                count: None,
            },
        ],
    },
);

let pipeline_layout = device.create_pipeline_layout(
    &wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    },
);

let shader = device.create_shader_module(
    wgpu::ShaderModuleDescriptor {
        label: Some("shader"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("shaders.wgsl").into(),
        ),
    },
);

let pipeline = device.create_render_pipeline(
    &wgpu::RenderPipelineDescriptor {
        label: Some("render_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    },
);
```

Each frame, create a bind group with the current texture and sampler, then draw
4 vertices as a triangle strip — same as the Metal shader in Issues 414/415.

##### FPS logging

Same format as Issues 414 and 415:

```
[Receiver] 60 frames (60.0 fps) | IOSurface 1600x1200
```

Frame counter incremented on each XPC message, logged every second via
`std::time::Instant`.

#### Launchd plist

Use the plist from the issue design (`com.termsurf.two-profiles-rust.plist`).
The binary path points to `target/debug/receiver`.

#### Build and run

```bash
# Build (from ts4/ workspace root)
cd ts4
cargo build -p two-profiles-rust

# Load launchd plist
launchctl load ~/dev/termsurf/ts4/two-profiles-rust/com.termsurf.two-profiles-rust.plist

# Start ONE profile server (single pane)
cd ~/dev/termsurf/ts4/termsurf-chromium/src
out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-rust \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &
```

#### Success criteria

- One pane showing the spinning blue square with localStorage identity
- 60fps sustained for 60+ seconds
- Pixel-perfect Retina quality (1600x1200 IOSurface in 800x600 logical window)
- sRGB color correctness (blue square matches Chrome)
- Receiver is Rust — no C/ObjC files, only FFI via `unsafe`
- Builds with `cargo build` from the ts4 workspace
- Uses `termsurf-xpc` crate for XPC handling
- WGSL shader (not Metal shader) via wgpu
- IOSurface → Metal → wgpu texture import via cef-rs pattern

#### What could go wrong

1. **wgpu HAL API incompatibility.** The cef-rs code uses
   `device.as_hal::<wgpu::wgc::api::Metal>()` and `Device::texture_from_raw()`.
   These are unstable internal APIs that change across wgpu versions. If wgpu 28
   has different signatures than when the cef-rs code was last updated, the
   texture import will fail to compile.

2. **`termsurf-xpc` callback lifetime issues.** The `set_new_connection_handler`
   and `set_event_handler` closures capture references that must outlive the XPC
   connection. If the closures or the `XpcConnection` values are dropped
   prematurely, the handlers become dangling pointers.

3. **IOSurface CFRelease mismatch.** `lookup_from_mach_port()` follows the
   Create Rule — the caller owns the returned IOSurface. If we forget
   `CFRelease` when replacing a surface, we leak kernel memory. If we release
   too early, the Metal texture becomes invalid mid-render.

4. **sRGB format mismatch.** The cef-rs code uses `Bgra8Unorm` (non-sRGB) for
   the Metal descriptor and handles sRGB via view formats. The Swift receiver
   used `bgra8Unorm_srgb` (sRGB) for both the Metal texture and pipeline. If the
   formats are wrong, colors will be washed out (double gamma) or too dark (no
   gamma). The fix is to try: (a) `BGRA8Unorm_sRGB` Metal + `Bgra8UnormSrgb`
   wgpu, (b) `BGRA8Unorm` Metal + `Bgra8Unorm` wgpu with sRGB view formats.

5. **winit API version.** winit 0.30 may use the `ApplicationHandler` trait
   instead of the closure-based `EventLoop::run()`. The cef-rs OSR example uses
   winit 0.29. If the APIs are incompatible, pin winit 0.29 instead.

6. **`*mut c_void` is not `Send`.** The IOSurface pointer must cross from the
   XPC dispatch queue thread to the winit event loop thread. Need a newtype
   wrapper with `unsafe impl Send` — a raw `Mutex<Option<*mut c_void>>` will not
   compile because `*mut c_void` is not `Send`.

7. **winit surface format.** wgpu's surface may prefer a different pixel format
   than `Bgra8UnormSrgb`. The render pipeline's output format must match the
   surface's preferred format. Query with
   `surface.get_capabilities(&adapter).formats[0]`.
