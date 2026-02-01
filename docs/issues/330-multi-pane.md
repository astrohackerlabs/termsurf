# 330: Multi-Webview Connection Bug

Closing one webview causes the profile server to exit, making other webviews in
the same profile inactive.

## Status

**Open.** Discovered during issue 329 testing.

## Problem

When two webviews are open in the same profile and one is closed, the profile
server exits even though the other webview should still be active. This causes
the remaining webview to become inactive (no longer receives frames).

**Steps to reproduce:**

1. Open a terminal
2. Run `web google.com` in pane 0
3. Split pane (Ctrl+Shift+E or similar)
4. Run `web google.com` in pane 1
5. Close pane 1's webview (Ctrl+C twice)
6. Observe: pane 0's webview becomes inactive

**Expected behavior:** Closing one webview should not affect other webviews in
the same profile. The profile server should remain running as long as at least
one webview is active.

## Background

The profile server tracks GUI connections with a counter:

```rust
static GUI_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);
```

When a connection is established, the count increments. When a connection closes
(error received), the count decrements. When the count reaches 0, the profile
server exits gracefully.

The launcher has profile reuse logic — when a second `web` command uses the same
profile as an existing profile server, the launcher forwards a `create_browser`
command to the existing server instead of spawning a new one.

## Observed Behavior

**Profile server logs:**

```
Profile: GUI connection established (total: 1)
Profile: GUI connection established (total: 2)
Profile: GUI disconnected (remaining: 1)
Profile: GUI disconnected (remaining: 0)
Profile: No more GUI connections, exiting gracefully
```

Both connections disconnect when only one webview is closed.

**GUI logs:**

```
13:23:15.279  [XPC-CONN] Stored connection for pane 0: 0x865940030
13:23:21.690  [XPC-CONN] Stored connection for pane 1: 0x8670b2810
13:23:27.279  [Webview] Ctrl+C in Control mode → Exit browser
13:23:27.279  [XPC] Removed connection for pane 1
13:23:27.279  [XPC] Removed invalidate callback for pane 1
13:23:27.279  ERROR [XPC Manager] Connection error: XPC connection invalid
13:23:27.279  [Webview] Closed webview for pane 1
```

The GUI correctly tracks separate connections for each pane, but an "XPC
connection invalid" error appears immediately after removing pane 1's
connection.

## Analysis

The GUI side properly maintains separate connections per pane:

- `peer_connections: HashMap<PaneId, Arc<XpcConnection>>`
- Each webview has its own connection stored by pane ID
- Removing one pane's connection should only affect that connection

The profile server also creates separate connections per browser:

- Each `create_browser_on_ui_thread` call creates a new `XpcConnection`
- Each connection has its own event handler that decrements the count on error

However, when one connection is dropped on the GUI side, both connections on the
profile server side appear to receive disconnect errors.

### Possible Causes

1. **XPC connection sharing** — The XPC library might share some state between
   connections created from endpoints on the same anonymous listener.

2. **Listener cleanup** — The GUI stores listeners in a `Vec<XpcListener>` but
   never removes them. When one webview closes, its listener might still be
   active, causing issues.

3. **macOS XPC behavior** — Closing one connection might invalidate the
   underlying Mach port in a way that affects other connections from the same
   process.

4. **Profile server browser cleanup** — When one browser's GUI connection
   closes, CEF or the browser cleanup code might be affecting the whole process.

## Files Involved

| File                                            | Role                               |
| ----------------------------------------------- | ---------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | GUI-side XPC manager               |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs`    | `close_webview_for_pane()`         |
| `ts3/termsurf-profile/src/main.rs`              | Profile server connection handling |
| `ts3/termsurf-xpc/src/lib.rs`                   | XPC connection wrapper             |

## Experiments

### Experiment 1: Diagnostic Logging

**Goal:** Add detailed connection identifiers to determine which specific
connections are receiving disconnect errors and in what order.

**Hypothesis:** The profile server's error handler is being invoked for both
connections when only one is closed. By adding unique identifiers to each
connection, we can determine if:

1. Both connections genuinely receive errors (XPC library issue)
2. One connection's error handler is being called twice (bug in our code)
3. The connections share some state that gets invalidated (XPC endpoint issue)

**Changes:**

1. **Add connection ID to profile server** (`ts3/termsurf-profile/src/main.rs`)

   Add a static counter and capture it in each connection's error handler:

   ```rust
   // Near other statics at top of file
   static CONNECTION_ID: AtomicU64 = AtomicU64::new(0);
   ```

   In `create_browser_on_ui_thread`, before setting up the event handler:

   ```rust
   let conn_id = crate::CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
   println!("[CONN-{}] Creating GUI connection for session {}", conn_id, session_id);
   ```

   Update the connection established message:

   ```rust
   let count = crate::GUI_CONNECTION_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
   println!("[CONN-{}] GUI connection established (total: {})", conn_id, count);
   ```

   Update the error handler to include the connection ID:

   ```rust
   Err(e) => {
       match e {
           XpcError::ConnectionInterrupted | XpcError::ConnectionInvalid => {
               let count = crate::GUI_CONNECTION_COUNT.fetch_sub(1, Ordering::Relaxed) - 1;
               println!("[CONN-{}] GUI disconnected (remaining: {})", conn_id, count);
               if count == 0 {
                   println!("[CONN-{}] No more GUI connections, exiting gracefully", conn_id);
                   crate::QUIT_FLAG.store(true, Ordering::Relaxed);
               }
           }
           _ => eprintln!("[CONN-{}] GUI connection error: {}", conn_id, e),
       }
   }
   ```

2. **Add logging to GUI connection removal**
   (`ts3/wezterm-gui/src/termwindow/webview_xpc.rs`)

   Update `remove_connection` to log the pointer before removal:

   ```rust
   pub fn remove_connection(&self, pane_id: PaneId) {
       let mut connections = self.peer_connections.lock().unwrap();
       if let Some(conn) = connections.remove(&pane_id) {
           log::info!(
               "[XPC] Removing connection for pane {}: {:p} (dropping Arc)",
               pane_id,
               Arc::as_ptr(&conn)
           );
           // conn is dropped here when it goes out of scope
       } else {
           log::warn!("[XPC] No connection found for pane {}", pane_id);
       }
   }
   ```

**Files to modify:**

| File                                            | Changes                         |
| ----------------------------------------------- | ------------------------------- |
| `ts3/termsurf-profile/src/main.rs`              | Add CONNECTION_ID, update logs  |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Update `remove_connection` logs |

**Verification:**

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test: Open two webviews, close one
web google.com
# Split pane
web google.com
# Close second webview (Ctrl+C twice)

# Check profile server logs
cat /tmp/termsurf-profile-*.log | grep "CONN-"
# Expected output should show:
# [CONN-0] Creating GUI connection for session pane-0-XXXXX
# [CONN-0] GUI connection established (total: 1)
# [CONN-1] Creating GUI connection for session pane-1-XXXXX
# [CONN-1] GUI connection established (total: 2)
# Then when closing pane 1:
# [CONN-1] GUI disconnected (remaining: 1)  <- expected
# [CONN-0] GUI disconnected (remaining: 0)  <- BUG: why is CONN-0 disconnecting?

# Check GUI logs
cat /tmp/termsurf-gui.log | grep "Removing connection"
# Should show only pane 1's connection being removed
```

**Success criteria:**

- [ ] Logs clearly show which connection ID receives each disconnect
- [ ] Can determine if both connections genuinely error or if it's a double-call
- [ ] Identify root cause for further experiments

**Expected outcome:** The logs will reveal whether:

- CONN-0 and CONN-1 both receive genuine XPC errors (library/OS issue)
- Only CONN-1 should disconnect but CONN-0's handler fires too (shared state)
- The error handler is being called twice for the same connection (bug)

### Future Experiments

**Experiment 2: Delay Connection Removal** — Test if timing affects the issue by
adding a delay before dropping the connection.

**Experiment 3: Listener Lifecycle** — Investigate whether XPC listeners need
cleanup when webviews close.

**Experiment 4: Separate Endpoints** — Test if using completely separate XPC
mechanisms for each browser avoids the issue.

## Success Criteria

- [ ] Closing one webview does not affect other webviews
- [ ] Profile server remains running while at least one webview is active
- [ ] Connection count accurately reflects active connections
- [ ] No "XPC connection invalid" errors when closing a single webview

## References

- Issue 329 — Where this bug was discovered
- Issue 326 — Profile server graceful shutdown (introduced connection counting)
- `CLAUDE.md` — Documents "Current gap" with multi-webview support
