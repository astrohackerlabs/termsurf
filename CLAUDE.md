# Agent Development Guide

A file for [guiding coding agents](https://agents.md/).

## AI Guidance

Never under any circumstances change the code unless explicitly asked by the
user. When in doubt, ask the user before making any changes.

## Settled Architectural Decisions

Do NOT suggest alternatives to these. They were chosen after extensive
experimentation across five generations (ts1–ts5) and hundreds of experiments.

### XPC is the only IPC mechanism on macOS

All inter-process communication on macOS MUST use XPC. There are no exceptions.
Do not suggest Unix domain sockets, named pipes, shared memory, or any other IPC
mechanism as an alternative.

IOSurface textures can only be transferred between processes via Mach ports, and
Mach ports can only be transferred via XPC. This is not a preference — it is a
hard macOS kernel constraint. Every IPC channel in TermSurf uses XPC because the
texture channel requires it, and using a second IPC mechanism for non-texture
messages would add complexity for zero benefit.

This was proven in ts3 (Issues 303, 325–350) and ts4 (Issues 403, 407).

## Project Overview

TermSurf is a terminal emulator with an integrated web browser. Users type
`web google.com` in their terminal and a webpage renders directly in the
terminal pane, sharing cookies and sessions across tabs within the same browser
profile.

The project has evolved through five generations:

- **ts1** (Ghostty + WKWebView) — macOS-only. WKWebView had limited API and no
  cross-platform path. Abandoned in favor of CEF.
- **ts2** (WezTerm + in-process CEF) — Embedded CEF directly in WezTerm. CEF
  allows only one `root_cache_path` per process, meaning one browser profile per
  application. Multiple profiles required moving CEF out-of-process. Abandoned.
- **ts3** (WezTerm + out-of-process CEF via XPC) — Each browser profile gets its
  own CEF process, solving the one-profile-per-process limitation. Processes
  communicate with the GUI via XPC Mach port transfer. Superseded by ts4 after
  26 experiments (Issues 325–350) proved CEF's headless off-screen rendering
  caps at ~31fps on macOS.
- **ts4** (Chromium Content API experiments) — Proved in-process Chromium works:
  multiple browser profiles coexisting in one process, 60fps rendering. PoC only
  — used content_shell inside the Chromium source tree. Superseded by ts5.
- **ts5** (Ghostty fork + in-process Chromium) — **Active development.** Forks
  Ghostty as the application (terminal panes are native, in-process). Will embed
  Chromium directly via the Content API for browser panes.

**Directory structure:**

- `ts5/` — TermSurf 5.0 (Ghostty fork + in-process Chromium). Active work.
- `web/` — `web` TUI (Rust/ratatui). Browser chrome in the terminal pane.
- `ts4/` — TermSurf 4.0 (Chromium Content API experiments). Superseded.
- `ts3/` — TermSurf 3.0 (WezTerm fork + out-of-process CEF). Superseded.
- `ts2/` — TermSurf 2.0 (WezTerm fork + in-process CEF). Superseded.
- `ts1/` — TermSurf 1.x (Ghostty fork + WKWebView). Legacy.
- `vendor/cef-rs/` — CEF Rust bindings. Used by `ts3/termsurf-profile/`.
- `docs/issues/` — All documentation across all generations.

## TermSurf 5.0 (ts5/) — Active Development

### Architecture

ts5 forks Ghostty as the application — terminal panes are native, in-process
Ghostty rendering. Browser panes will embed Chromium directly via the Content
API (not CEF, which cannot sustain 60fps headless). This combines the ts1
approach (Ghostty as the app) with the ts4 finding (in-process Chromium works).

```
Ghostty Fork (Zig + Swift macOS shell)
├── Terminal panes (in-process, native Ghostty rendering)
├── Browser panes (in-process Chromium via Content API) [TBD]
│   ├── BrowserContext "work" (Profile 1)
│   ├── BrowserContext "personal" (Profile 2)
│   └── BrowserContext "guest" (Profile N)
├── XPC client (connects to xpc-gateway, registers anonymous listener)
│   └── Receives overlay coordinates from `web` processes via direct connection
├── Metal renderer (inherited from Ghostty)
│   └── pink_overlay pipeline (Issue 505) — GPU quad at grid coordinates
├── Pane/tab/split management (inherited from Ghostty)
└── Keybindings, configuration (inherited from Ghostty)

xpc-gateway daemon (com.termsurf.xpc-gateway Mach service)
├── Tiny Swift binary (~80 lines), auto-registered via SMAppService
├── Stores app's anonymous listener endpoint
└── Returns endpoint to `web` processes (rendezvous only, no ongoing traffic)

web TUI (Rust/ratatui, runs inside a terminal pane)
├── Draws browser chrome (URL bar, viewport border, status bar)
├── Connects to xpc-gateway, claims app endpoint, then connects directly to app
├── Sends viewport grid coordinates on the direct connection
└── TERMSURF_PANE_ID env var identifies which pane it's in
```

### Current State

ts5 is a Ghostty fork (imported via `git subtree add`) with the following
TermSurf additions:

- **XPC gateway** (`xpc-gateway/`) — Tiny Swift daemon that owns the
  `com.termsurf.xpc-gateway` Mach service. The app registers an anonymous
  listener endpoint here; `web` processes claim it to connect directly to the
  app. Auto-registered via SMAppService (Issue 506).
- **XPC client** (`CompositorXPC.swift`) — Connects to the xpc-gateway as a
  client, creates an anonymous listener, and registers its endpoint. Receives
  overlay coordinates from `web` processes on the direct connection and passes
  them to the renderer via the C API.
- **Pink overlay pipeline** (`pink_overlay` in `shaders.zig` / `shaders.metal`)
  — Metal shader that renders a solid-color quad at grid coordinates. Proven
  working with correct alignment (Issue 505, Experiments 1–3).
- **C API bridge** (`ghostty_surface_set_overlay` / `clear_overlay`) — Lets
  Swift XPC code set overlay coordinates on the Zig renderer thread-safely via
  `draw_mutex`.
- **Pane ID propagation** — Each surface sets `TERMSURF_PANE_ID` (UUID) in the
  shell environment, inherited by child processes including `web`.

**Not yet started:** Chromium Content API embedding (proven in ts4's PoC). The
pink overlay will be replaced with a real IOSurface texture from Chromium.

### Directory Structure

- `ts5/src/` — Shared Zig core (libghostty)
- `ts5/src/renderer/generic.zig` — Main render logic, `drawFrame()`, pink
  overlay render step
- `ts5/src/renderer/metal/shaders.zig` — Pipeline definitions (`pink_overlay`)
- `ts5/src/renderer/shaders/shaders.metal` — Metal shaders (pink overlay vertex
  - fragment)
- `ts5/src/Surface.zig` — Core surface, `setOverlay()` / `clearOverlay()`
- `ts5/src/apprt/embedded.zig` — C API exports
- `ts5/include/ghostty.h` — libghostty C API headers
- `ts5/macos/` — Ghostty macOS app (Swift + Xcode)
- `ts5/xpc-gateway/` — XPC gateway daemon (Swift, ~80 lines)
- `ts5/xpc-gateway/Sources/main.swift` — Gateway: owns Mach service, stores/returns app endpoint
- `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — XPC client (connects to gateway, registers endpoint)
- `ts5/macos/Sources/App/macOS/AppDelegate.swift` — Starts compositor XPC on launch
- `ts5/macos/com.termsurf.xpc-gateway.plist` — launchd plist (dev, absolute paths)
- `ts5/macos/com.termsurf.xpc-gateway.bundle.plist` — launchd plist (bundled, BundleProgram)
- `ts5/build.zig` — Ghostty build system
- `ts5/build.zig.zon` — Ghostty dependencies
- `ts5/pkg/` — Platform packages (Linux, macOS, etc.)
- `web/` — `web` TUI (Rust/ratatui)
- `web/src/main.rs` — TUI event loop, layout, XPC overlay sending
- `web/src/xpc.rs` — Minimal XPC FFI client (two-step connect via gateway)

### Build Commands

```bash
# Build the xpc-gateway (must be done before zig build)
cd ts5/xpc-gateway && swift build

# Build TermSurf (Zig + Metal shaders, bundles gateway into app)
cd ts5 && zig build

# Build web TUI
cargo build -p web
```

### Launching

The app launches normally via `open`. The xpc-gateway LaunchAgent is
auto-registered via SMAppService on first launch (Issue 506).

```bash
open ts5/zig-out/TermSurf.app
```

### Upstream Merges

ts5 uses `git subtree` (not `git merge -X subtree`) because the repo's rename
history breaks the subtree merge strategy. See Issue 418 Experiments 1–3.

```bash
# Pull latest upstream Ghostty
git fetch upstream
git subtree pull --prefix=ts5 upstream main -m "Merge upstream Ghostty into ts5"
```

## TermSurf 4.0 (ts4/) — Superseded

ts4 proved that in-process Chromium works: multiple browser profiles in one
process at 60fps. The PoC modified Chromium's `content_shell` inside the
Chromium source tree. Superseded by ts5, which forks Ghostty as the actual
application.

### Key Findings

- **Chromium is in-process.** The browser host runs inside the application
  process. Chromium spawns its own renderer and GPU sub-processes internally.
- **Multiple profiles in one process.** Chromium's `content::BrowserContext`
  supports multiple instances with different storage paths. Each gets isolated
  cookies, localStorage, and cache. The one-profile-per-process constraint was a
  CEF limitation, not a Chromium limitation (Issue 406).
- **No CEF.** CEF's headless off-screen rendering caps at ~31fps on macOS. The
  Content API eliminates every CEF limitation.

### How We Got Here

| Issue | Finding                                                                              |
| ----- | ------------------------------------------------------------------------------------ |
| 400   | Original ts4 vision: own everything, use Content API directly                        |
| 401   | Content API feasibility study; ~2000 lines of OSR code needed                        |
| 402   | WezTerm vs Alacritty for terminal (superseded by Issue 404)                          |
| 403   | Proved multi-process IOSurface compositing works at 60fps                            |
| 404   | Selected Ghostty as terminal emulator (Metal renderer, IOSurface)                    |
| 405   | Fork Ghostty with browser out-of-process (Option B selected)                         |
| 406   | Profile isolation is CEF-only; Content API supports multiple profiles; CEF ruled out |
| 407   | In-process Chromium PoC: two profiles, side by side, high framerate                  |
| 408   | Two profiles side by side at 60fps (content_shell)                                   |
| 409   | Apply Electron's full 147-patch set to termsurf-chromium                             |
| 410   | Apply partial Electron patches to fix 2-3fps throttling                              |
| 411   | Achieve 60fps two profiles without Electron patches                                  |
| 412   | Isolate 2fps cause in minimal one-profile content_shell app                          |
| 413   | Convert one-profile app (60fps) into two-profile app                                 |
| 414   | Two profiles via XPC at full speed (design experiment 2)                             |
| 415   | Reimplement Issue 414 receiver in Swift                                              |
| 416   | Reimplement Issue 414 receiver in Rust                                               |

### Issue 407 PoC (Completed)

The PoC modified Chromium's `content_shell` (the minimal Content API embedder)
inside the Chromium source tree. Two panes in one window, each with a different
browser profile, rendering at 60fps. This validated the architecture now being
implemented in ts5.

### Directory Structure

- `ts4/box-demo/public/index.html` — Test page (blue spinning square,
  localStorage, FPS)
- `ts4/box-demo/server.ts` — Bun HTTP server on port 9407
- `chromium/` — Chromium build workspace (gitignored, top level)
  - `src/` — Chromium source tree (git repo)
  - `src/content/shell/` — content_shell (the embedder we modify)
  - `src/out/Default/` — Build output
  - `depot_tools/` — Chromium build tools

### Build Commands

```bash
# Test page server
cd ts4/box-demo && bun run server.ts

# Chromium (depot_tools lives at chromium/depot_tools)
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
gn gen out/Default --args='is_debug=false symbol_level=0 enable_nacl=false is_component_build=true'
autoninja -C out/Default content_shell
```

### Profile Data

- `~/.config/termsurf/poc/profile-a/` — Profile A storage (PoC)
- `~/.config/termsurf/poc/profile-b/` — Profile B storage (PoC)

## TermSurf 3.0 (ts3/) — Superseded

ts3 used out-of-process CEF via XPC for browser rendering. Superseded by ts4
after 26 experiments (Issues 325–350) proved CEF's headless off-screen rendering
cannot sustain 60fps on macOS. The XPC and IOSurface patterns developed in ts3
remain valuable reference for ts4's fallback architecture.

### Foundational Constraint: One CEF Process Per Profile

**This is the defining architectural rule of ts3.** There must be exactly one
`termsurf-profile` process per browser profile. This is not a design preference
— it is a hard technical constraint:

- CEF's `SingletonLock` file prevents two processes from opening the same
  `root_cache_path`. A second process will crash or fail to initialize.
- CEF Chrome runtime (post-M128) ignores custom `cache_path` — the
  `root_cache_path` IS the profile. One process = one profile.
- Multiple webviews within a single profile process share cookies and storage.
  This is desired behavior — like tabs in a browser.

**Current gap:** The code today spawns a new process for every `web` command.
This is broken for the multi-webview case (two `web google.com` commands with
the same profile). The fix requires the launcher to detect an existing profile
process and send a "create browser" command to it instead of spawning a new one.

### Process Topology

```
User types: web google.com
    │
    ▼
CLI (web command) ──Unix socket──▶ GUI (WezTerm)
                                       │
                                       ▼
                                  XPC Manager
                                       │
                                       ▼
                              Launcher XPC Service
                                       │
                                       ▼
                              Profile Server (CEF)
                                       │
                                       ▼
                              CEF off-screen render
                                       │
                                       ▼
                              IOSurface ──Mach port──▶ GUI ──wgpu──▶ screen
```

### Key Binaries

- **wezterm-gui** — Terminal emulator. Receives IOSurface Mach ports via XPC,
  imports them as wgpu textures, renders webview panes alongside terminal panes.
- **termsurf-launcher** — XPC Mach service (`com.termsurf.launcher`). Spawns
  profile server processes. Relays XPC endpoints between GUI and profile servers
  to enable direct Mach port transfer.
- **termsurf-profile** — One instance per browser profile. Runs CEF off-screen
  rendering. Sends IOSurface Mach ports to GUI when pages render. Manages all
  webviews for its profile.

### Cross-Process IOSurface Sharing

IOSurface IDs are process-local and cannot be shared across processes. Mach
ports can. The sharing flow:

1. GUI creates an anonymous XPC listener, sends its endpoint to the launcher
2. Launcher stores the endpoint, spawns a profile server
3. Profile server claims the endpoint from the launcher (with retry/backoff)
4. Profile server connects directly to GUI via the endpoint
5. CEF renders to IOSurface (`shared_texture_enabled`)
6. Profile server creates a Mach port from the IOSurface handle
   (`IOSurfaceCreateMachPort`)
7. Mach port sent to GUI via XPC (`set_mach_send` / `copy_mach_send`)
8. GUI imports IOSurface from Mach port (`IOSurfaceLookupFromMachPort`)
9. GUI creates wgpu texture from IOSurface for rendering

### IPC Architecture

| Channel              | Transport                       | Protocol       |
| -------------------- | ------------------------------- | -------------- |
| CLI ↔ GUI            | Unix domain socket (`/tmp/`)    | JSON messages  |
| GUI ↔ Launcher       | XPC Mach service                | XPC dictionary |
| GUI ↔ Profile Server | XPC anonymous endpoint (direct) | XPC dictionary |

Note: XPC dictionaries have no `set_f64`/`set_f32` — the scale factor is passed
as a string.

### CEF and Retina Handling

CEF works in logical pixels:

- `view_rect()` returns logical dimensions (e.g., 800x600)
- `screen_info()` returns `device_scale_factor` (e.g., 2.0 for Retina)
- CEF multiplies internally to get physical IOSurface size (e.g., 1600x1200)

Scale factor: `dpi / 72.0` (macOS base DPI = 72, Retina = 144 → scale 2.0). Pane
dimensions come from `Mux::try_get()` → `get_pane()` → `get_dimensions()`, which
returns `pixel_width`, `pixel_height`, `dpi` and is safe to call from any
thread.

### Current Implementation Status

| Feature                           | Status      |
| --------------------------------- | ----------- |
| Single webview per profile        | Working     |
| Dynamic initial pane sizing       | Working     |
| Profile path isolation            | Working     |
| Debug logging to `/tmp/`          | Working     |
| Multi-webview per profile         | Not started |
| Dynamic resize on pane change     | Not started |
| Input forwarding (keyboard/mouse) | Not started |
| Profile process reuse             | Not started |

### Build Commands

```bash
cd ts3 && ./scripts/build-debug.sh [--open] [--clean]
cd ts3 && ./scripts/build-release.sh [--open] [--clean]
```

Logs are written to `/tmp/`:

- `~/dev/termsurf/logs/termsurf-gui.log` — GUI process output
- `~/dev/termsurf/logs/termsurf-launcher.log` — Launcher output
- `~/dev/termsurf/logs/termsurf-profile-{session_id}.log` — Per-session profile
  server output

### Directory Structure and Key Files

**TermSurf-specific crates:**

- `ts3/termsurf-launcher/` — XPC launcher service
  - `src/main.rs` — Listens on `com.termsurf.launcher`, handles `spawn_profile`
    and `claim_session` actions
- `ts3/termsurf-profile/` — CEF profile server
  - `src/main.rs` — CLI args, CEF initialization, render handler that sends
    IOSurface Mach ports, context menu suppression
- `ts3/termsurf-xpc/` — Shared XPC bindings crate
  - `src/ffi.rs` — Raw XPC FFI bindings
  - `src/iosurface.rs` — IOSurface Mach port creation/lookup
- `ts3/termsurf-web/` — Web browser coordinator
- `ts3/termsurf-test-sender/` — Test harness for XPC experiments

**Modified WezTerm files:**

- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` — Unix socket handler for
  `web` command. Looks up pane dimensions via Mux, triggers XPC profile spawn.
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC manager (GUI side).
  Creates listeners, stores received IOSurface Mach ports, maps sessions to
  panes.

**Build scripts:**

- `ts3/scripts/build-debug.sh` — Debug build with `open --stdout --stderr`
- `ts3/scripts/build-release.sh` — Release build

**Profile data:**

- `~/.config/termsurf/cef/<profile>/` — Per-profile CEF data (cookies, cache,
  storage). Not `~/Library/Application Support/` — deliberately cross-platform.

## TermSurf 2.0 (ts2/) — Superseded

ts2 embedded CEF directly inside WezTerm's process. CEF allows only one
`root_cache_path` per process, which means one browser profile per application.
TermSurf requires multiple profiles (like Chrome profiles), so CEF had to move
to separate processes — one per profile. That's ts3.

Historical docs: `docs/issues/ts2-*.md`

## TermSurf 1.x (ts1/) — Legacy (superseded by ts5)

Ghostty fork with WKWebView for browser panes. macOS-only. Superseded by ts5
which starts from a clean upstream Ghostty and will use Chromium instead of
WKWebView.

### Commands

- **Build (Debug):** `cd ts1 && ./scripts/build-debug.sh` →
  `ts1/build/debug/TermSurf.app`
- **Build (Release):** `cd ts1 && ./scripts/build-release.sh` →
  `ts1/build/release/TermSurf.app`
- **Build & Open:** Add `--open` flag to either script
- **Clean Build:** Add `--clean` flag to either script
- **Zig build:** `cd ts1 && zig build`
- **Zig test:** `cd ts1 && zig build test`
- **Zig test filter:** `cd ts1 && zig build test -Dtest-filter=<test name>`
- **Zig format:** `cd ts1 && zig fmt .`

### Directory Structure

- `ts1/src/` — Shared Zig core (libghostty)
- `ts1/termsurf-macos/` — TermSurf macOS app (Swift + Xcode)
- `ts1/macos/` — Original Ghostty macOS app
- `ts1/include/` — C API headers
- `ts1/src/cli/web.zig` — CLI web command

### Browser Integration

Uses WKWebView (Apple's WebKit) with console message capture, Safari Web
Inspector support, session isolation via WKWebsiteDataStore, and an optional
JavaScript API (`--js-api` flag).

## cef-rs (`vendor/cef-rs/`)

Third-party CEF (Chromium Embedded Framework) Rust bindings, imported and
modified for TermSurf. Used by `ts3/termsurf-profile/` for off-screen browser
rendering.

### TermSurf Modifications to the Library

These are changes to `vendor/cef-rs/cef/src/` (the library itself, not
examples):

1. **IOSurface Metal API crash fix** — The original code used
   `std::mem::transmute` to cast raw pointers to Metal API references, causing
   memory corruption. Replaced with properly typed references via the `objc`
   crate. (`vendor/cef-rs/cef/src/osr_texture_import/iosurface.rs`)

2. **sRGB double-correction fix** — CEF outputs sRGB pixel data, but the texture
   pipeline applied gamma correction a second time, washing out all colors.
   Fixed by declaring the correct sRGB format on texture views.
   (`vendor/cef-rs/cef/src/osr_texture_import/common.rs`, `iosurface.rs`)

3. **IOSurface IPC module (failed experiment)** — Added `iosurface_ipc.rs` to
   share IOSurface across processes via IOSurface IDs. This failed because
   IOSurface IDs are process-local. This failure directly motivated the Mach
   port approach used in ts3. Module is deprecated.

4. **Mach port support for IOSurface** — Extended `iosurface.rs` with
   `IOSurfaceCreateMachPort` and `IOSurfaceLookupFromMachPort` for cross-process
   texture sharing. This is what ts3 uses to send rendered surfaces from the
   profile server to the GUI.

### OSR Example Validation

The OSR (off-screen rendering) example in `vendor/cef-rs/examples/osr/` was used
as a testbed before ts1 integration. Changes made to the example:

| Feature                    | Status     | Notes                                       |
| -------------------------- | ---------- | ------------------------------------------- |
| IOSurface texture import   | Working    | Fixed Metal API types in `iosurface.rs`     |
| Input handling             | Working    | Keyboard, mouse, scroll all functional      |
| Multiple browser instances | Working    | Per-instance TextureHolder, HashMap routing |
| Event-driven rendering     | Working    | Render only when CEF signals new frame      |
| Resize handling            | Working    | Browser resizes with window                 |
| Context menu               | Suppressed | Prevents winit NSApplication crash          |
| macOS terminal launch      | Fixed      | NSApp activation policy for multi-browser   |
| Fullscreen                 | Broken     | winit issue, defer to WezTerm               |

### Commands

- **Build:** `cd vendor/cef-rs && cargo build`
- **Build OSR example:** `cd vendor/cef-rs && cargo build -p cef-osr`
- **Bundle and run (macOS):**
  ```bash
  cd vendor/cef-rs
  cargo build -p cef-osr
  cargo run -p bundle-cef-app -- cef-osr -o cef-osr.app
  ./cef-osr.app/Contents/MacOS/cef-osr
  ```

### Key Files

- `vendor/cef-rs/cef/` — Main CEF wrapper crate
- `vendor/cef-rs/cef/src/osr_texture_import/` — Texture import (IOSurface on
  macOS, DMA-BUF on Linux, D3D11 on Windows)
- `vendor/cef-rs/cef/src/osr_texture_import/iosurface.rs` — IOSurface import +
  Mach port creation/lookup (modified for TermSurf)
- `vendor/cef-rs/cef/src/osr_texture_import/common.rs` — Shared texture handling
  (modified for sRGB fix)
- `vendor/cef-rs/examples/osr/` — Off-screen rendering example (validation
  testbed)
- `vendor/cef-rs/sys/` — Low-level CEF C API bindings (unmodified)
- `vendor/cef-rs/update-bindings/` — Tool to regenerate bindings from CEF
  headers

### Notes

- CEF binaries are downloaded automatically by the build system
- macOS apps must be bundled with `bundle-cef-app` to include CEF framework

## Documentation

### TermSurf 5.0 (active)

- `docs/issues/417-ghostty-vs-wezterm.md` — Terminal emulator selection
  (Ghostty)
- `docs/issues/418-repo-restructure.md` — Repo restructure and Ghostty import
- `docs/issues/500-rename.md` — Rename Ghostty references to TermSurf in ts5
- `docs/issues/501-two-profiles.md` — Two-profile browser demo in ts5
- `docs/issues/502-attach-delay.md` — Eliminate hardcoded capturer attach delay
- `docs/issues/503-one-two-three.md` — One, two, or three profiles (one process
  per profile)
- `docs/issues/504-web-tui.md` — `web` TUI chrome (ratatui terminal app)
- `docs/issues/505-pink-texture.md` — Pink texture overlay (GPU quad via XPC)

### TermSurf 4.0

- `docs/issues/400-a-new-hope.md` — Original ts4 vision and architecture sketch
- `docs/issues/401-chromium-feasibility.md` — Content API surface analysis
- `docs/issues/401-programming-language.md` — Language selection (Rust + C++)
- `docs/issues/402-wezterm-vs-alacritty.md` — Terminal emulator comparison
  (superseded by Issue 404)
- `docs/issues/403-swift-rust-cpp.md` — Multi-process IOSurface compositing PoC
- `docs/issues/404-terminal-emulator.md` — Terminal emulator evaluation (Ghostty
  selected)
- `docs/issues/405-architecture-comparison.md` — In-process vs out-of-process
  terminal (Ghostty fork selected)
- `docs/issues/406-chromium.md` — Profile isolation analysis; CEF ruled out
- `docs/issues/407-chromium-poc.md` — In-process Chromium PoC plan
- `docs/issues/408-two-profiles.md` — Two profiles side by side at 60fps
- `docs/issues/409-electron-patch.md` — Electron's full 147-patch set
- `docs/issues/410-two-profiles-2.md` — Two profiles attempt 2
- `docs/issues/410-partial-electron.md` — Partial Electron patches for fps fix
- `docs/issues/411-two-profiles-3.md` — 60fps two profiles without Electron
- `docs/issues/412-one-profile.md` — Isolate 2fps cause in one-profile app
- `docs/issues/413-one-profile-2.md` — One-profile to two-profile conversion
- `docs/issues/414-two-profiles-xpc.md` — Two profiles via XPC at full speed
- `docs/issues/415-swift-receiver.md` — Issue 414 receiver reimplemented in
  Swift
- `docs/issues/416-rust-receiver.md` — Issue 414 receiver reimplemented in Rust

### TermSurf 3.0

- `docs/issues/301-architecture.md` — High-level architecture overview
- `docs/issues/302-webview.md` — Webview rendering implementation
- `docs/issues/303-xpc.md` — XPC architecture for inter-process communication
- `docs/issues/304-webpage.md` — Webpage rendering solutions
- `docs/issues/305-profile.md` — Profile isolation for browser data
- `docs/issues/306-resize.md` — Resize support implementation

### TermSurf 2.0 (historical)

- `docs/issues/200-architecture.md` — Technical decisions and design rationale
- `docs/issues/201-cef.md` — CEF integration via cef-rs
- `docs/issues/207-cef-wezterm.md` — CEF + WezTerm integration details
- `docs/issues/202-cef-mvp.md` through `206-cef-mvp5.md` — MVP iteration
  experiments
- `docs/issues/208-profile.md` — CEF browser profile research
- `docs/issues/209-web.md` — Web command experiments
- `docs/issues/210-wezterm-analysis.md` — WezTerm + cef-rs architecture analysis

### TermSurf 1.x (legacy)

- `docs/issues/100-bookmarks.md` — Bookmarks implementation plan
- `docs/issues/101-build.md` — Build instructions and troubleshooting
- `docs/issues/102-console.md` — Console bridging and JavaScript API
- `docs/issues/103-ctrl-z.md` — ctrl+z/fg analysis (deferred)
- `docs/issues/104-keybindings.md` — Webview keyboard shortcuts and modes
- `docs/issues/105-libghostty.md` — Changes to libghostty
- `docs/issues/106-release.md` — Release procedure and versioning
- `docs/issues/107-target-blank.md` — target="_blank" link handling
- `docs/issues/108-webview.md` — WebView implementation and API checklist

### General

- `docs/issues/002-merge-upstream.md` — How to merge changes from upstream repos
- `docs/issues/001-competitors.md` — Terminal-browser hybrid comparison
- `docs/issues/003-website.md` — termsurf.com website

## AI Reminder

Never change any code unless the user explicitly asks. If you are unsure if
changing the code is what the user wants, ask the user first. If the user asks a
question, then answer the question WITHOUT modifying any code. If you need to
modify code to answer a question, then confirm with the user first that this is
what they want. Only make changes to the code after the user has granted
approval.
