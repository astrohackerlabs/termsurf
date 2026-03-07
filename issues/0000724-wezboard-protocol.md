# Issue 724: Implement TermSurf protocol in Wezboard

## Goal

Make Wezboard a fully functional TermSurf board — accepting TUI and browser
engine connections, managing browser overlays, forwarding input, and compositing
browser content via CALayerHost — so that `web` and Roamium work identically
whether connected to Ghostboard or Wezboard.

## Background

Issue 715 Experiment 5 established the socket foundation: Wezboard listens on
`$TMPDIR/termsurf/wezboard-{pid}.sock`, sets `TERMSURF_SOCKET`, accepts
connections, detects connection type (TUI vs Chromium) by first message, parses
length-prefixed protobuf, and has stub handlers for `ServerRegister`,
`SetOverlay`, and `HelloRequest`. The protobuf types are generated at build time
via prost.

The current implementation (`wezboard-gui/src/termsurf/`) is ~140 lines of
scaffolding. It logs messages but does not act on them. No state is tracked, no
browser processes are spawned, no overlays are rendered, and no input is
forwarded.

Ghostboard's full implementation (`ghostboard/src/apprt/xpc.zig`, 2,336 lines)
is the reference. It handles all 30 protocol messages across 17 handlers with
state tracking, process management, GPU compositing, and input routing.

### What exists in Wezboard

- **Socket listener** (`listener.rs`) — Binds socket, accepts connections,
  spawns async handler per connection on the main thread executor.
- **Connection handler** (`conn.rs`) — Length-prefixed protobuf parsing loop,
  connection type detection, stub message dispatch.
- **Proto types** (`mod.rs`) — prost-generated `TermSurfMessage` and all 30
  message types via `build.rs`.

### What needs to be built

Everything between "message arrives on socket" and "browser content appears on
screen with working input". This breaks down into five major systems:

1. **State management** — Pane registry, server registry, tab-to-pane mappings,
   focus tracking, last-browser-pane tracking.
2. **Process management** — Spawning Roamium (and future engines) as child
   processes with `--ipc-socket` argument, tracking process lifecycle.
3. **Overlay rendering** — CALayerHost layer tree in WezTerm's OpenGL/Metal
   renderer, positioned at grid coordinates from `SetOverlay`.
4. **Input routing** — Mouse, keyboard, and scroll events forwarded to Chromium
   when in browse mode, with hit testing against overlay bounds.
5. **Message forwarding** — Board acts as hub: TUI messages forwarded to
   Chromium, Chromium state updates forwarded to TUI.

### Protocol message inventory

All 30 messages grouped by the system that handles them:

**State management (foundation for everything else):**

| Message        | Direction       | Board action                                         |
| -------------- | --------------- | ---------------------------------------------------- |
| ServerRegister | Chromium->Board | Accept connection, set server.fd, flush pending tabs |
| TabReady       | Chromium->Board | Register tab_id on pane, update tab_to_pane map      |
| ModeChanged    | TUI->Board      | Update pane.browsing state                           |
| FocusChanged   | Board->Chromium | Enforce single-pane focus, send focus/unfocus        |

**Process management:**

| Message            | Direction       | Board action                                        |
| ------------------ | --------------- | --------------------------------------------------- |
| SetOverlay         | TUI->Board      | Create pane, spawn engine if needed, send CreateTab |
| SetDevtoolsOverlay | TUI->Board      | Create DevTools pane, link to inspected tab         |
| CloseTab           | Board->Chromium | Close tab when pane closes                          |
| OpenSplit          | TUI->Board      | Create split pane in terminal                       |

**Overlay rendering:**

| Message       | Direction       | Board action                                  |
| ------------- | --------------- | --------------------------------------------- |
| CaContext     | Chromium->Board | Create/update CALayerHost with GPU context ID |
| Resize        | Board->Chromium | Send new pixel dimensions on overlay resize   |
| CursorChanged | Chromium->Board | Update system cursor over overlay             |

**Input routing:**

| Message     | Direction       | Board action                                       |
| ----------- | --------------- | -------------------------------------------------- |
| MouseEvent  | Board->Chromium | Forward mouse down/up with overlay-relative coords |
| MouseMove   | Board->Chromium | Forward mouse movement                             |
| ScrollEvent | Board->Chromium | Forward scroll events                              |
| KeyEvent    | Board->Chromium | Forward keyboard events with Windows VK codes      |

**Message forwarding (TUI<->Chromium via Board):**

| Message           | Direction            | Board action                             |
| ----------------- | -------------------- | ---------------------------------------- |
| Navigate          | TUI->Board->Chromium | Resolve pane_id to tab_id, forward       |
| SetColorScheme    | TUI->Board->Chromium | Resolve pane_id to tab_id, forward       |
| UrlChanged        | Chromium->Board->TUI | Lookup pane by tab_id, forward to TUI fd |
| LoadingState      | Chromium->Board->TUI | Forward to TUI                           |
| TitleChanged      | Chromium->Board->TUI | Forward to TUI                           |
| CreateTab         | Board->Chromium      | Sent after SetOverlay or ServerRegister  |
| CreateDevtoolsTab | Board->Chromium      | Sent after SetDevtoolsOverlay            |

**Request/reply (synchronous TUI queries):**

| Message                    | Direction   | Board action                          |
| -------------------------- | ----------- | ------------------------------------- |
| HelloRequest/Reply         | TUI<->Board | Return homepage config + browser list |
| QueryLastRequest/Reply     | TUI<->Board | Return last active tab for profile    |
| QueryDevtoolsRequest/Reply | TUI<->Board | Validate DevTools creation            |
| QueryTabsRequest/Reply     | TUI<->Board | Return tab inventory for profile      |

### Architectural differences from Ghostboard

Ghostboard is Zig with GCD (Grand Central Dispatch) for concurrency. Wezboard is
Rust with smol (async executor) running on the main thread via
`promise::spawn::spawn_into_main_thread`. Key differences to account for:

1. **Concurrency model** — Ghostboard uses a serial GCD queue (`ipc_queue`) for
   all IPC state. Wezboard uses smol async tasks on the main thread. State
   access must be synchronized differently — likely via `Arc<Mutex<State>>` or
   by keeping all state on the main thread executor.

2. **Renderer** — Ghostboard uses a custom Metal renderer with direct
   `CALayerHost` setup in Zig. Wezboard uses `wgpu` (WebGPU abstraction) with a
   macOS backend. CALayerHost integration needs to work with wgpu's layer tree,
   not raw Metal.

3. **Pane model** — Ghostboard's `Surface` is a single pane with overlay state
   bolted on. WezTerm has a proper `Pane` trait with `PaneId`, dimensions, and a
   mux layer. Browser overlays could potentially be modeled as a custom `Pane`
   implementation, though an overlay approach (like Ghostboard) may be simpler
   initially.

4. **Input pipeline** — WezTerm routes input through `TermWindow::key_event()`
   and `TermWindow::mouse_event()` with a complex dispatch chain. Browser input
   forwarding needs to intercept at the right point.

5. **Window access** — The connection handler needs access to `TermWindow` state
   (pane dimensions, cell size, renderer) to compute pixel coordinates and
   create overlays. The current handler runs in an async context with no window
   reference — this bridge is the main architectural challenge.

### Approach

Build incrementally, one system at a time. Each experiment should produce a
testable result. Likely sequence:

1. State management — Pane and server registries, shared between socket handler
   and window.
2. Process spawning — Launch Roamium on SetOverlay, handle ServerRegister.
3. Tab lifecycle — CreateTab/TabReady/CloseTab flow.
4. CALayerHost rendering — Display browser content in overlay.
5. Input routing — Mouse, keyboard, scroll forwarding.
6. Message forwarding — TUI<->Chromium state updates.
7. Request/reply handlers — HelloReply, QueryLast, QueryDevtools, QueryTabs.

This order follows dependencies: state before process management, process
management before tab lifecycle, rendering before input (need something visible
to click), and forwarding last since it builds on all prior systems.
