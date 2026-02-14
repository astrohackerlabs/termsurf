# Agent Development Guide

A file for [guiding coding agents](https://agents.md/).

## AI Guidance

Never under any circumstances change the code unless explicitly asked by the
user. When in doubt, ask the user before making any changes.

## Project Overview

TermSurf is a terminal emulator with an integrated web browser. Users type
`web google.com` in their terminal and a webpage renders directly in the
terminal pane, sharing cookies and sessions across tabs within the same browser
profile.

The project has evolved through four generations:

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
- **ts4** (Ghostty fork + in-process Chromium) — **Active development.** Forks
  Ghostty as the application (terminal panes are native, in-process). Embeds
  Chromium directly via the Content API (not CEF) for browser panes, with
  multiple browser profiles coexisting in one process. No custom XPC protocol.

**Directory structure:**

- `ts4/` — TermSurf 4.0 (Ghostty fork + in-process Chromium). Active work.
- `ts3/` — TermSurf 3.0 (WezTerm fork + out-of-process CEF). Superseded.
- `ts2/` — TermSurf 2.0 (WezTerm fork + in-process CEF). Superseded.
- `ts1/` — TermSurf 1.x (Ghostty fork + WKWebView). Legacy, still builds.
- `cef-rs/` — CEF Rust bindings. Used by `ts3/termsurf-profile/`.
- `docs/issues/` — All documentation across all generations.

## TermSurf 4.0 (ts4/) — Active Development

### Architecture

ts4 returns to the ts1 approach — fork Ghostty as the application — with the
critical fix: replace WKWebView (which was too limited) with Chromium embedded
directly via the Content API (not CEF, which cannot sustain 60fps headless).

```
Ghostty Fork (Zig + Swift macOS shell)
├── Terminal panes (in-process, native Ghostty rendering)
├── Browser panes (in-process Chromium via Content API)
│   ├── BrowserContext "work" (Profile 1)
│   ├── BrowserContext "personal" (Profile 2)
│   └── BrowserContext "guest" (Profile N)
├── Pane/tab/split management (inherited from Ghostty)
├── Keybindings, configuration (inherited from Ghostty)
└── Metal renderer (inherited from Ghostty)
```

**Key architectural decisions:**

- **Chromium is in-process.** The browser host runs inside the Ghostty fork's
  process. Chromium may still spawn its own renderer and GPU sub-processes
  internally (this is Chromium's own multi-process architecture and is
  expected). We do not invent a custom XPC or IPC protocol.
- **Multiple profiles in one process.** Chromium's `content::BrowserContext`
  supports multiple instances with different storage paths. Each gets isolated
  cookies, localStorage, and cache. This is what Chrome and Electron do. The
  one-profile-per-process constraint was a CEF limitation, not a Chromium
  limitation (Issue 406).
- **No CEF.** CEF's headless off-screen rendering caps at ~31fps on macOS with
  an event-driven pump, or ~50fps with a busy-wait loop at 100% CPU. 26
  experiments across Issues 325–350 proved this is architectural, not
  configurable. The Content API eliminates every CEF limitation.
- **Fallback.** If multiple profiles cannot coexist in one process in practice,
  we may fall back to a multi-process approach with one Chromium process per
  profile, communicating via XPC (similar to ts3's architecture but with the
  Content API instead of CEF).

### How We Got Here

| Issue | Finding |
| ----- | ------- |
| 400   | Original ts4 vision: own everything, use Content API directly |
| 401   | Content API feasibility study; ~2000 lines of OSR code needed |
| 402   | WezTerm vs Alacritty for terminal (superseded by Issue 404) |
| 403   | Proved multi-process IOSurface compositing works at 60fps |
| 404   | Selected Ghostty as terminal emulator (Metal renderer, IOSurface) |
| 405   | Fork Ghostty with browser out-of-process (Option B selected) |
| 406   | Profile isolation is CEF-only; Content API supports multiple profiles; CEF ruled out |
| 407   | In-process Chromium PoC: two profiles, side by side, high framerate |

### Current Work: Issue 407 PoC

The PoC modifies Chromium's `content_shell` (the minimal Content API embedder)
inside the Chromium source tree. The modifications are small (~5 files) and use
Chromium's native windowed rendering. The resulting `.app` bundle is built by
Chromium's GN/Ninja build system.

**Phases:**

1. **Test page** — Bun app serving a blue spinning square with localStorage
   identity and FPS counter (`ts4/box-demo/public/index.html`, `ts4/box-demo/server.ts`)
2. **Merge Chromium** — Fork Chromium into `ts4/termsurf-chromium/` following
   the merge-upstream pattern (depot_tools fetch, move to subdirectory, merge
   unrelated histories)
3. **Build Chromium** — Configure GN, build content_shell with
   `autoninja -C out/Default content_shell`
4. **Modify content_shell** — Add second `ShellBrowserContext` with different
   storage path, display two `WebContents` side by side in one window
5. **Measure** — Verify profile isolation, measure framerate, document findings

**Success criteria:** Two panes in one window, each showing a different
localStorage string that persists across restarts. Both rendering at 60fps or
higher. No custom XPC/IPC protocol.

### Directory Structure

- `ts4/box-demo/public/index.html` — Test page (blue spinning square, localStorage, FPS)
- `ts4/box-demo/server.ts` — Bun HTTP server on port 9407
- `ts4/termsurf-chromium/` — Chromium source tree (fork)
  - `content/shell/` — content_shell (the embedder we modify)
  - `out/Default/` — Build output (gitignored)

### Build Commands

```bash
# Test page server
cd ts4/box-demo && bun run server.ts

# Chromium (depot_tools lives at ts4/termsurf-chromium/depot_tools)
cd ts4/termsurf-chromium/src
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
- `~/dev/termsurf/logs/termsurf-profile-{session_id}.log` — Per-session profile server output

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

## TermSurf 1.x (ts1/) — Legacy

Ghostty fork with WKWebView for browser panes. macOS-only. Still builds.

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

## cef-rs

Third-party CEF (Chromium Embedded Framework) Rust bindings, imported and
modified for TermSurf. Used by `ts3/termsurf-profile/` for off-screen browser
rendering.

### TermSurf Modifications to the Library

These are changes to `cef-rs/cef/src/` (the library itself, not examples):

1. **IOSurface Metal API crash fix** — The original code used
   `std::mem::transmute` to cast raw pointers to Metal API references, causing
   memory corruption. Replaced with properly typed references via the `objc`
   crate. (`cef-rs/cef/src/osr_texture_import/iosurface.rs`)

2. **sRGB double-correction fix** — CEF outputs sRGB pixel data, but the texture
   pipeline applied gamma correction a second time, washing out all colors.
   Fixed by declaring the correct sRGB format on texture views.
   (`cef-rs/cef/src/osr_texture_import/common.rs`, `iosurface.rs`)

3. **IOSurface IPC module (failed experiment)** — Added `iosurface_ipc.rs` to
   share IOSurface across processes via IOSurface IDs. This failed because
   IOSurface IDs are process-local. This failure directly motivated the Mach
   port approach used in ts3. Module is deprecated.

4. **Mach port support for IOSurface** — Extended `iosurface.rs` with
   `IOSurfaceCreateMachPort` and `IOSurfaceLookupFromMachPort` for cross-process
   texture sharing. This is what ts3 uses to send rendered surfaces from the
   profile server to the GUI.

### OSR Example Validation

The OSR (off-screen rendering) example in `cef-rs/examples/osr/` was used as a
testbed before ts1 integration. Changes made to the example:

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

- **Build:** `cd cef-rs && cargo build`
- **Build OSR example:** `cd cef-rs && cargo build -p cef-osr`
- **Bundle and run (macOS):**
  ```bash
  cd cef-rs
  cargo build -p cef-osr
  cargo run -p bundle-cef-app -- cef-osr -o cef-osr.app
  ./cef-osr.app/Contents/MacOS/cef-osr
  ```

### Key Files

- `cef-rs/cef/` — Main CEF wrapper crate
- `cef-rs/cef/src/osr_texture_import/` — Texture import (IOSurface on macOS,
  DMA-BUF on Linux, D3D11 on Windows)
- `cef-rs/cef/src/osr_texture_import/iosurface.rs` — IOSurface import + Mach
  port creation/lookup (modified for TermSurf)
- `cef-rs/cef/src/osr_texture_import/common.rs` — Shared texture handling
  (modified for sRGB fix)
- `cef-rs/examples/osr/` — Off-screen rendering example (validation testbed)
- `cef-rs/sys/` — Low-level CEF C API bindings (unmodified)
- `cef-rs/update-bindings/` — Tool to regenerate bindings from CEF headers

### Notes

- CEF binaries are downloaded automatically by the build system
- macOS apps must be bundled with `bundle-cef-app` to include CEF framework

## Documentation

### TermSurf 4.0 (active)

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
- `docs/issues/202-cef-mvp.md` through `206-cef-mvp5.md` — MVP iteration experiments
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
