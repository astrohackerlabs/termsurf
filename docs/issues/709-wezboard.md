# Issue 709: Wezboard

## Goal

Research what it would take to fork WezTerm into **Wezboard** — a
TermSurf-compatible "board" (terminal emulator with an integrated web browser).
Wezboard would speak the same Unix socket + protobuf protocol as the GUI, making
it a drop-in alternative to Ghostty-based TermSurf.

## Background

### What is a "board"

In TermSurf's architecture, a **board** is a terminal emulator that hosts the
TUI and renders browser overlays. The current board is the GUI (a Ghostty fork).
A board's responsibilities are:

1. **Terminal emulation** — Run shells, display terminal output.
2. **Socket server** — Listen on `$TMPDIR/termsurf/gui-{pid}.sock` for TUI and
   Chromium connections.
3. **Browser overlay rendering** — Composite Chromium's GPU output (via
   CALayerHost on macOS) into terminal panes.
4. **Input routing** — Forward keyboard/mouse events to either the terminal or
   Chromium depending on the current mode.
5. **IPC dispatch** — Handle all 30 protobuf message types (see Protocol section
   below).

### Why WezTerm

WezTerm is a GPU-accelerated terminal emulator written in Rust with:

- Cross-platform support (macOS, Linux, Windows)
- Built-in multiplexer (panes, tabs, splits)
- Lua scripting for configuration and extensibility
- WebGPU renderer (wgpu)
- Active development and large user base

Forking WezTerm would give TermSurf a second board option, proving the protocol
is board-agnostic. It also opens the door to cross-platform browser integration
(Linux, Windows) since WezTerm already runs there.

### Why not just port the protocol

The protocol is simple (30 messages, ~280 lines of protobuf). The hard parts
are:

1. **CALayerHost compositing** — The macOS-specific zero-copy GPU rendering path
   that displays Chromium's output. WezTerm uses wgpu, not Metal directly, so
   the layer tree integration will be different.
2. **Overlay geometry** — Mapping terminal cell coordinates to pixel coordinates
   for the browser overlay. WezTerm's pane layout system is different from
   Ghostty's.
3. **Input interception** — Intercepting keyboard/mouse events before they reach
   the terminal and forwarding them to Chromium in browse mode.
4. **Process management** — Spawning Roamium/Chromium server processes and
   managing their lifecycle.

## The TermSurf Protocol

The protocol uses Unix domain sockets with 4-byte little-endian length-prefixed
protobuf messages. The board listens; TUI and Chromium connect as clients.

### Connection lifecycle

1. **Board starts** — Listens on `$TMPDIR/termsurf/gui-{pid}.sock`.
2. **TUI connects** — Sends `SetOverlay` or `SetDevtoolsOverlay` as first
   message. Board creates a browser pane.
3. **Chromium connects** — Sends `ServerRegister` as first message. Board
   matches it to a pending browser profile.
4. **Disconnect** — Board detects EOF, closes associated tabs, kills Chromium if
   no tabs remain.

Connection type is determined by the first message: `ServerRegister` = Chromium,
anything else = TUI.

### All 30 message types

#### GUI → Chromium: Tab lifecycle and input (11 messages)

| #  | Message               | Fields                                                                                      | When sent                           |
| -- | --------------------- | ------------------------------------------------------------------------------------------- | ----------------------------------- |
| 1  | **CreateTab**         | `url`, `pane_id`, `pixel_width`, `pixel_height`, `dark`                                     | TUI creates browser pane            |
| 2  | **CreateDevtoolsTab** | `pane_id`, `inspected_tab_id`, `pixel_width`, `pixel_height`, `dark`                        | TUI creates DevTools pane           |
| 3  | **Resize**            | `tab_id`, `pixel_width`, `pixel_height`                                                     | Pane resized                        |
| 4  | **CloseTab**          | `tab_id`                                                                                    | TUI disconnects or pane closed      |
| 5  | **Navigate**          | `tab_id`, `url`                                                                             | URL navigation (forwarded from TUI) |
| 6  | **MouseEvent**        | `tab_id`, `type`, `button`, `x`, `y`, `click_count`, `modifiers`                            | Mouse click (down/up)               |
| 7  | **MouseMove**         | `tab_id`, `x`, `y`, `modifiers`                                                             | Mouse position change               |
| 8  | **ScrollEvent**       | `tab_id`, `x`, `y`, `delta_x`, `delta_y`, `phase`, `momentum_phase`, `precise`, `modifiers` | Scroll wheel/trackpad               |
| 9  | **KeyEvent**          | `tab_id`, `type`, `windows_key_code`, `utf8`, `modifiers`                                   | Keyboard input in browse mode       |
| 10 | **FocusChanged**      | `tab_id`, `focused`                                                                         | Pane enters/exits browse mode       |
| 11 | **SetColorScheme**    | `tab_id`, `dark`                                                                            | Color scheme changes                |

#### Chromium → GUI: State updates (7 messages)

| #  | Message            | Fields                                                   | When sent                                                  |
| -- | ------------------ | -------------------------------------------------------- | ---------------------------------------------------------- |
| 12 | **ServerRegister** | `profile`                                                | Chromium process connects                                  |
| 13 | **TabReady**       | `pane_id`, `tab_id`                                      | Tab created, ID assigned                                   |
| 14 | **CaContext**      | `tab_id`, `ca_context_id`, `pixel_width`, `pixel_height` | GPU layer ready for compositing                            |
| 15 | **UrlChanged**     | `tab_id`, `url`                                          | Page navigation completes                                  |
| 16 | **LoadingState**   | `tab_id`, `state`, `progress`                            | Loading state changes (loading/progress/done/error, 0–100) |
| 17 | **TitleChanged**   | `tab_id`, `title`                                        | Page title changed                                         |
| 18 | **CursorChanged**  | `tab_id`, `cursor_type`                                  | Cursor type changed (pointer/hand/text/resize)             |

#### TUI → GUI: Overlay setup (4 messages)

| #  | Message                | Fields                                                                                           | When sent                              |
| -- | ---------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------- |
| 19 | **SetOverlay**         | `pane_id`, `col`, `row`, `width`, `height`, `url`, `profile`, `browsing`, `browser`              | User opens browser pane                |
| 20 | **SetDevtoolsOverlay** | `pane_id`, `col`, `row`, `width`, `height`, `profile`, `browsing`, `inspected_tab_id`, `browser` | User opens DevTools pane               |
| 21 | **OpenSplit**          | `pane_id`, `direction`, `command`                                                                | Split command (`:split-h`, `:split-v`) |
| 22 | **ModeChanged**        | `browsing`, `pane_id`                                                                            | Toggle browse/control mode             |

#### TUI ↔ GUI: Synchronous queries (8 messages)

| #  | Message                  | Fields                                                                                   | When sent                                 |
| -- | ------------------------ | ---------------------------------------------------------------------------------------- | ----------------------------------------- |
| 23 | **HelloRequest**         | `pane_id`                                                                                | TUI startup — query config                |
| 24 | **HelloReply**           | `homepage`, `browsers[]`                                                                 | Board returns homepage URL + browser list |
| 25 | **QueryLastRequest**     | `pane_id`, `profile`                                                                     | Find last active tab for profile          |
| 26 | **QueryLastReply**       | `pane_id`, `tab_id`, `profile`, `error`                                                  | Last tab info or error                    |
| 27 | **QueryDevtoolsRequest** | `pane_id`, `inspected_tab_id`, `profile`                                                 | Validate DevTools request                 |
| 28 | **QueryDevtoolsReply**   | `tab_id`, `error`, `browser`, `profile`                                                  | DevTools validation result                |
| 29 | **QueryTabsRequest**     | `pane_id`, `profile`                                                                     | Inventory all tabs                        |
| 30 | **QueryTabsReply**       | `gui_panes`, `chromium_tabs`, `chromium_browser`, `chromium_devtools`, `tabs[]`, `error` | Tab inventory                             |

### Modifier bitmask

Used in `MouseEvent`, `MouseMove`, `ScrollEvent`, `KeyEvent`:

| Modifier | Bit             |
| -------- | --------------- |
| Shift    | `1 << 0` (0x01) |
| Ctrl     | `1 << 1` (0x02) |
| Alt      | `1 << 2` (0x04) |
| Super    | `1 << 3` (0x08) |

### Wire format

Every message on the socket:

```
[4 bytes: little-endian u32 length] [N bytes: serialized TermSurfMessage]
```

### Board state the protocol assumes

The board must maintain:

- **Pane registry** — Map `pane_id` (string) to overlay state (position, size,
  tab_id, profile, browser, mode).
- **Tab registry** — Map `tab_id` (int64) to pane_id. Assigned by Chromium via
  `TabReady`.
- **Server registry** — Map browser profile to Chromium server connection
  (socket fd). Populated by `ServerRegister`.
- **Browser registry** — Map browser name to binary path. Returned in
  `HelloReply`.
- **Pending tabs** — Queue of `CreateTab`/`CreateDevtoolsTab` messages waiting
  for a Chromium server to register.
- **Per-pane TUI socket** — Each pane tracks which TUI client connection owns
  it, for forwarding `UrlChanged`, `LoadingState`, `TitleChanged`, and
  `ModeChanged` back.

### CALayerHost compositing

On macOS, Chromium renders to a `CAContext` (a GPU layer with a numeric ID). The
board creates a `CALayerHost` layer with that ID and inserts it into the
window's layer tree at the overlay's pixel coordinates. Window Server composites
directly from GPU VRAM — zero per-frame IPC, zero texture copies.

The `CaContext` message delivers the `ca_context_id` (a `uint32_t` that
identifies the remote `CAContext`). The board positions and sizes the
`CALayerHost` layer to match the overlay's pixel bounds.

On Linux/Windows, a different compositing strategy would be needed (shared
memory, DMA-BUF, or frame capture). This is future work.

## Research questions

### WezTerm architecture

1. **Rendering pipeline** — WezTerm uses wgpu. Can we insert a `CALayerHost`
   layer into wgpu's Metal backend layer tree? Or do we need to composite the
   browser output differently (e.g., render the `CALayerHost` content into a
   texture and draw it as a quad)?

2. **Pane system** — WezTerm has its own pane/tab/window model with a built-in
   multiplexer. How do we map `SetOverlay` (col, row, width, height) to
   WezTerm's pane coordinates? Can we create "virtual panes" that display
   browser content instead of terminal output?

3. **Input handling** — Where does WezTerm intercept keyboard and mouse events?
   Can we hook into the event pipeline to route events to Chromium in browse
   mode?

4. **Process spawning** — WezTerm spawns shells via its `CommandBuilder`. Can we
   use the same mechanism to spawn Roamium, or do we need a separate process
   management layer?

5. **Configuration** — WezTerm uses Lua for configuration. How do we expose
   TermSurf settings (homepage, browser registry, keybindings) through Lua?

6. **Platform layer** — WezTerm's window management uses `window/` crate. Where
   does the macOS-specific layer tree setup happen? This is where `CALayerHost`
   integration would go.

### Protocol compatibility

7. **Socket path** — The current socket path is
   `$TMPDIR/termsurf/gui-{pid}.sock`. Wezboard would use the same convention
   with its own PID. The TUI discovers the path via `TERMSURF_SOCKET` env var
   (set by the board when spawning shells). No protocol change needed.

8. **Protobuf in Rust** — WezTerm is Rust, and the TUI already uses `prost` for
   protobuf. Wezboard can share the same proto definitions and prost codegen.

9. **Browser spawning** — The board spawns Roamium with `--ipc-socket={path}`.
   WezTerm's command execution infrastructure can handle this.

10. **`OpenSplit`** — The TUI sends `OpenSplit` to create new panes. WezTerm has
    its own split API. Can we bridge `OpenSplit` to WezTerm's native split
    mechanism?

### Build and distribution

11. **Fork strategy** — Fork WezTerm, add TermSurf protocol support as a feature
    flag? Or maintain as a patch set (like the Chromium fork)?

12. **Dependencies** — WezTerm has a large dependency tree. How does adding
    protobuf (prost) and Unix socket handling affect build times?

13. **Naming** — Binary name `wezboard`, config file `~/.config/wezboard/`,
    bundle ID `com.termsurf.wezboard`.

## Ideas for experiments

1. **Socket listener in WezTerm.** Add a Unix socket listener to WezTerm that
   accepts connections and parses length-prefixed protobuf messages. No
   rendering — just prove the event loop integration works.

2. **CALayerHost in wgpu.** Create a minimal test that inserts a `CALayerHost`
   layer into wgpu's Metal backend layer tree. Prove zero-copy compositing works
   with WezTerm's renderer.

3. **Browser overlay pane.** Create a WezTerm pane type that displays browser
   content via CALayerHost instead of terminal output. Integrate with the socket
   listener to handle `SetOverlay` → `CreateTab` → `CaContext` flow.

4. **Input routing.** Intercept keyboard/mouse events in browse mode and forward
   them to Chromium via the socket. Prove typing in a browser pane works.

5. **Full protocol.** Implement all 30 message types. Verify the existing TUI
   works unmodified with Wezboard.
