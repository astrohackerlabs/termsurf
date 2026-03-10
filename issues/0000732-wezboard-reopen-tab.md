# Issue 732: Wezboard cannot reopen browser after last tab closes

## Goal

After closing the last browser tab in Wezboard, opening a new webview should
work — launching a fresh browser process if needed. Currently, the second
webview never appears.

## Background

### Symptom

1. Open a webview in Wezboard (`web localhost:3000`).
2. Close that tab (the only browser tab).
3. Open a new webview (`web localhost:3000` again).
4. No webview appears. The browser overlay is missing.

### What happens under the hood

When the TUI disconnects (tab closes), `handle_disconnect` in `conn.rs:862-905`
cleans up the pane:

1. Removes the pane from `st.panes` (line 871).
2. Removes the tab-to-pane mapping (line 873).
3. Sends `CloseTab` to Chromium (lines 879-884).
4. Decrements `server.pane_count` (line 877).
5. Removes CALayers (lines 889-895).

But the **server entry is never removed** from `st.servers`. The `Server` struct
stays in the HashMap with `pane_count: 0` and potentially a stale `tx` channel.

When the user opens a new webview, `handle_set_overlay` (line 556) checks
`st.servers.contains_key(&key)`. Since the old server entry still exists, it
takes the `else` branch (line 568) — reusing the existing server instead of
spawning a new one. It increments `pane_count` and tries to send `CreateTab`
through the server's `tx` channel.

The problem: by this point, the Roamium process has likely exited (after its
last tab was closed), and the socket connection is dead. Two failure modes:

1. **Roamium exited, `ConnType::Chromium` disconnect was handled** — `server.tx`
   is `None`. The `CreateTab` message is never sent. The log shows:
   `"SetOverlay: server exists but tx is None — CreateTab not sent!"` (line
   583).

2. **Roamium exited but disconnect not yet processed** — `server.tx` is `Some`
   but the channel is closed. `try_send` silently fails or errors.

Either way, no `CreateTab` reaches Chromium, no `TabReady` comes back, and no
webview appears.

### The stale server problem

The root cause is that `st.servers` is never cleaned up. The server entry
persists indefinitely after the process exits. The get-or-create logic in
`handle_set_overlay` (line 556) trusts that an existing entry means a live,
reachable server — but that's not true after the process has exited.

### Relevant code paths

| Location          | What it does                                                         |
| ----------------- | -------------------------------------------------------------------- |
| `conn.rs:554-585` | Get-or-create server in `handle_set_overlay`                         |
| `conn.rs:862-905` | TUI disconnect cleanup (decrements pane_count, never removes server) |
| `conn.rs:906-919` | Chromium disconnect (sets `server.tx = None`, never removes server)  |
| `conn.rs:949-989` | `spawn_server()` — launches Roamium process                          |
| `state.rs:32-39`  | `Server` struct — holds process handle, tx, pane_count               |

### How Ghostboard handles this

In Ghostboard, the browser process lifecycle is managed differently — servers
are spawned per-connection and cleaned up when the connection drops. The stale
server problem doesn't exist because there's no persistent server registry that
outlives the connection.

### Fix strategy

When the get-or-create logic in `handle_set_overlay` finds an existing server
entry, it needs to check whether the server is actually alive and reachable. If
not, it should remove the stale entry and spawn a fresh server.

The simplest check: if `server.tx` is `None` and `server.pane_count == 0`, the
server is dead — remove it and fall through to the spawn path.

A more robust check: also verify the process handle (`server.process`) with
`try_wait()` to detect if the process has exited even before the socket
disconnect was processed.

## Experiments

### Experiment 1: Board-controlled server lifecycle via Shutdown message

#### Goal

The board (GUI) should own the Roamium process lifecycle. Currently Roamium
self-terminates when its last tab closes (`dispatch.rs:119`), but the board
doesn't know this happened, leaving a stale server entry that blocks future tab
creation.

The fix: add a `Shutdown` protocol message. The board sends it when `pane_count`
drops to 0. Roamium receives it and exits gracefully. Roamium no longer
self-terminates — the board is the only authority that decides when Roamium
exits.

This will temporarily break Ghostboard (which doesn't send `Shutdown` yet).
Ghostboard will be fixed in a later issue.

#### Changes

**1. `proto/termsurf.proto`** — add Shutdown message

Add to the `oneof msg` block (next available tag is 31):

```protobuf
Shutdown shutdown = 31;
```

Add the message definition (empty — it's a signal, no payload needed):

```protobuf
message Shutdown {}
```

**2. `roamium/src/dispatch.rs`** — handle Shutdown, remove self-kill

Add a `Shutdown` handler in the `match inner` block:

```rust
Msg::Shutdown(_) => {
    unsafe { ffi::ts_quit() };
}
```

Remove the self-kill from `CloseTab` (lines 118–120). The handler becomes:

```rust
Msg::CloseTab(m) => {
    let tab_id = m.tab_id;
    if let Some(t) = find_by_tab_id(tab_id) {
        unsafe { ffi::ts_destroy_web_contents(t.handle) };
    }
    tabs().retain(|t| t.tab_id != tab_id);
    // No longer self-kills — board sends Shutdown when ready.
}
```

**3. `wezboard/wezboard-gui/src/termsurf/conn.rs`** — send Shutdown on last pane
close

In `handle_disconnect`, TUI branch (lines 876–886), after decrementing
`server.pane_count`, check if it reached 0. If so, send `Shutdown` and mark the
server for removal:

```rust
server.pane_count = server.pane_count.saturating_sub(1);
if let Some(ref server_tx) = server.tx {
    let msg = TermSurfMessage {
        msg: Some(Msg::CloseTab(proto::CloseTab {
            tab_id: pane.tab_id,
        })),
    };
    let _ = server_tx.try_send(msg.encode_to_vec());

    // Last pane — tell Roamium to exit gracefully.
    if server.pane_count == 0 {
        log::info!(
            "last pane closed for server key={}, sending Shutdown",
            key
        );
        let shutdown_msg = TermSurfMessage {
            msg: Some(Msg::Shutdown(proto::Shutdown {})),
        };
        let _ = server_tx.try_send(shutdown_msg.encode_to_vec());
        servers_to_remove.push(key.clone());
    }
}
```

Collect keys during the pane removal loop, then remove after:

```rust
for key in &servers_to_remove {
    st.servers.remove(key);
}
```

This is needed because we can't remove from `st.servers` while iterating panes
that reference it.

No changes to `handle_set_overlay` — the get-or-create logic already works
correctly when the server entry doesn't exist.

**4. Rebuild Roamium protobuf** — the `prost-build` step in Roamium's `build.rs`
regenerates from `proto/termsurf.proto`, so rebuilding Roamium picks up the new
`Shutdown` message automatically.

#### Verification

1. `scripts/build-roamium.sh` — builds Roamium and copies binary to
   `chromium/src/out/Default/roamium` (must be colocated with Chromium's dylibs,
   `.pak` files, `icudtl.dat`, etc. because Chromium re-executes the binary for
   child processes)
2. `cd wezboard && cargo build` — compiles with Shutdown send logic
3. Test with
   `web --browser $REPO/chromium/src/out/Default/roamium localhost:3000` where
   `$REPO` is the repo root (e.g., `/Users/ryan/dev/termsurf`). The absolute
   path bypasses the installed version at `/usr/local/roamium/`
4. Confirm the webview loads
5. Close the tab — logs show "last pane closed for server key=..., sending
   Shutdown"
6. Open a new webview with the same `--browser` path — it should appear and load
7. Repeat steps 4–6 multiple times to confirm reliability
8. Open two webviews, close one — server stays alive (pane_count > 0). Close the
   second — Shutdown is sent, server exits
