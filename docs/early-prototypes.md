# Early Prototypes

Historical documentation for TermSurf's five prototype generations (ts1–ts5) and
the cef-rs dependency. These prototypes are no longer in the working tree but
are preserved in git history. See the [Archive Log](#archive-log) for recovery
instructions.

For the active codebase, see [CLAUDE.md](../CLAUDE.md).

## Archive Log

| What             | Commit          | Date       | Notes                                                                                                                                                                 |
| ---------------- | --------------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vendor/cef-rs/` | `2c7c5d7`       | 2026-02-21 | CEF Rust bindings (ts2, ts3)                                                                                                                                          |
| `ts1/`           | `0bdf837`       | 2026-02-25 | Ghostty + WKWebView                                                                                                                                                   |
| `ts2/`           | `0bdf837`       | 2026-02-25 | WezTerm + in-process CEF                                                                                                                                              |
| `ts3/`           | `0bdf837`       | 2026-02-25 | WezTerm + out-of-process CEF                                                                                                                                          |
| `ts4/`           | `0bdf837`       | 2026-02-25 | Chromium Content API PoC                                                                                                                                              |
| `ts5/`           | `0bdf837`       | 2026-02-25 | Ghostty + out-of-process Chromium                                                                                                                                     |
| `ghostboard/`    | `90b966458bd17` | 2026-03-11 | Ghostboard Legacy (Ghostty fork, Zig). Archived to focus on Wezboard during protocol iteration. Will be re-created from fresh Ghostty fork after protocol stabilizes. |

To recover a directory:

```bash
git checkout <commit>~1 -- <directory>
```

## TermSurf 5.0 (ts5/) — Superseded by TermSurf GUI

### Architecture

ts5 forks Ghostty as the application — terminal panes are native, in-process
Ghostty rendering. Browser panes will embed Chromium directly via the Content
API (not CEF, which cannot sustain 60fps headless). This combines the ts1
approach (Ghostty as the app) with the ts4 finding (in-process Chromium works).

```
Ghostty Fork (Zig + Swift macOS shell)
├── Terminal panes (in-process, native Ghostty rendering)
├── Browser panes (out-of-process Chromium streaming, in-process is the endgame)
│   └── IOSurface overlay pipeline — Metal texture from Chromium at 120fps
├── CompositorXPC (connects to xpc-gateway, manages Chromium servers)
│   ├── Receives overlay coordinates and URLs from `web` processes
│   ├── Spawns/reuses Chromium Profile Servers (one per browser profile)
│   ├── Receives IOSurface Mach ports at 120fps from servers
│   └── Passes IOSurface + coordinates to renderer via C API
├── Metal renderer (inherited from Ghostty)
│   └── overlay pipeline — composites IOSurface texture at grid coordinates
├── Pane/tab/split management (inherited from Ghostty)
└── Keybindings, configuration (inherited from Ghostty)

Chromium Profile Server (one process per browser profile)
├── Chromium fork built via Content API (not CEF)
├── Accepts create_tab/close_tab/resize commands via XPC
├── Runs FrameSinkVideoCapturer at 120fps (Issue 512 vsync fix)
├── Sends IOSurface Mach ports to app at 120fps per tab
└── Auto-exits when last tab closes

xpc-gateway daemon (com.termsurf.xpc-gateway Mach service)
├── Tiny Swift binary (~80 lines), auto-registered via SMAppService
├── Stores app's anonymous listener endpoint
└── Returns endpoint to `web` and Chromium server processes (rendezvous only)

web TUI (Rust/ratatui, runs inside a terminal pane)
├── Draws browser chrome (URL bar, viewport border, status bar)
├── Connects to xpc-gateway, claims app endpoint, then connects directly to app
├── Sends viewport grid coordinates + URL on the direct connection
└── TERMSURF_PANE_ID env var identifies which pane it's in
```

### Current State

ts5 is a Ghostty fork (imported via `git subtree add`) with the following
TermSurf additions:

- **XPC gateway** (`xpc-gateway/`) — Tiny Swift daemon that owns the
  `com.termsurf.xpc-gateway` Mach service. The app registers an anonymous
  listener endpoint here; `web` and Chromium server processes claim it to
  connect directly to the app. Auto-registered via SMAppService (Issue 506).
- **CompositorXPC** (`CompositorXPC.swift`) — Connects to the xpc-gateway,
  creates an anonymous listener, and registers its endpoint. Manages the full
  Chromium streaming lifecycle: receives overlay coordinates and URLs from `web`
  processes, spawns/reuses Chromium Profile Servers (one per browser profile),
  receives IOSurface Mach ports at 120fps, and passes them to the renderer via
  the C API (Issues 509–512).
- **IOSurface overlay pipeline** (`overlay` in `shaders.zig` / `shaders.metal`)
  — Metal shader that composites a Chromium IOSurface texture at grid
  coordinates. Zero-copy GPU memory — the texture is a view into the same
  IOSurface that Chromium rendered into (Issue 508).
- **120fps vsync oversampling** — The Chromium capturer runs at 120fps (2x the
  display rate) so there is always a fresh frame at every 60Hz vsync. Combined
  with the `overlay_surface_changed` flag that ensures every new IOSurface
  triggers a redraw (Issue 512).
- **Multi-profile server reuse** — Multiple panes sharing the same browser
  profile share one Chromium Profile Server process. Panes with different
  profiles get separate servers. Server auto-exits when its last tab closes
  (Issue 511).
- **Dynamic resize** — Pane resize propagates through XPC to the Chromium
  capturer, which adjusts IOSurface resolution in real time (Issue 510).
- **Retina resolution** — IOSurface capture at physical pixel dimensions. Cell
  size queries use font metrics for pixel-perfect grid alignment (Issue 509).
- **C API bridge** (`ghostty_surface_set_overlay` / `set_overlay_iosurface` /
  `clear_overlay`) — Lets Swift XPC code set overlay coordinates and IOSurface
  textures on the Zig renderer thread-safely via `draw_mutex`.
- **Pane ID propagation** — Each surface sets `TERMSURF_PANE_ID` (UUID) in the
  shell environment, inherited by child processes including `web`.

**Not yet started:** In-process Chromium embedding via the Content API (proven
in ts4's PoC). Currently Chromium runs out-of-process as the Chromium Profile
Server, streaming IOSurface frames over XPC. In-process embedding will eliminate
XPC overhead and enable single-clock vsync. Also not started: keyboard/mouse
input forwarding, navigation, and other browser interaction features.

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
- `ts5/xpc-gateway/Sources/main.swift` — Gateway: owns Mach service,
  stores/returns app endpoint
- `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — XPC client (connects to
  gateway, registers endpoint)
- `ts5/macos/Sources/App/macOS/AppDelegate.swift` — Starts compositor XPC on
  launch
- `ts5/macos/com.termsurf.xpc-gateway.plist` — launchd plist (dev, absolute
  paths)
- `ts5/macos/com.termsurf.xpc-gateway.bundle.plist` — launchd plist (bundled,
  BundleProgram)
- `ts5/build.zig` — Ghostty build system
- `ts5/build.zig.zon` — Ghostty dependencies
- `ts5/pkg/` — Platform packages (Linux, macOS, etc.)
- `tui/` — `web` TUI (Rust/ratatui)
- `tui/src/main.rs` — TUI event loop, layout, XPC overlay sending
- `tui/src/xpc.rs` — Minimal XPC FFI client (two-step connect via gateway)

### Build Commands

```bash
# Build the xpc-gateway (must be done before zig build)
cd ts5/xpc-gateway && swift build

# Build TermSurf (Zig + Metal shaders, bundles gateway into app)
cd ts5 && zig build

# Build web TUI
cd tui && cargo build
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

## Ghostboard Legacy (ghostboard/) — Archived

### Architecture

Ghostboard forks Ghostty with all browser integration in Zig. Swift remains a
thin macOS wrapper — window creation, menu bar, application lifecycle — matching
Ghostty's own architecture. This is a clean break from ts5, where browser
integration lived in Swift (CompositorXPC.swift).

Key architectural decisions:

- **Socket IPC in Zig.** All IPC uses Unix domain sockets with length-prefixed
  protobuf. Ghostboard listens on
  `$TMPDIR/termsurf/termsurf-ghostboard-{pid}.sock`. TUI and Chromium connect as
  clients. The IPC module (`ghostboard/src/apprt/xpc.zig`) handles accept,
  framing, dispatch, and per-connection lifecycle.
- **CALayerHost in Zig.** Browser panes render via `CALayerHost` — a CALayer
  subclass that displays a remote `CAContext` from Chromium's GPU process.
  Window Server composites directly from GPU VRAM. Zero per-frame IPC, zero
  texture copies. The Metal renderer sets up the CALayerHost layer tree in Zig.
- **Input routing in Zig.** Zig already receives all keyboard/mouse events
  through `Surface.keyCallback()` and `mouseButtonCallback()`. In browse mode,
  these route to Chromium via socket IPC instead of to the terminal.
- **Single source of truth.** Browse mode, focus state, pane profiles, overlay
  coordinates — all live in Zig's Surface struct.

### Current State

Ghostboard is a Ghostty fork with browser integration built in Zig. Current
additions: IPC gateway and connection management (Issues 601, 698–702), pink
texture proof-of-concept (Issue 602), live Chromium streaming at 60fps with
dynamic resize (Issue 603), multi-pane multi-profile server reuse (Issues
604–605), mouse input forwarding with cursor changes and text selection (Issue
606), keyboard input forwarding with Cmd+key bypass (Issues 607–609), branding
and app icon (Issues 611–612), directory rename from ghost/web to gui/tui (Issue
613), XDG directory compliance (Issue 615), loading progress indicator and
browser navigation keybindings (Issue 616), CALayerHost migration replacing
FrameSinkVideoCapturer with zero-copy Window Server compositing (Issues
624–632), reproducible rename script for upstream merges (Issue 656), purple
Edit mode border (Issue 657), vim-like editor modes and keybindings (Issue 658),
vim-style command mode (Issue 659), per-mode submode colors (Issue 660), tight
title spacing (Issue 661), clap CLI parser with subcommands (Issue 664),
context-sensitive Esc key navigation (Issue 665), Esc latency fix via unified
mpsc channel (Issue 666), active pane indicator with borders and desaturation
(Issues 667–669), click-to-focus without pass-through (Issue 670), app icon
update (Issue 671), inner border padding (Issue 672), script consolidation
(Issue 673), configurable homepage (Issue 674), hello message for live config
(Issue 675), URL normalization (Issue 676), website deps and linting (Issues
677–678), MIT license and trademark (Issue 679), dark mode with `:colorscheme`
command (Issue 680), `:quitall` and subsequence matching (Issue 681), Chrome
DevTools in split panes (Issues 684, 687, 690–691), multi-profile tracking fix
(Issue 685), tab lifecycle — close tabs when panes close (Issue 689), `web file`
subcommand (Issue 692), smart input resolution (Issue 693), replace pane_id with
tab_id in Chromium (Issue 694), activation drag suppression (Issue 695), double
click suppression fix (Issue 696), Unix socket + protobuf IPC replacing XPC
(Issues 698–702), click suppression removal (Issue 703), browser bindings C
library (Issues 704–706), Roamium Rust browser binary (Issue 707), clean
Chromium fork with renamed libtermsurf_chromium (Issue 708).

### Source Layout

- `ghostboard/src/` — Shared Zig core (libtermsurf)
- `ghostboard/src/Surface.zig` — Core surface (holds browser state)
- `ghostboard/src/renderer/Metal.zig` — Metal renderer
- `ghostboard/src/renderer/metal/` — Metal pipeline, shaders, IOSurface layer
- `ghostboard/src/apprt/embedded.zig` — C API exports
- `ghostboard/include/ghostty.h` — libghostty C API headers
- `ghostboard/macos/` — macOS app (Swift, thin wrapper)
- `ghostboard/build.zig` — Build system
- `ghostboard/build.zig.zon` — Dependencies

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

Historical docs: `issues/0200-*.md` through `issues/0210-*.md`

## TermSurf 1.x (ts1/) — Legacy

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

## Issue Documentation Index

### TermSurf 5.0

- `issues/0417-ghostty-vs-wezterm.md` — Terminal emulator selection (Ghostty)
- `issues/0418-repo-restructure.md` — Repo restructure and Ghostty import
- `issues/0500-rename.md` — Rename Ghostty references to TermSurf in ts5
- `issues/0501-two-profiles.md` — Two-profile browser demo in ts5
- `issues/0502-attach-delay.md` — Eliminate hardcoded capturer attach delay
- `issues/0503-one-two-three.md` — One, two, or three profiles (one process per
  profile)
- `issues/0504-web-tui.md` — `web` TUI chrome (ratatui terminal app)
- `issues/0505-pink-texture.md` — Pink texture overlay (GPU quad via XPC)
- `issues/0506-smappservice.md` — SMAppService for xpc-gateway registration
- `issues/0507-chromium.md` — First Chromium streaming attempt (IOSurface
  crashes)
- `issues/0508-checkerboard.md` — IOSurface overlay pipeline (Metal texture from
  IOSurface)
- `issues/0509-chromium.md` — Chromium streaming (retry), Retina resolution
- `issues/0510-two-profiles.md` — Two-profile streaming, dynamic resize
- `issues/0511-three-profiles.md` — Three profiles, server reuse per profile
- `issues/0512-vsync.md` — Vsync desynchronization, 120fps oversampling fix
- `issues/0513-ctrl-esc.md` — Ctrl+Esc escape hatch (mode switching)
- `issues/0514-mouse.md` — Mouse clicks and URL sync
- `issues/0515-drag.md` — Focus state and text selection

### TermSurf 4.0

- `issues/0400-a-new-hope.md` — Original ts4 vision and architecture sketch
- `issues/0401-chromium-feasibility.md` — Content API surface analysis
- `issues/0401-programming-language.md` — Language selection (Rust + C++)
- `issues/0402-wezterm-vs-alacritty.md` — Terminal emulator comparison
  (superseded by Issue 404)
- `issues/0403-swift-rust-cpp.md` — Multi-process IOSurface compositing PoC
- `issues/0404-terminal-emulator.md` — Terminal emulator evaluation (Ghostty
  selected)
- `issues/0405-architecture-comparison.md` — In-process vs out-of-process
  terminal (Ghostty fork selected)
- `issues/0406-chromium.md` — Profile isolation analysis; CEF ruled out
- `issues/0407-chromium-poc.md` — In-process Chromium PoC plan
- `issues/0408-two-profiles.md` — Two profiles side by side at 60fps
- `issues/0409-electron-patch.md` — Electron's full 147-patch set
- `issues/0410-two-profiles-2.md` — Two profiles attempt 2
- `issues/0410-partial-electron.md` — Partial Electron patches for fps fix
- `issues/0411-two-profiles-3.md` — 60fps two profiles without Electron
- `issues/0412-one-profile.md` — Isolate 2fps cause in one-profile app
- `issues/0413-one-profile-2.md` — One-profile to two-profile conversion
- `issues/0414-two-profiles-xpc.md` — Two profiles via XPC at full speed
- `issues/0415-swift-receiver.md` — Issue 414 receiver reimplemented in Swift
- `issues/0416-rust-receiver.md` — Issue 414 receiver reimplemented in Rust

### TermSurf 3.0

- `issues/0301-architecture.md` — High-level architecture overview
- `issues/0302-webview.md` — Webview rendering implementation
- `issues/0303-xpc.md` — XPC architecture for inter-process communication
- `issues/0304-webpage.md` — Webpage rendering solutions
- `issues/0305-profile.md` — Profile isolation for browser data
- `issues/0306-resize.md` — Resize support implementation

### TermSurf 2.0

- `issues/0200-architecture.md` — Technical decisions and design rationale
- `issues/0201-cef.md` — CEF integration via cef-rs
- `issues/0207-cef-wezterm.md` — CEF + WezTerm integration details
- `issues/0202-cef-mvp.md` through `206-cef-mvp5.md` — MVP iteration experiments
- `issues/0208-profile.md` — CEF browser profile research
- `issues/0209-web.md` — Web command experiments
- `issues/0210-wezterm-analysis.md` — WezTerm + cef-rs architecture analysis

### TermSurf 1.x

- `issues/0100-bookmarks.md` — Bookmarks implementation plan
- `issues/0101-build.md` — Build instructions and troubleshooting
- `issues/0102-console.md` — Console bridging and JavaScript API
- `issues/0103-ctrl-z.md` — ctrl+z/fg analysis (deferred)
- `issues/0104-keybindings.md` — Webview keyboard shortcuts and modes
- `issues/0105-libghostty.md` — Changes to libghostty
- `issues/0106-release.md` — Release procedure and versioning
- `issues/0107-target-blank.md` — target="\_blank" link handling
- `issues/0108-webview.md` — WebView implementation and API checklist
