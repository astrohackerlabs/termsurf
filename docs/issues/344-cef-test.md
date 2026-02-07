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
`cargo build -p cef-test-gui` builds only that crate and its direct
dependencies — it does not compile WezTerm.

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
`foo.workspace = true` for shared dependencies. `termsurf-xpc` is referenced
via `path = "../termsurf-xpc"` — no extraction needed.

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

**Test:** `cd ts3 && cargo build -p cef-test-gui -p cef-test-profile -p
cef-test-launcher` succeeds. `cargo build -p wezterm-gui` still succeeds (no
regressions).

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
5. Create two vertex buffers: left quad (NDC x ∈ [-1, 0]) and right quad (NDC
   x ∈ [0, +1])
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
8. Create `Contents/XPCServices/com.cef-test.launcher.xpc/` with launcher
   binary and `Info.plist`
9. Create app `Info.plist`
10. Handle code signing if required for XPC

**Test:** Run `./cef-test-scripts/build.sh`. It produces `CefTest.app`. Run
`./CefTest.app/Contents/MacOS/cef-test-gui`. The window opens with the colored
halves from Phase 3. The launcher binary exists at the correct path inside the
bundle.

**Results:**

- `./cef-test-scripts/build.sh` builds all three binaries and creates CefTest.app
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
- Logs spread across three files: `/tmp/cef-test-{gui,launcher,profile-left-1}.log`
- Known issue: GUI's anonymous listener callback doesn't fire because
  `XpcListener::new_anonymous()` dispatches on `dispatch_get_main_queue()` and
  winit's `run_app` may not pump it actively. The profile IS connected (confirmed
  by profile log), but the GUI doesn't see the incoming connection in its
  callback. This must be resolved in Phase 6 — likely by switching to
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
(google.com). The right half remains the solid placeholder color. The page should
be static (no input yet) but fully rendered.

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

### Phase 8: Mouse Input

Route mouse events from the GUI to the correct profile server. This makes the
browsers interactive — hover effects, clicking links, scrolling.

**Steps:**

1. Track cursor position in GUI
2. On `CursorMoved`: determine target (left if x < half, right otherwise)
3. Translate coordinates to profile-local space (right side: subtract half
   width)
4. Scale to logical coordinates (divide by scale factor)
5. Send `mouse_move` via XPC to target profile
6. On `MouseInput`: send `mouse_click` with button, up/down, click count
7. On `MouseWheel`: convert line delta to pixels, send `mouse_wheel`
8. Track modifier state (`ModifiersChanged`)
9. Update `cef-test-profile`: receive `mouse_move`, `mouse_click`,
   `mouse_wheel` messages, forward to CEF `BrowserHost`

**Test:** Move the mouse over both browsers. Hover effects appear (link
underlines, button highlights). Click links — navigation works. Scroll on both
sides. Right-click does nothing (context menu suppressed). Verify input goes to
the correct browser based on cursor position.

### Phase 9: Keyboard Input & Focus

Route keyboard events to the focused profile. This completes the core user
interaction — typing into forms, submitting searches.

**Steps:**

1. Track focus state in GUI (which side was last clicked)
2. On `MouseInput` (click): set focus to the side the click landed on
3. Send `focus` message: `{focused: 0}` to old side, `{focused: 1}` to new side
4. On `KeyboardInput`: convert to CEF key event (native key code, char code,
   modifiers)
5. Send `key_event` to focused profile
6. Handle special keys: Tab, Enter, Backspace, arrow keys, copy/paste shortcuts
7. Update `cef-test-profile`: receive `key_event`, forward to CEF `BrowserHost`
   as `KEYDOWN` then `CHAR` events

**Test:** Click on Google's search box on the right side. Type a search query.
Press Enter. Search results appear. Click a link. Navigate back. Switch focus to
the left side (github.com) by clicking it. Type in GitHub's search box. Both
browsers respond to keyboard input independently.

**Acceptance test:** This is the full user scenario — "open github.com on the
left and google.com on the right, type something into google.com, search, and
scroll around."

### Phase 10: Resize

Handle window resize so both browsers re-render at the correct dimensions.

**Steps:**

1. On winit `Resized` event: recalculate per-profile dimensions (half window
   width, full height)
2. Reconfigure wgpu surface
3. Update vertex buffers if needed (NDC coordinates are resolution-independent,
   so likely no change)
4. Send `resize` message to both profile servers with new logical dimensions and
   scale factor
5. Profile servers update `view_rect` return values and call
   `browser_host.was_resized()`
6. CEF re-renders at new size, sends new IOSurface

**Test:** Drag the window corner to resize. Both browsers re-render at the new
dimensions without distortion or crashes. Maximize the window — both browsers
fill their halves correctly.

### Phase 11: Performance Measurement & Analysis

Instrument everything and collect the data that answers the fundamental question:
does the multi-process architecture reproduce the performance problem?

**Steps:**

1. GUI logs per-profile frame intervals:
   `[LEFT] frame=N interval=Tms` / `[RIGHT] frame=N interval=Tms`
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

### Phase Summary

| Phase | Deliverable                         | Key risk addressed                        |
| ----- | ----------------------------------- | ----------------------------------------- |
| 1     | Workspace scaffold                  | Dependency structure, ts3 not broken       |
| 2     | Standalone headless CEF binary      | CEF initializes and renders in new binary  |
| 3     | Window with split-view rendering    | wgpu pipeline and quad geometry correct    |
| 4     | App bundle with build script        | Bundle structure valid for CEF + XPC       |
| 5     | XPC bootstrap chain                 | GUI ↔ Launcher ↔ Profile connection works |
| 6     | Live webpage in window (one side)   | Cross-process IOSurface sharing works      |
| 7     | Two browsers side by side           | Multi-profile architecture works           |
| 8     | Mouse interaction                   | Input routing and coordinate translation   |
| 9     | Keyboard interaction                | Full user interaction (acceptance test)    |
| 10    | Window resize                       | Dynamic dimension changes                  |
| 11    | Performance numbers                 | The answer to the fundamental question     |
