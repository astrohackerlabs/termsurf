+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 401: Programming Language & Process Architecture

## The Fundamental Question

TermSurf 4.0 requires three capabilities in one window:

1. GPU-accelerated terminal panes
2. Chromium browser panes
3. Multiple browser profiles sharing the same window

The multi-process architecture is non-negotiable (Issue 305, ts3 learnings):
each browser profile requires its own process because CEF/Chromium enforces one
`root_cache_path` per process. But this constraint, combined with XPC for IPC,
means **each process can be written in whatever language is best for that
process.** The processes communicate via XPC dictionaries and Mach port transfer
— language-agnostic protocols.

This document investigates which languages to use for each process and proposes
a concrete process architecture.

## Process Architecture

### What Processes Must Exist

```
┌─────────────────────────────────────────────────────────────┐
│                     macOS Process Tree                      │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Window Process                                      │    │
│  │ • Owns the window, event loop, compositor           │    │
│  │ • Renders terminal panes (in-process)               │    │
│  │ • Composites browser textures (from IOSurface)      │    │
│  │ • Routes input to the correct pane/process          │    │
│  │ • Manages tabs and pane layout                      │    │
│  │                                                     │    │
│  │   Contains:                                         │    │
│  │   • Terminal library (alacritty_terminal)           │    │
│  │   • GPU compositor (wgpu or Metal)                  │    │
│  │   • XPC client (talks to launcher + profiles)       │    │
│  │   • PTY children: bash, zsh, fish, etc.             │    │
│  └───────────────┬─────────────────────────────────────┘    │
│                  │ XPC                                      │
│                  ▼                                          │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Launcher Process                                    │    │
│  │ • XPC Mach service (com.termsurf.launcher)          │    │
│  │ • Spawns browser profile processes on demand        │    │
│  │ • Relays XPC endpoints for direct GUI↔profile comm  │    │
│  │ • Detects existing profile processes for reuse      │    │
│  └───────────────┬─────────────────────────────────────┘    │
│                  │ XPC                                      │
│          ┌───────┴───────┐                                  │
│          ▼               ▼                                  │
│  ┌──────────────┐ ┌──────────────┐                          │
│  │ Profile:     │ │ Profile:     │  ← one per profile       │
│  │ "default"    │ │ "work"       │                          │
│  │              │ │              │                          │
│  │ Chromium     │ │ Chromium     │                          │
│  │ browser proc │ │ browser proc │                          │
│  │              │ │              │                          │
│  │ Renderer(s)  │ │ Renderer(s)  │  ← Chromium-managed      │
│  │ GPU process  │ │ GPU process  │  ← Chromium-managed      │
│  │ Utilities    │ │ Utilities    │  ← Chromium-managed      │
│  └──────────────┘ └──────────────┘                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Process Count for a Typical Session

A window with 2 terminal panes and 2 browser panes (1 profile):

| Process                     | Count    | Managed by |
| --------------------------- | -------- | ---------- |
| Window                      | 1        | Us         |
| Launcher                    | 1        | Us         |
| Browser profile ("default") | 1        | Us         |
| Chromium renderer           | 1-2      | Chromium   |
| Chromium GPU                | 1        | Chromium   |
| Chromium utility            | 1-2      | Chromium   |
| Shell (bash/zsh)            | 2        | PTY        |
| **Total**                   | **8-10** |            |

This is comparable to running Chrome + Terminal.app simultaneously. The user
sees one window.

### Why Terminal Is In-Process

The terminal could be a separate process (like the browser), but there are
strong reasons to keep it in the window process:

1. **Typing latency.** Every keystroke must travel: window → terminal → PTY →
   response → render → display. Adding XPC round trips adds ~2ms per frame
   (measured in ts3). For terminal use, where keystroke-to-pixel latency
   matters, in-process is better.

2. **Rendering is lightweight.** Terminal rendering is text on a grid. It
   doesn't need process isolation for stability or security. A browser executes
   arbitrary JavaScript from the internet — a terminal runs a local shell.

3. **`alacritty_terminal` is a library.** It's designed for embedding: no GUI
   dependencies, clean trait-based API, ~50 lines to integrate. There's no
   technical reason to put it in a separate process.

4. **Simplicity.** Fewer processes = fewer IPC channels = fewer failure modes.

If we later need terminal process isolation (e.g., for remote terminals over
SSH), the protocol-oriented architecture makes it possible to move the terminal
out-of-process without changing the window compositor. But for v1, in-process.

### Why Browser Must Be Out-of-Process

1. **One profile per process.** Chromium's `root_cache_path` constraint. This is
   the entire reason ts3 exists.

2. **Crash isolation.** A webpage crash shouldn't kill the terminal.

3. **Security.** Browser processes run untrusted web content. Sandboxing is
   easier with process boundaries.

4. **Chromium's own multi-process model.** Chromium already spawns renderer and
   GPU processes internally. Our browser profile process is the "browser
   process" in Chromium's terminology — the parent of the renderer/GPU/utility
   tree.

## IPC Protocol

### The Protocol Is the Architecture

Because each process can be any language, the IPC protocol is what defines the
system. If two implementations follow the same protocol, they're
interchangeable.

### Window ↔ Browser Profile Protocol

Reuses the proven ts3 pattern:

**Setup (via launcher):**

```
Window                    Launcher                Profile
  │                          │                       │
  │ spawn_profile            │                       │
  │ (url, profile, size,     │                       │
  │  scale, endpoint)        │                       │
  │─────────────────────────▶│                       │
  │                          │   exec termsurf-      │
  │                          │   profile --args      │
  │                          │──────────────────────▶│
  │                          │                       │
  │                          │   claim_session       │
  │                          │   (session_id)        │
  │                          │◀──────────────────────│
  │                          │                       │
  │                          │   reply: endpoint     │
  │                          │──────────────────────▶│
  │                          │                       │
  │         direct XPC connection                    │
  │◀─────────────────────────────────────────────────│
  │                                                  │
```

**Frame delivery:**

```
Profile                              Window
  │                                    │
  │ iosurface_port (Mach port)         │
  │ session_id (string)                │
  │ width, height (int64)              │
  │───────────────────────────────────▶│
  │                                    │ IOSurfaceLookupFromMachPort()
  │                                    │ Create wgpu texture
  │                                    │ Composite into frame
  │                                    │
```

**Input forwarding:**

```
Window                               Profile
  │                                    │
  │ mouse_event                        │
  │ (x, y, type, button, modifiers)    │
  │───────────────────────────────────▶│
  │                                    │
  │ key_event                          │
  │ (keycode, modifiers, type)         │
  │───────────────────────────────────▶│
  │                                    │
  │ scroll_event                       │
  │ (dx, dy, phase)                    │
  │───────────────────────────────────▶│
  │                                    │
  │ resize                             │
  │ (width, height, scale)             │
  │───────────────────────────────────▶│
  │                                    │
```

**Browser → Window events:**

```
Profile                              Window
  │                                    │
  │ url_changed (string)               │
  │───────────────────────────────────▶│
  │                                    │
  │ title_changed (string)             │
  │───────────────────────────────────▶│
  │                                    │
  │ loading_state (bool)               │
  │───────────────────────────────────▶│
  │                                    │
  │ cursor_changed (int64)             │
  │───────────────────────────────────▶│
  │                                    │
```

All messages are XPC dictionaries with string keys and typed values (string,
int64, uint64, bool, mach_send). No floating point — scale factor is passed as a
string (XPC limitation from ts3 learnings).

### Window ↔ Terminal Protocol

No protocol needed — the terminal is in-process. The window calls
`alacritty_terminal` directly:

```rust
// Input
term.lock().input(bytes);

// Read state for rendering
let guard = term.lock();
for cell in guard.grid().display_iter() {
    // render cell to GPU
}
```

If terminal moves out-of-process in the future, it would use the same XPC
pattern as the browser, with IOSurface for texture sharing.

## Language Analysis

### Chromium Browser Profile Process: C++

**This is not a choice — it's a given.** Chromium is 30+ million lines of C++.
The Content API is C++. The build system is GN + Ninja. There is no practical
way to embed Chromium in another language without a C++ component.

The question is how much C++ and what form it takes.

**Option A: Fork `content/shell/`**

Modify Chromium's reference embedder to add off-screen rendering and XPC
communication. Keep it in the Chromium source tree. Build with GN + Ninja.

- Pro: Proven build path, minimal code, direct Content API access
- Pro: content_shell is ~105 files — small enough to understand
- Con: Tied to Chromium source tree (updates require rebasing our changes)
- Con: XPC code in C++ (more boilerplate than Rust)

**Option B: Standalone C++ binary linking Chromium**

Build Chromium as a shared library, write our own `main()` that links against
it. Our binary does Content API init, off-screen rendering, XPC communication.

- Pro: Our code lives outside Chromium's tree
- Con: Defining the shared library boundary is hard
- Con: Chromium doesn't have a stable ABI

**Option C: C++ shim with C API**

Write a thin C++ shim inside the Chromium tree that exposes a C API
(`ts_init()`, `ts_create_browser()`, `ts_set_frame_callback()`). Build as
`.dylib`. Rust or Swift calls the C API.

- Pro: Clean FFI boundary
- Con: Two layers of abstraction (C++ → C → Rust)
- Con: Still tied to Chromium tree for builds

**Recommendation: Option A.** Fork `content/shell/`. The browser profile process
is a C++ binary that builds with GN + Ninja inside the Chromium tree. It does
three things: initialize Chromium, render off-screen, and speak XPC. This is the
simplest path — no FFI boundaries, no shared library management, no abstraction
layers. XPC from C++ is straightforward (libxpc is a C API).

### Window Process: Language Options

The window process is the heart of the application. It must:

1. Create and manage a window (event loop, resize, fullscreen)
2. GPU-composite terminal + browser textures into one frame
3. Render terminal text (GPU text rasterization)
4. Communicate via XPC (Mach port transfer)
5. Manage tabs, panes, focus, input routing

#### Option 1: Rust

| Aspect             | Assessment                                                  |
| ------------------ | ----------------------------------------------------------- |
| Window + GPU       | winit + wgpu (proven, cross-platform)                       |
| Terminal embedding | `alacritty_terminal` is Rust — zero FFI, direct integration |
| XPC                | `termsurf-xpc` crate exists (1,417 lines, proven in ts3)    |
| Text rendering     | Need to write or port (Alacritty uses OpenGL, we need wgpu) |
| Cross-platform     | Strong (winit + wgpu work on macOS/Linux/Windows)           |
| Ecosystem          | Cargo, strong type system, memory safety                    |
| Risk               | GPU text rendering in wgpu is the main unknown              |

#### Option 2: Swift

| Aspect             | Assessment                                                  |
| ------------------ | ----------------------------------------------------------- |
| Window + GPU       | AppKit + Metal (native macOS, excellent)                    |
| Terminal embedding | `alacritty_terminal` is Rust — needs C FFI bridge           |
| XPC                | Native, first-class (`xpc_connection_*` or NSXPCConnection) |
| Text rendering     | Core Text + Metal (native macOS text rendering)             |
| Cross-platform     | None — macOS only, full rewrite for Linux/Windows           |
| Ecosystem          | Xcode, Swift Package Manager                                |
| Risk               | Swift↔Rust FFI adds complexity for terminal integration     |

#### Option 3: C++

| Aspect             | Assessment                                                 |
| ------------------ | ---------------------------------------------------------- |
| Window + GPU       | SDL2 + Metal/Vulkan, or raw AppKit from C++                |
| Terminal embedding | Alacritty is Rust — needs C FFI. Or use a C++ terminal lib |
| XPC                | libxpc is C — trivial from C++                             |
| Text rendering     | FreeType + HarfBuzz (proven stack, used by many terminals) |
| Cross-platform     | Moderate (SDL2 is cross-platform, text rendering varies)   |
| Ecosystem          | CMake or Meson, manual memory management                   |
| Risk               | More boilerplate, no memory safety guarantees              |

#### Recommendation: Rust

**Rust is the most pragmatic choice for the window process.** The decisive
factors:

1. **Terminal integration.** `alacritty_terminal` is a Rust library. Embedding
   it in Rust is zero-cost: direct function calls, shared types, no FFI. In
   Swift or C++, every terminal interaction crosses a language boundary.

2. **XPC already works.** The `termsurf-xpc` crate (1,417 lines) is complete,
   tested, and proven in ts3. It handles connections, listeners, endpoints, Mach
   port transfer, and IOSurface import/lookup. Rewriting this in Swift gains
   nothing — XPC from Rust is already working.

3. **Cross-platform foundation.** winit + wgpu work on macOS, Linux, and
   Windows. Even though we're macOS-first, building on a cross-platform
   foundation means the window process doesn't need a full rewrite later. Only
   the XPC layer needs platform-specific alternatives (D-Bus on Linux, named
   pipes on Windows).

4. **GPU compositing.** wgpu supports Metal (macOS), Vulkan (Linux/Windows), and
   DX12 (Windows). IOSurface → wgpu texture import is already proven in ts3.

The one gap is **GPU text rendering**. Alacritty uses OpenGL (via glutin +
crossfont). We need wgpu. Options:

- **Port Alacritty's renderer to wgpu.** The renderer is ~2000 lines of OpenGL.
  wgpu's shader model is different but the concepts (glyph atlas, instanced
  quads) transfer directly.
- **Use `cosmic-text` + `glyphon`.** cosmic-text is a Rust text layout engine;
  glyphon renders text with wgpu. This is a newer, wgpu-native approach.
- **Write a minimal text renderer.** A terminal needs monospace text on a grid —
  simpler than general text layout. A glyph atlas + instanced quad shader is
  ~500 lines of wgpu code.

### Launcher: Rust

The launcher is a small XPC service (~300 lines in ts3). It spawns processes and
relays endpoints. Rust is the obvious choice — the `termsurf-xpc` crate is
already written, and the launcher logic is trivial.

Alternatively, the launcher could be eliminated entirely in ts4. The window
process could spawn browser profile processes directly and establish XPC
connections without a mediator. The launcher exists in ts3 because WezTerm
didn't own process management — in ts4, we do.

**Recommendation: Start without a launcher.** The window process spawns profile
processes directly. If we later need a daemon (for profile process reuse across
multiple windows), add one then.

## Recommended Architecture

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│  Window Process (Rust)                                   │
│  ├── winit: window, event loop                           │
│  ├── wgpu: GPU compositor (Metal backend on macOS)       │
│  ├── alacritty_terminal: PTY, VTE, grid state            │
│  ├── text renderer: glyph atlas + wgpu shaders           │
│  ├── termsurf-xpc: XPC client for browser profiles       │
│  ├── pane manager: layout, focus, input routing          │
│  └── tab manager: multiple pane layouts per window       │
│       │                                                  │
│       │ XPC (Mach port transfer)                         │
│       ▼                                                  │
│  Browser Profile Process (C++)                           │
│  ├── Chromium Content API: browser init, WebContents     │
│  ├── Custom RenderWidgetHostView: off-screen rendering   │
│  ├── FrameSinkVideoCapturer: GPU frame capture           │
│  ├── IOSurface → Mach port: texture sharing              │
│  ├── libxpc: XPC communication with window               │
│  └── Chromium sub-processes (renderer, GPU, utility)     │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

### Language Summary

| Process            | Language       | Rationale                                                                      |
| ------------------ | -------------- | ------------------------------------------------------------------------------ |
| Window             | Rust           | Terminal embedding (zero FFI), XPC crate exists, cross-platform (winit + wgpu) |
| Browser profile    | C++            | Chromium IS C++, Content API is C++, no FFI boundary needed                    |
| Launcher           | Rust (or none) | Existing crate, or eliminate by spawning directly from window                  |
| Chromium internals | C++            | Managed by Chromium, not our code                                              |
| Shell processes    | Any            | bash/zsh/fish — managed by PTY, language irrelevant                            |

### What We Reuse from ts3

| Component               | ts3 Location                                    | Reuse in ts4                        |
| ----------------------- | ----------------------------------------------- | ----------------------------------- |
| XPC bindings            | `ts3/termsurf-xpc/`                             | Direct — copy crate into ts4        |
| IOSurface utilities     | `ts3/termsurf-xpc/src/iosurface.rs`             | Direct                              |
| XPC connection patterns | `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Adapt (remove WezTerm dependencies) |
| Launcher logic          | `ts3/termsurf-launcher/src/main.rs`             | Adapt or eliminate                  |
| IOSurface → wgpu import | `ts3` + `cef-rs` wgpu texture code              | Direct                              |

### What We Write New

| Component              | Language | Estimated size                                   |
| ---------------------- | -------- | ------------------------------------------------ |
| Window + event loop    | Rust     | ~500 lines (winit boilerplate)                   |
| GPU compositor         | Rust     | ~1000 lines (wgpu pipeline, texture compositing) |
| Terminal text renderer | Rust     | ~1500 lines (glyph atlas + wgpu shaders)         |
| Terminal integration   | Rust     | ~200 lines (alacritty_terminal embedding)        |
| Pane/tab manager       | Rust     | ~800 lines (layout, focus, routing)              |
| Browser profile binary | C++      | ~3000 lines (Content API, OSR view, XPC)         |
| **Total**              |          | **~7000 lines**                                  |

## The Protocol-Oriented Insight

The user's key insight: **by establishing clear IPC protocols, each part can be
swapped independently.** This is correct and powerful.

The XPC protocol between window and browser is language-agnostic:

- XPC dictionaries are key-value stores with string, int64, bool, and Mach port
  values
- IOSurface Mach ports carry GPU textures across any process boundary
- Any language that can call `xpc_connection_send_message()` and
  `IOSurfaceLookupFromMachPort()` can participate

This means:

1. **The browser process could be CEF instead of raw Chromium.** If direct
   Chromium embedding proves too hard (Issue 401 feasibility research), we can
   fall back to a CEF-based profile process. The window doesn't care — it
   receives IOSurface Mach ports either way.

2. **The window process could be rewritten in Swift.** If we later want a more
   native macOS experience, the browser profile process doesn't change — it
   still sends IOSurface Mach ports via XPC.

3. **The terminal could move out-of-process.** If we want remote terminals or
   crash isolation, we extract `alacritty_terminal` into its own process with
   the same IOSurface + XPC protocol. The window compositor doesn't change.

4. **Linux/Windows ports replace only the IPC layer.** The XPC protocol maps to
   D-Bus + DMA-BUF on Linux, or named pipes + DXGI on Windows. The compositor
   and terminal embedding remain the same.

## Risk Assessment

| Decision            | Risk                                   | Mitigation                                                   |
| ------------------- | -------------------------------------- | ------------------------------------------------------------ |
| Rust for window     | GPU text rendering is unproven in wgpu | cosmic-text + glyphon exist; fallback to OpenGL              |
| C++ for browser     | Chromium build system is alien to Rust | Keep C++ binary separate, communicate only via XPC           |
| In-process terminal | Terminal crash kills window            | alacritty_terminal is mature, crashes are rare               |
| No launcher         | Can't reuse profiles across windows    | Add launcher later if needed                                 |
| macOS-first         | Linux/Windows deferred                 | XPC is the only macOS-specific part; isolated behind a trait |

## Comparison: Three-Language vs Two-Language

The user noted we might end up with three languages (C++, Rust, Swift). Here's
the comparison:

### Two languages: Rust + C++

```
Window (Rust) ←XPC→ Browser (C++)
```

- Terminal is in-process (Rust)
- XPC via existing Rust crate
- Two build systems: Cargo + GN/Ninja
- Clean: Rust handles everything except Chromium

### Three languages: Swift + Rust + C++

```
Window (Swift) ←XPC→ Browser (C++)
                ↑
          Terminal (Rust, out-of-process)
              ←XPC→
```

- Terminal must be out-of-process (Swift can't embed Rust libraries easily)
- Or: Swift↔Rust FFI via C ABI (adds complexity)
- Three build systems: Xcode + Cargo + GN/Ninja
- XPC is native in Swift (simpler) but terminal integration is harder

### Three languages: Swift + Rust + C++ (hybrid)

```
Window (Swift, with Rust static lib for terminal) ←XPC→ Browser (C++)
```

- Terminal compiled as Rust static library with C ABI
- Swift calls C functions for terminal operations
- More complex build (Swift + Rust static lib + Chromium)
- Native macOS feel but FFI bridge for every terminal operation

### Recommendation

**Two languages: Rust + C++.** The Rust window process embeds
`alacritty_terminal` directly (zero FFI) and uses the existing `termsurf-xpc`
crate for browser communication. The C++ browser profile process is a standalone
binary built with GN + Ninja. The two never share code — they share a protocol.

Swift adds a third build system and either forces the terminal out-of-process or
requires FFI bridging. The marginal benefit (native AppKit) doesn't justify the
integration cost, especially since winit provides adequate macOS window
management and we can always add native touches via Objective-C FFI from Rust
(which is already proven in the `termsurf-xpc` crate's block2 + objc usage).

## Build System

```
termsurf/
├── ts4/                         # TermSurf 4.0 root
│   ├── Cargo.toml               # Rust workspace
│   ├── termsurf-window/         # Window process (Rust binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── termsurf-xpc/            # XPC bindings (Rust library, from ts3)
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── termsurf-browser/        # Browser profile (C++ binary)
│   │   ├── BUILD.gn             # Chromium GN target
│   │   └── src/
│   └── scripts/
│       ├── build-window.sh      # cargo build
│       └── build-browser.sh     # gn gen + ninja
├── chromium/                    # Chromium source (gitignored)
├── alacritty/                   # Alacritty source (gitignored, reference)
└── ...
```

Two independent build commands:

```bash
# Build the window (Rust)
cd ts4 && cargo build

# Build the browser profile binary (C++, inside Chromium tree)
cd chromium && gn gen out/Release && ninja -C out/Release termsurf-browser
```

The Rust build doesn't depend on the Chromium build. The two binaries
communicate only via XPC at runtime. This keeps build times independent and
avoids cross-language build system integration.

## Next Steps

1. **Issue 401 (Chromium feasibility):** Determine if direct Chromium embedding
   is viable. This is the existential risk and must come first.

2. **Phase 1 (window + compositor):** Rust binary with winit + wgpu. Render
   colored rectangles. Prove the compositor works.

3. **Phase 2 (terminal):** Embed `alacritty_terminal`. Write GPU text renderer.
   Prove the terminal works in our window.

4. **Phase 3 (browser):** Build the C++ browser profile binary. Fork
   `content/shell/`, add OSR + XPC. Prove we can send an IOSurface from Chromium
   to our Rust window at 60fps.

5. **Phase 4 (integration):** Terminal panes + browser panes in one window.
   Input routing, tab management, `web` command.
