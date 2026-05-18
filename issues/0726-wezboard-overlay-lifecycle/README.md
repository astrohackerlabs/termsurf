+++
status = "closed"
opened = "2026-03-08"
closed = "2026-03-08"
+++

# Issue 726: Wezboard overlay lifecycle and remaining protocol

## Goal

Make the browser overlay respond correctly to window and pane changes (resize,
splits, tab switching), then handle all remaining TermSurf protocol messages to
complete the Wezboard PoC.

## Background

Issue 725 solved overlay rendering: browser content is visible in the Wezboard
terminal window with correct size and position. But the overlay is static — it
doesn't respond to window resizes, split pane changes, or tab switches. And
Wezboard only handles 11 of 30 TermSurf protocol messages.

### Overlay lifecycle issues

Four overlay behaviors need to work:

1. **Window resize** — When the window resizes, the webview should resize with
   it. The metrics bridge (`metrics.rs`) updates on resize, but `conn.rs`
   doesn't re-read the metrics or call `update_ca_layer_frame()`. There's no
   notification path from TermWindow to the connection code.
2. **Split pane resize** — When opening or closing a split pane, the terminal
   pane shrinks or grows. The webview should resize to match the new pane
   dimensions.
3. **Tab switch away** — When opening a new tab or switching to a tab without a
   webview, the overlay should hide.
4. **Tab switch back** — When navigating back to a tab with an active webview,
   the overlay should reappear.

### Remaining protocol messages

Wezboard currently handles 11 of 30 TermSurf protocol messages. The remaining 19
fall into four categories:

**Input forwarding (4 messages):**

- `KeyEvent` — Keyboard input to browser
- `MouseEvent` — Mouse clicks to browser
- `MouseMove` — Mouse movement to browser
- `ScrollEvent` — Scroll wheel to browser

Without input forwarding, the browser overlay is view-only. This is the most
important missing piece after overlay lifecycle.

**Tab queries (6 messages):**

- `QueryLastRequest` / `QueryLastReply` — Get last active tab for session
  restore
- `QueryDevtoolsRequest` / `QueryDevtoolsReply` — Get DevTools tab
- `QueryTabsRequest` / `QueryTabsReply` — Get all tabs for a profile

**DevTools (2 messages):**

- `CreateDevtoolsTab` — Create DevTools tab
- `SetDevtoolsOverlay` — Create/resize DevTools overlay

**Other (3 messages):**

- `FocusChanged` — Tab focus state
- `CursorChanged` — Browser cursor type updates
- `OpenSplit` — Open split pane

**Already handled (11 messages):**

- `HelloRequest` / `HelloReply` — Handshake
- `ServerRegister` — Chromium process registration
- `SetOverlay` — Create/resize browser overlay
- `CreateTab` (sent, not received) — Create tab in browser
- `TabReady` — Tab initialized
- `CaContext` — CALayerHost context for compositing
- `Navigate` — URL navigation forwarding
- `UrlChanged` / `LoadingState` / `TitleChanged` — State forwarding to TUI
- `SetColorScheme` — Dark/light mode
- `ModeChanged` — Browse/edit mode toggle

### Priority order

1. Overlay lifecycle (this issue's primary focus)
2. Input forwarding (makes the browser usable)
3. Tab queries (session restore, DevTools discovery)
4. Auxiliary features (focus, cursor, DevTools, splits)

## Proposed solutions

### Overlay lifecycle

For **resize**, TermWindow could send a notification through the TermSurf shared
state or a channel whenever dimensions change. Alternatively, `conn.rs` could
poll the metrics atomics periodically — but that's wasteful. A better approach:
when `SetOverlay` arrives with updated dimensions, re-read metrics and update
the CALayer frame.

For **tab switching**, the overlay NSView or its sublayers need to be
shown/hidden based on which tab is active. The mux (WezTerm's tab/pane manager)
knows which pane is focused. When the focused pane changes, the board needs to
hide overlays for inactive panes and show overlays for the active pane.

### Input forwarding

The TUI already captures keyboard and mouse events and sends them as protobuf
messages. The board needs to receive these messages and forward them to the
correct Chromium process based on pane-to-tab mapping.

## Experiments

### Experiment 1: Hide overlay on tab switch

#### Background

ts3 (Issue 310) hit this exact bug: browser overlay from Tab A leaked into Tab
B. ts3 rendered browser content as IOSurface textures, so the fix was filtering
at render time — skip drawing overlays whose `tab_id != active_tab_id`. Wezboard
uses CALayerHost (zero-copy GPU compositing), so there's no render loop to
filter. Instead we toggle the `hidden` property on each pane's
`ca_layer_flipped` (the per-pane root in the overlay layer tree).

Ghostboard removes/adds layers on focus change, but it owns the surface
lifecycle in Zig. In Wezboard, the overlay code runs in async connection tasks
(`conn.rs`) on a different thread from TermWindow. Currently the only bridge is
`metrics.rs` global atomics — a write-only path from TermWindow to conn.rs.

To toggle layer visibility we need TermWindow to access the TermSurf shared
state (which holds all pane CALayer pointers). Currently `SharedState` is
created in `main.rs` and passed to the listener, but TermWindow has no access to
it. We fix this by making `SharedState` globally accessible via `OnceLock` — the
same global pattern as `metrics.rs` but for the full state. This also simplifies
the existing code (listener.rs and conn.rs can read the global instead of
threading state through function arguments).

Every tab switch in WezTerm flows through `Window::set_active_without_saving()`,
which fires `MuxNotification::WindowInvalidated`. TermWindow already handles
this at `mod.rs:1298`. We add a `sync_overlay_visibility()` call there that
reads the active mux pane ID and toggles `setHidden:` on each pane's
`ca_layer_flipped`.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/state.rs`** — Make SharedState globally
accessible:

Add a `OnceLock` global and accessor functions:

```rust
use std::sync::OnceLock;

static GLOBAL_STATE: OnceLock<SharedState> = OnceLock::new();

pub fn init_global(state: SharedState) {
    GLOBAL_STATE.set(state).ok();
}

pub fn global() -> Option<&'static SharedState> {
    GLOBAL_STATE.get()
}
```

**`wezboard/wezboard-gui/src/termsurf/mod.rs`** — Re-export the global accessor:

```rust
pub use state::global as shared_state;
```

**`wezboard/wezboard-gui/src/main.rs`** — Initialize the global after creating
state (line 429):

```rust
let termsurf_state = Arc::new(std::sync::Mutex::new(termsurf::state::TermSurfState::new()));
termsurf::state::init_global(termsurf_state.clone());
```

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Add
`sync_overlay_visibility`:

The function takes a `HashSet<String>` of all active pane IDs across all
windows. A pane is shown if its ID is in the set, hidden otherwise. This
correctly handles multiple windows — each window contributes its active pane to
the set.

```rust
use std::collections::HashSet;

#[cfg(target_os = "macos")]
pub fn sync_overlay_visibility(active_pane_ids: &HashSet<String>) {
    let Some(state) = super::shared_state() else {
        return;
    };
    let st = state.lock().unwrap();
    for (pane_id, pane) in &st.panes {
        if pane.ca_layer_flipped == 0 {
            continue;
        }
        let is_active = active_pane_ids.contains(pane_id);
        unsafe {
            use objc2::msg_send;
            use objc2::runtime::Bool;
            let layer = pane.ca_layer_flipped as *mut objc2::runtime::AnyObject;
            let hidden = if is_active { Bool::NO } else { Bool::YES };
            let _: () = msg_send![layer, setHidden: hidden];
        }
    }
}
```

**`wezboard/wezboard-gui/src/termwindow/mod.rs`** — Call sync on
`WindowInvalidated` (line 1298):

On every `WindowInvalidated`, iterate all mux windows and collect each window's
active pane ID into a `HashSet`. The TermSurf `pane_id` is the mux pane ID as a
string (WezTerm sets `WEZBOARD_PANE` env var → TUI reads it → sends as
`HelloRequest.pane_id`).

```rust
MuxNotification::WindowInvalidated(_) => {
    window.invalidate();
    self.update_title_post_status();

    // Gather active pane IDs across all windows
    let mux = Mux::get();
    let mut active_ids = std::collections::HashSet::new();
    for window_id in mux.iter_windows() {
        if let Some(w) = mux.get_window(window_id) {
            if let Some(tab) = w.get_active() {
                if let Some(pane) = tab.get_active_pane() {
                    active_ids.insert(pane.pane_id().to_string());
                }
            }
        }
    }
    crate::termsurf::conn::sync_overlay_visibility(&active_ids);
}
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com` in the first tab
3. Open a new tab (Cmd+T)
4. **Expected:** browser overlay disappears — new tab shows only terminal
5. Switch back to the first tab
6. **Expected:** browser overlay reappears
7. Open a third tab with another `web` instance
8. Switch between all three tabs — each shows only its own overlay (or no
   overlay)
9. Open a second window with a webview — both windows' overlays visible
   simultaneously

**Result:** Fail

Switching to a new tab correctly hides the overlay. But switching back to the
tab with the webview does not restore it — the overlay stays hidden.

The hide works because `sync_overlay_visibility` sets `setHidden:YES` on every
pane whose `pane_id` is not in the active set. The show fails because
`active_pane_ids.contains(pane_id)` returns false even when the tab is active.

The most likely cause is a **pane_id mismatch**. The TermSurf state stores panes
keyed by the string the TUI sends in `HelloRequest.pane_id`. The
`WindowInvalidated` handler builds the active set from
`pane.pane_id().to_string()` (the mux's `PaneId` as a string). If these two
strings don't match, the pane is never recognized as active.

In Ghostboard, `TERMSURF_PANE_ID` is set to the surface's pane ID. In Wezboard,
`WEZBOARD_PANE` is set to the mux pane ID at `mux/src/domain.rs:482` — but the
TUI reads `TERMSURF_PANE_ID`, not `WEZBOARD_PANE`. If `TERMSURF_PANE_ID` isn't
set, the TUI may send a different value (or empty string) that doesn't match the
mux pane ID.

#### Conclusion

Research confirmed the root cause: Wezboard never sets `TERMSURF_PANE_ID`. The
TUI reads this env var to get its pane identity — without it, either the TUI
doesn't connect at all, or it sends a pane_id that doesn't match the mux pane
ID. Either way, `sync_overlay_visibility` can never match the TermSurf pane keys
against the mux active pane set.

### Experiment 2: Set TERMSURF_PANE_ID in Wezboard

#### Background

Ghostboard sets `TERMSURF_PANE_ID` at `Surface.zig:662` when spawning child
processes. The TUI reads it at `webtui/src/main.rs:223` to identify itself to
the board. Without this env var, the TUI either cannot connect or sends an
unrecognized pane_id.

Wezboard already sets `WEZBOARD_PANE` to the mux pane ID at
`mux/src/domain.rs:482`. We just need to also set `TERMSURF_PANE_ID` to the same
value. This ensures:

1. The TUI connects and sends `HelloRequest.pane_id` matching the mux pane ID
2. The TermSurf state stores panes keyed by the mux pane ID string
3. `sync_overlay_visibility` can match these keys against the active pane set

**Hypothesis:** This single-line fix will make Experiment 1's tab switching
logic work — overlays will hide on tab switch away and reappear on switch back.

#### Changes

**`wezboard/mux/src/domain.rs`** — Add `TERMSURF_PANE_ID` after line 482:

```rust
cmd.env("WEZBOARD_PANE", pane_id.to_string());
cmd.env("TERMSURF_PANE_ID", pane_id.to_string());
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com` in the first tab
3. Open a new tab (Cmd+T)
4. **Expected:** browser overlay disappears
5. Switch back to the first tab
6. **Expected:** browser overlay reappears

**Result:** Pass

Overlays hide on tab switch away and reappear on switch back. The missing
`TERMSURF_PANE_ID` env var was the root cause of Experiment 1's failure —
without it, the TUI couldn't identify itself to the board, so the TermSurf pane
keys never matched the mux active pane set.

#### Conclusion

Setting `TERMSURF_PANE_ID` alongside `WEZBOARD_PANE` in `domain.rs` completes
the pane identity bridge between WezTerm's mux and the TermSurf protocol.
Combined with Experiment 1's `sync_overlay_visibility` (called from the
`WindowInvalidated` handler), tab switching now correctly hides and shows
browser overlays.

### Experiment 3: Fix overlay visibility for splits and multiple windows

#### Background

Experiments 1–2 solved tab switching: overlays hide when switching to a tab
without a webview and reappear when switching back. But opening a second pane
(split or new window) with a webview exposes a bug — the second overlay flashes
briefly then disappears, while the first remains visible.

The root cause is in the `WindowInvalidated` handler at `mod.rs:1298`. It builds
the active pane set using `tab.get_active_pane()`, which returns only the single
**focused** pane per tab. In a split layout, both panes are visible on screen,
but only one is focused. The non-focused pane's overlay gets hidden by
`sync_overlay_visibility`.

The flash occurs because:

1. The second TUI connects and sends `SetOverlay` → CALayer created → overlay
   visible
2. `WindowInvalidated` fires (triggered by the new pane or focus change)
3. `sync_overlay_visibility` runs — the focused pane is still the first one, so
   the second pane's overlay is hidden

The fix is to collect **all panes in each window's active tab**, not just the
focused pane. WezTerm's `Tab::iter_panes()` returns a `Vec<PositionedPane>` with
every pane in the tab's split tree. Each `PositionedPane` has a `.pane` field
(`Arc<dyn Pane>`) with a `.pane_id()` method.

This correctly handles all cases:

- **Single pane**: one pane in the active tab → its overlay is visible
- **Split panes**: all panes in the active tab → all overlays visible
- **Tab switch**: panes on inactive tabs are not iterated → their overlays
  hidden
- **Multiple windows**: each window contributes all panes from its active tab

#### Changes

**`wezboard/wezboard-gui/src/termwindow/mod.rs`** — In the `WindowInvalidated`
handler (~line 1298), replace `get_active_pane()` with `iter_panes()`:

```rust
// Before (only the focused pane):
if let Some(tab) = w.get_active() {
    if let Some(pane) = tab.get_active_pane() {
        active_ids.insert(pane.pane_id().to_string());
    }
}

// After (all panes in the active tab):
if let Some(tab) = w.get_active() {
    for positioned in tab.iter_panes() {
        active_ids.insert(positioned.pane.pane_id().to_string());
    }
}
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com` in the first pane
3. Open a split pane, run `web google.com` again
4. **Expected:** both overlays visible simultaneously
5. Switch to a new tab (Cmd+T)
6. **Expected:** both overlays disappear
7. Switch back
8. **Expected:** both overlays reappear
9. Open a second window, run `web google.com`
10. **Expected:** all three overlays visible across both windows

**Result:** Fail

The second pane's webview does not appear. The first webview remains visible
after a brief flash. Behavior is identical to before the change — replacing
`get_active_pane()` with `iter_panes()` had no effect.

This means the problem is not about which pane IDs are in the active set. The
second pane IS in the set (since `iter_panes()` returns all panes in the active
tab), yet its overlay still disappears. The root cause must be elsewhere —
likely in the overlay creation or CALayer setup path for the second pane, not in
the visibility sync logic.

#### Conclusion

The `get_active_pane()` vs `iter_panes()` distinction was not the cause. The
second overlay flashes (briefly appears then vanishes) regardless of which pane
IDs are in the active set. The bug is upstream of `sync_overlay_visibility` —
something in the overlay creation, CALayer tree setup, or the interaction
between two concurrent pane connections is preventing the second overlay from
persisting.

### Experiment 4: Debug logs for second pane overlay lifecycle

#### Background

Experiment 3 proved the bug is not in `sync_overlay_visibility`. The second
overlay flashes (appears briefly then vanishes) regardless of which pane IDs are
in the active set. Static analysis of the code shows the overlay creation path
_should_ work for multiple panes — each gets its own 3-layer hierarchy under a
shared root layer. But something is wrong at runtime.

Three hypotheses remain:

1. **The second `CaContext` never arrives.** If `TabReady` for the second tab is
   delayed or lost, `handle_ca_context` logs `"unknown tab_id"` and skips layer
   creation. The "flash" could be the first overlay flickering during a
   `WindowInvalidated` redraw, not the second overlay appearing at all.

2. **The second `SetOverlay` hits the resize path instead of the new-pane
   path.** If somehow the second pane's `pane_id` collides with an existing key
   in `state.panes`, line 248 (`!st.panes.contains_key`) returns false and it
   takes the resize-only path, never creating a tab.

3. **`sync_overlay_visibility` runs between layer creation and the
   `CATransaction` commit.** The second pane's `ca_layer_flipped` is set at line
   734 but the transaction doesn't commit until line 764. If `WindowInvalidated`
   fires on the main thread during this window, sync could set `hidden: YES` on
   the new layer before it becomes visible.

This experiment adds targeted debug logs at every critical point in the second
pane's lifecycle to determine which hypothesis (or what other cause) is correct.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Add logs at these points:

1. **`handle_set_overlay` entry** (~line 229): Log whether the pane is new or
   existing, and the pane_id:

   ```rust
   log::info!(
       "SetOverlay: pane_id={} is_new={} pixel={}x{}",
       overlay.pane_id, is_new, pixel_w, pixel_h
   );
   ```

2. **`handle_tab_ready`** (~line 378): Log the tab_to_pane mapping:

   ```rust
   log::info!(
       "TabReady: tab_id={} pane_id={} tab_to_pane_count={}",
       ready.tab_id, ready.pane_id, st.tab_to_pane.len()
   );
   ```

3. **`handle_ca_context` pane lookup** (~line 672): Log whether the pane was
   found and its current layer state:

   ```rust
   log::info!(
       "handle_ca_context: tab_id={} pane_id={} has_layers={}",
       ca_context.tab_id, pane_id, pane.ca_layer_host != 0
   );
   ```

4. **`handle_ca_context` after layer creation** (~line 738): Log the layer
   pointers:

   ```rust
   log::info!(
       "CALayerHost created: pane_id={} flipped={:#x} host={:#x}",
       pane_id, pane.ca_layer_flipped, pane.ca_layer_host
   );
   ```

5. **`sync_overlay_visibility`** (~line 818): Log what's being shown/hidden:

   ```rust
   log::info!(
       "sync_overlay_visibility: active_ids={:?} pane_count={}",
       active_pane_ids, st.panes.len()
   );
   ```

   And inside the loop:

   ```rust
   log::info!(
       "  pane_id={} is_active={} ca_layer_flipped={:#x}",
       pane_id, is_active, pane.ca_layer_flipped
   );
   ```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard with logs: redirect stderr to
   `~/dev/termsurf/logs/wezboard.log`
3. Run `web google.com` in the first pane — confirm overlay appears
4. Open a split pane, run `web google.com` again
5. Check the log for the second pane's message sequence:
   - Does `SetOverlay` arrive with `is_new=true`?
   - Does `TabReady` arrive with the correct pane_id?
   - Does `handle_ca_context` find the pane and create layers?
   - Does `sync_overlay_visibility` hide the second pane's layer?
6. The log output will identify the exact failure point

**Result:** Partial

The debug logs revealed that the board-side overlay code works correctly for
multiple panes. The second pane's full lifecycle completes successfully:

```
SetOverlay: pane_id=1 is_new=true pixel=1014x1980
sent CreateTab: pane_id=1 url=https://ryanxcharles.com
TabReady: pane_id=1 tab_id=2 tab_to_pane_count=2
CaContext: tab_id=2 context_id=2345823755
handle_ca_context: tab_id=2 pane_id=1 has_layers=false
CALayerHost created: pane_id=1 contextId=2345823755 flipped=0xbbf61c000 host=0xbbf4d99c0
```

The overlay IS created and briefly visible. Then ~2 seconds later, the TUI
disconnects:

```
TermSurf client disconnected (Tui)
removed pane 1 on TUI disconnect
```

The "flash" is not a visibility sync issue — it's the overlay appearing, then
the TUI's socket connection dropping, and `handle_disconnect` cleaning up the
pane and its CALayers. The TUI process itself does not crash — it stays alive
but loses its webview because the board-side cleanup removes the overlay.

The disconnect also happens when closing the first TUI and reopening a new one.
Any TUI after the first one loses its webview the same way.

Wezboard only implements 11 of 30 TermSurf protocol messages. The missing query
handlers (`QueryLastRequest`, `QueryDevtoolsRequest`, `QueryTabsRequest`) were
hypothesized as the cause, but Experiment 5 disproved this — implementing all
three query handlers had no effect. The TUI handles missing query replies
gracefully (5-second timeout, returns `None`) and continues running.

The actual cause of the socket disconnect / missing webview remains unknown. The
overlay creation pipeline works correctly (as proven by the logs above), but
something causes the connection to drop shortly after.

#### Conclusion

The debug logs proved the overlay creation path works correctly for multiple
panes. The second pane's full lifecycle (SetOverlay → CreateTab → TabReady →
CaContext → CALayerHost) completes successfully and the overlay briefly appears.
But the TUI's socket connection drops ~2 seconds later, causing board-side
cleanup to remove the overlay. The root cause of the disconnect is still unknown
— it is not the missing query handlers (disproved by Experiment 5).

### Experiment 5: Implement query request/reply handlers

#### Background

Experiment 4 proved the overlay code works for multiple panes. The TUI crashes
because Wezboard silently drops synchronous query messages that expect replies.
The TUI calls `recv_reply()` with a 5-second timeout, gets nothing back, and
dies.

These three query handlers are the first priority because they're the immediate
cause of the TUI crash. The remaining missing messages (`SetDevtoolsOverlay`,
`OpenSplit`, `CursorChanged`) are fire-and-forget — missing them won't crash the
TUI, just limit features. Input forwarding (`KeyEvent`, `MouseEvent`,
`MouseMove`, `ScrollEvent`) is a separate feature that neither board implements
yet.

By implementing the query handlers first, we unblock multi-pane and can verify
the overlay lifecycle work from Experiments 1–4 actually works end-to-end.

The three query messages and their semantics (from Ghostboard's implementation
in `xpc.zig` and the TUI's `ipc.rs`):

**QueryLastRequest → QueryLastReply:**

The TUI sends `{ pane_id, profile }`. The board finds the last browser pane
(optionally filtered by profile) and replies with `{ pane_id, tab_id, profile }`
or `{ error }`. Used by `:last` to reopen the previous session. Ghostboard
tracks this in `last_browser_pane`. Wezboard already stores
`state.last_browser_pane` (set in `handle_tab_ready`) but never reads it.

**QueryDevtoolsRequest → QueryDevtoolsReply:**

The TUI sends `{ pane_id, inspected_tab_id, profile }`. The board validates the
request: if `inspected_tab_id == 0`, auto-targets to `last_browser_pane`'s
tab_id; checks no existing DevTools pane already inspects that tab; looks up the
inspected tab's browser and profile. Replies with `{ tab_id, browser, profile }`
or `{ error }`. Used by `web devtools` to open DevTools in a split.

**QueryTabsRequest → QueryTabsReply:**

The TUI sends `{ pane_id, profile }`. The board counts GUI panes matching the
profile and replies with `{ gui_panes }` (Ghostboard currently leaves
`chromium_tabs`, `chromium_browser`, `chromium_devtools`, and `tabs` unpopulated
— defaults to 0/empty). Used by `:status` command.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Add three match arms in
`handle_message` (before the `Some(other)` catch-all):

**1. QueryLastRequest:**

```rust
Some(Msg::QueryLastRequest(q)) => {
    log::info!("QueryLastRequest: pane_id={} profile={}", q.pane_id, q.profile);
    let st = state.lock().unwrap();
    let reply = if let Some(ref last_id) = st.last_browser_pane {
        if let Some(pane) = st.panes.get(last_id) {
            if q.profile.is_empty() || pane.profile == q.profile {
                proto::QueryLastReply {
                    pane_id: last_id.clone(),
                    tab_id: pane.tab_id,
                    profile: pane.profile.clone(),
                    error: String::new(),
                }
            } else {
                proto::QueryLastReply {
                    error: "No matching pane for profile".into(),
                    ..Default::default()
                }
            }
        } else {
            proto::QueryLastReply {
                error: "Last pane no longer exists".into(),
                ..Default::default()
            }
        }
    } else {
        proto::QueryLastReply {
            error: "No browser pane yet".into(),
            ..Default::default()
        }
    };
    drop(st);
    let msg = TermSurfMessage { msg: Some(Msg::QueryLastReply(reply)) };
    let payload = msg.encode_to_vec();
    let len = (payload.len() as u32).to_le_bytes();
    (&**stream).write_all(&len).await?;
    (&**stream).write_all(&payload).await?;
}
```

**2. QueryDevtoolsRequest:**

```rust
Some(Msg::QueryDevtoolsRequest(q)) => {
    log::info!(
        "QueryDevtoolsRequest: pane_id={} inspected_tab_id={} profile={}",
        q.pane_id, q.inspected_tab_id, q.profile
    );
    let st = state.lock().unwrap();

    // Resolve inspected_tab_id (0 means auto-target to last browser pane)
    let resolved_tab_id = if q.inspected_tab_id != 0 {
        q.inspected_tab_id
    } else if let Some(ref last_id) = st.last_browser_pane {
        st.panes.get(last_id).map(|p| p.tab_id).unwrap_or(0)
    } else {
        0
    };

    let reply = if resolved_tab_id == 0 {
        proto::QueryDevtoolsReply {
            error: "No browser tab found".into(),
            ..Default::default()
        }
    } else {
        // Check for duplicate DevTools
        let already_open = st.panes.values().any(|p| p.inspected_tab_id == resolved_tab_id);
        if already_open {
            proto::QueryDevtoolsReply {
                error: format!("Tab {} already has DevTools open", resolved_tab_id),
                ..Default::default()
            }
        } else if let Some(inspected_pane_id) = st.tab_to_pane.get(&resolved_tab_id) {
            let inspected_pane = st.panes.get(inspected_pane_id).unwrap();
            proto::QueryDevtoolsReply {
                tab_id: resolved_tab_id,
                browser: inspected_pane.browser.clone(),
                profile: inspected_pane.profile.clone(),
                error: String::new(),
            }
        } else {
            proto::QueryDevtoolsReply {
                error: "Inspected tab not found".into(),
                ..Default::default()
            }
        }
    };
    drop(st);
    let msg = TermSurfMessage { msg: Some(Msg::QueryDevtoolsReply(reply)) };
    let payload = msg.encode_to_vec();
    let len = (payload.len() as u32).to_le_bytes();
    (&**stream).write_all(&len).await?;
    (&**stream).write_all(&payload).await?;
}
```

**3. QueryTabsRequest:**

```rust
Some(Msg::QueryTabsRequest(q)) => {
    log::info!("QueryTabsRequest: pane_id={} profile={}", q.pane_id, q.profile);
    let st = state.lock().unwrap();
    let gui_panes = st.panes.values()
        .filter(|p| q.profile.is_empty() || p.profile == q.profile)
        .count() as i64;
    let reply = proto::QueryTabsReply {
        gui_panes,
        chromium_tabs: 0,
        chromium_browser: 0,
        chromium_devtools: 0,
        tabs: vec![],
        error: String::new(),
    };
    drop(st);
    let msg = TermSurfMessage { msg: Some(Msg::QueryTabsReply(reply)) };
    let payload = msg.encode_to_vec();
    let len = (payload.len() as u32).to_le_bytes();
    (&**stream).write_all(&len).await?;
    (&**stream).write_all(&payload).await?;
}
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com` in the first pane
3. Open a split pane, run `web google.com` again
4. **Expected:** both overlays visible simultaneously, second TUI does not crash
5. Close the first TUI, open a new one
6. **Expected:** the new overlay appears, TUI stays alive
7. Open a new tab, switch back — overlays show/hide correctly

#### Result: Failure

The code compiled successfully (required a minor fix: block-scoping the
`MutexGuard` so it drops before `.await` points, avoiding a `Send` bound error).
But at runtime there was no noticeable behavioral difference — the second pane's
webview still does not appear.

The hypothesis was wrong. The TUI never crashed — it stays alive but simply
doesn't get a webview in the second pane. Query handler replies are irrelevant
because the TUI handles query failures gracefully and continues running. The
actual problem is somewhere in the overlay creation pipeline for the second
pane: SetOverlay → CreateTab → TabReady → CaContext → CALayerHost. One of these
steps is failing silently for the second pane. Likely candidates:

1. Server reuse path doesn't send CreateTab to the already-running Chromium
2. Chromium creates the tab but never sends CaContext for the second tab
3. CaContext arrives but `sync_overlay_visibility` hides the layer
4. The CALayerHost layer is created but positioned off-screen or zero-sized

### Experiment 6: Debug logs for second webview failure

#### Background

Experiment 4 showed the second pane's overlay pipeline completes (SetOverlay →
CreateTab → TabReady → CaContext → CALayerHost) but then the TUI's socket
connection drops ~2 seconds later, causing cleanup. Experiment 5 proved query
handlers aren't the cause. The TUI never crashes — it stays alive but has no
webview.

The problem reproduces for ANY second `web` command — same pane, new split, new
window. This means it's a Wezboard-side state issue, not a TUI issue. The first
TUI works; every subsequent one fails. Something about the first connection
changes Wezboard's state in a way that breaks all future connections.

There are several failure points we haven't instrumented:

1. **Listener accept loop** — Does Wezboard even accept the second connection?
   The listener runs in `std::thread::spawn` and calls `spawn_into_main_thread`
   for each connection. If the first connection's async task somehow blocks the
   executor, new connections may be accepted by the thread but never polled by
   the async runtime.

2. **Connection read loop** — The read loop at `handle_connection:47` uses `?`
   on the async read. If `Async::new()` fails or the read errors (not EOF), the
   handler exits via the `?` at line 47 or the `?` at line 22, logging to
   `listener.rs:33` ("TermSurf connection error") but never calling
   `handle_disconnect`. We'd see the accept log but no messages.

3. **Protobuf decode** — The decode at line 63 also uses `?`. If a message fails
   to decode, the entire connection handler exits. This would manifest as an
   accept + type detection, then sudden disconnect.

4. **First connection still alive vs. closed** — When the first TUI is still
   running and we open a second, both connections share the same `SharedState`.
   When the first TUI has exited and we open a second, the first connection's
   `handle_disconnect` already ran. Does it leave state in a bad place? For
   instance, if a server's `tx` was cleared on Chromium disconnect, the second
   SetOverlay would find the server exists but `tx` is `None`, so `CreateTab`
   never sends.

5. **Server reuse path** — When the second TUI's SetOverlay arrives and a server
   already exists for the profile, the code at line 331-337 increments
   `pane_count` and sends `CreateTab` if `server_tx` is `Some`. If the first TUI
   disconnected and the Chromium process also disconnected, `server.tx` would be
   `None` and `CreateTab` silently doesn't send.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/listener.rs`** — Log connection count:

```rust
// At the top of the accept loop body, before spawn_into_main_thread:
log::info!("TermSurf client connected: {} (connection #{})", peer, conn_count);
```

Add a counter variable before the `for stream in listener.incoming()` loop.

**`wezboard/wezboard-gui/src/termsurf/conn.rs`** — Add logs at these points:

**1. handle_connection entry and exit** — Log when the connection starts and
when it exits (both normal EOF and error paths):

```rust
// Line 21, after function signature:
log::info!("handle_connection: starting");

// Line 47, after read returns 0:
log::info!("handle_connection: EOF after processing, conn_type={:?}", conn_type);

// Wrap the entire function body in a block that logs on exit:
// At end of handle_connection, the Ok(()) or ? propagation already exists.
```

**2. handle_connection read errors** — The `?` at line 47 silently propagates
read errors. The caller in listener.rs:32 logs "TermSurf connection error" but
doesn't distinguish read errors from other failures. Add a log before the `?`:

```rust
let n = match (&*stream).read(&mut tmp).await {
    Ok(n) => n,
    Err(e) => {
        log::error!("handle_connection: read error: {:#}", e);
        handle_disconnect(conn_type, &tx, &state);
        tx.close();
        return Err(e.into());
    }
};
```

**3. Every message received** — Log every message type as it arrives, with a
connection-scoped counter, so we can see the exact message sequence for each
connection:

```rust
// Before the conn_type detection block:
msg_count += 1;
log::info!(
    "handle_connection: msg #{} type={:?} conn_type={:?}",
    msg_count, msg_type_name(&msg), conn_type
);
```

Add a helper `msg_type_name` that returns a short string for the message variant
(e.g., "HelloRequest", "SetOverlay", "QueryLastRequest").

**4. handle_set_overlay server reuse path** — Log whether `server.tx` is
available when reusing an existing server:

```rust
// Line 331-337, in the server-exists branch:
log::info!(
    "SetOverlay: reusing server key={} pane_count={} has_tx={}",
    key, st.servers.get(&key).unwrap().pane_count, server_tx.is_some()
);
```

**5. handle_disconnect** — Log the full state snapshot on disconnect so we can
see what's left behind:

```rust
// At the start of handle_disconnect:
log::info!(
    "handle_disconnect: conn_type={:?} panes={} servers={} tab_to_pane={}",
    conn_type, st.panes.len(), st.servers.len(), st.tab_to_pane.len()
);

// After TUI disconnect cleanup:
log::info!(
    "handle_disconnect: after cleanup panes={} servers={}",
    st.panes.len(), st.servers.len()
);
```

**6. State snapshot on second SetOverlay** — Dump the full server and pane state
when a new SetOverlay arrives so we can see if stale state from the first
connection is causing the problem:

```rust
// At the top of handle_set_overlay, after locking state:
log::info!(
    "SetOverlay state: panes={:?} servers={:?}",
    st.panes.keys().collect::<Vec<_>>(),
    st.servers.keys().collect::<Vec<_>>()
);
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard with stderr captured to a log file
3. Run `web google.com` in the first pane — confirm overlay appears
4. Close the first TUI (`:q`), then run `web google.com` again in the same pane
5. Check the log. The sequence should reveal:
   - Was the second connection accepted? (connection # log)
   - Did messages arrive? (msg # logs)
   - Did SetOverlay reach the server reuse path? (has_tx log)
   - Did the connection drop? (EOF/error log)
   - What state was left after the first disconnect? (state snapshot)
6. If same-pane works, also test: new split pane, new window

#### Result: Success (diagnosis)

The debug logs proved the second overlay IS fully created — every step of the
pipeline succeeds (SetOverlay → CreateTab → TabReady → CaContext → CALayerHost).
The second pane gets its own 3-layer hierarchy with a valid context ID. The TUI
never crashes and never disconnects unexpectedly.

The real bug is **overlay positioning**. `update_ca_layer_frame` computes the
overlay's pixel origin from `super::metrics::get()` which returns a single
global `(origin_x, origin_y)` — the content area offset for the entire window.
Every pane's positioning layer gets the same origin, so the second overlay is
drawn on top of the first pane's area. The second pane's screen area has no
overlay.

The SetOverlay message sends `col` and `row` (grid coordinates), and
`handle_set_overlay` receives them, but the `Pane` struct doesn't store them.
Ghostboard handles this correctly — it passes col/row to `setOverlay()` on each
Surface, which computes per-surface pixel origin. Wezboard has no per-pane
position tracking.

The white flash on pane 0 is a separate issue: when the split opens, the first
pane resizes, Chromium re-sends CaContext with the same context ID, and
`handle_ca_context` swaps the CALayerHost (old removed, new added). The swap
briefly shows a blank frame.

### Experiment 7: Per-pane overlay positioning

#### Background

Experiment 6 proved the second overlay is created but drawn at the wrong
position — every pane's CALayerHost positioning layer gets the same global
origin from `metrics::get()`. The fix: store per-pane `col`/`row` from the
SetOverlay message and compute pixel origin as
`global_origin + col * cell_w, global_origin_y + row * cell_h`.

The SetOverlay proto already sends `col` and `row` (grid coordinates). The TUI
computes these from ratatui's viewport rect — `col` is the column offset and
`row` is the row offset of the browser area within the terminal pane. For a
single pane, col=0 and row=1 (row 0 is the URL bar). For a right split, col
would be the split boundary column.

Ghostboard handles this in `xpc.zig:273` —
`surface.core().setOverlay(col, row,
width, height)` stores the grid coordinates
per-surface, and the Metal renderer uses them to position the CALayerHost.
Wezboard needs the same per-pane positioning.

#### Changes

**1. `wezboard/wezboard-gui/src/termsurf/state.rs`** — Add `col` and `row`
fields to the `Pane` struct:

```rust
pub struct Pane {
    pub pane_id: String,
    pub profile: String,
    pub browser: String,
    pub url: String,
    pub col: u64,        // NEW: grid column offset
    pub row: u64,        // NEW: grid row offset
    pub pixel_width: u64,
    pub pixel_height: u64,
    // ... rest unchanged
}
```

**2. `wezboard/wezboard-gui/src/termsurf/conn.rs` — `handle_set_overlay`** —
Store col/row in both the new-pane and resize paths:

In the new-pane path (Pane construction, ~line 410):

```rust
let pane = Pane {
    pane_id: overlay.pane_id.clone(),
    col: overlay.col,      // NEW
    row: overlay.row,      // NEW
    // ... rest unchanged
};
```

In the resize path (~line 372):

```rust
let pane = st.panes.get_mut(&overlay.pane_id).unwrap();
pane.col = overlay.col;        // NEW
pane.row = overlay.row;        // NEW
pane.pixel_width = pixel_w;
pane.pixel_height = pixel_h;
```

**3. `wezboard/wezboard-gui/src/termsurf/conn.rs` — `update_ca_layer_frame`** —
Compute per-pane pixel origin from col/row and cell metrics:

```rust
unsafe fn update_ca_layer_frame(pane: &Pane, root_layer: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let scale: f64 = msg_send![root_layer, contentsScale];
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let w = pane.pixel_width as f64 / scale;
    let h = pane.pixel_height as f64 / scale;
    let (cell_w, cell_h, origin_x, origin_y) = super::metrics::get();
    let x = (origin_x as u64 + pane.col * cell_w as u64) as f64 / scale;
    let y = (origin_y as u64 + pane.row * cell_h as u64) as f64 / scale;
    let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));

    let positioning = pane.ca_layer_positioning as *mut AnyObject;
    let _: () = msg_send![positioning, setFrame: frame];
}
```

The key change: `x` and `y` now include `pane.col * cell_w` and
`pane.row * cell_h` respectively, offsetting each pane's overlay to its correct
grid position within the window.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com` — single pane overlay at correct
   position
3. Open a vertical split, run `web google.com` in the second pane
4. **Expected:** both overlays visible, each in their own pane area
5. Resize the split boundary — both overlays should resize and reposition
6. Close one pane — remaining overlay should expand to fill the window

#### Result: Failure

The first pane's overlay shifted down and to the right — margins roughly
doubled. The second pane's overlay still doesn't appear.

The problem: `origin_x`/`origin_y` from `metrics::get()` is the pixel offset of
the terminal content area from the window origin (padding, title bar). The TUI's
`col`/`row` values are grid coordinates relative to that same content area. So
adding `col * cell_w` to `origin_x` double-counts the offset — `origin_x`
already positions us at the content area, and `col`/`row` position us within it.

The correct formula likely needs to use `col * cell_w` and `row * cell_h` as the
sole source of per-pane positioning (replacing the global origin), or
`origin_x`/`origin_y` needs to be understood as a window-level offset that
should be added only once, not combined with grid coordinates that already
include it. The exact relationship between the TUI's col/row, the global metrics
origin, and the CALayer coordinate space needs closer inspection.

## Conclusion

### Progress

Seven experiments across two tracks: **overlay visibility** (solved) and
**multi-pane overlay positioning** (unsolved).

**Solved:**

- **Tab switching** (Exps 1–2): Overlays hide when switching to a tab without a
  webview and reappear on switch back. Required two changes: a
  `sync_overlay_visibility` function called from the `WindowInvalidated` handler,
  and setting `TERMSURF_PANE_ID` in Wezboard's `domain.rs` so the TUI's pane
  identity matches the mux pane ID.

- **Query handlers** (Exp 5): Implemented `QueryLastRequest`,
  `QueryDevtoolsRequest`, and `QueryTabsRequest` reply handlers. These don't
  affect the multi-pane bug but complete the protocol for `:last`, `web devtools`,
  and `:status`.

- **Debug instrumentation** (Exps 4, 6): Comprehensive logging across the
  connection lifecycle — accept, message sequence, server reuse, disconnect state
  snapshots, and per-message type names.

**Diagnosed but unsolved:**

- **Multi-pane overlays** (Exps 3, 6, 7): The second overlay IS fully created —
  the entire pipeline (SetOverlay → CreateTab → TabReady → CaContext →
  CALayerHost) completes successfully. But all panes' positioning layers get the
  same pixel origin from the global `metrics::get()`, so the second overlay is
  drawn on top of the first. Experiment 7 tried adding `col * cell_w` and
  `row * cell_h` to the global origin, but this double-counted the offset,
  pushing the first overlay down/right.

### Remaining bugs

1. **Overlay positioning formula** — The relationship between `origin_x`/
   `origin_y` (global content area offset), the TUI's `col`/`row` (grid
   coordinates), and the CALayer coordinate space is not yet understood. The
   global origin may already include padding that the grid coordinates replicate,
   or the grid coordinates may be window-relative rather than content-relative.
   Need to inspect the actual col/row values the TUI sends and compare with
   Ghostboard's coordinate handling in `Surface.zig` and `Metal.zig`.

2. **White flash on resize** — When a split opens and the first pane resizes,
   Chromium re-sends CaContext with the same context ID. `handle_ca_context`
   swaps the CALayerHost (remove old, add new), briefly showing a blank frame.
   This is cosmetic but noticeable. Could be fixed by skipping the swap when the
   context ID hasn't changed, or by using an opacity transition.

### Remaining protocol work

Wezboard now handles 14 of 30 TermSurf protocol messages (47%). Still missing:

- **Input forwarding** (4): `KeyEvent`, `MouseEvent`, `MouseMove`, `ScrollEvent`
- **DevTools** (2): `CreateDevtoolsTab`, `SetDevtoolsOverlay`
- **Other** (3): `FocusChanged`, `CursorChanged`, `OpenSplit`
- Plus several reply-only messages the board sends but doesn't receive
