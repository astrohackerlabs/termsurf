+++
status = "open"
opened = "2026-04-05"
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
