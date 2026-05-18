+++
status = "closed"
opened = "2026-03-05"
closed = "2026-03-05"
+++

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

| #   | Message               | Fields                                                                                      | When sent                           |
| --- | --------------------- | ------------------------------------------------------------------------------------------- | ----------------------------------- |
| 1   | **CreateTab**         | `url`, `pane_id`, `pixel_width`, `pixel_height`, `dark`                                     | TUI creates browser pane            |
| 2   | **CreateDevtoolsTab** | `pane_id`, `inspected_tab_id`, `pixel_width`, `pixel_height`, `dark`                        | TUI creates DevTools pane           |
| 3   | **Resize**            | `tab_id`, `pixel_width`, `pixel_height`                                                     | Pane resized                        |
| 4   | **CloseTab**          | `tab_id`                                                                                    | TUI disconnects or pane closed      |
| 5   | **Navigate**          | `tab_id`, `url`                                                                             | URL navigation (forwarded from TUI) |
| 6   | **MouseEvent**        | `tab_id`, `type`, `button`, `x`, `y`, `click_count`, `modifiers`                            | Mouse click (down/up)               |
| 7   | **MouseMove**         | `tab_id`, `x`, `y`, `modifiers`                                                             | Mouse position change               |
| 8   | **ScrollEvent**       | `tab_id`, `x`, `y`, `delta_x`, `delta_y`, `phase`, `momentum_phase`, `precise`, `modifiers` | Scroll wheel/trackpad               |
| 9   | **KeyEvent**          | `tab_id`, `type`, `windows_key_code`, `utf8`, `modifiers`                                   | Keyboard input in browse mode       |
| 10  | **FocusChanged**      | `tab_id`, `focused`                                                                         | Pane enters/exits browse mode       |
| 11  | **SetColorScheme**    | `tab_id`, `dark`                                                                            | Color scheme changes                |

#### Chromium → GUI: State updates (7 messages)

| #   | Message            | Fields                                                   | When sent                                                  |
| --- | ------------------ | -------------------------------------------------------- | ---------------------------------------------------------- |
| 12  | **ServerRegister** | `profile`                                                | Chromium process connects                                  |
| 13  | **TabReady**       | `pane_id`, `tab_id`                                      | Tab created, ID assigned                                   |
| 14  | **CaContext**      | `tab_id`, `ca_context_id`, `pixel_width`, `pixel_height` | GPU layer ready for compositing                            |
| 15  | **UrlChanged**     | `tab_id`, `url`                                          | Page navigation completes                                  |
| 16  | **LoadingState**   | `tab_id`, `state`, `progress`                            | Loading state changes (loading/progress/done/error, 0–100) |
| 17  | **TitleChanged**   | `tab_id`, `title`                                        | Page title changed                                         |
| 18  | **CursorChanged**  | `tab_id`, `cursor_type`                                  | Cursor type changed (pointer/hand/text/resize)             |

#### TUI → GUI: Overlay setup (4 messages)

| #   | Message                | Fields                                                                                           | When sent                              |
| --- | ---------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------- |
| 19  | **SetOverlay**         | `pane_id`, `col`, `row`, `width`, `height`, `url`, `profile`, `browsing`, `browser`              | User opens browser pane                |
| 20  | **SetDevtoolsOverlay** | `pane_id`, `col`, `row`, `width`, `height`, `profile`, `browsing`, `inspected_tab_id`, `browser` | User opens DevTools pane               |
| 21  | **OpenSplit**          | `pane_id`, `direction`, `command`                                                                | Split command (`:split-h`, `:split-v`) |
| 22  | **ModeChanged**        | `browsing`, `pane_id`                                                                            | Toggle browse/control mode             |

#### TUI ↔ GUI: Synchronous queries (8 messages)

| #   | Message                  | Fields                                                                                   | When sent                                 |
| --- | ------------------------ | ---------------------------------------------------------------------------------------- | ----------------------------------------- |
| 23  | **HelloRequest**         | `pane_id`                                                                                | TUI startup — query config                |
| 24  | **HelloReply**           | `homepage`, `browsers[]`                                                                 | Board returns homepage URL + browser list |
| 25  | **QueryLastRequest**     | `pane_id`, `profile`                                                                     | Find last active tab for profile          |
| 26  | **QueryLastReply**       | `pane_id`, `tab_id`, `profile`, `error`                                                  | Last tab info or error                    |
| 27  | **QueryDevtoolsRequest** | `pane_id`, `inspected_tab_id`, `profile`                                                 | Validate DevTools request                 |
| 28  | **QueryDevtoolsReply**   | `tab_id`, `error`, `browser`, `profile`                                                  | DevTools validation result                |
| 29  | **QueryTabsRequest**     | `pane_id`, `profile`                                                                     | Inventory all tabs                        |
| 30  | **QueryTabsReply**       | `gui_panes`, `chromium_tabs`, `chromium_browser`, `chromium_devtools`, `tabs[]`, `error` | Tab inventory                             |

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

## Experiment 1: WezTerm architecture audit

### Goal

Map every TermSurf protocol requirement to a specific location in the WezTerm
codebase. Answer: what needs to change, where, and how hard is each piece?

### Requirement 1: Unix socket server

**TermSurf needs:** Board listens on `$TMPDIR/termsurf/gui-{pid}.sock`, accepts
TUI and Chromium connections, parses 4-byte length-prefixed protobuf messages.

**WezTerm already has:** A full Unix socket + length-prefixed protobuf mux
server (`wezterm-mux-server-impl/`). The listener lives in
`wezterm-mux-server-impl/src/local.rs` — a `UnixListener` that spawns an async
task per connection via `promise::spawn::spawn_into_main_thread()`. Message
dispatch is in `dispatch.rs` using `smol` channels. The async runtime is a
custom `SimpleExecutor` (smol-based) with a `tick()` loop.

**What to do:** Add a second `UnixListener` for the TermSurf protocol alongside
(or instead of) WezTerm's native mux protocol. The socket path changes to
`$TMPDIR/termsurf/gui-{pid}.sock`. The message format changes from WezTerm's
`codec::DecodedPdu` to TermSurf's `TermSurfMessage` protobuf. The async task
infrastructure is identical — spawn a new task per connection, multiplex reads
and Mux notifications.

**Difficulty:** Low. The socket/async plumbing is 1:1 with what TermSurf needs.
Only the message codec changes.

### Requirement 2: Environment variable for TUI discovery

**TermSurf needs:** Board sets `TERMSURF_SOCKET={path}` in child process
environment so the TUI can discover the socket.

**WezTerm already has:** `CommandBuilder` (`pty/src/cmdbuilder.rs`) with
`.env(key, value)` method. Domain setup in `mux/src/domain.rs` (line 480)
already sets `WEZTERM_UNIX_SOCKET` and `WEZTERM_PANE` on child processes.

**What to do:** Add `cmd.env("TERMSURF_SOCKET", socket_path)` in
`LocalDomain::build_command()`. One line.

**Difficulty:** Trivial.

### Requirement 3: Protobuf message parsing (prost)

**TermSurf needs:** Parse/serialize `TermSurfMessage` protobuf (30 message
types, ~280 lines of `.proto`).

**WezTerm is Rust.** The TUI and Roamium already use `prost` for protobuf
codegen. Wezboard can share the same `proto/termsurf.proto` and `prost-build`
setup.

**What to do:** Add `prost` and `prost-build` to Wezboard's dependencies. Copy
the build.rs pattern from `tui/` or `roamium/`. The proto file already exists at
`proto/termsurf.proto`.

**Difficulty:** Trivial. Prost is a standard Rust dependency.

### Requirement 4: CALayerHost compositing

**TermSurf needs:** When Chromium sends a `CaContext` message with a
`ca_context_id`, the board creates a `CALayerHost` sublayer positioned at the
overlay's pixel coordinates. Window Server composites directly from GPU VRAM.

**WezTerm's layer tree on macOS:**

```
NSWindow
  └─ NSView (WindowView) [setWantsLayer: YES]
       └─ CALayer (view's backing layer)
            └─ CAMetalLayer (created by ANGLE as sublayer)
```

The view's backing layer is created in `make_backing_layer()`
(`window/src/os/macos/window.rs:3033`), which returns a `CAMetalLayer`. ANGLE
(EGL path) creates a `CAMetalLayer` sublayer automatically. The existing code
already enumerates sublayers to fix opacity (lines 201–208).

**What to do:** After receiving `CaContext`, create a `CALayerHost` sublayer on
the view's backing layer:

```objc
let host: id = msg_send![class!(CALayerHost), layer];
msg_send![host, setContextId: ca_context_id as u32];
msg_send![host, setFrame: CGRect { origin, size }];
msg_send![backing_layer, addSublayer: host];
```

Position and resize the `CALayerHost` when the overlay pane moves or resizes.
Remove it when the tab closes.

**Key concern:** The `CALayerHost` must be ordered above the `CAMetalLayer` (so
browser content renders on top of terminal content). CALayer sibling ordering is
controlled by `zPosition` or insertion order.

**What to do differently from the GUI:** The GUI (Ghostty fork) uses Metal
directly and manages the layer tree in Zig. WezTerm uses ANGLE/EGL, which
creates its own `CAMetalLayer` sublayer. The `CALayerHost` needs to coexist with
ANGLE's sublayer. This means the browser overlay sits _above_ the terminal
rendering layer in the same view. This should work — `CALayerHost` is just
another `CALayer` sublayer, and Window Server composites them by z-order.

**Difficulty:** Medium. The `CALayerHost` creation itself is straightforward
(~20 lines of `objc` message sends). The complexity is in lifecycle management
(create/resize/remove per overlay) and ensuring correct z-ordering with ANGLE's
sublayer.

### Requirement 5: Overlay pane geometry

**TermSurf needs:** `SetOverlay` provides `col`, `row`, `width`, `height` in
terminal cell units. The board converts these to pixel coordinates for the
`CALayerHost` frame.

**WezTerm's coordinate system:** The `PositionedPane` struct (`mux/src/tab.rs`)
provides `left`, `top` (in cells) and `pixel_width`, `pixel_height` for every
pane. The `iter_panes_impl()` method traverses the binary tree layout and
accumulates pixel positions. Cell-to-pixel conversion uses `TerminalSize` (which
stores both cell counts and pixel dimensions).

**What to do:** When `SetOverlay` arrives, convert (col, row, width, height)
from cell units to pixels using the current cell size (available from
`TerminalSize.pixel_width / TerminalSize.cols`). Position the `CALayerHost` at
the computed pixel rect.

**Alternative approach:** Instead of overlaying on top of terminal panes, create
a custom `Pane` implementation (see Requirement 6) that occupies a real slot in
WezTerm's binary tree layout. This way WezTerm's own layout engine computes the
pixel position automatically.

**Difficulty:** Low–Medium. The coordinate math is simple. The question is
whether to overlay (position manually) or integrate into the pane tree (let
WezTerm compute position).

### Requirement 6: Custom browser pane type

**TermSurf needs:** A pane that displays browser content instead of terminal
output.

**WezTerm's Pane trait** (`mux/src/pane.rs:165`) is the core abstraction — 20+
methods including `pane_id()`, `get_dimensions()`, `get_title()`, `resize()`,
`key_down()`, `key_up()`, `mouse_event()`, `is_dead()`, etc. It uses
`downcast_rs` for type erasure.

**Existing implementations:**

- `LocalPane` — Terminal with PTY (the standard pane)
- `TermWizTerminalPane` — UI dialog pane (search, launcher)
- `ClientPane` — Remote mux client pane

**What to do:** Create `BrowserPane` implementing the `Pane` trait:

- `pane_id()` → allocated via `alloc_pane_id()`
- `get_dimensions()` → return the overlay's cell/pixel dimensions
- `get_title()` → return the page title (from `TitleChanged`)
- `resize()` → send `Resize` message to Chromium
- `key_down()` / `key_up()` → send `KeyEvent` to Chromium
- `mouse_event()` → send `MouseEvent` / `MouseMove` / `ScrollEvent` to Chromium
- `is_dead()` → true when tab is closed
- `get_lines()` — return empty lines (the CALayerHost renders visually; the
  terminal text buffer is unused)

Create a `BrowserDomain` implementing the `Domain` trait whose `spawn_pane()`
returns a `BrowserPane`. Register it with the global `Mux`.

**Key advantage:** Browser panes participate in WezTerm's native split/tab/zoom
system. The binary tree layout (`bintree/src/lib.rs`) positions them
automatically. `OpenSplit` maps directly to WezTerm's `Tab::split_and_insert()`.

**Difficulty:** Medium. The `Pane` trait has many methods, but most can return
stubs (empty lines, no scrollback, no search). The core methods that matter are
resize, input forwarding, title, and lifecycle.

### Requirement 7: Input routing (browse mode)

**TermSurf needs:** In browse mode, keyboard and mouse events go to Chromium
instead of the terminal.

**WezTerm's input chain:**

```
macOS NSView → keyDown:/mouseDown: (Objective-C)
  → key_common() / mouse_common() (window.rs)
    → WindowEvent::RawKeyEvent / WindowEvent::MouseEvent
      → TermWindow::raw_key_event_impl() / mouse_event_impl()
        → Key table lookup / pane routing
          → pane.key_down() / pane.mouse_event()
```

**Hook points:**

1. `RawKeyEvent` handler — can call `event.set_handled()` to intercept
2. Key table system — modal key tables with `activate()` / `pop()`
3. Pane-level — `pane.key_down()` and `pane.mouse_event()` are already per-pane

**What to do:** If using a custom `BrowserPane` (Requirement 6), input routing
is automatic. When the browser pane is the active pane, `key_down()` /
`mouse_event()` dispatch to it. The `BrowserPane` implementation forwards to
Chromium via the socket.

Mode switching (browse ↔ control) needs a keybinding (Esc / Enter) that changes
the active pane or toggles a mode flag. WezTerm's key table system can handle
this — activate a "browse" key table that routes all keys to the browser pane,
with Esc popping back to the default table.

**Modifier mapping:** WezTerm uses `Modifiers` bitflags (u16):

| WezTerm | Bit    | TermSurf | Bit    |
| ------- | ------ | -------- | ------ |
| `SHIFT` | `1<<1` | Shift    | `1<<0` |
| `ALT`   | `1<<2` | Alt      | `1<<2` |
| `CTRL`  | `1<<3` | Ctrl     | `1<<1` |
| `SUPER` | `1<<4` | Super    | `1<<3` |

The bit positions differ — a translation function is needed.

**Difficulty:** Low if using BrowserPane approach. The Pane trait already
receives all input. The only work is modifier translation and mode toggling.

### Requirement 8: Process spawning (Roamium)

**TermSurf needs:** Board spawns Roamium with
`--ipc-socket={path} --profile={name} --hidden`.

**WezTerm has:** `CommandBuilder` with full env/arg/cwd control, spawned via
`std::process::Command` with pre_exec hooks for signal handling, session
leaders, and FD cleanup.

**What to do:** When a `SetOverlay` arrives requesting a browser profile that
has no running server, spawn Roamium:

```rust
let mut cmd = CommandBuilder::new("roamium");
cmd.arg("--ipc-socket").arg(&socket_path);
cmd.arg("--profile").arg(&profile);
cmd.arg("--hidden");
cmd.spawn()?;
```

Track the child process. Kill it when all its tabs close.

**Difficulty:** Trivial. WezTerm's process spawning is robust.

### Requirement 9: State registries

**TermSurf needs:** Pane registry, tab registry, server registry, browser
registry, pending tab queue, per-pane TUI socket tracking.

**WezTerm already has:** A global `Mux` singleton (`mux/src/lib.rs:102`) with
`HashMap<PaneId, Arc<dyn Pane>>`, `HashMap<TabId, Arc<Tab>>`,
`HashMap<WindowId, Window>`, `HashMap<DomainId, Arc<dyn Domain>>`. It also has a
subscriber notification system for state changes.

**What to do:** Add TermSurf-specific registries alongside the Mux:

- **Server registry:** `HashMap<String, UnixStream>` — browser profile →
  Chromium connection
- **Tab ID mapping:** `HashMap<i64, PaneId>` — Chromium tab_id → WezTerm pane_id
- **Browser registry:** `HashMap<String, PathBuf>` — browser name → binary path
- **Pending tabs:** `Vec<TermSurfMessage>` — queued CreateTab messages

These can live in a new `TermSurfState` struct stored in the Mux or alongside
it.

**Difficulty:** Low. Standard Rust data structures.

### Requirement 10: Message forwarding (GUI ↔ TUI)

**TermSurf needs:** Forward `UrlChanged`, `LoadingState`, `TitleChanged`,
`ModeChanged` from Chromium back to the TUI that owns the pane.

**WezTerm has:** The `Mux::subscribe()` system for broadcasting notifications.
Also, per-connection async tasks in `dispatch.rs` that multiplex between client
input and Mux notifications.

**What to do:** When a Chromium state update arrives (e.g., `UrlChanged`), look
up the owning TUI connection from the pane registry and write the message to
that TUI's socket. This mirrors the GUI's current `handleSocketUrlChanged()` →
find TUI fd → write protobuf pattern.

**Difficulty:** Low. The forwarding logic is straightforward.

### Requirement 11: OpenSplit bridging

**TermSurf needs:** TUI sends `OpenSplit` with direction and command. Board
creates a split pane.

**WezTerm has:** `Tab::split_and_insert()` (`mux/src/tab.rs:1960`) which takes a
`SplitRequest` (direction, target_is_second, size) and inserts a pane into the
binary tree. `Domain::split_pane()` orchestrates spawning + insertion.

**What to do:** Map TermSurf's `OpenSplit` to WezTerm's split:

```rust
let request = SplitRequest {
    direction: match msg.direction.as_str() {
        "horizontal" => SplitDirection::Horizontal,
        "vertical" => SplitDirection::Vertical,
    },
    target_is_second: true,
    top_level: false,
    size: SplitSize::Percent(50),
};
tab.split_and_insert(active_pane_index, request, new_pane)?;
```

**Difficulty:** Low. Direct mapping.

### Requirement 12: Rendering the empty browser pane

**TermSurf needs:** Browser panes have no terminal content — the visual output
is entirely from the `CALayerHost`.

**WezTerm's render pipeline:** `paint_pass()` iterates `PositionedPane` entries
and calls `render_screen_line()` for each pane's visible lines. Each line
produces quads (vertices) submitted to the GPU.

**What to do:** For a `BrowserPane`, `get_lines()` returns empty/blank lines.
The terminal renderer draws a transparent/black background. The `CALayerHost`
sublayer (positioned at the pane's pixel rect) renders the browser content on
top.

**Key concern:** The terminal renderer's background might obscure the
`CALayerHost`. Need to ensure the browser pane's background is transparent or
that the `CALayerHost` has a higher z-order than the terminal layer.

**Difficulty:** Medium. Requires understanding the interaction between
ANGLE/CAMetalLayer rendering and CALayerHost compositing order.

### Requirement 13: Synchronous query handling

**TermSurf needs:** `HelloRequest`, `QueryLastRequest`, `QueryDevtoolsRequest`,
`QueryTabsRequest` are synchronous — TUI blocks until the board replies.

**WezTerm's mux server** already handles request/response patterns in
`dispatch.rs` — reads a PDU, processes it, writes a response PDU. The async task
per connection naturally supports this.

**What to do:** When a query message arrives on a TUI connection, compute the
answer from the registries and write the reply message back on the same socket.

**Difficulty:** Trivial. Standard request/response on a bidirectional socket.

### Summary

| Requirement          | Difficulty | WezTerm support                                                |
| -------------------- | ---------- | -------------------------------------------------------------- |
| Socket server        | Low        | Has Unix socket + async task infrastructure                    |
| Env var for TUI      | Trivial    | CommandBuilder.env() already used                              |
| Protobuf (prost)     | Trivial    | Rust crate, shared proto file                                  |
| CALayerHost          | Medium     | CAMetalLayer sublayer pattern exists, need CALayerHost sibling |
| Overlay geometry     | Low–Medium | PositionedPane has pixel coords, or manual overlay             |
| Custom BrowserPane   | Medium     | Pane trait is large but most methods can stub                  |
| Input routing        | Low        | Pane trait receives all input; mode toggle via key tables      |
| Process spawning     | Trivial    | CommandBuilder handles it                                      |
| State registries     | Low        | Standard HashMaps alongside Mux                                |
| Message forwarding   | Low        | Per-connection async tasks exist                               |
| OpenSplit bridging   | Low        | Direct mapping to split_and_insert()                           |
| Empty pane rendering | Medium     | Need transparent background + z-order                          |
| Sync queries         | Trivial    | Request/response pattern in dispatch                           |

### Key architectural decision

**BrowserPane vs. overlay approach:**

- **BrowserPane (recommended):** Implement the `Pane` trait. Browser panes live
  in WezTerm's binary tree layout, get automatic positioning, resizing, splits,
  zoom, and focus management. Input routing is automatic (active pane gets
  events). The `CALayerHost` is positioned to match the pane's pixel rect.

- **Overlay approach:** Position `CALayerHost` manually on top of the terminal,
  bypassing WezTerm's layout system. Simpler initially but requires
  reimplementing position tracking, resize handling, and focus management.

The BrowserPane approach is more work upfront but integrates cleanly with
WezTerm's existing architecture. It also means `OpenSplit`, zoom, tab switching,
and pane focus all work for free.

### Hardest parts

1. **CALayerHost + ANGLE coexistence** — Making a `CALayerHost` sublayer render
   on top of ANGLE's `CAMetalLayer` in the same view. Need to verify z-ordering
   and transparency. May need to switch from ANGLE to wgpu's native Metal
   backend (`FrontEndSelection::WebGpu`) for cleaner layer control.

2. **BrowserPane rendering** — Ensuring the terminal renderer doesn't draw over
   the `CALayerHost`. The pane returns empty lines, but the renderer still draws
   a background. Need transparent background for browser panes, or clip the
   render region.

3. **Mode switching UX** — WezTerm has its own modal system (copy mode, search
   overlay, launcher). Integrating TermSurf's browse/control mode without
   conflicting with existing modals.

### Result

**Research complete.** WezTerm's architecture is a strong match for the TermSurf
protocol. The codebase already has Unix socket servers, per-connection async
dispatch, a pluggable Pane trait, binary tree pane layout, process spawning with
env vars, and a macOS layer tree with CAMetalLayer sublayers. The recommended
approach is to implement `BrowserPane` (the Pane trait) so browser content
integrates natively into WezTerm's tab/split/focus system. The three hardest
problems are CALayerHost compositing alongside ANGLE, transparent pane
rendering, and mode switching UX. None are blockers — they're engineering
problems with known solutions.
