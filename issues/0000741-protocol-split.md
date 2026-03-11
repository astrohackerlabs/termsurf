# Issue 741: Split protocol into two channels

## Goal

Replace the single `termsurf.proto` with two protocols — one for the GUI channel
(TUI↔GUI) and one for the browser channel (GUI+TUI↔Browser) — and let the TUI
talk directly to the browser engine over its own socket, eliminating all message
proxying through the GUI.

## Background

The current protocol is a single `TermSurfMessage` oneof with 30 message types.
Five of these are proxied through the GUI:

- **UrlChanged, LoadingState, TitleChanged** — Browser sends to GUI, GUI
  forwards verbatim to TUI. The GUI does nothing with the data.
- **Navigate** — TUI sends with `pane_id`, GUI swaps it for `tab_id` and
  forwards to browser. Pure ID translation.
- **SetColorScheme** — Same as Navigate, except the GUI also stores `pane.dark`
  (only used to populate `CreateTab.dark` for new tabs).

These dual-use messages have overloaded fields (`tab_id` for one direction,
`pane_id` for the other), which is a design smell. Worse, the proxy pattern
scales badly — every future browser feature (JS dialogs, downloads, file
uploads, auth challenges, permissions, find-in-page, console capture) would need
forwarding code in both Ghostboard (Zig) and Wezboard (Rust). That's two
implementations of the same do-nothing relay, per message, forever.

### Process responsibilities

**Current state** — The GUI does two unrelated jobs: terminal emulation and
message relay. It proxies content messages it doesn't use, maintains ID maps it
only needs because it's in the middle, and stores state (like `pane.dark`) only
to echo it back later. Every new browser feature adds forwarding code to two GUI
implementations (Ghostboard in Zig, Wezboard in Rust).

**What each process should own:**

- **TUI** — Owns user intent. Browser chrome: URL bar, navigation, modes,
  commands. Talks to the GUI about layout (overlays, splits, mode changes).
  Talks to the browser about content (navigation, page state, dialogs,
  downloads). Direct browser client.
- **GUI** — Owns the window. Terminal rendering, pane layout, overlay
  compositing, input capture, process lifecycle. Tells the browser "create a tab
  at these dimensions" and "here are mouse/key events." Tells the TUI "browser
  is ready, here's how to connect." Does NOT relay content messages. Does NOT
  track URLs, titles, loading state, or color schemes.
- **Browser** — Owns web content. Renders pages, manages tabs, reports state.
  Accepts connections from anyone — GUI for input/compositing, TUI for content.
  Doesn't care who's asking, just handles messages.

The key shift: the GUI stops being a router and becomes purely a rendering/input
layer. The TUI becomes the browser's direct client. The browser becomes a
multi-client server.

### The hub-and-spoke assumption

The current architecture routes all communication through the GUI:

```
TUI ──socket──> GUI ──socket──> Browser
```

This was inherited from the XPC era (ts5), where the GUI was necessarily the
hub. With Unix sockets there is no such constraint. The TUI can connect directly
to the browser:

```
TUI ──socket──> GUI        (overlay geometry, mode changes, queries)
TUI ──socket──> Browser    (navigation, page state, content features)
GUI ──socket──> Browser    (input, compositing, tab lifecycle, focus)
```

### What changes

The GUI remains responsible for:

- **Process lifecycle** — Launching and killing browser engine processes.
- **Overlay rendering** — CALayerHost setup, pixel coordinates, resize.
- **Input forwarding** — Keyboard/mouse events come from the GUI's window.
- **Tab lifecycle** — CreateTab, CloseTab, Resize (the GUI knows pixel
  dimensions from overlay geometry).
- **Focus and cursor** — FocusChanged, CursorChanged (the GUI owns window
  focus).
- **Configuration queries** — Hello, QueryLast, QueryDevtools (the GUI knows
  what browsers and profiles exist).

The TUI takes over:

- **Navigation** — Navigate (the TUI already knows the URL, now sends it
  directly to the browser with `tab_id`).
- **Page state** — UrlChanged, LoadingState, TitleChanged (the browser sends
  directly to the TUI).
- **Color scheme** — SetColorScheme (the TUI sends directly to the browser).
- **All future content features** — JS dialogs, downloads, file uploads, auth,
  permissions, find-in-page, console capture. These are all TUI↔Browser
  conversations that the GUI has no business intermediating.

### Connection handoff

The key mechanism is the GUI telling the TUI how to connect to the browser:

1. TUI sends `SetOverlay` to GUI (as today).
2. GUI launches Roamium (if needed) and sends `CreateTab` to the browser.
3. Browser responds with `TabReady { pane_id, tab_id }` to the GUI.
4. GUI sends a new `BrowserReady { tab_id, browser_socket }` message to the TUI.
5. TUI connects directly to Roamium's socket using the provided path.
6. TUI registers itself with the browser via a new `TuiRegister { tab_id }`
   message so the browser knows which connection owns which tab.
7. All content-level messages now flow directly: TUI↔Browser.

The browser needs to accept multiple connections — one from the GUI (for input,
compositing, lifecycle) and one or more from TUIs (for content). Today Roamium
has a single connection to the GUI. It would need to listen on its own socket
(or accept multiple connections on the GUI's socket — but a dedicated browser
socket is cleaner).

### Roamium socket model

Today Roamium connects to the GUI's socket as a client (`--ipc-socket={path}`).
For TUI↔Browser direct communication, Roamium needs its own listening socket so
TUIs can connect to it:

1. GUI spawns Roamium with `--ipc-socket={gui_socket}` (as today) plus a new
   `--listen-socket={browser_socket}` argument.
2. Roamium connects to the GUI socket (for input/compositing/lifecycle) and
   listens on its own socket (for TUI content connections).
3. GUI sends the browser socket path to the TUI in `BrowserReady`.
4. TUI connects to the browser socket directly.

The browser socket path follows the existing convention:
`$TMPDIR/termsurf/termsurf-roamium-{pid}.sock`.

### ID model

With a direct connection, the TUI learns `tab_id` from `BrowserReady` and uses
it in all messages to the browser. No more `pane_id` in browser messages, no
more `tab_id` in TUI↔GUI messages. Each protocol uses its own natural
identifier:

- **TUI↔GUI:** `pane_id` (string, assigned by TUI)
- **GUI↔Browser:** `tab_id` (int64, assigned by Chromium)
- **TUI↔Browser:** `tab_id` (int64, learned from `BrowserReady`)

### QueryTabs

`QueryTabsRequest/Reply` currently flows TUI→GUI→Browser→GUI→TUI. The GUI asks
the browser for tab counts, merges in its own pane count, and replies to the
TUI. With the split:

- The TUI can query the browser directly for tab info (TUI↔Browser).
- The TUI can query the GUI for pane info (TUI↔GUI).
- The TUI assembles the combined view itself.

Or `QueryTabs` could stay on TUI↔GUI for now and be refactored later. Either
way, it's no longer a three-hop relay.

### Proto file structure

Two proto files, not three. The browser doesn't need separate protocols for GUI
and TUI connections — a CreateTab is a CreateTab regardless of who sends it. The
browser receives protobuf messages and acts on them; it doesn't need to restrict
which client can send which message.

The only connection-awareness the browser needs is registration:
`ServerRegister` identifies a GUI connection, `TuiRegister` identifies a TUI
connection. After registration, the browser knows where to route events
(CaContext → GUI, UrlChanged → TUI). But all messages share one proto, one
wrapper, one handler.

This also future-proofs the protocol. If the GUI ever needs UrlChanged (e.g.,
for a window title), it just listens for it — no protocol change. If the TUI
ever needs to send Resize directly, it just sends it.

**`proto/termsurf_gui.proto`** — TUI↔GUI channel

```
SetOverlay, SetDevtoolsOverlay, OpenSplit     (TUI → GUI)
ModeChanged                                    (GUI → TUI)
BrowserReady                                   (GUI → TUI) — NEW
HelloRequest/Reply                             (TUI ↔ GUI)
QueryLastRequest/Reply                         (TUI ↔ GUI)
QueryDevtoolsRequest/Reply                     (TUI ↔ GUI)
```

**`proto/termsurf_browser.proto`** — Browser channel (GUI and TUI both connect)

```
ServerRegister                                 (GUI → Browser)
TuiRegister                                    (TUI → Browser) — NEW
CreateTab, CreateDevtoolsTab, CloseTab, Resize (GUI → Browser)
MouseEvent, MouseMove, ScrollEvent, KeyEvent   (GUI → Browser)
FocusChanged                                   (GUI → Browser)
Navigate                                       (TUI → Browser)
SetColorScheme                                 (TUI → Browser)
TabReady                                       (Browser → GUI)
CaContext                                      (Browser → GUI)
CursorChanged                                  (Browser → GUI)
UrlChanged                                     (Browser → TUI)
LoadingState                                   (Browser → TUI)
TitleChanged                                   (Browser → TUI)
QueryTabsRequest/Reply                         (TUI ↔ Browser)
Shutdown                                       (GUI → Browser)
```

Navigate and SetColorScheme lose their dual-use fields — no more `pane_id` in
Navigate, no more `tab_id`-or-`pane_id` ambiguity. Each message has exactly the
fields it needs for its channel.

### What the GUI loses

The GUI no longer sees UrlChanged, TitleChanged, LoadingState, Navigate, or
SetColorScheme. Examining each:

- **UrlChanged, TitleChanged, LoadingState** — The GUI never used these. Pure
  relay today.
- **Navigate** — The GUI never used the URL. Pure relay with ID swap.
- **SetColorScheme** — The GUI stored `pane.dark` to pass to `CreateTab`. Fix:
  the TUI already sends `dark` information — either include it in `SetOverlay`,
  or have the TUI send `SetColorScheme` to the browser after connecting. The GUI
  doesn't need to track dark mode state.

### Process management

The GUI remains the process manager:

- **Launching:** GUI spawns Roamium with both `--ipc-socket` (GUI connection)
  and `--listen-socket` (TUI connection). Same as today plus one argument.
- **Shutdown:** GUI sends `Shutdown` message to browser (Issue 732/733). No
  change.
- **Crash detection:** GUI monitors child processes. If Roamium dies, GUI
  notifies all TUIs that had tabs on that browser (new message or error on
  existing queries).
- **Reuse:** GUI tracks which profile/browser combinations already have a
  running Roamium. When a TUI requests a new overlay on an existing profile, the
  GUI sends `CreateTab` to the existing Roamium and returns the same browser
  socket path to the TUI.

The TUI does NOT launch or kill browser processes. It asks the GUI (via
`SetOverlay`), gets back a `BrowserReady` with the socket path and tab_id, and
connects directly.

### Why direct sockets, not a proxy envelope

An alternative approach would keep the hub-and-spoke topology and add a generic
proxy envelope (`ProxyToBrowser { pane_id, bytes }` /
`ProxyToTui { tab_id, bytes }`). The GUI would relay opaque bytes between TUI
and browser, replacing per-message forwarding with a single generic function.
This is less upfront work — no new sockets, no multi-connection handling.

However, the proxy envelope is a detour, not a stepping stone. The work does not
carry over to direct sockets:

- The generic relay code in both GUIs would be written and then deleted.
- The `tab_to_pane` / `pane_to_tab` ID mapping would be maintained and then
  deleted.
- The TUI would wrap messages in envelopes and then stop wrapping them.
- The browser would receive unwrapped messages from the GUI and then switch to
  receiving them from a TUI connection.

The direct socket approach has three concrete pieces of work:

1. **Roamium listener** (~50 lines of Rust) — Add `--listen-socket=`, accept TUI
   connections, tag connections as TUI vs GUI. Same pattern as the existing
   `ipc::connect` but in reverse.
2. **GUI sends `BrowserReady` to TUI** — One new message sent after `TabReady`
   arrives. A few lines in each GUI.
3. **TUI opens a second connection** — Connect to browser socket, send
   `TuiRegister`, spawn a second reader thread. The event loop already
   multiplexes GUI events via `mpsc` — the browser reader thread sends to the
   same channel.

After that, forwarding code is deleted from both GUIs — a net reduction in
complexity. No intermediate state, no throwaway work.

### Staged implementation

The three pieces above map to three experiments, each independently testable:

1. **Roamium listener** — Add the listening socket and TUI registration. The GUI
   still works as before. Nothing is removed yet. Verify: a test client can
   connect and register.
2. **TUI direct connection** — GUI sends `BrowserReady`, TUI connects to browser
   socket. Content messages flow directly. GUI forwarding still exists but is
   now unused for these messages. Verify: navigation works end-to-end over the
   direct socket.
3. **Remove forwarding** — Delete proxy code from both GUIs, remove ID maps,
   split proto files. Verify: everything still works, GUI code is smaller.
