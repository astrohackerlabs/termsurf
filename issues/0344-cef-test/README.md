+++
status = "closed"
opened = "2026-02-07"
closed = "2026-02-07"
+++

# Issue 344: cef-test — Minimal Multi-Process CEF Test Harness

## Goal

Build a minimal, standalone test application that loads two CEF browsers side by
side in a single window, each running in a separate process with a separate
profile, communicating via XPC Mach port transfer. No terminal emulator, no
WezTerm, no pane management — just the core product requirement stripped to its
essence.

This isolates the single architectural variable that separates the working
cef-rs OSR example (60fps) from the struggling ts3 profile server (38fps):
**multi- process CEF with cross-process IOSurface sharing.**

## Why This Matters

Eight experiments in [Issue 343](./343-optimal-performance.md) failed to improve
the profile server's frame rate. The problem is clear (`do_message_loop_work()`
takes >1ms on 100% of calls in the headless profile server vs 5.7% in the
windowed cef-rs example), but the cause is buried under layers of ts3
complexity: WezTerm's event loop, the launcher lifecycle, terminal multiplexing,
pane management, the web command flow, and more.

cef-test eliminates all of that. If the performance problem reproduces in
cef-test, the root cause is inherent to the multi-process/headless architecture
and we can iterate here 10x faster. If it doesn't, the root cause is in ts3's
integration and we know where to look.

## Architecture Overview

### Process Topology

```
cef-test-gui (single window, wgpu rendering)
    │
    ├── Connects to cef-test-launcher (Mach service bootstrap)
    │
    ├── Requests profile "left" → launcher spawns cef-test-profile
    │   │
    │   └── Profile "left" ←─ XPC direct ──→ GUI
    │       (headless CEF, github.com)     (receives Mach ports)
    │
    └── Requests profile "right" → launcher spawns cef-test-profile
        │
        └── Profile "right" ←─ XPC direct ──→ GUI
            (headless CEF, google.com)     (receives Mach ports)
```

### Data Flow

```
Profile Server (headless CEF)                GUI (windowed)
─────────────────────────────                ──────────────
CEF renders to IOSurface
    │
    ▼
on_accelerated_paint callback
    │
    ▼
IOSurfaceCreateMachPort(handle)
    │
    ▼
XPC send: {                          ──▶    XPC receive
  action: "display_surface",                    │
  iosurface_port: <mach_port>,                  ▼
  width, height                          IOSurfaceLookupFromMachPort(port)
}                                               │
                                                ▼
                                         Metal: newTexture(iosurface:)
                                                │
                                                ▼
                                         wgpu bind group + render pass
                                                │
                                                ▼
                                         Draw to left or right half
                                                │
                                                ▼
                                         surface.present()


GUI (windowed)                           Profile Server
──────────────                           ──────────────
winit captures mouse/key event
    │
    ▼
Determine target (left/right)
based on cursor position
    │
    ▼
Translate coordinates to                 XPC receive
local profile space              ──▶         │
    │                                        ▼
XPC send: {                          CEF host.send_mouse_move_event()
  action: "mouse_move",             CEF host.send_key_event()
  x, y, modifiers                   etc.
}
```

### Why a Launcher is Required

XPC endpoints are opaque kernel objects that can only be transferred over
existing XPC connections. To establish the first connection between the GUI and
a profile server, both processes need a shared bootstrap point. A named Mach
service (registered with launchd) serves this role:

1. GUI connects to the launcher's named service
2. GUI creates an anonymous XPC listener, sends the endpoint to the launcher
3. Launcher spawns a profile server process
4. Profile server connects to the launcher, claims the endpoint
5. Profile server connects directly to the GUI via the endpoint
6. All further communication is direct GUI ↔ Profile (launcher not involved)

This is identical to ts3's pattern, proven to work. The launcher itself is ~150
lines — trivial plumbing, not complexity.

## Binaries

### cef-test-gui

The windowed process. Creates a single window, renders two browser textures side
by side, captures input, routes it to the correct profile server.

**Responsibilities:**

- Create a winit window (1600x800 logical, 3200x1600 physical on Retina)
- Initialize wgpu with Metal backend
- Connect to the launcher Mach service
- For each browser slot (left, right):
  - Create an anonymous XPC listener
  - Send the listener's endpoint + metadata to the launcher
  - Receive the profile server's direct connection
  - Receive IOSurface Mach ports from the profile server
  - Import IOSurface → Metal texture → wgpu texture → bind group
- Run the event loop:
  - `pump_app_events` (winit) — process window events
  - On `RedrawRequested`: render both textures to their respective halves
  - On mouse/key events: route to the correct profile server via XPC
- Log per-frame timing for performance measurement

**Does NOT run CEF.** No `do_message_loop_work()`, no CEF initialization. The
GUI is purely a renderer and input dispatcher.

### cef-test-profile

The headless CEF process. One instance per browser profile. Renders web pages
off-screen and sends IOSurface Mach ports to the GUI.

**Responsibilities:**

- Parse CLI args (session-id, url, profile, width, height, scale)
- Load CEF framework, run subprocess check
- Connect to launcher, claim the GUI endpoint for its session
- Connect directly to the GUI via the endpoint
- Initialize CEF with:
  - `windowless_rendering_enabled: true`
  - `shared_texture_enabled: true`
  - `root_cache_path: ~/.config/cef-test/{profile}/`
  - `windowless_frame_rate: 60`
- Create a render handler that:
  - On `on_accelerated_paint`: create Mach port from IOSurface, send to GUI
  - On `view_rect`: return stored width/height
  - On `screen_info`: return device_scale_factor
- Receive input events from GUI via XPC:
  - `mouse_move`, `mouse_click`, `mouse_wheel` → forward to CEF browser host
  - `key_event` → forward to CEF browser host
  - `resize` → update browser size
  - `focus` → set/kill browser focus
- Run the message loop: `do_message_loop_work()` + `cfrunloop::run_for(0.001)`
- Log `[FRAME-TX]` timing for performance measurement

**Matches ts3's profile server** in message loop structure and CEF
configuration. This is deliberate — we want to reproduce the same performance
characteristics so we can experiment from there.

### cef-test-launcher

The bootstrap service. Forwards XPC endpoints between the GUI and profile
servers. Exits when the GUI disconnects.

**Responsibilities:**

- Register as Mach service `com.cef-test.launcher`
- Handle `spawn_profile`:
  - Store GUI endpoint by session-id
  - Spawn `cef-test-profile` with CLI args
- Handle `claim_session`:
  - Look up and return stored GUI endpoint
- Handle `register_profile`:
  - Store profile connection for reuse (same profile, second browser)
- Exit when GUI connection closes

This is a simplified version of ts3's `termsurf-launcher` (~150 lines). The
simplifications:

- No multi-GUI support (single GUI connection)
- No crash recovery
- No log redirection complexity

## XPC Protocol

### Bootstrap Flow

```
GUI                          Launcher                     Profile
 │                              │                            │
 │── connect ──────────────────▶│                            │
 │                              │                            │
 │── spawn_profile ────────────▶│                            │
 │   {session_id, url,          │                            │
 │    profile, width, height,   │                            │
 │    scale, gui_endpoint}      │── spawn process ──────────▶│
 │                              │                            │
 │                              │◀────── connect ────────────│
 │                              │                            │
 │                              │◀── claim_session ──────────│
 │                              │   {session_id}             │
 │                              │                            │
 │                              │── reply ──────────────────▶│
 │                              │   {endpoint}               │
 │                              │                            │
 │◀───────── XPC direct connection (via endpoint) ──────────▶│
 │                              │                            │
 │◀── display_surface ──────────────────────────────────────│
 │   {iosurface_port, w, h}                                  │
 │                                                           │
 │── mouse_move ────────────────────────────────────────────▶│
 │   {x, y, modifiers}                                      │
```

### Messages: Profile → GUI

**display_surface** — sent on every CEF frame

| Field            | Type      | Description                         |
| ---------------- | --------- | ----------------------------------- |
| `action`         | string    | `"display_surface"`                 |
| `iosurface_port` | mach_send | IOSurface Mach port (set_mach_send) |
| `width`          | i64       | Physical pixel width                |
| `height`         | i64       | Physical pixel height               |

### Messages: GUI → Profile

**mouse_move**

| Field       | Type   | Description                     |
| ----------- | ------ | ------------------------------- |
| `action`    | string | `"mouse_move"`                  |
| `x`         | i64    | Logical x (relative to profile) |
| `y`         | i64    | Logical y (relative to profile) |
| `modifiers` | i64    | CEF modifier flags              |

**mouse_click**

| Field         | Type   | Description                        |
| ------------- | ------ | ---------------------------------- |
| `action`      | string | `"mouse_click"`                    |
| `x`           | i64    | Logical x (relative to profile)    |
| `y`           | i64    | Logical y (relative to profile)    |
| `button`      | i64    | 0=left, 1=middle, 2=right          |
| `is_up`       | i64    | 1 if button released, 0 if pressed |
| `click_count` | i64    | 1 for single, 2 for double-click   |
| `modifiers`   | i64    | CEF modifier flags                 |

**mouse_wheel**

| Field       | Type   | Description        |
| ----------- | ------ | ------------------ |
| `action`    | string | `"mouse_wheel"`    |
| `x`         | i64    | Logical cursor x   |
| `y`         | i64    | Logical cursor y   |
| `delta_x`   | i64    | Horizontal scroll  |
| `delta_y`   | i64    | Vertical scroll    |
| `modifiers` | i64    | CEF modifier flags |

**key_event**

| Field         | Type   | Description                |
| ------------- | ------ | -------------------------- |
| `action`      | string | `"key_event"`              |
| `key_is_down` | i64    | 1 for keydown, 0 for keyup |
| `key_type`    | i64    | CEF key event type         |
| `raw_code`    | i64    | Native macOS key code      |
| `char_code`   | i64    | Unicode character          |
| `shift`       | i64    | Shift modifier             |
| `ctrl`        | i64    | Control modifier           |
| `alt`         | i64    | Alt/Option modifier        |
| `meta`        | i64    | Command modifier           |

**resize**

| Field    | Type   | Description         |
| -------- | ------ | ------------------- |
| `action` | string | `"resize"`          |
| `width`  | i64    | Logical width       |
| `height` | i64    | Logical height      |
| `scale`  | string | Device scale factor |

**focus**

| Field     | Type   | Description             |
| --------- | ------ | ----------------------- |
| `action`  | string | `"focus"`               |
| `focused` | i64    | 1 for focus, 0 for blur |

## Window Layout & Rendering

### Layout

Single window, 1600x800 logical pixels (3200x1600 physical on Retina):

```
┌──────────────────────┬──────────────────────┐
│                      │                      │
│    Profile "left"    │   Profile "right"    │
│    (github.com)      │   (google.com)       │
│                      │                      │
│    800 x 800 logical │   800 x 800 logical  │
│                      │                      │
│                      │                      │
└──────────────────────┴──────────────────────┘
                1600 x 800 logical
```

Each profile server sees an 800x800 logical viewport (1600x1600 physical on
Retina). The GUI receives two independent IOSurface textures and composites them
side by side.

### Rendering Pipeline

Two draw calls per frame, one per browser. Each draw call renders a fullscreen
quad mapped to half the window:

**Left quad vertices (NDC):**

```
(-1, +1) → (0, 0)    // top-left of window
( 0, +1) → (1, 0)    // top-center of window
(-1, -1) → (0, 1)    // bottom-left of window
( 0, -1) → (1, 1)    // bottom-center of window
```

**Right quad vertices (NDC):**

```
( 0, +1) → (0, 0)    // top-center of window
(+1, +1) → (1, 0)    // top-right of window
( 0, -1) → (0, 1)    // bottom-center of window
(+1, -1) → (1, 1)    // bottom-right of window
```

Same shader, same pipeline, same sampler. Different vertex buffer and different
bind group (different texture) per draw call. This matches the cef-rs OSR
example's rendering approach — a pass-through fragment shader sampling from the
CEF texture.

### wgpu Setup

Identical to the cef-rs OSR example:

- Metal backend
- Bgra8UnormSrgb surface format (sRGB fix from cef-rs)
- Linear sampler, clamp to edge
- TriangleStrip topology, 4 vertices per quad
- Alpha blending (Over)

## Input Routing

### Focus Model

- **Mouse events** are routed based on cursor position:
  - `cursor.x < window_width / 2` → left profile
  - `cursor.x >= window_width / 2` → right profile
- **Keyboard events** go to the last-clicked side (focus follows click)
- **Scroll events** go to whichever side the cursor is over

### Coordinate Translation

Mouse coordinates must be translated from window-space to profile-local space:

```
Window space (logical):  (x, y) where x ∈ [0, 1600], y ∈ [0, 800]

Left profile:   (x, y)           → same coordinates, x ∈ [0, 800]
Right profile:  (x - 800, y)     → offset by half window width
```

Scale factor is applied when constructing CEF mouse events. The profile server
receives logical coordinates and multiplies by `device_scale_factor` internally
via CEF's `screen_info()` callback.

### Input Handling

Keyboard and mouse handling adapted directly from the cef-rs OSR example's input
code (main.rs lines 407-556), which already handles:

- Mouse movement with modifier tracking
- Mouse clicks with button state bitmask
- Scroll wheel with line-to-pixel conversion
- Keyboard events with native key code mapping
- Modifier state (Shift, Control, Alt, Command)

The only difference: instead of calling `host.send_mouse_move_event()` directly,
the GUI serializes the event into an XPC dictionary and sends it to the
appropriate profile server.

## Performance Measurement

### Profile Server Logging

Each profile server logs `[FRAME-TX]` on every `on_accelerated_paint` callback,
identical to ts3's format:

```
[FRAME-TX] frame=42 w=1600 h=1600 port=12345 url=github.com time=1234567890
```

### GUI Logging

The GUI logs frame intervals, measuring the time between consecutive
`display_surface` messages from each profile:

```
[LEFT]  frame=42 interval=16ms
[RIGHT] frame=37 interval=17ms
```

### Comparison Targets

| Source                  | fps   | 60fps % | Max streak |
| ----------------------- | ----- | ------- | ---------- |
| cef-rs OSR (in-process) | ~60   | ~95%    | ~400+      |
| ts3 profile server      | 38.2  | 71%     | 424        |
| **cef-test (target)**   | **?** | **?**   | **?**      |

If cef-test matches cef-rs: the problem is in ts3's integration. If cef-test
matches ts3: the problem is inherent to multi-process headless CEF.

## Directory Structure

The cef-test crates live inside the ts3 Cargo workspace. This lets them depend
on `termsurf-xpc` and share workspace-level dependency versions (wgpu, libc,
block2, clap, etc.) without depending on any WezTerm or terminal emulator code.

```
ts3/
├── Cargo.toml                      (workspace — add 3 new members)
├── cef-test-gui/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                 (window, event loop, XPC manager)
│       └── webrender.rs            (wgpu pipeline, texture import, rendering)
├── cef-test-profile/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                 (CEF init, render handler, message loop)
├── cef-test-launcher/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                 (XPC bootstrap service)
├── cef-test-scripts/
│   └── build.sh                    (build all, bundle as macOS app)
├── termsurf-xpc/                   (shared, no changes needed)
├── termsurf-profile/               (existing ts3 code, untouched)
├── termsurf-launcher/              (existing ts3 code, untouched)
└── wezterm-gui/                    (existing ts3 code, untouched)
```

Workspace membership is a build system convenience, not a code dependency.
`cargo build -p cef-test-gui` builds only that crate and its direct dependencies
— it does not compile WezTerm.

### macOS App Bundle

CEF requires a proper macOS app bundle. The build script produces:

```
CefTest.app/
├── Contents/
│   ├── MacOS/
│   │   └── cef-test-gui                (main binary)
│   ├── Frameworks/
│   │   ├── Chromium Embedded Framework.framework/
│   │   └── cef-test-profile            (profile server binary)
│   ├── XPCServices/
│   │   └── com.cef-test.launcher.xpc/
│   │       └── Contents/
│   │           ├── MacOS/
│   │           │   └── cef-test-launcher
│   │           └── Info.plist          (XPC service registration)
│   └── Info.plist
```

The launcher's `Info.plist` registers the `com.cef-test.launcher` Mach service
with launchd, enabling both the GUI and profile servers to connect to it by
name.

## Dependencies

All three cef-test crates live in the ts3 Cargo workspace and use
`foo.workspace = true` for shared dependencies. `termsurf-xpc` is referenced via
`path = "../termsurf-xpc"` — no extraction needed.

### cef-test-gui

```toml
[dependencies]
termsurf-xpc = { path = "../termsurf-xpc" }
wgpu.workspace = true
winit = "0.30"
pollster = "0.4"
bytemuck.workspace = true

[target.'cfg(target_os = "macos")'.dependencies]
cef = { path = "../../cef-rs/cef", features = ["accelerated_osr"] }
metal.workspace = true
objc.workspace = true
io-surface.workspace = true
```

Depends on the `cef` crate only for `IOSurfaceImporter` and texture import
utilities. Does NOT initialize CEF or call any CEF browser APIs.

### cef-test-profile

```toml
[dependencies]
termsurf-xpc = { path = "../termsurf-xpc" }
clap.workspace = true
ctrlc = "3.4"

[target.'cfg(target_os = "macos")'.dependencies]
cef = { path = "../../cef-rs/cef", features = ["accelerated_osr"] }
```

No wgpu, no winit, no window. Headless.

### cef-test-launcher

```toml
[dependencies]
termsurf-xpc = { path = "../termsurf-xpc" }
```

Nothing else. The launcher is pure XPC plumbing.

## Key Simplifications vs ts3

| Aspect             | ts3                                     | cef-test                     |
| ------------------ | --------------------------------------- | ---------------------------- |
| GUI                | WezTerm (terminal emulator + webview)   | Bare winit window + wgpu     |
| Window management  | Tabs, splits, panes, multiplexing       | Fixed 2-panel layout         |
| Browser lifecycle  | Dynamic via `web` command               | Fixed at startup             |
| Profile reuse      | Launcher detects existing, forwards     | Launcher does same (simpler) |
| Input pipeline     | Terminal → web command → socket → XPC   | winit → XPC (direct)         |
| Rendering          | wgpu integrated into WezTerm's renderer | Standalone wgpu pipeline     |
| Event loop         | WezTerm's complex event loop            | Simple winit pump_app_events |
| Configuration      | WezTerm config, profiles, multiplexer   | CLI args only                |
| Total lines (est.) | ~100k+ (WezTerm fork)                   | ~2000                        |

## Build & Run

```bash
cd ts3 && ./cef-test-scripts/build.sh
./CefTest.app/Contents/MacOS/cef-test-gui
```

The build script:

1. `cargo build -p cef-test-gui -p cef-test-profile -p cef-test-launcher`
2. Bundle into CefTest.app with correct directory structure
3. Copy CEF framework into Frameworks/
4. Copy helper processes
5. Create Info.plist files

## Expected Outcomes

### If cef-test reproduces the problem (~38fps)

The root cause is inherent to headless CEF processes. Experiments to try here:

1. **`external_message_pump: true`** — The cef-rs example uses this and achieves
   60fps. ts3 couldn't use it due to a deadlock during init (Issue 342 Exp 4).
   cef-test may avoid the deadlock since it has a simpler init sequence.

2. **CVDisplayLink in profile server** — Create a CVDisplayLink (requires a
   hidden CAMetalLayer or IOSurface-based display link) to provide hardware
   vsync timing to the headless process.

3. **winit in profile server** — Add a hidden winit window to the profile server
   purely for `pump_app_events`. This is ugly but would definitively test
   whether the windowed event loop is what makes the cef-rs example fast.

4. **Vary the message loop** — Much easier to iterate on message loop
   experiments (cfrunloop timeout, NSApp pump, timer-based scheduling) with a
   2000-line codebase than a 100k-line one.

### If cef-test achieves ~60fps

The root cause is in ts3's integration. Suspects:

- WezTerm's event loop interfering with XPC message handling
- Additional latency in the web command → socket → GUI → XPC path
- Pane management overhead in the rendering path
- WezTerm's wgpu integration conflicting with IOSurface import

In this case, cef-test becomes the reference implementation and we progressively
add ts3 features until performance degrades, identifying the exact culprit.

### Either way, cef-test wins

A minimal reproduction is the gold standard for performance debugging. Whether
the problem reproduces or not, we learn something definitive and have a fast
iteration environment.

## Build Plan

Each phase produces something testable. Fix issues before moving on.

### Phase 1: Scaffold

Add three new crate stubs to the ts3 Cargo workspace. They share the workspace's
dependency declarations and `Cargo.lock` but have no code dependencies on
WezTerm or any other ts3 crate.

**Steps:**

1. Add `"cef-test-gui"`, `"cef-test-profile"`, `"cef-test-launcher"` to the
   `[workspace] members` list in `ts3/Cargo.toml`
2. Create `ts3/cef-test-gui/Cargo.toml` with dependencies listed in the
   Dependencies section above
3. Create `ts3/cef-test-profile/Cargo.toml` similarly
4. Create `ts3/cef-test-launcher/Cargo.toml` similarly
5. Create minimal `src/main.rs` for each (just `fn main() {}`)
6. Create `ts3/cef-test-scripts/` directory for build scripts

**Test:**
`cd ts3 && cargo build -p cef-test-gui -p cef-test-profile -p
cef-test-launcher`
succeeds. `cargo build -p wezterm-gui` still succeeds (no regressions).

### Phase 2: Profile Server — Standalone Headless CEF

Build `cef-test-profile` as a standalone headless CEF process. No XPC yet — just
initialize CEF, load a URL, and log frame output. This validates that CEF works
in our new binary before adding cross-process complexity.

**Steps:**

1. CLI args: `--url <url>` (plus `--width`, `--height`, `--scale` with defaults)
2. Load CEF framework (`LibraryLoader`)
3. Subprocess check (`execute_process`)
4. CEF settings: `windowless_rendering_enabled`, `shared_texture_enabled`,
   `root_cache_path` → `~/.config/cef-test/default/`
5. Render handler: `view_rect`, `screen_info`, `on_accelerated_paint`
6. In `on_accelerated_paint`: log `[FRAME-TX] frame=N w=W h=H time=T` (no Mach
   port creation yet)
7. Context menu handler: suppress (prevents crash)
8. Message loop: `do_message_loop_work()` + `cfrunloop::run_for(0.001)`
9. Ctrl+C handler for graceful shutdown
10. Bundle with custom build script (`cef-test-scripts/build-profile.sh`)

**Test:** Run the bundled binary:

```bash
cd ts3 && ./cef-test-scripts/build-profile.sh
./cef-test-profile.app/Contents/MacOS/cef-test-profile --url https://google.com
```

Observe `[FRAME-TX]` log lines appearing.

**Note:** `bundle-cef-app` is in the cef-rs workspace and uses `cargo_metadata`
to find binaries, so it can't bundle binaries from the ts3 workspace. The build
script creates the bundle manually instead: it builds cef-osr first (to get the
CEF framework and helpers), then creates a standalone app bundle with the
cef-test-profile binary, renamed helpers, and the CEF framework.

**Results:**

- CEF framework loads, browser creates, `on_accelerated_paint` fires
- Physical dimensions correct: 800x600 logical at 2.0 scale = 1600x1200 physical
- Initial burst: ~49 frames in ~800ms (~61fps) during page load
- After page loads: frames drop to ~250ms intervals (expected — CEF only paints
  on content changes, and google.com is static after load)
- The initial 60fps burst confirms the headless standalone process CAN achieve
  60fps during active rendering — the frame rate question will need interactive
  content (scrolling, animation) to answer definitively

**Conclusion:** We have 60fps. This is the critical baseline. The headless CEF
process — identical message loop, identical settings, identical
`do_message_loop_work()` + `cfrunloop::run_for(0.001)` — produces frames at
60fps during active rendering. Every subsequent phase adds one layer of
complexity (XPC, Mach ports, IOSurface transfer, GUI rendering). If any phase
drops below 60fps, we know exactly which layer caused the regression. Measure
frame rate at every phase boundary. Do not proceed past a phase that loses
frames without understanding why.

### Phase 3: GUI — Window + Split Rendering

Build `cef-test-gui` with a winit window and wgpu rendering pipeline. No CEF
textures yet — render two different solid colors in the left and right halves.
This validates the window, wgpu setup, and the split-view quad geometry before
adding IOSurface import complexity.

**Steps:**

1. Create winit window (1600x800 logical)
2. Initialize wgpu (Metal backend, Bgra8UnormSrgb surface format)
3. Create shader (same pass-through as cef-rs OSR example)
4. Create render pipeline with bind group layout (texture + sampler)
5. Create two vertex buffers: left quad (NDC x ∈ [-1, 0]) and right quad (NDC x
   ∈ [0, +1])
6. Create two solid-color textures (e.g., dark blue and dark green) as
   placeholder bind groups
7. Event loop: `pump_app_events` → on `RedrawRequested`, draw left quad with
   blue texture, draw right quad with green texture
8. Handle window close

**Test:** Run `cef-test-gui`. A window opens showing a blue left half and a
green right half. Resizing the window updates the surface correctly. Closing the
window exits cleanly.

**Results:**

- Window opens at 3200x1600 physical (1600x800 logical on Retina) — correct
- Left half renders dark blue, right half renders dark green — correct
- Resize events fire and surface reconfigures without crashes
- Window close exits cleanly
- ts3 workspace still builds (`cargo check -p termsurf-gui` passes)
- Uses `run_app` event loop (not `pump_app_events`) — no CEF interleaving needed
  in the GUI process at this phase
- Surface format: Bgra8Unorm (not Bgra8UnormSrgb — sRGB correction applies to
  IOSurface texture views in later phases, not the window surface)

### Phase 4: Build Script & App Bundle

Create the build script and macOS app bundle structure. This is needed before
Phase 5 because the launcher must be registered as an XPC service inside the app
bundle.

**Steps:**

1. Create `ts3/cef-test-scripts/build.sh`
2. `cargo build` all three binaries
3. Create `CefTest.app/Contents/` directory structure
4. Copy `cef-test-gui` to `Contents/MacOS/`
5. Copy `cef-test-profile` to `Contents/Helpers/` (or `Frameworks/`)
6. Copy CEF framework to `Contents/Frameworks/`
7. Copy CEF helper processes
8. Create `Contents/XPCServices/com.cef-test.launcher.xpc/` with launcher binary
   and `Info.plist`
9. Create app `Info.plist`
10. Handle code signing if required for XPC

**Test:** Run `./cef-test-scripts/build.sh`. It produces `CefTest.app`. Run
`./CefTest.app/Contents/MacOS/cef-test-gui`. The window opens with the colored
halves from Phase 3. The launcher binary exists at the correct path inside the
bundle.

**Results:**

- `./cef-test-scripts/build.sh` builds all three binaries and creates
  CefTest.app
- Bundle structure verified:
  - `Contents/MacOS/cef-test-gui` — main binary
  - `Contents/Frameworks/cef-test-profile` — profile server binary
  - `Contents/Frameworks/Chromium Embedded Framework.framework/` — CEF
  - `Contents/Frameworks/cef-test-profile Helper*.app/` — renamed CEF helpers
  - `Contents/XPCServices/com.cef-test.launcher.xpc/` — launcher + Info.plist
- `com.cef-test.launcher` registered with launchd as Mach service
- GUI opens from app bundle, shows blue/green split — same as Phase 3
- Build script also registers the launcher with launchd (same pattern as ts3's
  build-debug.sh), cleans stale registrations on re-run

### Phase 5: Launcher & XPC Bootstrap

Implement the launcher and the XPC connection chain: GUI → Launcher → Profile
Server → direct GUI connection. No data transfer yet — just prove the bootstrap
works.

**Steps:**

1. Implement `cef-test-launcher`:
   - Register as Mach service `com.cef-test.launcher`
   - Handle `spawn_profile`: store GUI endpoint, spawn profile process
   - Handle `claim_session`: return stored endpoint to profile
   - Exit when GUI disconnects
2. Update `cef-test-gui`:
   - Connect to `com.cef-test.launcher`
   - Create anonymous XPC listener for left slot
   - Send `spawn_profile` with endpoint, session-id, url, dimensions
   - Accept incoming connection from profile server
   - Log success: `"GUI: Profile 'left' connected"`
3. Update `cef-test-profile`:
   - Accept `--session-id` and `--service` (launcher name) CLI args
   - Connect to launcher, send `claim_session`
   - Receive GUI endpoint from reply
   - Connect to GUI via endpoint
   - Log success: `"Profile: Connected to GUI"`
   - Continue running CEF (from Phase 2) after connecting

**Test:** Run via the app bundle. Logs show the full chain:

```
Launcher: Starting...
GUI: Connected to launcher
GUI: Requesting profile 'left' (session=left-1, url=google.com)
Launcher: Spawning profile (session=left-1)
Profile: Claiming session left-1
Launcher: Session left-1 claimed
Profile: Connected to GUI
```

Profile server continues rendering (FRAME-TX logs appear). The GUI window shows
colored halves (no texture transfer yet).

**Results:**

- Full XPC bootstrap chain works across three processes:
  - Launcher: starts, receives `spawn_profile`, stores endpoint, spawns profile
  - Profile: connects to launcher, claims session, gets GUI endpoint, connects
  - GUI: connects to launcher, sends `spawn_profile` with anonymous endpoint
- Profile server runs CEF and produces `[FRAME-TX]` at expected rates
- Profile output: 70 frames in 10 seconds (google.com, static after load)
- GUI window shows blue/green placeholder halves (no texture transfer yet)
- Logs spread across three files:
  `/tmp/cef-test-{gui,launcher,profile-left-1}.log`
- Known issue: GUI's anonymous listener callback doesn't fire because
  `XpcListener::new_anonymous()` dispatches on `dispatch_get_main_queue()` and
  winit's `run_app` may not pump it actively. The profile IS connected
  (confirmed by profile log), but the GUI doesn't see the incoming connection in
  its callback. This must be resolved in Phase 6 — likely by switching to
  `pump_app_events` or using a private dispatch queue.

### Phase 6: IOSurface Transfer — One Browser Visible

Connect the rendering pipeline: profile server sends IOSurface Mach ports to the
GUI, GUI imports them as wgpu textures, renders in the left half. This is the
critical phase — it proves cross-process GPU texture sharing works in cef-test.

**Steps:**

1. Update `cef-test-profile` `on_accelerated_paint`:
   - Create Mach port: `IOSurfaceCreateMachPort(handle)`
   - Send XPC message: `display_surface` with `iosurface_port`, `width`,
     `height`
2. Update `cef-test-gui` XPC event handler:
   - Receive `display_surface` message
   - Extract Mach port: `copy_mach_send("iosurface_port")`
   - Look up IOSurface: `IOSurfaceLookupFromMachPort(port)`
   - Import via Metal: `IOSurfaceImporter::from_mach_port()` →
     `import_to_wgpu()`
   - Create bind group from imported texture
   - Store as left slot's current texture
   - Deallocate Mach port
   - Request window redraw
3. Update rendering: replace left placeholder bind group with the live texture

**Test:** Run via app bundle. The left half of the window shows a live webpage
(google.com). The right half remains the solid placeholder color. The page
should be static (no input yet) but fully rendered.

**Result:** SUCCESS. Cross-process GPU texture sharing works in cef-test.

- Switched GUI event loop from `run_app` to `pump_app_events` +
  `CFRunLoopRunInMode` to pump the main dispatch queue — this fixed the Phase 5
  anonymous listener callback issue. XPC callbacks now fire reliably.
- Profile server creates Mach port from IOSurface handle in
  `on_accelerated_paint`, sends `display_surface` XPC message with port, width,
  height, frame timing.
- GUI receives `display_surface`, stores pending Mach port. Main loop imports
  via `IOSurfaceImporter::from_mach_port()` → `import_to_wgpu()` (Metal path),
  creates sRGB texture view (`Bgra8UnormSrgb`), builds bind group, replaces left
  slot.
- Left half shows live google.com (animated doodle was playing). Right half
  remains green placeholder. 215 frames received over ~15 seconds — frame rate
  matches the doodle animation rate, not a pipeline bottleneck.
- Bug found: `termsurf_xpc::iosurface::deallocate_mach_port()` crashes because
  `mach_task_self_` is declared as `fn` in FFI but is actually a static variable
  in C (`extern mach_port_t mach_task_self_`; the `mach_task_self()` macro
  returns the variable, not a function call). The FFI jumps to a data address
  and gets `KERN_PROTECTION_FAILURE`. Workaround: skip Mach port deallocation —
  the real termsurf code also never calls it. Ports are cleaned up at process
  exit.
- Pending surface slot uses `Arc<Mutex<Option<PendingSurface>>>` shared between
  XPC dispatch queue callback and main loop. Latest frame overwrites any
  unprocessed frame (only latest matters for display).

### Phase 7: Two Profiles Side by Side

Spawn a second profile server and render both textures. This proves the full
multi-process architecture works.

**Steps:**

1. Update `cef-test-gui`:
   - Create a second anonymous XPC listener for right slot
   - Send a second `spawn_profile` to the launcher (different profile name,
     different URL, different session-id)
   - Accept the second profile server's connection
   - Store right slot's texture from its `display_surface` messages
2. Update rendering: draw both textures (left and right bind groups)
3. Update launcher: handle second spawn (may reuse or spawn new process
   depending on profile name)

**Test:** Run via app bundle. The left half shows github.com, the right half
shows google.com. Both are fully rendered, side by side, in a single window. No
interaction yet — just visual confirmation of two independent browser processes
sharing a window.

**Result:** Working. Both profiles render side by side in a single window.
GitHub.com (left, static) renders its initial frames then goes idle. Google.com
(right, animated doodle) renders continuously at ~14fps — matching the actual
doodle animation rate as confirmed in a separate browser. No crashes.

Key implementation details:

- `create_profile_listener()` helper extracts per-profile XPC listener setup for
  reuse. Each profile gets its own anonymous listener, endpoint, and connection
  tracking.
- `PendingSurfaces` struct holds separate `left` and `right` slots. XPC
  callbacks route to the correct slot via an `is_left` boolean captured in the
  closure.
- Launcher needed no changes — it already handles multiple `spawn_profile`
  calls, spawning a separate process for each.
- Profile server needed no changes — each instance independently connects back
  to its designated anonymous listener.
- Window size: 1600x800 logical (3200x1600 physical on Retina), with each
  profile rendering at 800x800 logical (1600x1600 physical).

### Phase 8: Simulated Mouse Input

Simulate mouse scroll events directly in the profile server to force continuous
re-rendering. This removes the GUI from the input path entirely, making the test
deterministic and reproducible — the same scroll pattern runs every time, with
no human variance.

**Steps:**

1. Change both profile URLs to Google search results pages
   (`https://www.google.com/search?q=asdf+asdf`) so there is scrollable content
2. After the page loads (use `on_loading_state_change` or a fixed delay), start
   a scroll loop in the profile server
3. Call `BrowserHost::send_mouse_wheel_event()` directly on a timer — no XPC
   round-trip needed since the profile server owns the browser
4. Alternate scroll direction (down then up) at a fixed interval to keep the
   page in motion for the entire test duration
5. Log frame rate during the scroll window to measure rendering throughput

**Test:** Run via app bundle. Both halves show Google search results. After a
brief load delay, both browsers begin scrolling automatically — down then back
up, in a repeating cycle. The `on_accelerated_paint` frame count during the
scroll window gives the rendering throughput. Each run produces identical
behavior with no manual interaction.

**Result:** Working. Both profiles load Google search results, detect page load
via `on_loading_state_change`, then begin automated scrolling. Pages scroll down
for 3 seconds, reverse to scroll up for 3 seconds, and repeat.

Key implementation details:

- Added `wrap_load_handler` with `on_loading_state_change` — sets a global
  `PAGE_LOADED` flag when `is_loading == 0`.
- Browser stored in `ProfileState` via `Mutex<Option<Browser>>` after creation
  in `on_context_initialized`, so the message loop can call
  `BrowserHost::send_mouse_wheel_event()` directly.
- Scroll events sent at ~125Hz (8ms interval) matching Apple mouse polling rate.
  `MouseEvent` positioned at center of viewport (400, 400 logical), delta=120
  (one standard scroll notch per event).
- Direction reversal every 3 seconds keeps page in continuous motion.
- Both URLs changed to `https://www.google.com/search?q=asdf+asdf` for
  scrollable content.
- No GUI-side changes needed for input — profile server drives scrolling
  entirely on its own.

### Phase 9: Performance Measurement & Analysis

Instrument everything and collect the data that answers the fundamental
question: does the multi-process architecture reproduce the performance problem?

**Steps:**

1. GUI logs per-profile frame intervals: `[LEFT] frame=N interval=Tms` /
   `[RIGHT] frame=N interval=Tms`
2. Profile server logs message loop timing (same instrumentation as Issue 343
   Exp 3): `do_message_loop_work` duration, `cfrunloop` duration, total loop
   time, spike counts
3. Run for 60+ seconds with both browsers loaded
4. Collect data, compute: average fps, % frames at 60fps (16-17ms), max
   consecutive 60fps streak, spike distribution
5. Compare against baselines:

   | Source                  | fps  | 60fps % | Max streak |
   | ----------------------- | ---- | ------- | ---------- |
   | cef-rs OSR (in-process) | ~60  | ~95%    | ~400+      |
   | ts3 profile server      | 38.2 | 71%     | 424        |
   | cef-test left profile   | ?    | ?       | ?          |
   | cef-test right profile  | ?    | ?       | ?          |

**Test:** Run, interact with both browsers for 60 seconds, collect logs. Produce
a performance summary. The numbers tell us which path to take:

- **~38fps (matches ts3):** Problem is inherent to multi-process headless CEF.
  Iterate on message loop experiments here in cef-test.
- **~60fps (matches cef-rs):** Problem is in ts3's integration. Use cef-test as
  the reference, bisect ts3.
- **Something in between:** Both factors contribute. Identify which experiments
  close the remaining gap.

**Result:** Something in between. 65-second run with continuous scrolling (final
run, with background XPC dispatch queue fix):

   | Source                  | fps  | 60fps % | Max streak | p50    | p95    |
   | ----------------------- | ---- | ------- | ---------- | ------ | ------ |
   | cef-rs OSR (in-process) | ~60  | ~95%    | ~400+      | ~16ms  | ~17ms  |
   | ts3 profile server      | 38.2 | 71%     | 424        | —      | —      |
   | cef-test left profile   | 50.0 | 80.8%   | 139        | 16.7ms | 33.6ms |
   | cef-test right profile  | 49.7 | 80.8%   | 80         | 16.7ms | 33.6ms |

Key observations:

- **p50 = 16.7ms (60fps).** The median frame lands exactly at 60fps. Most frames
  are on time — the average is dragged down by periodic spikes.
- **p95 = 33.6ms (~30fps).** Roughly 1 in 20 frames takes double the expected
  time. These are the `do_message_loop_work` spikes visible in the profile's
  `[LOOP-TIMING]` data (max_mlw > 30ms).
- **81% at 60fps vs ts3's 71%.** cef-test is measurably better, confirming that
  some of the problem is in ts3's integration, not just multi-process CEF.
- **Max streak of 80-139 vs cef-rs's 400+.** The periodic spikes break long
  streaks. This is consistent with CEF's internal scheduling interacting with
  the manual `do_message_loop_work` + `CFRunLoopRunInMode` pump pattern.
- **Both profiles perform nearly identically** (~50fps), confirming that
  running two CEF processes doesn't materially degrade per-profile performance.

Key implementation details:

- Scroll oscillation: reversed direction every 25 events (~200ms at 125Hz) to
  keep the page in constant visual motion without hitting top/bottom stalls.
- Profile server logs frame intervals in `on_accelerated_paint` via
  `LAST_PAINT_TIME` mutex.
- GUI tracks per-side `FrameStats` with frame count, intervals, and computes
  avg fps, 60fps%, max streak, percentiles. Summary prints every 10 seconds.
- `PendingSurface` includes `rx_time` timestamp for accurate GUI-side interval
  measurement.
- XPC callbacks dispatch on a background queue (not the main queue) so they
  don't conflict with winit's `pump_app_events`. This was critical: dispatching
  on the main queue required `CFRunLoopRunInMode` to pump callbacks, but that
  consumed NSEvents (like close button clicks) before winit could process them.
  Moving to a background queue fixed both the close button and preserved
  performance.

**Conclusion:**

The multi-process CEF architecture is **not** the primary bottleneck. cef-test
achieves ~50fps with two independent profile processes rendering simultaneously
— a 30% improvement over ts3's 38fps. The remaining gap to 60fps is caused by
periodic `do_message_loop_work` spikes in CEF's external message pump, not by
cross-process IPC or IOSurface sharing.

Three things are now established:

1. **Multi-process works.** Two CEF processes sharing one window via Mach port
   IOSurface transfer achieve ~50fps each with p50 = 16.7ms (exactly 60fps).
   The architecture is sound.

2. **ts3's 38fps may be an input problem, not a rendering problem.** cef-test
   sends scroll events at 125Hz directly inside the profile server. ts3's 38fps
   was measured during manual scrolling, where input traverses macOS → winit →
   WezTerm pane routing → XPC → profile server. If any stage drops or throttles
   events, the effective input rate reaching CEF is lower, and CEF renders fewer
   frames — not because it's slow, but because it's not being asked to render.
   See [Issue 345](./345-benchmark.md) for the test that resolves this.

3. **CEF's message pump has inherent jitter.** The p95 = 33.6ms (~30fps) spikes
   occur roughly every 80-139 frames and are visible in both cef-test and the
   cef-rs OSR example (though less frequently in-process). This is a CEF
   scheduling artifact, not an IPC issue.

**Next steps for ts3:** Build `web benchmark` ([Issue 345](./345-benchmark.md))
to test whether ts3's profile server achieves ~50fps when input is simulated
directly (bypassing the input routing pipeline). If it does, the 38fps was an
input rate problem. If it doesn't, the problem is in the GUI rendering path.

## Conclusion

### Why we built cef-test

Eight experiments in [Issue 343](./343-optimal-performance.md) failed to improve
ts3's profile server frame rate (38fps vs the cef-rs OSR example's 60fps). The
root cause was buried under 100k+ lines of WezTerm integration: terminal
multiplexing, pane management, the web command flow, complex event loops. We
needed to isolate the one architectural variable that separates ts3 from the
working example — **multi-process CEF with cross-process IOSurface sharing** —
and test it in a minimal environment where every variable is controlled.

### What we built

A standalone 3-process test harness (~2000 lines total):

- **cef-test-gui** — winit window + wgpu, renders two browser textures side by
  side. No CEF, no terminal emulator.
- **cef-test-profile** — headless CEF process, one per browser profile. Sends
  IOSurface Mach ports to the GUI via XPC. Simulates mouse scrolling at 125Hz
  for deterministic, reproducible load.
- **cef-test-launcher** — XPC bootstrap service. Forwards endpoints between GUI
  and profile servers. ~150 lines of plumbing.

The harness reproduces the exact ts3 architecture (XPC Mach port transfer,
anonymous listeners, per-profile CEF processes, IOSurface GPU texture sharing)
without any of the integration complexity.

### What we learned

The final benchmark (65 seconds, two profiles, continuous scrolling):

| Source                  | fps  | 60fps % | Max streak | p50    | p95    |
| ----------------------- | ---- | ------- | ---------- | ------ | ------ |
| cef-rs OSR (in-process) | ~60  | ~95%    | ~400+      | ~16ms  | ~17ms  |
| ts3 profile server      | 38.2 | 71%     | 424        | —      | —      |
| cef-test (left)         | 50.0 | 80.8%   | 139        | 16.7ms | 33.6ms |
| cef-test (right)        | 49.7 | 80.8%   | 80         | 16.7ms | 33.6ms |

Three conclusions:

1. **Multi-process CEF is not the bottleneck.** Two independent CEF processes
   sharing a single window via Mach port IOSurface transfer achieve ~50fps each.
   The architecture is sound. Cross-process IPC and GPU texture sharing cost
   almost nothing.

2. **ts3's 38fps is likely an input rate problem.** The 38fps was measured during
   manual scrolling, where input traverses a long pipeline (macOS → winit →
   WezTerm → XPC → profile server). cef-test bypasses this entirely by
   simulating scroll events at 125Hz directly in the profile server. The 12fps
   gap between cef-test (50fps) and ts3 (38fps) may simply reflect fewer scroll
   events reaching CEF, not slower rendering. [Issue
   345](./345-benchmark.md) tests this hypothesis with `web benchmark`.

3. **CEF's message pump has inherent jitter.** The p95 = 33.6ms spikes occur
   every 80-139 frames regardless of architecture. This is a CEF scheduling
   artifact in `do_message_loop_work()`, not an IPC issue. It accounts for the
   gap from 50fps (cef-test) to 60fps (in-process cef-rs), and may be fixable
   with `external_message_pump: true` (which cef-rs uses but ts3 could not due
   to a deadlock — see Issue 342 Exp 4).

### Bonus finding: XPC and the main queue

Dispatching XPC callbacks on `dispatch_get_main_queue()` conflicts with winit's
`pump_app_events` on macOS. The original design used `CFRunLoopRunInMode` to
pump XPC callbacks on the main queue, but this consumed NSEvents (like close
button clicks) before winit could process them. Moving XPC to a background
dispatch queue (`xpc_connection_set_target_queue` with NULL) fixed both the
window close bug and preserved full performance. The `Arc<Mutex<>>` on shared
state makes this thread-safe. This finding applies directly to ts3's GUI.

### Benchmark script

Run a reproducible 70-second benchmark:

```bash
cd ts3 && ./cef-test-scripts/benchmark.sh
```

The script builds the app, launches it, waits 70 seconds, kills it, and prints
a formatted summary. Use this to validate any future changes to the profile
server's message loop, XPC transport, or rendering pipeline.

### Next steps

1. **Build `web benchmark` ([Issue 345](./345-benchmark.md)).** Add simulated
   scrolling to ts3's profile server, bypassing the input pipeline entirely. If
   ts3 matches cef-test's ~50fps, the 38fps was an input rate problem. If it
   doesn't, the problem is in the GUI rendering path.

2. **Try `external_message_pump: true` in cef-test.** The cef-rs OSR example
   uses this and achieves 60fps. ts3 hit a deadlock during CEF init (Issue 342
   Exp 4), but cef-test's simpler init sequence may avoid it. If it works, the
   p95 spikes should disappear.

3. **Apply the background XPC queue pattern to ts3.** The
   `set_target_queue_background()` fix discovered here should be applied to
   ts3's `webview_xpc.rs` to prevent the same main queue conflict with
   WezTerm's event loop.
