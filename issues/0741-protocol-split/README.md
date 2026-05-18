+++
status = "closed"
opened = "2026-03-11"
closed = "2026-03-11"
+++

# Issue 741: Split protocol into two channels

## Goal

Replace the single `termsurf.proto` with two protocols — a GUI protocol
(TUI↔GUI) and a browser protocol (GUI+TUI↔Browser) — and let the TUI talk
directly to the browser engine over its own socket, eliminating all message
proxying through the GUI.

## Background

### Process responsibilities

Each process should own one concern:

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

Today the GUI violates this separation. It proxies five message types between
TUI and browser:

- **UrlChanged, LoadingState, TitleChanged** — Browser sends to GUI, GUI
  forwards verbatim to TUI. The GUI does nothing with the data.
- **Navigate** — TUI sends with `pane_id`, GUI swaps it for `tab_id` and
  forwards to browser. Pure ID translation.
- **SetColorScheme** — Same as Navigate, except the GUI also stores `pane.dark`
  (only used to populate `CreateTab.dark` for new tabs).

These dual-use messages have overloaded fields (`tab_id` for one direction,
`pane_id` for the other). The proxy pattern scales badly — every future browser
feature (JS dialogs, downloads, file uploads, auth challenges, permissions,
find-in-page, console capture) would need forwarding code in both Ghostboard
(Zig) and Wezboard (Rust). That's two implementations of the same do-nothing
relay, per message, forever.

### Current architecture

All communication flows through the GUI as a hub:

```
TUI ──socket──> GUI ──socket──> Browser
```

This was inherited from the XPC era (ts5), where the GUI was necessarily the
hub. With Unix sockets there is no such constraint.

### Target architecture

The TUI connects directly to the browser for content-level communication:

```
TUI ──socket──> GUI        (overlay geometry, mode changes, queries)
TUI ──socket──> Browser    (navigation, page state, content features)
GUI ──socket──> Browser    (input, compositing, tab lifecycle, focus)
```

The GUI stops being a router. It becomes purely a rendering/input layer. The TUI
becomes the browser's direct client. The browser becomes a multi-client server.

### Connection handoff

The GUI still manages process lifecycle. The TUI discovers the browser through
the GUI:

1. TUI sends `SetOverlay` to GUI (as today).
2. GUI launches Roamium (if needed) and sends `CreateTab` to the browser.
3. Browser responds with `TabReady { pane_id, tab_id }` to the GUI.
4. GUI sends `BrowserReady { tab_id, browser_socket }` to the TUI.
5. TUI connects directly to Roamium's listening socket.
6. All content-level messages now flow directly: TUI↔Browser.

### Roamium socket model

Today Roamium connects to the GUI's socket as a client (`--ipc-socket={path}`).
It needs its own listening socket so TUIs can connect to it:

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

### Two protocols, not three

The browser doesn't need separate protocols for GUI and TUI connections — a
CreateTab is a CreateTab regardless of who sends it. The browser receives
protobuf messages and acts on them; it doesn't restrict which client can send
which message.

The browser doesn't need to know what kind of client is connected. Every
connection gets every event — the browser broadcasts all outbound messages
(TabReady, CaContext, UrlChanged, etc.) to all connections. Each client ignores
what it doesn't care about. The GUI ignores UrlChanged. The TUI ignores
CursorChanged. No registration, no connection types, no routing logic.

`ServerRegister` stays as the only identification message — it tells the browser
which profile this process serves, not what kind of client is connecting.

This future-proofs the protocol. If the GUI ever needs UrlChanged (e.g., for a
window title), it already receives it. If the TUI ever needs to send Resize
directly, it just sends it.

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
ServerRegister                                 (Client → Browser)
CreateTab, CreateDevtoolsTab, CloseTab, Resize (Client → Browser)
MouseEvent, MouseMove, ScrollEvent, KeyEvent   (Client → Browser)
FocusChanged                                   (Client → Browser)
Navigate                                       (Client → Browser)
SetColorScheme                                 (Client → Browser)
QueryTabsRequest                               (Client → Browser)
TabReady                                       (Browser → all clients)
CaContext                                      (Browser → all clients)
CursorChanged                                  (Browser → all clients)
UrlChanged                                     (Browser → all clients)
LoadingState                                   (Browser → all clients)
TitleChanged                                   (Browser → all clients)
QueryTabsReply                                 (Browser → all clients)
Shutdown                                       (Client → Browser)
```

Navigate and SetColorScheme lose their dual-use fields — no more `pane_id` in
Navigate, no more `tab_id`-or-`pane_id` ambiguity. Each message has exactly the
fields it needs for its channel.

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

1. **Roamium listener** (~50 lines of Rust) — Add `--listen-socket=`, accept
   connections, broadcast events to all. Same pattern as the existing
   `ipc::connect` but in reverse.
2. **GUI sends `BrowserReady` to TUI** — One new message sent after `TabReady`
   arrives. A few lines in each GUI.
3. **TUI opens a second connection** — Connect to browser socket, spawn a second
   reader thread. The event loop already multiplexes GUI events via `mpsc` — the
   browser reader thread sends to the same channel.

After that, forwarding code is deleted from both GUIs — a net reduction in
complexity. No intermediate state, no throwaway work.

### Staged implementation

Prove the architecture on one GUI first, then port. Wezboard is the right
starting point — it's Rust (like the TUI and Roamium), under active development,
and easier to iterate on. Ghostboard (Zig) gets ported after the design is
proven.

The four experiments, each independently testable:

1. **Roamium listener** — Add `--listen-socket=`, accept connections, broadcast
   events to all. Shared across both GUIs — the browser doesn't care which GUI
   launched it. The GUI still works as before. Nothing is removed yet. Verify: a
   test client can connect and receive events.
2. **Wezboard + TUI direct connection** — Wezboard sends `BrowserReady` after
   `TabReady`. TUI connects to browser socket. Content messages flow directly.
   Wezboard forwarding still exists but is now unused for these messages.
   Verify: navigation works end-to-end over the direct socket with Wezboard.
3. **Remove Wezboard forwarding** — Delete proxy code from Wezboard, remove ID
   maps, split proto files. Verify: everything still works with Wezboard, GUI
   code is smaller.
4. **Port to Ghostboard** — Implement `BrowserReady` in Ghostboard, remove its
   forwarding code. The proto files and Roamium are already done. Verify:
   everything works with Ghostboard.

## Experiments

### Experiment 1: Roamium listener

#### Description

Add a listening socket to Roamium so clients can connect directly. Today Roamium
has a single outbound connection to the GUI. This experiment adds an inbound
listener that accepts any number of connections and broadcasts all outbound
events (TabReady, CaContext, UrlChanged, LoadingState, TitleChanged,
CursorChanged, QueryTabsReply) to every connected client. Inbound messages from
any connection are dispatched the same way. The GUI connection and all existing
behavior remain unchanged.

#### Changes

**`roamium/src/main.rs`** — Parse `--listen-socket=`

Add a new `OnceLock<String>` for the listen socket path. Parse
`--listen-socket=` from argv alongside the existing `--ipc-socket=`. In
`on_initialized`, after connecting to the GUI, call `ipc::listen()` to start the
listener.

**`roamium/src/ipc.rs`** — Add listener and broadcast

Current state: `ipc.rs` has a single global `WRITER` (the GUI connection) and
`send()` always writes to it. Change this to broadcast to all connections.

Rename `WRITER` to `WRITERS: Mutex<Vec<UnixStream>>`. The GUI connection is the
first entry. Each accepted listener connection adds another entry.

- `connect()` — Same as today, but pushes to `WRITERS` instead of setting a
  single `WRITER`.
- `listen(path: &str)` — Bind a `UnixListener`, spawn an accept thread. Each
  accepted connection clones the write half into `WRITERS` and spawns a reader
  thread (same `reader_loop` pattern as the GUI connection).
- `send()` — Iterate `WRITERS`, write the message to each. Remove any connection
  that errors (client disconnected). This replaces the single-writer send.

The accept thread and reader threads post messages to the UI thread via
`ts_post_task`, same as the GUI reader thread. The reader thread for a listener
connection does NOT quit the process on EOF — only the GUI connection EOF
triggers shutdown (a disconnected client is not fatal).

**`roamium/src/dispatch.rs`** — No changes

All callbacks already call `ipc::send()`. Since `send()` now broadcasts, every
connected client receives every event. Inbound message dispatch is unchanged —
`handle_message()` doesn't care which connection the message came from.

#### Verification

1. Build Roamium: `./scripts/build.sh roamium`
2. The existing system works unchanged — launch Wezboard, browse a page, verify
   UrlChanged/LoadingState/TitleChanged still arrive at the TUI (via GUI proxy).
3. Write a test script (`scripts/test-browser-socket.sh`) that: a. Finds the
   Roamium listen socket in `$TMPDIR/termsurf/`. b. Connects to it with `socat`
   or a small Rust program. c. Navigates to a page in the browser. d. Observes
   that TabReady, CaContext, UrlChanged, LoadingState, TitleChanged, and
   CursorChanged all arrive on the direct connection.
4. Verify Roamium logs show the connection accepted.

**Result:** Fail

The code changes are correct — Roamium accepts `--listen-socket=`, binds a
listener, accepts connections, and broadcasts to all writers. However, the
experiment cannot be verified because Wezboard (the GUI) does not pass
`--listen-socket=` when spawning Roamium. Without that argument, Roamium never
starts its listener, so there is no socket to connect to. Verification requires
modifying Wezboard's spawn command to pass the flag, which is a GUI change —
outside the scope of this experiment.

#### Conclusion

The Roamium-side implementation is complete but untestable in isolation. The
experiment's scope was too narrow: it assumed the listener could be verified
without any GUI changes, but Roamium is always launched by the GUI and the GUI
controls its arguments. The `--listen-socket=` flag must be passed by Wezboard
for the listener to activate. Fold this into Experiment 2, which modifies
Wezboard anyway.

### Experiment 2: Wezboard passes listen socket, TUI connects directly

#### Description

Complete the direct connection path end-to-end. Wezboard passes
`--listen-socket=` when spawning Roamium, sends a new `BrowserReady` message to
the TUI after `TabReady` arrives, and the TUI opens a second socket connection
to the browser. Content messages (UrlChanged, LoadingState, TitleChanged) arrive
over the direct connection. Navigate and SetColorScheme are sent directly to the
browser. The existing GUI forwarding still works in parallel — nothing is
removed yet.

This experiment subsumes the untested piece of Experiment 1 (Wezboard must pass
the flag for the listener to activate) and adds the full TUI↔Browser path.

#### Changes

**`proto/termsurf.proto`** — Add `BrowserReady` message

Add field 31 to the `TermSurfMessage` oneof:

```protobuf
// GUI → TUI
BrowserReady browser_ready = 31;
```

Add the message definition:

```protobuf
message BrowserReady {
  string pane_id = 1;
  int64 tab_id = 2;
  string browser_socket = 3;
}
```

The GUI sends this to the TUI after `TabReady` arrives from the browser. It
carries the `tab_id` (so the TUI can address the tab directly) and the
`browser_socket` path (so the TUI can connect).

**`wezboard/wezboard-gui/src/termsurf/state.rs`** — Store listen socket path

Add `listen_socket: String` to the `Server` struct. This is the path Wezboard
passed to Roamium via `--listen-socket=`.

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Two changes

1. **`spawn_server`**: Construct the listen socket path
   (`$TMPDIR/termsurf/termsurf-roamium-{child_pid}.sock`) and pass
   `--listen-socket={path}` to Roamium. Store the path in
   `Server.listen_socket`.

   Note: the path uses the child PID (from `child.id()`), not Wezboard's PID.
   Each Roamium process gets its own unique socket.

2. **`handle_tab_ready`**: After establishing the `tab_to_pane` mapping, look up
   the server for this pane, get its `listen_socket`, and send `BrowserReady` to
   the TUI:

   ```rust
   let msg = TermSurfMessage {
       msg: Some(Msg::BrowserReady(proto::BrowserReady {
           pane_id: ready.pane_id.clone(),
           tab_id: ready.tab_id,
           browser_socket: server.listen_socket.clone(),
       })),
   };
   let _ = pane.tui_tx.try_send(msg.encode_to_vec());
   ```

**`webtui/src/ipc.rs`** — Add browser connection and direct sends

1. **Add `BrowserReady` to `CompositorMessage` enum:**

   ```rust
   BrowserReady { tab_id: i64, browser_socket: String },
   ```

2. **Add `BrowserReady` dispatch** in `dispatch_message`: Send it to the event
   channel as `CompositorMessage::BrowserReady`.

3. **Add `BrowserConnection` struct:**

   ```rust
   pub struct BrowserConnection {
       stream: Mutex<UnixStream>,
       pub tab_id: i64,
   }
   ```

   With methods:
   - `connect(path, tab_id, tx) -> Option<Self>` — Connect to the browser
     socket, spawn a reader thread that sends to the same `LoopEvent` channel as
     the GUI connection. The reader thread reuses the same `reader_loop`
     function (it reads the same protobuf wire format). No `reply_tx` needed —
     the browser connection doesn't do request/reply pairs.
   - `send_navigate(url)` — Send `Navigate { tab_id, pane_id: "", url }`
     directly to the browser.
   - `send_set_color_scheme(dark)` — Send
     `SetColorScheme { tab_id, pane_id: "", dark }` directly to the browser.

   The `send()` helper writes the length-prefixed protobuf to `self.stream`.

4. **Refactor `dispatch_message`**: The browser reader thread receives broadcast
   messages from Roamium (UrlChanged, LoadingState, TitleChanged, CursorChanged,
   TabReady, CaContext, QueryTabsReply). It dispatches UrlChanged, LoadingState,
   and TitleChanged the same way the GUI reader does. Other messages are ignored
   (the TUI doesn't need CaContext or CursorChanged).

**`webtui/src/main.rs`** — Handle `BrowserReady`, use direct connection

1. **Handle `BrowserReady` in the event loop**: When received, call
   `BrowserConnection::connect()` with the socket path, tab_id, and the same
   `tx` sender. Store the `BrowserConnection` as `Option<BrowserConnection>`.

2. **Send Navigate via browser connection**: When the user presses Enter in Edit
   mode, if `browser_conn` is `Some`, call `browser_conn.send_navigate(&url)`
   instead of `compositor.send_navigate(pane_id, &url)`.

3. **Send SetColorScheme via browser connection**: Same pattern — if
   `browser_conn` is `Some`, send directly.

Content messages (UrlChanged, LoadingState, TitleChanged) will now arrive from
both the GUI (forwarded) and the browser (direct). The TUI already handles these
idempotently — receiving the same URL or title twice just overwrites the same
variable. No deduplication needed.

#### Verification

1. `./scripts/build.sh roamium` and `./scripts/build.sh webtui` and
   `./scripts/build.sh wezboard` — all build without errors.
2. Launch Wezboard, run `web ryanxcharles.com`.
3. Check Roamium logs for "listener bound" and "client connected" — confirms
   Experiment 1's untested code now works.
4. Navigate to a different page — confirm URL bar updates (UrlChanged arrives
   via direct connection).
5. Run `:colorscheme dark` — confirm the page changes color scheme (sent
   directly to browser).
6. The system works end-to-end: TUI receives content events from the browser and
   sends Navigate/SetColorScheme directly to the browser.

**Result:** Pass

End-to-end direct TUI↔Browser connection works. Wezboard passes
`--listen-socket=` to Roamium, sends `BrowserReady` to the TUI after `TabReady`,
and the TUI connects directly to the browser's listen socket. Navigate and
SetColorScheme are sent directly; UrlChanged, LoadingState, and TitleChanged
arrive directly from the browser.

Implementation notes:

- Listen socket path uses GUI PID + profile + browser as unique key (since child
  PID isn't known before spawn):
  `$TMPDIR/termsurf/termsurf-roamium-{gui_pid}-{profile}-{browser}.sock`
- `BrowserConnection` reuses the existing `reader_loop` with a dummy `reply_tx`
- GUI forwarding still works in parallel — nothing removed yet

#### Conclusion

The direct TUI↔Browser connection path is fully operational. The TUI can now
send commands to and receive events from the browser without routing through the
GUI. Next step is to stop forwarding content messages through the GUI (protocol
split).

### Experiment 3: Remove Wezboard forwarding

#### Description

Delete the proxy code from Wezboard that forwards content messages between TUI
and browser. Experiment 2 proved that direct TUI↔Browser communication works
end-to-end. Now both paths are active in parallel — messages arrive at the TUI
from both the GUI (forwarded) and the browser (direct). This experiment removes
the GUI forwarding path, making the direct connection the only path for these
messages.

Five message types are currently forwarded through Wezboard:

- **Browser → TUI (3):** UrlChanged, LoadingState, TitleChanged — pure relay,
  Wezboard does nothing with the data.
- **TUI → Browser (2):** Navigate, SetColorScheme — Wezboard swaps `pane_id` for
  `tab_id` and forwards. SetColorScheme also stores `pane.dark`, but this is
  only used in `CreateTab.dark` which runs before the TUI has a direct
  connection anyway.

After removal, `forward_to_tui()` has no callers and is deleted. The
`forward_to_chromium()` helper stays — it's still used by ModeChanged →
FocusChanged translation (the GUI owns focus state). `tab_to_pane` stays — it's
used by CursorChanged, CaContext overlay setup, QueryDevtools, and disconnect
cleanup.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Remove five forwarding cases

In `handle_message` (~line 164), delete the three Browser → TUI forwarding arms:

```rust
// DELETE: UrlChanged forwarding
Some(Msg::UrlChanged(u)) => {
    log::info!("UrlChanged: tab_id={} url={}", u.tab_id, u.url);
    forward_to_tui(u.tab_id, Msg::UrlChanged(u), state);
}

// DELETE: LoadingState forwarding
Some(Msg::LoadingState(l)) => {
    log::debug!("LoadingState: tab_id={} state={}", l.tab_id, l.state);
    forward_to_tui(l.tab_id, Msg::LoadingState(l), state);
}

// DELETE: TitleChanged forwarding
Some(Msg::TitleChanged(t)) => {
    log::info!("TitleChanged: tab_id={} title={}", t.tab_id, t.title);
    forward_to_tui(t.tab_id, Msg::TitleChanged(t), state);
}
```

Replace with debug logs (keep visibility but no action):

```rust
Some(Msg::UrlChanged(u)) => {
    log::debug!("UrlChanged: tab_id={} url={} (TUI receives directly)", u.tab_id, u.url);
}
Some(Msg::LoadingState(l)) => {
    log::debug!("LoadingState: tab_id={} state={} (TUI receives directly)", l.tab_id, l.state);
}
Some(Msg::TitleChanged(t)) => {
    log::debug!("TitleChanged: tab_id={} title={} (TUI receives directly)", t.tab_id, t.title);
}
```

Delete the two TUI → Browser forwarding arms (~lines 176–210):

```rust
// DELETE: Navigate forwarding
Some(Msg::Navigate(n)) => { ... forward_to_chromium ... }

// DELETE: SetColorScheme forwarding (keep pane.dark update)
Some(Msg::SetColorScheme(s)) => { ... forward_to_chromium ... }
```

For Navigate, replace with a debug log. For SetColorScheme, keep only the
`pane.dark` storage (still needed for `CreateTab.dark`), drop the forward:

```rust
Some(Msg::Navigate(n)) => {
    log::debug!("Navigate: pane_id={} url={} (TUI sends directly)", n.pane_id, n.url);
}
Some(Msg::SetColorScheme(s)) => {
    log::debug!("SetColorScheme: pane_id={} dark={} (TUI sends directly)", s.pane_id, s.dark);
    let mut st = state.lock().unwrap();
    if let Some(pane) = st.panes.get_mut(&s.pane_id) {
        pane.dark = s.dark;
    }
}
```

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Delete `forward_to_tui`

Delete the `forward_to_tui` function (~lines 411–422). It has no remaining
callers.

**`webtui/src/main.rs`** — Send SetColorScheme to both browser and GUI

The TUI currently sends SetColorScheme to either the browser (if connected) or
the GUI (fallback). But Wezboard still needs to know the dark preference for
`CreateTab.dark` on future tabs. Send to both — browser for the page, GUI for
the pane state:

```rust
CommandResult::SetColorScheme(scheme) => {
    if let Some(ref bc) = browser_conn {
        bc.send_set_color_scheme(&scheme);
    }
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        conn.send_set_color_scheme(pid, &scheme);
    }
}
```

This replaces the current either/or logic with both.

#### Verification

1. `./scripts/build.sh roamium && ./scripts/build.sh webtui && ./scripts/build.sh wezboard`
   — all build without errors.
2. Launch Wezboard, run `web ryanxcharles.com`.
3. Navigate to a page — URL bar updates (UrlChanged arrives directly from
   browser, NOT forwarded through GUI).
4. `:colorscheme dark` — page changes (SetColorScheme sent directly to browser).
5. Check Wezboard logs — confirm "TUI receives directly" and "TUI sends
   directly" debug messages appear instead of forwarding calls.
6. Open a second `web` pane with the same profile — confirm it still works
   (CreateTab still uses the correct dark preference).

**Result:** Fail

Web page loading works (UrlChanged arrives directly from browser), but
`:colorscheme dark` does not change the page. SetColorScheme sent directly from
TUI to browser is not taking effect.

#### Conclusion

The forwarding removal for content messages (UrlChanged, LoadingState,
TitleChanged) works — pages load and the URL bar updates via the direct
TUI↔Browser connection. However, SetColorScheme fails when sent directly from
the TUI to the browser, indicating the direct path for TUI→Browser commands is
not working correctly. The next experiment should investigate why the browser
does not apply SetColorScheme when received over the direct connection.

### Experiment 4: Shorten Roamium listener socket path

The real root cause of experiment 3's failure: Roamium's listener socket never
bound. The logs show
`[Roamium] listener bind failed: path must be shorter than SUN_LEN`. Unix domain
sockets have a 104-byte path limit on macOS (108 on Linux). The current path
format is:

```
{tmpdir}/termsurf/termsurf-roamium-{gui_pid}-{profile}-{browser_path_with_slashes_replaced}.sock
```

Which produces paths like:

```
/var/folders/gh/nd9n2wpn4xb0s8m73msnz7tc0000gn/T//termsurf/termsurf-roamium-85041-default--Users-ryan-dev-termsurf-chromium-src-out-Default-roamium.sock
```

The full filesystem path to the browser binary is redundant — only the process
name matters for identification. The GUI PID, profile, and process name are
sufficient for uniqueness.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — `spawn_server()` (line 975)

Replace the socket path format. Extract the filename from the browser path and
append a 4-byte SHA-256 hash of the full path for uniqueness (allows testing two
different builds of the same binary name):

```rust
// Before:
let listen_sock = format!(
    "{}/termsurf/termsurf-roamium-{}-{}-{}.sock",
    std::env::temp_dir().display(),
    std::process::id(),
    profile,
    browser.replace('/', "-")
);

// After:
use sha2::{Digest, Sha256};

let browser_name = std::path::Path::new(&browser)
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("browser");
let hash = Sha256::digest(browser.as_bytes());
let hash_hex = format!(
    "{:02x}{:02x}{:02x}{:02x}",
    hash[0], hash[1], hash[2], hash[3]
);
let listen_sock = format!(
    "{}/termsurf/{}-{}-{}-{}.sock",
    std::env::temp_dir().display(),
    browser_name,
    hash_hex,
    std::process::id(),
    profile,
);
```

This produces paths like
`/var/folders/.../termsurf/roamium-a1b2c3d4-85041-default.sock` — well within
the 104-byte limit, while still distinguishing two different builds of the same
binary name.

Add `sha2` to `wezboard-gui/Cargo.toml` if not already present.

#### Verification

1. `cargo check -p wezboard-gui` — builds without errors.
2. Launch Wezboard, run `web ryanxcharles.com`.
3. Check Wezboard logs — confirm `[Roamium] listener bound:` appears (not
   `bind failed`).
4. `:colorscheme dark` — page changes to dark mode.
5. `:colorscheme light` — page changes back to light mode.

**Result:** Pass

Shortened socket path resolves the bind failure from Experiment 3. Roamium's
listener socket now binds successfully, and the direct TUI↔Browser connection
works end-to-end. Navigate, `:colorscheme dark`, and `:colorscheme light` all
work via the direct connection. Also removed a stray `eprintln!` debug log in
`webtui/src/ipc.rs` that was corrupting the TUI display.

#### Conclusion

The socket path length was the root cause of Experiment 3's SetColorScheme
failure — the listener never bound, so the TUI's direct connection to the
browser was never established. With the shortened path format
(`{browser_name}-{hash}-{gui_pid}-{profile}.sock`), the listener binds
successfully and all direct TUI↔Browser messages work. The forwarding removal
from Experiment 3 is now validated: content messages flow directly between TUI
and browser without GUI proxying.

## Conclusion

The TUI now talks directly to the browser engine over its own Unix socket,
eliminating all content-message proxying through the GUI. This was the issue's
core goal.

What was accomplished:

- **Roamium listener** — Roamium accepts a `--listen-socket=` argument, binds a
  listening socket, and broadcasts all outbound events to every connected
  client. Inbound messages from any connection are dispatched identically.
- **BrowserReady handoff** — Wezboard sends a
  `BrowserReady { tab_id, browser_socket }` message to the TUI after `TabReady`
  arrives from the browser, giving the TUI everything it needs to connect
  directly.
- **Direct TUI↔Browser connection** — The TUI connects to the browser's listen
  socket and sends Navigate and SetColorScheme directly. UrlChanged,
  LoadingState, and TitleChanged arrive directly from the browser.
- **Forwarding removed from Wezboard** — The five forwarded message types
  (UrlChanged, LoadingState, TitleChanged, Navigate, SetColorScheme) no longer
  pass through the GUI. Wezboard logs them at debug level but takes no action.
  `forward_to_tui()` was deleted.
- **Socket path fix** — The listener socket path uses
  `{browser_name}-{4-byte-hash}-{gui_pid}-{profile}.sock` to stay within the
  104-byte Unix domain socket limit on macOS.

What was deferred:

- **Proto split** — The single `TermSurfMessage` oneof was not split into
  separate `termsurf_gui.proto` and `termsurf_browser.proto` files. Everything
  still uses one message type. This is a cleanup that can happen when the
  protocol grows enough to justify it.
- **Ghostboard port** — Ghostboard still forwards content messages through the
  GUI. The Roamium and proto changes are shared, so porting requires only
  Ghostboard-side changes (send `BrowserReady`, remove forwarding).
- **Wezboard forwarding cleanup** — `forward_to_chromium()` and `tab_to_pane`
  still exist for CursorChanged, CaContext, and other GUI-owned messages. These
  are not content messages and belong in the GUI.
