+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 400: A New Hope — TermSurf 4.0

## Background

TermSurf 1.x through 3.x explored three different approaches to combining a
terminal emulator with a web browser:

- **ts1** (Ghostty + WKWebView) — macOS-only, limited WebKit API, no
  cross-platform path.
- **ts2** (WezTerm + in-process CEF) — CEF's one-profile-per-process constraint
  made multi-profile impossible.
- **ts3** (WezTerm + out-of-process CEF via XPC) — Solved the profile constraint
  but hit a performance ceiling. The out-of-process IOSurface pipeline achieves
  ~50fps busy-wait (100% CPU) or ~31fps event-driven. Nine experiments
  (Issue 350) proved the pump architecture is optimized — the remaining gap is
  fundamental to CEF's off-screen rendering and cross-process texture sharing.

All three approaches share a common problem: **we don't own the window.** We
fork a terminal emulator (Ghostty or WezTerm) and try to inject browser
rendering into its architecture. This creates friction at every level — event
loops, rendering pipelines, input routing, process models. The terminal's
assumptions about owning the window conflict with our need to composite browser
content alongside terminal content.

CEF compounds the problem. It is a C++ wrapper around Chromium designed for
embedding, but its off-screen rendering path (`OnAcceleratedPaint`) has
hard-coded frame throttling (`CefCopyFrameGenerator` discards frames when one is
in-progress), forces texture copies through IOSurface, and limits architectural
choices. The `external_message_pump` callback fires from a background thread,
making tight integration with the host event loop impossible.

## TermSurf 4.0: Own Everything

TermSurf 4.0 starts from the opposite end: **we own the window, we own the event
loop, we own the compositor.** The terminal and browser are libraries embedded
into our application, not the other way around.

### Product Requirements

1. **GPU-accelerated terminal.** Full terminal emulator with modern rendering
   (ligatures, true color, GPU text rasterization).

2. **Chromium web browser.** Not CEF. Not WebKit. Direct Chromium embedding with
   access to the full rendering pipeline.

3. **Multi-pane, multi-tab support.** Split the window into terminal panes and
   browser panes. Multiple tabs, each with their own pane layout.

4. **`web google.com` command.** Type it in the terminal, a browser pane opens
   in the same window. Cookies and sessions shared within a profile.

5. **No CEF. No cef-rs.** Direct Chromium embedding with our own wrapper.

6. **No full terminal window manager.** No forking WezTerm or Ghostty as the
   host application. We don't inherit someone else's window management, tab
   system, or event loop.

7. **We own the window.** We create the window, we run the event loop, we
   composite terminal and browser content into a single frame. The terminal
   library and browser library are components, not hosts.

### Architecture Sketch

```
┌──────────────────────────────────────┐
│           TermSurf 4.0 Window        │
│  ┌────────────┬─────────────────┐    │
│  │  Terminal   │   Browser Pane  │    │
│  │   Pane      │  (Chromium)     │    │
│  │ (Alacritty  │                 │    │
│  │  or WezTerm │                 │    │
│  │  as lib)    │                 │    │
│  └────────────┴─────────────────┘    │
│  ┌──────────────────────────────┐    │
│  │       Terminal Pane 2        │    │
│  └──────────────────────────────┘    │
│  [Tab 1] [Tab 2] [Tab 3]            │
└──────────────────────────────────────┘

Our code owns:
  - Window creation (winit, raw platform, or custom)
  - Event loop (keyboard, mouse, resize)
  - Compositor (wgpu/Metal/Vulkan — merges terminal + browser textures)
  - Pane layout (splitting, resizing, focus)
  - Tab management
  - Input routing (which pane gets keyboard/mouse)

Libraries provide:
  - Terminal: VTE parsing, PTY management, GPU text rendering
  - Browser: Chromium content rendering, JavaScript, networking
```

### Key Decisions to Make

1. **Terminal library.** Alacritty's core (`alacritty_terminal`) is a clean
   library with VTE parsing, grid management, and PTY handling. WezTerm's
   `termwiz` crate provides similar functionality. Neither requires their
   window/GUI layer — we supply our own.

2. **Chromium embedding strategy.** Without CEF, we need our own way to
   initialize Chromium, create browser instances, and receive rendered frames.
   Options include:
   - Chromium's Content API (C++) — the layer Electron and CEF are built on
   - Servo's embedding approach (Rust-native, but not Chromium)
   - A minimal C++ shim around Chromium's content layer with Rust FFI

3. **Rendering pipeline.** How terminal and browser textures are composited into
   a single frame. Options: wgpu, raw Metal/Vulkan, or a higher-level framework.

4. **Window framework.** winit (Rust, cross-platform), raw platform APIs
   (NSWindow/HWND), or a toolkit like Tauri's window layer.

5. **Language.** Chromium is C++. The terminal libraries are Rust. TermSurf 4.0
   will likely be a Rust application with C++ FFI for Chromium, or a C++
   application with Rust components.

### What Changes from ts3

| Aspect          | ts3                                   | ts4                              |
| --------------- | ------------------------------------- | -------------------------------- |
| Window owner    | WezTerm                               | TermSurf                         |
| Terminal        | WezTerm (forked)                      | Library (Alacritty/WezTerm core) |
| Browser engine  | CEF (off-screen rendering)            | Chromium (direct embedding)      |
| IPC             | XPC Mach ports, Unix sockets          | In-process (or minimal IPC)      |
| Texture sharing | IOSurface via Mach port transfer      | Direct GPU texture access        |
| Event loop      | WezTerm's winit + AppKit              | Our own                          |
| Process model   | 3+ processes (GUI, launcher, profile) | TBD                              |
| Platform        | macOS only                            | macOS first, cross-platform goal |

### Why This Might Work

1. **No more IPC bottleneck.** ts3's ~2ms per-frame overhead from IOSurface Mach
   port transfer disappears if Chromium renders to a texture we can read
   directly.

2. **No more event loop conflicts.** We own the run loop. No fighting with
   WezTerm's assumptions about window ownership, no headless NSApp hacks, no
   dummy NSEvent posting.

3. **No more CEF limitations.** No `CefCopyFrameGenerator` throttling, no
   `external_message_pump` background thread callbacks, no single
   `root_cache_path` constraint.

4. **Simpler architecture.** One process (plus Chromium's renderer/GPU
   processes, which Chromium manages internally). No launcher, no XPC service,
   no endpoint relay.

### Risks

1. **Chromium embedding is hard.** CEF exists because embedding Chromium
   directly is a massive undertaking. The Content API is unstable, poorly
   documented, and changes with every Chromium release. Electron and CEF teams
   employ full-time engineers to track these changes.

2. **Build complexity.** Chromium's build system (GN + Ninja) is its own
   ecosystem. Integrating it with Cargo/Rust is non-trivial.

3. **Binary size.** Chromium is ~150MB+ of shared libraries. CEF abstracts this;
   direct embedding doesn't reduce it.

4. **Cross-platform surface.** Chromium embedding differs significantly across
   macOS, Windows, and Linux. CEF abstracts platform differences; we'd need to
   handle them ourselves.

## Order of Operations

### Risk assessment

| Component                  | Risk          | Reason                                                    |
| -------------------------- | ------------- | --------------------------------------------------------- |
| Own window + wgpu          | Low           | winit + wgpu is well-documented Rust                      |
| Terminal embedding         | Low           | `alacritty_terminal` is a clean library crate             |
| GPU text rendering         | Medium        | Non-trivial but solved by Alacritty/WezTerm               |
| Chromium embedding         | **Very high** | CEF exists because this is hard. Unstable C++ API, massive build system |
| Multi-process + XPC        | Low           | Already proven in ts3                                     |

Chromium embedding is the existential risk. If it turns out to be infeasible
without CEF, everything else is wasted. But it also has the longest lead time —
understanding the Content API, building a C++ shim, integrating with GN/Ninja.

The strategy: de-risk Chromium with research first (Phase 0), then build the
terminal stack (Phases 1-2) to validate the rendering pipeline and have a
working product, then tackle Chromium (Phases 3-4) with a proven compositor.

### Phase 0: Chromium feasibility research

**Goal:** Determine whether direct Chromium embedding is feasible before writing
any code.

Study how CEF and Electron wrap Chromium's Content API. The CEF source is in
`/cef/` — its `libcef/` directory is literally the answer to "how do you embed
Chromium." The Chromium source is in `/chromium/` for deep reference. Electron's
source is in `/electron/`.

Key questions to answer:

1. What is the minimal Content API surface for: initialize browser process,
   create off-screen browser, receive rendered frames, send input?
2. How does CEF's `CefBrowserHost::CreateBrowser()` map to Content API calls?
3. How does CEF's `OnAcceleratedPaint` receive rendered frames from Chromium's
   compositor? Can we get frames without CEF's `CefCopyFrameGenerator`
   throttling?
4. What does Electron's `OffScreenRenderWidgetHostView` do differently to
   achieve 240fps?
5. What is the build system integration story? Can we build a minimal Chromium
   shared library with GN and link it from Rust via C FFI?

If this research reveals that direct embedding is a multi-year effort or
fundamentally blocked, we stop here and reconsider (maybe CEF with workarounds,
maybe WebKit, maybe Servo).

### Phase 1: Window + GPU compositor

**Goal:** Create the rendering foundation that both terminal and browser will
use.

Create a winit window with wgpu. Render two colored rectangles side by side at
60fps. This proves the compositor works before adding real content.

Deliverable: a Rust binary that opens a window and composites multiple textures
into a single frame at 60fps. The compositor API should accept arbitrary textures
(from terminal rendering, browser rendering, or test patterns) and place them in
a pane layout.

### Phase 2: Terminal in our window

**Goal:** Embed a real terminal and validate the compositor with real content.

Use `alacritty_terminal` for PTY management, VTE parsing, and grid state. Write
or adapt GPU text rendering (Alacritty's `alacritty/src/renderer/` uses OpenGL
— we need wgpu, so adaptation is required).

This gives us a working, usable product — a terminal running in our own window.
It validates the entire rendering pipeline with real content: input handling,
PTY I/O, VTE parsing, GPU text rasterization, compositor presentation.

Deliverable: a terminal emulator that runs in our window at 60fps. Not
feature-complete — just enough to run a shell, display output, and handle basic
input.

### Phase 3: Chromium in-process

**Goal:** Get a webpage rendering into a texture in our process.

Build the C++ shim around Chromium's Content API based on Phase 0 research.
Initialize a browser instance, navigate to a URL, receive rendered frames as
GPU textures, composite them alongside the terminal in our window.

This is where we prove the concept. If a webpage renders at 60fps in our
compositor alongside the terminal, the architecture is validated.

Deliverable: our window showing a terminal pane and a browser pane side by side,
both at 60fps, in a single process.

### Phase 4: Chromium out-of-process

**Goal:** Move Chromium to a separate process per profile and prove 60fps with
the full multi-process pipeline.

Reuse ts3's proven patterns:

- One process per browser profile (non-negotiable architectural constraint)
- XPC Mach service for process management
- IOSurface Mach port transfer for cross-process texture sharing
- Anonymous XPC endpoints for direct GUI ↔ profile communication

The difference from ts3: we own the window and compositor, so the GUI side is
simpler. No WezTerm integration, no fighting with someone else's event loop.

Deliverable: terminal + browser in our window at 60fps, with the browser running
in a separate process communicating via XPC.

### Why terminal before browser

- **Phase 0 (research) de-risks Chromium before any code is written.** If
  embedding is infeasible, we find out with zero wasted implementation effort.
- **Terminal gives a working product immediately.** While Chromium embedding
  takes weeks or months, we have a functional terminal to iterate on.
- **The compositor built in Phases 1-2 is exactly what Phase 3 needs.** The
  browser pane is just another texture in the compositor.
- **Building Chromium takes ~40 minutes per compile.** Starting there means days
  before seeing any pixels. A terminal window renders in minutes.
- **If Chromium takes months, we still have something to ship and use.**

### Why NOT browser first

- Chromium embedding requires building Chromium itself (GN + Ninja, ~40min).
  Starting there means days of build system work before any visible progress.
- A window + terminal can be built in days, validating the entire rendering
  pipeline that the browser will later plug into.
- The research phase (Phase 0) catches feasibility issues without code
  investment. If direct embedding is blocked, we pivot before writing a line.
