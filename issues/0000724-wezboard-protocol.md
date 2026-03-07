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

## Experiments

### Experiment 1: Shared state, process spawning, and tab lifecycle

Establish shared state between the socket handler and the rest of the
application, spawn Roamium on `SetOverlay`, and complete the full tab lifecycle:
SetOverlay → spawn Roamium → ServerRegister → CreateTab → TabReady. No rendering
— verification is entirely through logs.

#### Architecture decision: shared state

The socket listener runs on a background thread (`std::thread::spawn` in
`listener.rs`). Connection handlers run as async tasks on the main thread
(`promise::spawn::spawn_into_main_thread`). Both need access to the same state:
pane registry, server registry, tab-to-pane mappings.

Use `Arc<Mutex<TermSurfState>>`:

- Created in `async_run_terminal_gui()` before `spawn_termsurf_server()`.
- Passed to `spawn_termsurf_server()` which passes it to each
  `handle_connection()`.
- The mutex is held briefly per message — no contention concern since connection
  handlers run on the main thread and don't hold the lock across await points.

#### Changes

**1. `wezboard-gui/src/termsurf/state.rs`** — New file. Shared state and data
structures:

```rust
use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};
use smol::channel::Sender;
use prost::Message;

use super::proto;

/// Per-pane state. One pane = one browser overlay in one terminal pane.
pub struct Pane {
    pub pane_id: String,           // UUID from TUI
    pub profile: String,           // browser profile name
    pub browser: String,           // engine name (e.g. "roamium")
    pub url: String,               // pending or current URL
    pub pixel_width: u64,          // overlay pixel dimensions
    pub pixel_height: u64,
    pub tab_id: i64,               // 0 until TabReady received
    pub tui_tx: Sender<Vec<u8>>,   // channel to send messages back to TUI
    pub browsing: bool,            // browse mode active
    pub dark: bool,                // color scheme
    pub inspected_tab_id: i64,     // nonzero = DevTools pane
}

/// Per-server state. One server = one Roamium process = one profile.
pub struct Server {
    pub profile: String,
    pub browser: String,
    pub process: Option<Child>,
    pub tx: Option<Sender<Vec<u8>>>,  // channel to send messages to this server
    pub pane_count: usize,
}

/// Global shared state for the TermSurf protocol.
pub struct TermSurfState {
    /// pane_id → Pane
    pub panes: HashMap<String, Pane>,
    /// "{profile}\0{browser}" → Server
    pub servers: HashMap<String, Server>,
    /// tab_id → pane_id
    pub tab_to_pane: HashMap<i64, String>,
    /// Currently focused pane (only one at a time)
    pub focused_pane: Option<String>,
    /// Last browser pane (for DevTools auto-targeting)
    pub last_browser_pane: Option<String>,
}

impl TermSurfState {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
            servers: HashMap::new(),
            tab_to_pane: HashMap::new(),
            focused_pane: None,
            last_browser_pane: None,
        }
    }

    /// Build the composite server key.
    pub fn server_key(profile: &str, browser: &str) -> String {
        format!("{}\0{}", profile, browser)
    }
}

pub type SharedState = Arc<Mutex<TermSurfState>>;
```

**2. `wezboard-gui/src/termsurf/mod.rs`** — Add `pub mod state;` and re-export
`SharedState`.

**3. `wezboard-gui/src/termsurf/listener.rs`** — Accept `SharedState` parameter:

- `spawn_termsurf_server(sock_path, state)` signature gains
  `state: SharedState`.
- Clone `state` for each accepted connection.
- Pass cloned state to `handle_connection(stream, state)`.

**4. `wezboard-gui/src/termsurf/conn.rs`** — Major rewrite. Accept
`SharedState`, implement real handlers:

**(a) Connection-level state:**

Each connection owns a `Sender<Vec<u8>>` (write end) and reads from the socket.
Use a `smol::channel::bounded` pair: the write end is stored either on a `Pane`
(TUI connection) or a `Server` (Chromium connection). A background write loop
drains the channel and writes to the socket.

```rust
pub async fn handle_connection(
    stream: UnixStream,
    state: SharedState,
) -> anyhow::Result<()>
```

**(b) Write loop:**

Split the `UnixStream` into reader and writer. Spawn a write task that receives
`Vec<u8>` from the channel and writes length-prefixed messages to the socket.

```rust
let (tx, rx) = smol::channel::bounded::<Vec<u8>>(64);
let stream = Async::new(stream)?;
let (reader, writer) = smol::io::split(stream);

// Write loop: drain channel → socket
let write_task = smol::spawn(async move {
    while let Ok(payload) = rx.recv().await {
        let len = (payload.len() as u32).to_le_bytes();
        writer.write_all(&len).await?;
        writer.write_all(&payload).await?;
    }
    Ok::<_, anyhow::Error>(())
});
```

Wait — `Async<UnixStream>` doesn't implement `AsyncRead + AsyncWrite` in a way
that allows splitting. Instead, use the existing pattern: keep the stream whole,
and use `tx` to queue outbound messages. The read loop reads from the stream;
when we need to write, we encode the reply and write directly (for synchronous
replies like `HelloReply`) or queue via `tx` (for forwarded messages).

Actually, simpler approach: the `tx` channel is stored on `Pane` or `Server` so
that _other_ connections can send messages to this one. The read loop handles
incoming messages. For outbound replies (like `HelloReply`), write directly to
the stream. For cross-connection forwarding (Chromium state update → TUI), the
handler looks up the target pane's `tui_tx` and sends through that channel.

Revised connection structure:

```rust
pub async fn handle_connection(
    stream: UnixStream,
    state: SharedState,
) -> anyhow::Result<()> {
    let stream = Arc::new(Async::new(stream)?);
    let (tx, rx) = smol::channel::bounded::<Vec<u8>>(64);

    // Spawn writer task
    let write_stream = stream.clone();
    let write_task = promise::spawn::spawn_into_main_thread(async move {
        while let Ok(payload) = rx.recv().await {
            let len = (payload.len() as u32).to_le_bytes();
            write_stream.write_all(&len).await?;
            write_stream.write_all(&payload).await?;
        }
        Ok::<_, anyhow::Error>(())
    });

    // Read loop (existing pattern but with state + tx)
    let mut buf = Vec::with_capacity(4096);
    let mut conn_type = ConnType::Unknown;
    loop {
        // ... read, parse, dispatch to handlers ...
    }
}
```

**(c) Handler: `SetOverlay`**

```rust
fn handle_set_overlay(
    overlay: proto::SetOverlay,
    tui_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    let browser = if overlay.browser.is_empty() {
        "roamium".to_string()
    } else {
        overlay.browser.clone()
    };

    // Create or update pane
    let is_new = !st.panes.contains_key(&overlay.pane_id);
    let pane = st.panes.entry(overlay.pane_id.clone()).or_insert_with(|| {
        Pane { /* ... fields from overlay ... */ }
    });

    if !is_new {
        // Resize: update dimensions, send Resize if tab_id known
        pane.pixel_width = pixel_w;
        pane.pixel_height = pixel_h;
        if pane.tab_id != 0 {
            // send Resize to server
        }
        return Ok(());
    }

    // New pane: get or create server
    let key = TermSurfState::server_key(&overlay.profile, &browser);
    if !st.servers.contains_key(&key) {
        // Spawn Roamium
        let server = spawn_server(&overlay.profile, &browser, &key)?;
        st.servers.insert(key.clone(), server);
    }
    let server = st.servers.get_mut(&key).unwrap();
    server.pane_count += 1;

    // If server already connected, send CreateTab immediately
    if let Some(ref server_tx) = server.tx {
        send_create_tab(server_tx, &overlay, pane)?;
    }
    // Otherwise, CreateTab will be sent when ServerRegister arrives

    Ok(())
}
```

**(d) Handler: `ServerRegister`**

```rust
fn handle_server_register(
    reg: proto::ServerRegister,
    server_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();

    // Find the server with matching profile that has no tx yet
    for (key, server) in st.servers.iter_mut() {
        if server.profile == reg.profile && server.tx.is_none() {
            server.tx = Some(server_tx.clone());

            // Flush pending tabs: send CreateTab for every pane
            // on this server that hasn't gotten a tab_id yet
            let pending: Vec<String> = st.panes.iter()
                .filter(|(_, p)| {
                    p.profile == server.profile
                        && p.browser == server.browser
                        && p.tab_id == 0
                })
                .map(|(id, _)| id.clone())
                .collect();

            for pane_id in pending {
                let pane = st.panes.get(&pane_id).unwrap();
                send_create_tab(&server_tx, pane)?;
            }
            break;
        }
    }
    Ok(())
}
```

**(e) Handler: `TabReady`**

```rust
fn handle_tab_ready(
    ready: proto::TabReady,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    if let Some(pane) = st.panes.get_mut(&ready.pane_id) {
        pane.tab_id = ready.tab_id;
        st.tab_to_pane.insert(ready.tab_id, ready.pane_id.clone());
        if pane.inspected_tab_id == 0 {
            st.last_browser_pane = Some(ready.pane_id.clone());
        }
        log::info!(
            "TabReady: pane_id={} tab_id={}",
            ready.pane_id, ready.tab_id
        );
    }
    Ok(())
}
```

**(f) Helper: `spawn_server`**

Resolves the browser binary path and spawns it as a child process:

```rust
fn spawn_server(
    profile: &str,
    browser: &str,
    key: &str,
) -> anyhow::Result<Server> {
    let binary = resolve_browser_path(browser)?;
    let sock = std::env::var("TERMSURF_SOCKET")?;

    let data_home = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.local/share", home)
        });
    let user_data_dir = format!(
        "{}/termsurf/chromium-profiles/{}",
        data_home, profile
    );

    let state_home = std::env::var("XDG_STATE_HOME")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.local/state", home)
        });
    let log_file = format!("{}/termsurf/chromium-server.log", state_home);

    let child = std::process::Command::new(&binary)
        .arg(format!("--ipc-socket={}", sock))
        .arg(format!("--user-data-dir={}", user_data_dir))
        .arg("--hidden")
        .arg("--no-sandbox")
        .arg("--enable-logging")
        .arg(format!("--log-file={}", log_file))
        .spawn()
        .with_context(|| format!("spawn {}", binary))?;

    log::info!("spawned {} (pid={}) for profile={}", browser, child.id(), profile);

    Ok(Server {
        profile: profile.to_string(),
        browser: browser.to_string(),
        process: Some(child),
        tx: None,  // set when ServerRegister arrives
        pane_count: 1,
    })
}
```

**(g) Helper: `resolve_browser_path`**

```rust
fn resolve_browser_path(browser: &str) -> anyhow::Result<String> {
    let name = if browser.is_empty() { "roamium" } else { browser };

    // Absolute path passthrough
    if name.starts_with('/') {
        return Ok(name.to_string());
    }

    // Registry lookup
    let home = std::env::var("HOME")?;
    let candidates = &[
        ("roamium", format!("{}/dev/termsurf/chromium/src/out/Default/roamium", home)),
    ];

    for (n, path) in candidates {
        if *n == name && std::path::Path::new(path).exists() {
            return Ok(path.clone());
        }
    }

    anyhow::bail!("browser '{}' not found", name)
}
```

**(h) Helper: `send_create_tab`**

```rust
fn send_create_tab(
    server_tx: &Sender<Vec<u8>>,
    pane: &Pane,
) -> anyhow::Result<()> {
    let msg = proto::TermSurfMessage {
        msg: Some(proto::term_surf_message::Msg::CreateTab(
            proto::CreateTab {
                url: pane.url.clone(),
                pane_id: pane.pane_id.clone(),
                pixel_width: pane.pixel_width,
                pixel_height: pane.pixel_height,
                dark: pane.dark,
            },
        )),
    };
    let payload = msg.encode_to_vec();
    server_tx.try_send(payload)?;
    log::info!("sent CreateTab: pane_id={} url={}", pane.pane_id, pane.url);
    Ok(())
}
```

**5. `wezboard-gui/src/main.rs`** — Create `SharedState` and pass to
`spawn_termsurf_server`:

```rust
// In async_run_terminal_gui(), before the socket setup:
let termsurf_state = Arc::new(Mutex::new(
    termsurf::state::TermSurfState::new(),
));

// Pass to socket server:
if let Err(err) = termsurf::spawn_termsurf_server(termsurf_sock, termsurf_state.clone()) {
    log::warn!("TermSurf socket: {:#}", err);
}
```

**6. `wezboard-gui/Cargo.toml`** — Add `smol` dependency if not already present
(needed for `smol::channel`). The crate already depends on `smol` transitively
through `promise`, but may need a direct dependency for `smol::channel`.

#### Pixel dimensions

`SetOverlay` sends grid coordinates (col, row, width, height in cells). The
board must convert to pixels using cell dimensions. For this experiment, the
connection handler doesn't have access to `TermWindow`'s `RenderMetrics` yet.
Use a placeholder: store the grid dimensions on the `Pane` and set
`pixel_width`/`pixel_height` to `width * 10` and `height * 20` (rough
estimates). A later experiment will wire up real cell dimensions from
`TermWindow`.

#### Cleanup on disconnect

When a TUI connection drops (read returns 0), remove all panes owned by that
connection's `tui_tx`. When a Chromium connection drops, clear the server's `tx`
(but don't remove the server — it may reconnect or be respawned).

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors.
2. Launch Wezboard. Confirm log:
   `TermSurf socket listening on /var/folders/.../T/termsurf/wezboard-{pid}.sock`.
3. In a Wezboard pane, run `web localhost:3000`.
4. Confirm logs in order:
   - `TermSurf client connected` (TUI connection)
   - `TermSurf connection type: Tui`
   - `SetOverlay: pane_id=...`
   - `spawned roamium (pid=...) for profile=...`
   - `TermSurf client connected` (Chromium connection)
   - `TermSurf connection type: Chromium`
   - `ServerRegister: profile=...`
   - `sent CreateTab: pane_id=... url=...`
   - `TabReady: pane_id=... tab_id=...`
5. No crashes. Roamium process visible in Activity Monitor.
6. Close the `web` pane. Confirm TUI disconnect log and pane cleanup.
