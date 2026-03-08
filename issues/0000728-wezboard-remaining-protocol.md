# Issue 728: Complete remaining TermSurf protocol in Wezboard

## Goal

Implement the remaining unhandled TermSurf protocol messages in Wezboard so that
the `web` TUI works identically whether connected to Ghostboard or Wezboard.

## Background

Issues 715–727 built Wezboard from scratch — fork, rename, build cleanup, socket
server, protocol scaffolding, state management, process spawning, message
forwarding, CALayerHost rendering, overlay positioning, overlay lifecycle,
per-window overlays, and pane borders. Wezboard now handles 14 of 30 TermSurf
protocol messages (47%).

### What works

The full browser overlay pipeline is functional:

- **Socket server** (Issue 715) — Listens on
  `$TMPDIR/termsurf/wezboard-{pid}.sock`, sets `TERMSURF_SOCKET`, detects
  connection type (TUI vs Chromium), parses length-prefixed protobuf.
- **State management** (Issue 724) — Pane registry, server registry, tab-to-pane
  mappings, last-browser-pane tracking, server pane counting.
- **Process spawning** (Issue 724) — Spawns Roamium with `--ipc-socket`, tracks
  process lifecycle, reuses servers for same profile.
- **Tab lifecycle** (Issues 724, 726) — CreateTab, TabReady, CloseTab with
  proper cleanup on TUI disconnect.
- **Message forwarding** (Issue 724) — Navigate, UrlChanged, LoadingState,
  TitleChanged, SetColorScheme, ModeChanged, Resize.
- **CALayerHost rendering** (Issues 724, 725) — Transparent overlay NSView with
  layer-hosting, three-layer hierarchy (flipped → positioning → host), zero-copy
  GPU compositing.
- **Overlay positioning** (Issues 725–727) — Cell metrics bridge, per-pane grid
  offset from mux PositionedPane, contentsScale fix for Retina, TUI viewport
  offset (col/row), padding + border metrics, per-window overlay views.
- **Overlay lifecycle** (Issue 726) — Tab switching visibility sync, resize with
  Chromium Resize messages.
- **Query handlers** (Issue 726) — QueryLastRequest, QueryDevtoolsRequest,
  QueryTabsRequest with proper replies.
- **Pane borders** (Issue 723) — Configurable focused/unfocused colors, width,
  content inset.

### Messages currently handled (14 of 30)

| #  | Message              | Direction        | Handler                |
| -- | -------------------- | ---------------- | ---------------------- |
| 1  | ServerRegister       | Chromium → Board | handle_server_register |
| 2  | SetOverlay           | TUI → Board      | handle_set_overlay     |
| 3  | TabReady             | Chromium → Board | handle_tab_ready       |
| 4  | HelloRequest         | TUI → Board      | inline reply           |
| 5  | UrlChanged           | Chromium → Board | forward_to_tui         |
| 6  | LoadingState         | Chromium → Board | forward_to_tui         |
| 7  | TitleChanged         | Chromium → Board | forward_to_tui         |
| 8  | Navigate             | TUI → Board      | forward_to_chromium    |
| 9  | SetColorScheme       | TUI → Board      | forward_to_chromium    |
| 10 | ModeChanged          | TUI → Board      | update pane state      |
| 11 | CaContext            | Chromium → Board | handle_ca_context      |
| 12 | QueryLastRequest     | TUI → Board      | inline reply           |
| 13 | QueryDevtoolsRequest | TUI → Board      | inline reply           |
| 14 | QueryTabsRequest     | TUI → Board      | inline reply           |

### Messages NOT handled (16 of 30)

**Reply-only messages (6) — sent by the board, never received:**

These are outbound-only messages that the board sends in response to requests or
as state updates. The board never receives them. They are already "handled" in
the sense that their send paths exist (e.g., `HelloReply` is sent in response to
`HelloRequest`). No additional handler code is needed:

| Message            | Direction     | Status       |
| ------------------ | ------------- | ------------ |
| HelloReply         | Board → TUI   | Already sent |
| QueryLastReply     | Board → TUI   | Already sent |
| QueryDevtoolsReply | Board → TUI   | Already sent |
| QueryTabsReply     | Board → TUI   | Already sent |
| CreateTab          | Board → Chrom | Already sent |
| CloseTab           | Board → Chrom | Already sent |

**Board-initiated messages (4) — board generates and sends, never receives:**

These are messages the board originates in response to user input events or
window state changes. The board never receives them on the socket — it creates
and sends them. They require hooking into WezTerm's event system:

| Message     | Direction        | What it does                       |
| ----------- | ---------------- | ---------------------------------- |
| KeyEvent    | Board → Chromium | Forward keyboard events to browser |
| MouseEvent  | Board → Chromium | Forward mouse clicks to browser    |
| MouseMove   | Board → Chromium | Forward mouse movement to browser  |
| ScrollEvent | Board → Chromium | Forward scroll wheel to browser    |

**Received but unhandled (6) — arrive on socket, currently ignored:**

| Message            | Direction        | What it does                           |
| ------------------ | ---------------- | -------------------------------------- |
| SetDevtoolsOverlay | TUI → Board      | Create DevTools pane linked to tab     |
| OpenSplit          | TUI → Board      | Create a split pane in the terminal    |
| CursorChanged      | Chromium → Board | Update system cursor over overlay      |
| FocusChanged       | Board → Chromium | Notify browser of focus change         |
| Resize             | Board → Chromium | Already partially handled (SetOverlay) |
| CreateDevtoolsTab  | Board → Chromium | Send DevTools tab creation to Chromium |

## Approach

Group the remaining work into experiments by functional area, ordered by user
impact:

1. **Input forwarding** — KeyEvent, MouseEvent, MouseMove, ScrollEvent. This is
   the highest-impact missing feature. Without input, the browser overlay is
   view-only. Ghostboard hooks into Surface.keyCallback() and
   mouseButtonCallback() to intercept events when in browse mode. Wezboard needs
   equivalent hooks in the WezTerm event path, translating WezTerm key/mouse
   events into TermSurf proto messages and sending them to Chromium via the
   server's tx channel.

2. **Cursor changes** — CursorChanged. When the browser changes the cursor
   (pointer, text, hand, etc.), the board should update the system cursor.
   Ghostboard handles this in `handleCursorChanged`. The proto sends a cursor
   type integer that maps to macOS NSCursor types.

3. **Focus management** — FocusChanged. When a pane gains or loses focus, the
   board should notify Chromium so it can update its internal focus state
   (affects text selection, form focus, etc.). Ghostboard sends FocusChanged
   when the active pane changes.

4. **DevTools support** — SetDevtoolsOverlay and CreateDevtoolsTab. The TUI
   sends SetDevtoolsOverlay to open DevTools for a specific tab. The board
   creates a pane with `inspected_tab_id` set, then sends CreateDevtoolsTab to
   Chromium instead of CreateTab. Ghostboard implements this in
   `handleSetDevtoolsOverlay`.

5. **Split management** — OpenSplit. The TUI sends OpenSplit to create a new
   terminal split pane. The board should call WezTerm's split pane API to create
   a new pane in the specified direction.

## Reference: Ghostboard implementations

### Input forwarding (Ghostboard)

Ghostboard routes input in `Surface.zig`:

- `keyCallback()` — In browse mode, converts key events to TermSurf KeyEvent
  proto and sends via socket. Maps Ghostty key codes to Windows virtual key
  codes. Handles Cmd+key bypass (Cmd+C/V/A/L pass to the TUI, not the browser).
- `mouseButtonCallback()` — Converts mouse events to TermSurf MouseEvent proto.
  Computes overlay-relative coordinates from window-absolute position.
- `mouseMotion()` — Sends MouseMove with overlay-relative coords.
- `scrollCallback()` — Sends ScrollEvent with delta values and phase info.

### Modifier translation

WezTerm and TermSurf use different modifier bit positions:

| Modifier | WezTerm  | TermSurf |
| -------- | -------- | -------- |
| Shift    | `1 << 1` | `1 << 0` |
| Ctrl     | `1 << 3` | `1 << 1` |
| Alt      | `1 << 2` | `1 << 2` |
| Super    | `1 << 4` | `1 << 3` |

### Key code translation

Ghostboard maps its internal key codes to Windows virtual key codes (VK\_\*) for
the TermSurf KeyEvent proto. WezTerm uses its own `KeyCode` enum. The mapping
needs to convert WezTerm KeyCode variants to Windows VK codes.

### Cursor type mapping

The CursorChanged proto sends an integer cursor type. Ghostboard maps these to
Ghostty cursor shapes in `handleCursorChanged`. Wezboard needs to map them to
WezTerm's `MouseCursor` enum or directly to macOS NSCursor types.
