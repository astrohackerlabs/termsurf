+++
status = "closed"
opened = "2026-04-05"
closed = "2026-04-05"
+++

# Issue 771: Tab ID collision across browser profiles

## Goal

Fix the bug where having two browser profiles open simultaneously causes one
pane to visually "clone" the other when navigating.

## Background

This is the same bug as Issue 769, reopened now that the browser is working
again (Issue 770 was a macOS SDK mismatch unrelated to the tab_id fix).

Issue 769's experiment 1 had the correct approach (composite key) but failed
because it was tested while the browser was broken due to the macOS 26.4 sandbox
issue. The experiments were reverted and the issue was closed. Now that Chromium
has been rebuilt against 26.4, we can attempt the fix again.

### The bug

Each browser profile runs as a separate Roamium process. Chromium assigns each
tab a `tab_id` — a per-process integer. Two separate Chromium processes
independently generate the same `tab_id` values.

The GUI maintains `tab_to_pane: HashMap<i64, String>` mapping `tab_id` →
`pane_id`. When two profiles produce the same `tab_id`, the second insert
overwrites the first. All subsequent `CaContext` messages with that `tab_id`
route to the wrong pane, causing one profile to visually display the other's
content.

Refreshing fixes it temporarily because the correct browser re-renders and sends
a new `CaContext` that gets routed to the right pane.

### Reproduction

1. Open two panes with different profiles (e.g., "default" and "work").
2. Navigate to different URLs in each.
3. Navigate in pane 1 → pane 2 visually shows pane 1's page.
4. Refresh pane 2 → correct page returns until next navigation in pane 1.

## Analysis

### The fix

Change `tab_to_pane` from `HashMap<i64, String>` to
`HashMap<(String, i64), String>` where the `String` is the server key
(`"{profile}\0{browser}"`). Every pane stores `profile` and `browser`, so the
key is available at every site.

### All code sites

**Declaration** (`state.rs:52`):

```rust
pub tab_to_pane: HashMap<i64, String>,
// → HashMap<(String, i64), String>,
```

**Insert** (`conn.rs` `handle_tab_ready`):

```rust
st.tab_to_pane.insert(ready.tab_id, ready.pane_id.clone());
// → use pane.profile + pane.browser to build composite key
```

**Lookups** (`conn.rs`):

- `handle_ca_context` — routes CaContext to pane
- CursorChanged handler — routes cursor type to pane
- DevTools lookup — finds inspected pane

**Remove** (`conn.rs` `handle_disconnect`):

```rust
st.tab_to_pane.remove(&pane.tab_id);
// → use pane.profile + pane.browser to build composite key
```

### How to thread the server_key

Messages from browser sockets (CaContext, CursorChanged) need the server_key to
build the composite lookup key. The connection reader loop must track which
server this connection belongs to.

**Approach:** When `handle_server_register` runs, it matches a server by
profile. Store the matched server_key on the connection. Pass it to
`handle_message` so browser-originated lookups can use it.

### Lessons from Issue 769

Issue 769 experiment 1 failed for two reasons:

1. The implementation intercepted `ServerRegister` in the connection loop with a
   double-match pattern (`matches!` then `if let`) and removed it from
   `handle_message`. This restructuring may have introduced a subtle bug.

2. The failure could not be diagnosed because the browser was simultaneously
   broken by the macOS 26.4 sandbox issue (Issue 770). All testing showed
   "browser doesn't load" which was blamed on the code changes but was actually
   the OS.

For this attempt: keep `ServerRegister` inside `handle_message` (don't
restructure the message loop). Instead, have `handle_server_register` store the
server_key in shared state keyed by the connection's `tx` channel, so other
handlers can look it up without needing a parameter threaded through the loop.

Alternatively, since `handle_tab_ready` already builds the key from the pane
(not the connection), and `handle_ca_context` / CursorChanged receive messages
from a browser whose `tab_id` is already in `tab_to_pane` — we could do a
**reverse lookup**: for browser-originated messages, iterate `tab_to_pane` to
find any entry matching the `tab_id` on the connection's server. But this
defeats the purpose of the HashMap.

The simplest correct approach: add a `server_key: Option<String>` field to the
connection state, set it when `ServerRegister` is processed, and pass it to
`handle_message`. This is what 769 tried. The key difference: don't remove
`ServerRegister` from `handle_message`'s match — just add a side effect that
also stores the key on the connection.

## Experiments

### Experiment 1: Composite key with server_key on connection

Use a composite `(String, i64)` key for `tab_to_pane`. Thread the server_key
through the connection by having `handle_server_register` return it, then pass
it to `handle_message`. Keep `ServerRegister` inside `handle_message` — don't
restructure the message loop.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/state.rs`**

1. Change `tab_to_pane` type from `HashMap<i64, String>` to
   `HashMap<(String, i64), String>`.

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

2. Add `server_key: Option<String>` local var in `handle_connection` (line 46,
   alongside `conn_type`). Initialize to `None`.

3. Change `handle_server_register` return type from `anyhow::Result<()>` to
   `anyhow::Result<Option<String>>`. Return `Some(key.clone())` on match,
   `Ok(None)` on no match. Keep the function body otherwise identical.

4. In `handle_connection` (line 96): after `handle_message` returns, check if
   `server_key` is still `None` and the message was a `ServerRegister`. But
   since `handle_message` consumes the message, we can't check it afterward.
   Instead, change `handle_message` to return the server_key when it processes a
   `ServerRegister`:

   Change `handle_message` return type from `anyhow::Result<()>` to
   `anyhow::Result<Option<String>>`. Return `Ok(None)` from every arm except
   `ServerRegister`, which returns the key from `handle_server_register`.

   In the connection loop, capture the return:
   ```rust
   match handle_message(msg, &stream, &tx, &server_key, &state).await {
       Ok(Some(key)) => server_key = Some(key),
       Ok(None) => {}
       Err(err) => log::error!("TermSurf handle error: {:#}", err),
   }
   ```

5. Add `server_key: &Option<String>` parameter to `handle_message` (line 136).
   The `ServerRegister` arm still calls `handle_server_register` as before, but
   now returns its key. All other arms return `Ok(None)`.

6. **Insert** (`handle_tab_ready`, line 731): Build composite key from the
   pane's profile/browser (already available from `st.panes`):
   ```rust
   let pane = st.panes.get(&ready.pane_id).unwrap();
   let skey = TermSurfState::server_key(&pane.profile, &pane.browser);
   st.tab_to_pane.insert((skey, ready.tab_id), ready.pane_id.clone());
   ```

7. **CaContext lookup** (line 228): Pass `server_key` to `handle_ca_context`.
   Inside, build composite key:
   ```rust
   let skey = server_key.as_deref().unwrap_or("");
   let lookup = (skey.to_string(), ca_context.tab_id);
   st.tab_to_pane.get(&lookup)
   ```

8. **CursorChanged lookup** (line 238): Same pattern — use `server_key` from the
   connection to build composite key.

9. **DevTools lookup** (line 323): This comes from a TUI connection where
   `server_key` is `None`. Use the resolved pane's profile/browser instead:
   ```rust
   let resolved_pane = st.panes.values().find(|p| p.tab_id == resolved_tab_id);
   if let Some(rp) = resolved_pane {
       let skey = TermSurfState::server_key(&rp.profile, &rp.browser);
       st.tab_to_pane.get(&(skey, resolved_tab_id))
   }
   ```

10. **Remove on disconnect** (line 882): Build composite key from the pane being
    removed:
    ```rust
    let skey = TermSurfState::server_key(&pane.profile, &pane.browser);
    st.tab_to_pane.remove(&(skey, pane.tab_id));
    ```

#### Key difference from Issue 769

- `ServerRegister` stays inside `handle_message`'s match. No restructuring of
  the message loop.
- The server_key flows out via `handle_message`'s return value, not by
  intercepting the message before `handle_message`.
- `handle_server_register` body is minimally changed (just the return type and
  value).

#### Verification

1. **Single profile works:**
   - Open one pane, `web ryanxcharles.com`.
   - **Pass:** Browser loads and displays the page.

2. **Two profiles, no cloning:**
   - Open two panes with different profiles.
   - Navigate in pane 1.
   - **Pass:** Pane 2 continues showing its own page.

3. **Two profiles, independent navigation:**
   - Navigate in both panes independently.
   - **Pass:** Each pane shows its own page throughout.

4. **Single profile regression:**
   - Navigate, refresh, open DevTools.
   - **Pass:** Everything works as before.

5. **Close and reopen:**
   - Open two profiles, close one, reopen it.
   - **Pass:** No stale mappings.

**Result:** Pass

Two profiles work independently with no cloning.

## Conclusion

The composite `(server_key, tab_id)` key fixes the tab ID collision. The key
difference from Issue 769's failed attempt: `ServerRegister` stays inside
`handle_message`'s match — the server_key flows out via the return value instead
of restructuring the connection loop. Issue 769 failed due to a combination of
the loop restructuring and a coincident macOS SDK mismatch (Issue 770).
