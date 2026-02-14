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

## Comparison: Swift vs Rust receiver

| Aspect                 | Swift (Issue 415)              | Rust (this issue)             |
| ---------------------- | ------------------------------ | ----------------------------- |
| Lines of code          | ~330                           | ~500+ (estimated)             |
| Unsafe code            | None                           | ~200 lines in `unsafe` blocks |
| XPC                    | C API via automatic bridging   | C API via FFI + `block2`      |
| IOSurface              | ARC-managed, no manual release | Manual `CFRelease` required   |
| Metal texture          | 1 safe method call             | `objc::msg_send!` (unsafe)    |
| GPU abstraction        | Raw Metal (native)             | wgpu (via Metal HAL)          |
| Window management      | NSApplication (native)         | winit                         |
| Shader compilation     | Manual xcrun (SPM can't)       | build.rs (automated)          |
| Display sync           | CADisplayLink                  | winit redraw events           |
| Existing code to reuse | None (written from scratch)    | termsurf-xpc, cef-rs importer |
| Target integration     | Ghostty (Swift + Zig)          | WezTerm (Rust)                |
