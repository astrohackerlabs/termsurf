# Issue 689: Tab Lifecycle — Close Tabs When Panes Close

## Problem

When a GUI pane is removed (user closes a split, TUI exits, etc.), the
corresponding Chromium tab is never closed. The tab persists inside the profile
server — Shell, WebContents, compositor, renderer — all stay alive, consuming
memory and GPU resources. This is a silent leak for browser tabs and a crash
trigger for DevTools tabs.

### How to reproduce

1. Open `web google.com`
2. Open DevTools: `web devtools`
3. Close the DevTools pane (`:q` or close the split)
4. Open DevTools again: `web devtools`
5. Resize the terminal window
6. **Crash.** The main browser tab crashes.

The crash happens because the first DevTools tab was never closed in Chromium.
Two `InspectorOverlayAgent` instances attach to the same renderer, and on
resize, the duplicate triggers a `PaintController` DCHECK (Issue 686).

### Scope

This affects **all tabs**, not just DevTools:

- **Browser tabs:** Every closed pane leaks its Chromium tab. Orphaned tabs
  accumulate memory and GPU resources for the lifetime of the profile server.
  Masked by `killServer` — when the last pane on a profile closes, the entire
  server process is killed, destroying all tabs (orphaned or not). So single-tab
  workflows never notice. Multi-tab workflows silently leak.
- **DevTools tabs:** Same leak, but visible because orphaned DevTools crash when
  a new inspector attaches to the same renderer.

### Root cause

Two XPC connections exist per tab:

- **Connection A** (TUI ↔ GUI): Created by the TUI via the gateway. Stored as
  `web_peer` on the Pane struct. Drops when the TUI exits.
- **Connection B** (Profile Server → GUI): Created by the profile server in
  `CreateTab`/`CreateDevToolsTab`. Stored as `tab_connection` in `TabState`.
  Stays alive when the TUI exits because nobody cancels it.

When Connection A drops, `handleDisconnect` cleans up the GUI pane (overlay,
maps, focus state) and decrements the server's pane count. But it never tells
the profile server to close the Chromium tab. The profile server has no idea the
pane is gone.

### Prior art (Issue 688)

Issue 688 attempted three approaches to fix this. All failed:

1. **Experiment 1:** Built `:devtools` command. Orphaned tabs crashed on reopen.
2. **Experiment 2:** Cancelled `xpc_dictionary_get_remote_connection(msg)` — but
   that returns the shared control connection, killing all tabs.
3. **Experiment 3:** Added explicit `close_tab` XPC message with
   `CloseTabByPaneId`. Crashed on first invocation for unknown reasons.

The failures showed we don't understand the tab lifecycle well enough. Before
fixing, we need to **measure**: see exactly how many tabs Chromium thinks are
alive vs how many the GUI thinks are alive, and watch the counts change in real
time.

## Plan

### Phase 1: Measure — `web status` command

Add a `web status` subcommand that queries the Chromium profile server for its
live tab list and prints it. This lets us observe orphaned tabs directly and
verify any future fix.

### Phase 2: Fix — `close_tab` on pane cleanup

Once we can measure the leak, add an explicit `close_tab` message on pane
cleanup (same direction as Issue 688 Experiment 3) and use `web status` to
verify the fix works.

### Phase 3: Verify

Use `web status` through open/close/reopen cycles to confirm tab counts match
and no orphans accumulate.

## Relevant Code

- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — `tabs_` vector, `CreateTab`, `CreateDevToolsTab`, `CloseTab`,
  `StartDynamicMode` handler
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.h`
  — `TabState` struct, method declarations
- `gui/src/apprt/xpc.zig` — `panes` map, `handleDisconnect`, `cleanupPane`,
  message handlers
- `tui/src/main.rs` — CLI subcommands, `Commands` enum
- `tui/src/xpc.rs` — XPC query functions

## Experiment 1: `web status` diagnostic command

### Hypothesis

If the TUI sends a `query_tabs` synchronous XPC message to the GUI, and the GUI
forwards it synchronously to the Chromium profile server, we can display a live
tab inventory showing each tab's ID, type, URL, and pane ID — making orphaned
tabs immediately visible.

### Design

#### Data flow

```
web status → GUI (query_tabs) → Chromium (query_tabs) → reply
                                                          ↓
           ← GUI combines pane count + Chromium reply  ←──┘
           ↓
         print tab list and exit
```

Three synchronous hops. The TUI blocks on the GUI's reply, the GUI blocks on
Chromium's reply (via `xpc_connection_send_message_with_reply_sync` on
`server.peer`), and Chromium reads `tabs_` and responds.

#### Output format

```
Chromium tabs (profile: default):
  [1] https://google.com           pane=abc-123
  [0] devtools://1                 pane=def-456  (inspecting tab 1)
  ---
  browser: 1  devtools: 1  total: 2

GUI panes: 2
```

If there's a mismatch (e.g., Chromium has 2 tabs but GUI has 1 pane), the
orphaned tab is obvious.

### Changes

#### 1. TUI: add `Status` subcommand (`main.rs`)

Add a new variant to the `Commands` enum:

```rust
#[derive(Subcommand)]
enum Commands {
    Url { url: String },
    Last,
    Status,  // New
}
```

Handle it early in `main()`, same pattern as `Commands::Last`:

```rust
if let Some(Commands::Status) = cli.command {
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        match conn.send_query_tabs(pid, &profile) {
            Ok(status) => println!("{}", status),
            Err(e) => eprintln!("Error: {}", e),
        }
    } else {
        eprintln!("Not running inside TermSurf.");
    }
    return Ok(());
}
```

#### 2. TUI: add `send_query_tabs` function (`xpc.rs`)

Follow the `send_query_devtools` pattern — synchronous XPC round trip:

```rust
pub fn send_query_tabs(
    &self,
    pane_id: &str,
    profile: &str,
) -> Result<String, String>
```

Sends:

```
{
  action: "query_tabs",
  pane_id: "...",
  profile: "default"
}
```

Receives a reply with:

- `gui_panes` (int64) — number of GUI panes on this profile
- `chromium_tabs` (int64) — number of Chromium tabs
- `chromium_browser` (int64) — count of browser tabs (tab_id > 0)
- `chromium_devtools` (int64) — count of DevTools tabs (tab_id == 0)
- `tab_0`, `tab_1`, ... (strings) — per-tab summaries from Chromium

Formats the reply into the output string shown above.

#### 3. GUI: add `handleQueryTabs` handler (`xpc.zig`)

Register `"query_tabs"` in `handleMessage`. The handler:

1. Creates a reply via `xpc_dictionary_create_reply(msg)`.
2. Counts GUI panes for the requested profile by iterating `panes` and matching
   `p.server.profile_key`.
3. Forwards a synchronous `query_tabs` to the profile server via
   `xpc_connection_send_message_with_reply_sync(server.peer, ...)`.
4. Copies Chromium's reply fields (`chromium_tabs`, `chromium_browser`,
   `chromium_devtools`, `tab_0`, `tab_1`, ...) into the TUI reply.
5. Sets `gui_panes` on the reply.
6. Sends the reply back to the TUI.

The synchronous forward is safe because:

- The GUI's `xpc_queue` blocks waiting for Chromium's reply.
- Chromium processes the request on its own dispatch queue + UI thread.
- The reply returns directly to the blocked thread (XPC sync replies don't go
  through the event handler).

#### 4. Chromium: add `query_tabs` action handler (`shell_browser_main_parts.cc`)

In the control connection handler (`StartDynamicMode`), add:

```cpp
} else if (action && std::string_view(action) == "query_tabs") {
    xpc_object_t reply = xpc_dictionary_create_reply(event);
    if (reply) {
        content::GetUIThreadTaskRunner({})->PostTask(
            FROM_HERE,
            base::BindOnce(&ShellBrowserMainParts::HandleQueryTabs,
                           base::Unretained(self), reply));
    }
}
```

New method `HandleQueryTabs` on `ShellBrowserMainParts`:

```cpp
void ShellBrowserMainParts::HandleQueryTabs(xpc_object_t reply) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);

    int64_t total = static_cast<int64_t>(tabs_.size());
    int64_t browser_count = 0;
    int64_t devtools_count = 0;

    for (size_t i = 0; i < tabs_.size(); i++) {
        auto& tab = tabs_[i];
        if (tab->tab_id > 0) browser_count++;
        else devtools_count++;

        // Per-tab summary: "id=1 inspected=0 pane=abc-123 url=https://..."
        std::string url = tab->shell->web_contents()->GetURL().spec();
        std::string val = "id=" + std::to_string(tab->tab_id)
            + " inspected=" + std::to_string(tab->inspected_tab_id)
            + " pane=" + tab->pane_id
            + " url=" + url;
        std::string key = "tab_" + std::to_string(i);
        xpc_dictionary_set_string(reply, key.c_str(), val.c_str());
    }

    xpc_dictionary_set_int64(reply, "chromium_tabs", total);
    xpc_dictionary_set_int64(reply, "chromium_browser", browser_count);
    xpc_dictionary_set_int64(reply, "chromium_devtools", devtools_count);

    xpc_connection_send_message(control_connection_, reply);
    xpc_release(reply);
}
```

The reply is created on the XPC handler thread (where
`xpc_dictionary_create_reply` must be called) and populated + sent on the UI
thread. XPC supports sending replies from any thread.

Add declaration in `shell_browser_main_parts.h`:

```cpp
void HandleQueryTabs(xpc_object_t reply);
```

#### 5. Chromium branch

Per `/build-chromium`:

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
git checkout 146.0.7650.0-issue-684
git checkout -b 146.0.7650.0-issue-689
```

Build with `autoninja -C out/Default chromium_profile_server`. After
verification, generate patches and update `docs/chromium.md`.

### Result: SUCCESS

The `web status` command works. Three tests confirm the orphan leak:

**Test 1: DevTools orphans.** Opened one browser tab, then opened and closed
DevTools three times. `web status` showed 4 Chromium tabs (1 browser + 3
devtools) but only 2 GUI panes. Each DevTools close leaked an orphan.

```
Chromium tabs (profile: default):
  id=1 inspected=0 pane=C0C286D0-... url=https://ryanxcharles.com/
  id=0 inspected=1 pane=C06D2EC5-... url=http://127.0.0.1:.../devtools/...
  id=0 inspected=1 pane=C06D2EC5-... url=http://127.0.0.1:.../devtools/...
  id=0 inspected=1 pane=C06D2EC5-... url=http://127.0.0.1:.../devtools/...
  ---
  browser: 1  devtools: 3  total: 4

GUI panes: 2
```

**Test 2: Browser tab orphans.** Opened one browser tab, then opened and closed
additional browser tabs in the same pane (navigating away). `web status` showed
5 Chromium tabs but only 2 GUI panes. All the "intermediate" tabs leaked — same
pane ID, different tab IDs.

```
Chromium tabs (profile: default):
  id=1 inspected=0 pane=8A5A71D9-... url=https://ryanxcharles.com/
  id=2 inspected=0 pane=936A2645-... url=https://ryanxcharles.com/
  id=3 inspected=0 pane=936A2645-... url=https://ryanxcharles.com/
  id=4 inspected=0 pane=936A2645-... url=https://ryanxcharles.com/
  id=5 inspected=0 pane=936A2645-... url=https://ryanxcharles.com/
  ---
  browser: 5  devtools: 0  total: 5

GUI panes: 2
```

**Test 3: Last-tab cleanup.** When the last GUI pane closes, `killServer` kills
the entire profile server process, destroying all tabs (orphaned or not). After
reopening, `web status` showed a clean slate: 1 tab, 1 pane.

```
Chromium tabs (profile: default):
  id=1 inspected=0 pane=8A5A71D9-... url=https://ryanxcharles.com/
  ---
  browser: 1  devtools: 0  total: 1

GUI panes: 1
```

### Findings

1. **No tabs are ever closed.** Closing a GUI pane does not close the Chromium
   tab. Tabs accumulate for the lifetime of the profile server process.
2. **`killServer` masks the leak.** When the last pane on a profile closes, the
   entire server is killed, destroying all orphans. This is why single-tab
   workflows never notice the leak.
3. **Both browser and DevTools tabs leak.** The orphan problem is universal, not
   DevTools-specific. DevTools orphans are just more visible because they crash
   on reopen (duplicate `InspectorOverlayAgent`).
4. **The fix is Phase 2:** Send an explicit `close_tab` message from the GUI to
   Chromium when a pane is cleaned up. This is the same direction as Issue 688
   Experiment 3, but now we can verify the fix with `web status`.
