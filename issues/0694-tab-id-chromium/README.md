+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 694: Replace pane_id with tab_id in Chromium

Remove all `pane_id` usage from the Chromium profile server. Chromium should
only know about tabs (identified by `tab_id`). The GUI manages the pane ↔ tab
relationship.

## Why

Currently there's a 1:1 relationship between panes and tabs — every pane has
exactly one Chromium tab, and Chromium identifies tabs by `pane_id`. This blocks
multiple tabs per pane.

Multiple tabs per pane enables:

- Split views (two webpages side by side in one pane)
- Webview scrolling with a shell session (webpage renders inline, scrolls up as
  shell output continues)
- Tab stacking (multiple tabs behind one pane, switched with keybindings)
- Picture-in-picture (small overlay webview inside a pane)

None of these are possible while Chromium uses `pane_id` as its primary
identifier, because Chromium can only have one tab per pane_id.

The fix is clean: Chromium already generates `tab_id` (auto-incrementing
integer). Use it everywhere. The GUI already stores `tab_id` in the Pane struct
(received from `tab_ready`). The GUI translates between pane_id (its domain) and
tab_id (Chromium's domain).

## How It Works Now

### The three layers

```
TUI (Rust)  ←→  GUI (Zig)  ←→  Chromium (C++)
  pane_id         pane_id         pane_id ← WRONG
                  tab_id          tab_id
```

### Chromium's pane_id usage (4 files, 78 occurrences)

**Storage:**

- `TabState::pane_id` (string) in `shell_browser_main_parts.h`
- `ShellTabObserver::pane_id_` (string) in `shell_tab_observer.h`

**Inbound messages (GUI → Chromium, on control connection):**

All use `pane_id` to identify the target tab:

| Message               | Uses pane_id | Uses tab_id     |
| --------------------- | ------------ | --------------- |
| `create_tab`          | YES          | NO              |
| `create_devtools_tab` | YES          | YES (inspected) |
| `resize`              | YES          | NO              |
| `mouse_event`         | YES          | NO              |
| `scroll_event`        | YES          | NO              |
| `mouse_move`          | YES          | NO              |
| `focus_changed`       | YES          | NO              |
| `key_event`           | YES          | NO              |
| `navigate`            | YES          | NO              |
| `set_color_scheme`    | YES          | NO              |
| `close_tab`           | YES          | NO              |
| `query_tabs`          | NO           | NO              |

**Outbound messages (Chromium → GUI, on per-tab connection):**

All echo `pane_id` back:

| Message          | Sends pane_id | Sends tab_id |
| ---------------- | ------------- | ------------ |
| `tab_ready`      | YES           | YES          |
| `ca_context`     | YES           | NO           |
| `cursor_changed` | YES           | NO           |
| `url_changed`    | YES           | NO           |
| `loading_state`  | YES           | NO           |
| `title_changed`  | YES           | NO           |

**Tab lookup pattern (repeated ~10 times):**

```cpp
TabState* tab = nullptr;
for (auto& t : tabs_) {
    if (t->pane_id == pane_id) {
        tab = t.get();
        break;
    }
}
```

### GUI's mapping (xpc.zig)

The GUI already maintains the pane_id ↔ tab_id relationship:

```zig
const Pane = struct {
    pane_id_key: []const u8,        // UUID string
    tab_id: i64,                     // From Chromium's tab_ready
    overlay_surface: ?*CoreSurface,
    server: ?*Server,                // Which Chromium process
    // ...
};

var panes: StringHashMap(*Pane);     // pane_id → Pane
```

When the GUI receives `tab_ready { pane_id, tab_id }`, it stores `tab_id` in the
Pane. All subsequent forwarding could use `tab_id` instead of `pane_id` — it
just doesn't yet.

## Target Architecture

```
TUI (Rust)  ←→  GUI (Zig)  ←→  Chromium (C++)
  pane_id         pane_id         tab_id ← CORRECT
                  tab_id          tab_id
```

The boundary is at the GUI. Pane_id stays in the GUI/TUI world. Tab_id is
Chromium's world. The GUI translates between them.

## Design

### Chromium changes (4 files)

#### 1. `shell_browser_main_parts.h`

- Remove `std::string pane_id` from `TabState`
- Remove `CloseTabByPaneId` declaration
- Add `FindTabById(int tab_id)` helper
- Add `CloseTabById(int tab_id)` to replace `CloseTabByPaneId`

#### 2. `shell_browser_main_parts.cc`

**Tab lookup:** Replace all `pane_id` string comparisons with `tab_id` integer
comparisons. The repeated pattern:

```cpp
// Before:
for (auto& t : tabs_) {
    if (t->pane_id == pane_id) { tab = t.get(); break; }
}

// After:
TabState* tab = FindTabById(tab_id);
```

**Inbound messages:** Change every `xpc_dictionary_get_string(event, "pane_id")`
to `xpc_dictionary_get_int64(event, "tab_id")`. Affects: `resize`,
`mouse_event`, `scroll_event`, `mouse_move`, `focus_changed`, `key_event`,
`navigate`, `set_color_scheme`, `close_tab`.

**`create_tab`:** Special case — the GUI doesn't know the tab_id yet (Chromium
assigns it). `create_tab` is fire-and-forget (`xpc_connection_send_message`, not
`send_message_with_reply_sync`), so we can't return `tab_id` in a reply. Keep
`pane_id` in `create_tab` as an opaque correlation ID. Chromium stores it and
echoes it back in `tab_ready { pane_id, tab_id }`. After that, Chromium never
uses `pane_id` again — all subsequent messages use `tab_id`.

**`create_devtools_tab`:** Same — keep `pane_id` for correlation, echo in
`tab_ready`. `inspected_tab_id` is already a tab_id, no change.

**`tab_ready`:** Keep both `pane_id` (for GUI correlation) and `tab_id`. This is
the ONLY message that includes `pane_id` after this change.

**Outbound messages:** Remove `pane_id` from `ca_context`, `cursor_changed`,
`url_changed`, `loading_state`, `title_changed`. These go on the per-tab
connection, so the GUI already knows which tab they belong to (by connection
identity). Include `tab_id` for robustness.

**`CloseTabByPaneId` → `CloseTabById`:** Change the lookup from string
comparison to integer comparison.

#### 3. `shell_tab_observer.h`

- Remove `std::string pane_id_`
- Remove `SetPaneId()`
- Add `int tab_id_` and `SetTabId(int tab_id)`

#### 4. `shell_tab_observer.cc`

- Replace all `pane_id_` with `tab_id_` in outbound messages
- Change `xpc_dictionary_set_string(msg, "pane_id", pane_id_.c_str())` to
  `xpc_dictionary_set_int64(msg, "tab_id", tab_id_)`

### GUI changes (xpc.zig)

#### Outbound (GUI → Chromium)

Every message that currently sends `pane_id` to Chromium must send `tab_id`
instead. The GUI already has `tab_id` in the Pane struct.

Functions affected: `sendCreateTab` (no pane_id, read tab_id from reply),
`sendCreateDevToolsTab` (same), all forwarding functions (resize, mouse, key,
navigate, etc.).

**`sendCreateTab` flow:** Unchanged — still sends `pane_id` for correlation.
`sendCreateDevToolsTab`: same.

**All other outbound functions** (resize, mouse, key, navigate, etc.): send
`tab_id` (int64) instead of `pane_id` (string). The Pane struct already has
`tab_id`.

#### Inbound (Chromium → GUI)

Messages from Chromium now include `tab_id` instead of `pane_id`. The GUI needs
a reverse lookup: tab_id → pane_id. Add a new map:

```zig
var tab_to_pane: AutoHashMap(i64, []const u8) = .init(alloc);  // tab_id → pane_id
```

Populated in `handleTabReady` (which still receives `pane_id` for correlation).
Used to route `ca_context`, `cursor_changed`, `url_changed`, `loading_state`,
`title_changed` back to the correct pane/TUI.

Cleaned up in `handleDisconnect` when a pane is removed.

#### `handleDisconnect` change

Currently sends `close_tab { pane_id }` to Chromium. Change to
`close_tab { tab_id }`.

### TUI changes: None

The TUI only communicates with the GUI using `pane_id`. The TUI never talks to
Chromium directly. No TUI changes needed.

## Test

1. `web google.com` → page loads, URL bar updates
2. Navigate in Edit mode → URL changes, loading indicator works
3. Mouse clicks, scroll, text selection → all work
4. Keyboard input → typing in search bars works
5. `:devtools right` → DevTools opens, inspects correct tab
6. Close DevTools pane → pane closes, no crash
7. Reopen DevTools → works multiple times
8. Open two browser panes (Cmd+D split) → both work independently
9. Different profiles → each profile's Chromium process handles its tabs
10. Close a pane → tab cleaned up, other panes unaffected
11. `web status` → shows tab inventory with tab_ids

## Experiment 1: Replace pane_id with tab_id everywhere except create/tab_ready

### Hypothesis

If we change all Chromium inbound messages (except `create_tab` and
`create_devtools_tab`) to use `tab_id` instead of `pane_id`, change all outbound
messages to send `tab_id` instead of `pane_id`, and update the GUI to translate
between the two, then Chromium no longer depends on `pane_id` for tab routing —
only for initial correlation during tab creation.

### Changes

Four Chromium files, one GUI file.

#### 1. Chromium: `shell_browser_main_parts.h`

Add `FindTabById` helper. Rename `CloseTabByPaneId` → `CloseTabById`. Keep
`pane_id` in TabState (still needed for `tab_ready` echo).

```cpp
// Add:
TabState* FindTabById(int tab_id);
void CloseTabById(int tab_id);

// Remove:
void CloseTabByPaneId(const std::string& pane_id);
```

#### 2. Chromium: `shell_browser_main_parts.cc`

**Add `FindTabById` helper:**

```cpp
ShellBrowserMainParts::TabState* ShellBrowserMainParts::FindTabById(int tab_id) {
    for (auto& t : tabs_) {
        if (t->tab_id == tab_id) return t.get();
    }
    return nullptr;
}
```

**Inbound message handlers — change pane_id → tab_id:**

For `resize`, `mouse_event`, `scroll_event`, `mouse_move`, `focus_changed`,
`key_event`, `navigate`, `set_color_scheme`:

```cpp
// Before:
const char* pane = xpc_dictionary_get_string(event, "pane_id");
// ... lookup by pane string ...

// After:
int tab_id = (int)xpc_dictionary_get_int64(event, "tab_id");
// ... pass tab_id to handler ...
```

Each handler function signature changes from `const std::string& pane_id` to
`int tab_id`, and uses `FindTabById(tab_id)` instead of the linear string scan.

**`close_tab` handler:**

```cpp
// Before:
const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
// ... CloseTabByPaneId(pane_id) ...

// After:
int tab_id = (int)xpc_dictionary_get_int64(event, "tab_id");
// ... CloseTabById(tab_id) ...
```

**`CloseTabById` (replacing `CloseTabByPaneId`):**

```cpp
void ShellBrowserMainParts::CloseTabById(int tab_id) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);
    for (auto it = tabs_.begin(); it != tabs_.end(); ++it) {
        if ((*it)->tab_id == tab_id) {
            // Same teardown as before (Issue 689 Exp 6).
            (*it)->tab_observer.reset();
            delete (*it)->shell.get();
            (*it)->shell = nullptr;
            if ((*it)->tab_connection) {
                xpc_connection_cancel((*it)->tab_connection);
                xpc_release((*it)->tab_connection);
                (*it)->tab_connection = nullptr;
            }
            tabs_.erase(it);
            if (tabs_.empty()) {
                Shell::Shutdown();
            }
            return;
        }
    }
}
```

**`create_tab` and `create_devtools_tab`:** Unchanged — still receive `pane_id`
for correlation.

**`tab_ready` messages:** Keep both `pane_id` and `tab_id` (unchanged).

**Log messages:** Update from "pane X" to "tab Y" where appropriate.

#### 3. Chromium: `shell_tab_observer.h`

Replace `pane_id_` with `tab_id_`:

```cpp
// Remove:
std::string pane_id_;
void SetPaneId(const std::string& pane_id);

// Add:
int tab_id_ = 0;
void SetTabId(int tab_id);
```

#### 4. Chromium: `shell_tab_observer.cc`

Replace all outbound `pane_id` with `tab_id`:

```cpp
// Before (in OnCursorChanged, DidFinishNavigation, SendLoadingState, TitleWasSet):
xpc_dictionary_set_string(msg, "pane_id", pane_id_.c_str());

// After:
xpc_dictionary_set_int64(msg, "tab_id", tab_id_);
```

Update `SetPaneId` → `SetTabId`:

```cpp
void ShellTabObserver::SetTabId(int tab_id) {
    tab_id_ = tab_id;
}
```

Update callers in `shell_browser_main_parts.cc`:

```cpp
// Before (in CreateTab, CreateDevToolsTab):
tab_observer->SetPaneId(pane_id);

// After:
tab_observer->SetTabId(tab->tab_id);
```

#### 5. GUI: `xpc.zig`

**Add `tab_to_pane` map (near other global maps):**

```zig
var tab_to_pane: std.AutoHashMap(i64, []const u8) = .init(alloc);
```

**`handleTabReady`:** Populate the new map:

```zig
fn handleTabReady(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");

    if (panes.get(pane_id)) |p| {
        p.tab_id = tab_id;
        if (tab_id > 0) {
            last_browser_pane = p.pane_id_key;
        }
        // Register reverse lookup.
        tab_to_pane.put(tab_id, p.pane_id_key) catch {};
    }
}
```

**Inbound Chromium handlers — look up by tab_id:**

For `handleCAContext`, `handleCursorChanged`, `handleUrlChanged`,
`handleTitleChanged`, `handleLoadingState`:

```zig
// Before:
const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
const p = panes.get(pane_id) orelse return;

// After:
const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
const pane_id = tab_to_pane.get(tab_id) orelse return;
const p = panes.get(pane_id) orelse return;
```

**Outbound to Chromium — send tab_id instead of pane_id:**

For `sendResize`, `sendMouseEvent`, `sendScrollEvent`, `sendMouseMove`,
`sendKeyEvent`, `sendFocusChanged/sendFocusMessage`, `handleNavigate`,
`handleSetColorScheme`:

```zig
// Before:
xpc_dictionary_set_string(msg, "pane_id", pane_id_z);

// After:
xpc_dictionary_set_int64(msg, "tab_id", p.tab_id);
```

This eliminates the null-terminated pane_id buffer copies in each function —
simpler and faster.

**`handleDisconnect` — send tab_id:**

```zig
// Before:
xpc_dictionary_set_string(close_msg, "pane_id", pane_id_z);

// After:
xpc_dictionary_set_int64(close_msg, "tab_id", p.tab_id);

// Also clean up reverse map:
_ = tab_to_pane.remove(p.tab_id);
```

**`sendCreateTab` and `sendCreateDevToolsTab`:** Keep sending `pane_id` (for
tab_ready correlation). No change.

### What stays the same

- TUI ↔ GUI communication: all `pane_id`, completely unchanged
- `create_tab` / `create_devtools_tab` messages: still include `pane_id`
- `tab_ready` response: still includes both `pane_id` and `tab_id`
- `query_tabs`: doesn't use pane_id or tab_id for routing

### What changes

| Layer    | Before              | After                  |
| -------- | ------------------- | ---------------------- |
| Chromium | Lookup by pane_id   | Lookup by tab_id       |
| Chromium | Send pane_id back   | Send tab_id back       |
| Chromium | String comparisons  | Integer comparisons    |
| GUI out  | Send pane_id string | Send tab_id int64      |
| GUI in   | Look up by pane_id  | Look up by tab_id→pane |

### Test

Same as issue-level test plan:

1. `web google.com` → page loads, URL bar updates
2. Navigate in Edit mode → URL changes, loading indicator works
3. Mouse clicks, scroll, text selection → all work
4. Keyboard input → typing in search bars works
5. `:devtools right` → DevTools opens, inspects correct tab
6. Close DevTools pane → pane closes, no crash
7. Reopen DevTools → works multiple times
8. Open two browser panes (Cmd+D split) → both work independently
9. Different profiles → each profile's Chromium process handles its tabs
10. Close a pane → tab cleaned up, other panes unaffected
11. `web status` → shows tab inventory with tab_ids

### Result: Success

All tests pass. Chromium build (13 steps) and GUI build both compile cleanly.
Runtime testing confirms all browser features work with tab_id routing:
navigation, mouse/keyboard input, DevTools, multi-pane, resize, focus, cursor
changes, loading state, URL/title updates, and pane close/cleanup.

Key implementation detail not in the original design: DevTools tabs now also get
auto-incrementing tab_ids (previously they stayed at 0). This was necessary
because tab_id is the primary identifier in all outbound messages — multiple
DevTools tabs at tab_id=0 would be indistinguishable. Browser vs DevTools
detection changed from `tab_id > 0` to `inspected_tab_id > 0`.

## Conclusion

Experiment 1 succeeded. Chromium no longer depends on `pane_id` for tab routing.
The only remaining `pane_id` usage is in `create_tab`/`create_devtools_tab`
(correlation) and `tab_ready` (echo). All other messages use `tab_id` (int64).

The GUI maintains a `tab_to_pane` reverse map to translate Chromium's tab_id
back to pane_id for internal routing. This cleanly separates concerns: Chromium
knows tabs, the GUI knows panes, and the boundary is explicit.

This unblocks future multi-tab-per-pane features (tab stacking, split views,
inline webviews) since Chromium can now have multiple tabs without requiring
multiple pane_ids.
