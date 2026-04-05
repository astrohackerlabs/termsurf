+++
status = "closed"
opened = "2026-04-05"
closed = "2026-04-05"
+++

# Issue 769: Tab ID collision across browser profiles

## Goal

Fix the bug where having two browser profiles open simultaneously causes one
pane to visually "clone" the other when navigating. The root cause is that
`tab_id` values collide across separate browser processes.

## Background

Each browser profile runs as a separate Roamium process (one Chromium instance
per profile). When a tab is created, Chromium assigns it a `tab_id` — a
per-process integer. Two separate Chromium processes will independently generate
the same `tab_id` values (e.g., both start at 1).

The GUI (Wezboard) maintains a global `tab_to_pane: HashMap<i64, String>` that
maps `tab_id` → `pane_id`. This HashMap assumes `tab_id` is globally unique.
When two profiles produce the same `tab_id`, the second `insert` overwrites the
first, and all subsequent messages with that `tab_id` route to the wrong pane.

### Reproduction

1. Open two panes with different profiles (e.g., "default" and "work").
2. Navigate to different URLs in each.
3. Navigate in pane 1 → pane 2 visually shows pane 1's page (the "clone").
4. Refresh pane 2 → it shows the correct page again.

### Why refreshing fixes it

Refreshing the cloned pane causes its browser to re-render and send a new
`CaContext` message. This `CaContext` carries the same colliding `tab_id`, which
the GUI routes to pane 2 (the overwritten mapping). Pane 2's CALayerHost is
swapped to the correct browser's rendering context. The fix persists until the
next navigation in pane 1 triggers another `CaContext` from browser A, which
also routes to pane 2 and clones it again.

## Analysis

### The collision

The `tab_to_pane` key is a bare `tab_id` (`i64`). But `tab_id` is only unique
within a single browser process. To be globally unique, the key must include the
profile/browser pair that identifies which process the tab belongs to.

The server key `"{profile}\0{browser}"` (from `TermSurfState::server_key()`)
already uniquely identifies a browser process. Combining this with `tab_id`
creates a globally unique key.

### All code sites that use `tab_to_pane`

**Inserts (1 site):**

| File      | Line | Code                                    | Purpose                  |
| --------- | ---- | --------------------------------------- | ------------------------ |
| `conn.rs` | 731  | `tab_to_pane.insert(ready.tab_id, ...)` | Register tab on TabReady |

**Lookups (4 sites):**

| File      | Line | Code                                  | Purpose                       |
| --------- | ---- | ------------------------------------- | ----------------------------- |
| `conn.rs` | 238  | `tab_to_pane.get(&c.tab_id)`          | Route CursorChanged to pane   |
| `conn.rs` | 323  | `tab_to_pane.get(&resolved_tab_id)`   | DevTools: find inspected pane |
| `conn.rs` | 1200 | `tab_to_pane.get(&ca_context.tab_id)` | Route CaContext to pane       |
| `conn.rs` | 882  | `tab_to_pane.remove(&pane.tab_id)`    | Clean up on disconnect        |

**Declaration:**

| File       | Line | Code                                    |
| ---------- | ---- | --------------------------------------- |
| `state.rs` | 52   | `pub tab_to_pane: HashMap<i64, String>` |

### The fix

Change the `tab_to_pane` key from bare `tab_id` to `(server_key, tab_id)`. Each
pane already stores `profile` and `browser`, so the server key is available at
every insert and lookup site.

**`state.rs`:** Change the type:

```rust
// Before:
pub tab_to_pane: HashMap<i64, String>,

// After:
pub tab_to_pane: HashMap<(String, i64), String>,
```

**`conn.rs` line 731 (insert):** The pane's profile and browser are available
from the pane struct (already looked up on line 729-730):

```rust
let key = TermSurfState::server_key(&pane.profile, &pane.browser);
st.tab_to_pane.insert((key, ready.tab_id), ready.pane_id.clone());
```

**`conn.rs` lines 238, 1200 (lookups from browser messages):** These messages
arrive from a browser socket. The reader loop for each browser connection knows
which server_key it belongs to. Thread the server_key through so lookups use
`(server_key, tab_id)`.

**`conn.rs` line 323 (DevTools lookup):** The inspected tab's profile/browser is
known from the requesting pane. Use those to build the composite key.

**`conn.rs` line 882 (remove on disconnect):** The pane being removed has
`profile` and `browser` fields. Build the composite key for removal.

### Message routing context

The challenge is that messages from browser sockets (CaContext, CursorChanged,
UrlChanged, TitleChanged, LoadingState) arrive via a reader loop that currently
doesn't track which server_key the socket belongs to. The `handle_message`
function receives the raw message without knowing which browser sent it.

The fix needs to either:

1. **Thread the server_key through the reader loop** — when a browser connection
   is established, associate the socket with its server_key. Pass the server_key
   to `handle_message` so it can build composite keys for lookups.

2. **Or store the server_key on the Pane struct and look it up from tab_id** —
   but this has the same collision problem we're trying to fix.

Approach 1 is correct. The browser reader loop already knows which server it
belongs to (the connection is established per-server). Adding a `server_key`
parameter to the reader and `handle_message` is the clean solution.

## Experiments

### Experiment 1: Use composite (server_key, tab_id) key

Thread the server_key through the connection reader loop and use a composite
`(String, i64)` key for `tab_to_pane` so tab IDs are scoped per browser process.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/state.rs`**

1. Change `tab_to_pane` type from `HashMap<i64, String>` to
   `HashMap<(String, i64), String>`.

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

2. Add a `server_key: Option<String>` field to the connection reader's local
   state (alongside `conn_type`). Initialize to `None`.

3. In `handle_server_register` (line 681): return the matched server_key so the
   caller can store it. Change the return type to
   `anyhow::Result<Option<String>>` and return `Some(key)` on match.

4. In the `handle_connection` reader loop (line 96): after `handle_message`
   returns for a `ServerRegister`, capture the server_key. Restructure so
   `ServerRegister` is handled in the loop body directly (calling
   `handle_server_register` and storing the returned key), then all other
   messages go through `handle_message`.

5. Add `server_key: &Option<String>` parameter to `handle_message` (line 136).
   Pass it from the connection loop.

6. **Insert (line 731):** In `handle_tab_ready`, the pane already has `profile`
   and `browser` fields. Build the composite key:
   ```rust
   let key = TermSurfState::server_key(&pane.profile, &pane.browser);
   st.tab_to_pane.insert((key, ready.tab_id), ready.pane_id.clone());
   ```
   `handle_tab_ready` doesn't need `server_key` from the connection — the pane
   struct already has the profile/browser.

7. **CaContext lookup (line 1200):** `handle_ca_context` receives a bare
   `tab_id`. Pass `server_key` to it. Use it to build the composite key:
   ```rust
   fn handle_ca_context(ca_context: proto::CaContext, server_key: &str, state: &SharedState) {
       let key = (server_key.to_string(), ca_context.tab_id);
       let Some(pane_id) = st.tab_to_pane.get(&key).cloned() else { ... };
   ```

8. **CursorChanged lookup (line 238):** Same pattern — use `server_key` from the
   connection to build the composite key for the lookup.

9. **DevTools lookup (line 323):** The `QueryDevtoolsRequest` comes from a TUI,
   not a browser, so `server_key` is `None`. Instead, look up the requesting
   pane's profile/browser to build the key:
   ```rust
   let inspected_key = TermSurfState::server_key(&pane.profile, &pane.browser);
   st.tab_to_pane.get(&(inspected_key, resolved_tab_id))
   ```

10. **Remove on disconnect (line 882):** The pane being removed has `profile`
    and `browser`. Build the composite key for removal:
    ```rust
    let key = TermSurfState::server_key(&pane.profile, &pane.browser);
    st.tab_to_pane.remove(&(key, pane.tab_id));
    ```

11. Update all log lines that print `tab_to_pane` counts (lines 766-769,
    854-858, 916-919) — no functional change, just ensure they still compile.

#### Verification

1. **Two profiles, no cloning:**
   - Open two panes with different profiles.
   - Navigate in pane 1.
   - **Pass:** Pane 2 continues showing its own page. No cloning.

2. **Two profiles, independent navigation:**
   - Open two panes with different profiles.
   - Navigate in both panes independently.
   - **Pass:** Each pane shows its own page throughout.

3. **Single profile (regression):**
   - Open one pane with one profile.
   - Navigate, refresh, open DevTools.
   - **Pass:** Everything works as before.

4. **DevTools with two profiles:**
   - Open two panes with different profiles.
   - Open DevTools on one pane.
   - **Pass:** DevTools opens for the correct pane, not the other.

5. **Close and reopen:**
   - Open two profiles, close one, reopen it.
   - **Pass:** No stale mappings. The new pane works correctly.

**Result:** Fail

No browser loads at all — complete breakage.

#### Failure analysis

The most likely cause is that `ServerRegister` is now handled in the connection
loop body **before** `handle_message`, but the `ServerRegister` match in
`handle_message` was removed. This means `handle_server_register` runs correctly
and returns the `server_key`. However, the problem is likely in how
`handle_message` receives the `server_key`.

The connection loop has one socket per connection. A TUI and a browser engine
are separate connections. The `server_key` is set when a `ServerRegister`
message arrives on a browser connection. But `TabReady` also arrives on that
same browser connection — so the `server_key` should be available.

The real issue is probably that `handle_tab_ready` does NOT receive the
connection's `server_key`. It builds its own key from `pane.profile` and
`pane.browser`. But `TabReady` arrives on the browser's socket, and
`handle_tab_ready` looks up `st.panes[ready.pane_id]` to get profile/browser. If
the pane exists and has the correct profile/browser, the insert should work.

However, `handle_ca_context` receives `server_key` from the connection. If
`server_key` is `None` at the time `CaContext` arrives (e.g., because the
`ServerRegister` → `handle_server_register` call didn't return the key
correctly, or because the `matches!` guard consumed the message before the inner
destructure could extract it), then the lookup key would be `("", tab_id)` which
would never match the insert key `("profile\0browser", tab_id)`.

The most suspicious code is the double-match pattern:

```rust
if matches!(&msg.msg, Some(Msg::ServerRegister(_))) {
    if let Some(Msg::ServerRegister(r)) = msg.msg {
```

The outer `matches!` borrows `msg.msg`, then the inner `if let` moves it. This
should work in Rust (the borrow from `matches!` is temporary), but if the
compiler optimizes differently or if `msg.msg` is `None` after the borrow, the
inner match would fail silently — `server_key` would stay `None`, and all
subsequent lookups on this connection would use `("", tab_id)`.

#### Conclusion

The approach is correct but the implementation has a bug, likely in how
`server_key` is captured from `ServerRegister`. Experiment 2 should simplify the
pattern to avoid the double-match issue.

### Experiment 2: Add debug logs to diagnose the failure

Code analysis cannot determine why the browser fails to load. The logic traces
correctly on paper, but something fails at runtime. Add targeted debug logs at
every decision point in the composite-key flow to identify exactly which step
breaks.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

1. After `handle_server_register` returns in the connection loop (line 103):
   ```rust
   log::info!("ServerRegister: captured server_key={:?}", server_key);
   ```

2. In `handle_tab_ready`, after inserting the composite key (line 773):
   ```rust
   log::info!(
       "TabReady: inserted tab_to_pane key=({:?}, {}) -> pane_id={}",
       skey, ready.tab_id, ready.pane_id
   );
   ```

3. In `handle_ca_context`, before the lookup (line 1248):
   ```rust
   log::info!(
       "handle_ca_context: looking up key=({:?}, {}), tab_to_pane has {} entries: {:?}",
       skey, ca_context.tab_id, st.tab_to_pane.len(),
       st.tab_to_pane.keys().collect::<Vec<_>>()
   );
   ```

4. In `handle_message` for CursorChanged, before the lookup (line 238):
   ```rust
   log::info!(
       "CursorChanged: server_key={:?} tab_id={}",
       server_key, c.tab_id
   );
   ```

#### Verification

1. Build Wezboard, launch it, run `web ryanxcharles.com`.
2. Check the log output (stdout or log file).
3. Look for:
   - `ServerRegister: captured server_key=Some(...)` — confirms key was set.
     If `None`, the `handle_server_register` matching failed.
   - `TabReady: inserted tab_to_pane key=(...)` — confirms the insert happened
     and what key was used.
   - `handle_ca_context: looking up key=(...)` — confirms what key is used for
     lookup and whether the tab_to_pane map contains a matching entry.
4. The mismatch between insert and lookup keys (if any) will reveal the bug.

**Result:** Fail

No debug logs appeared because the browser process never connects to the GUI.
Roamium is spawned (PID visible in logs) but immediately exits. The composite
key changes broke something fundamental that prevents the browser from loading
at all, though the exact mechanism is unclear from code analysis.

#### Conclusion

Experiments 1 and 2 must be fully reverted before any further work on this
issue. The composite key approach needs to be redesigned from scratch.

### Experiment 3: Revert experiments 1 and 2

Restore `conn.rs` and `state.rs` to their exact state before experiment 1 to
recover a working browser.

#### Changes

One command:
```
git checkout 3aeb2b2 -- wezboard/wezboard-gui/src/termsurf/conn.rs wezboard/wezboard-gui/src/termsurf/state.rs
```

This restores both files to commit `3aeb2b2` (the last commit before experiment
1 was implemented).

#### Verification

1. Build Wezboard: `scripts/build.sh wezboard`
2. Launch Wezboard, run `web ryanxcharles.com`.
3. **Pass:** The browser loads and displays the page.

**Result:** Fail

Browser still does not load after reverting. The failure is not caused by the
composite key changes — it is a separate, pre-existing issue.

## Conclusion

The tab_id collision bug (the original issue) remains unfixed. The composite key
approach in experiment 1 was correct in principle but the implementation broke
the browser entirely. Reverting did not restore a working browser, indicating
the root cause of the current failure is unrelated to the changes made in this
issue. A new issue should be opened to diagnose why the browser process fails
to connect to the GUI.
