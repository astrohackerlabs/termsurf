+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

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

## Experiment 2: Close tabs on pane cleanup

### Hypothesis

If the GUI sends a `close_tab` XPC message (with `pane_id`) to the Chromium
profile server whenever a pane is cleaned up, and Chromium finds and destroys
the matching tab using the same `Shell::Close()` path that `CloseTab` already
uses, then `web status` will show tab counts matching pane counts after
open/close cycles.

### Background

Research into Chromium's tab closure confirms `Shell::Close()` is the correct
Content Shell API. Our existing `CloseTab(xpc_connection_t conn)` already calls
it. The method works — it's just never triggered because pane cleanup in the GUI
never signals Chromium. This is the same direction as Issue 688 Experiment 3,
which crashed for unknown reasons. The difference now: we have `web status` to
verify tab counts at every step.

### Design

#### Data flow

```
GUI pane closes → cleanupPane() / handleDisconnect()
                     ↓
          send close_tab { pane_id } on server.peer (control connection)
                     ↓
Chromium: StartDynamicMode handler → PostTask(CloseTabByPaneId)
                     ↓
          CloseTabByPaneId: find tab by pane_id → Shell::Close()
                           cancel + release tab_connection
                           erase from tabs_
                           if empty → Shell::Shutdown()
```

### Changes

#### 1. Chromium: add `CloseTabByPaneId` method (`shell_browser_main_parts.cc`)

New method that mirrors `CloseTab` but matches by `pane_id` instead of
connection pointer:

```cpp
void ShellBrowserMainParts::CloseTabByPaneId(const std::string& pane_id) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);
  for (auto it = tabs_.begin(); it != tabs_.end(); ++it) {
    if ((*it)->pane_id == pane_id) {
      LOG(INFO) << "[ProfileServer] CloseTabByPaneId pane=" << pane_id
                << ", " << (tabs_.size() - 1) << " tab(s) remaining";
      (*it)->tab_observer.reset();
      (*it)->shell->Close();
      if ((*it)->tab_connection) {
        xpc_connection_cancel((*it)->tab_connection);
        xpc_release((*it)->tab_connection);
      }
      tabs_.erase(it);
      if (tabs_.empty()) {
        LOG(INFO) << "[ProfileServer] No tabs remaining, exiting";
        Shell::Shutdown();
      }
      return;
    }
  }
  LOG(WARNING) << "[ProfileServer] CloseTabByPaneId: no tab for pane="
               << pane_id;
}
```

#### 2. Chromium: add `close_tab` action handler (`shell_browser_main_parts.cc`)

In `StartDynamicMode`, after the `query_tabs` handler:

```cpp
} else if (action && std::string_view(action) == "close_tab") {
    const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
    std::string pane_id(pane_id_str ? pane_id_str : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::CloseTabByPaneId,
                       base::Unretained(self), pane_id));
}
```

#### 3. Chromium: declare `CloseTabByPaneId` (`shell_browser_main_parts.h`)

```cpp
void CloseTabByPaneId(const std::string& pane_id);
```

#### 4. GUI: send `close_tab` in `handleDisconnect` (`xpc.zig`)

In the web peer disconnect path (line 1787), before decrementing
`server.pane_count`, send the close message. Only send if the server is alive
and the pane had a tab:

```zig
// Close the Chromium tab (Issue 689).
if (p.tab_sent) {
    if (server.peer != null) {
        const close_msg = xpc_dictionary_create(null, null, 0);
        defer xpc_release(close_msg);
        xpc_dictionary_set_string(close_msg, "action", "close_tab");
        var pane_z: [37]u8 = undefined;
        if (pane_id_key.len > 0 and pane_id_key.len <= 36) {
            @memcpy(pane_z[0..pane_id_key.len], pane_id_key);
            pane_z[pane_id_key.len] = 0;
            xpc_dictionary_set_string(close_msg, "pane_id", @ptrCast(&pane_z));
            xpc_connection_send_message(server.peer, close_msg);
            log.info("sent close_tab pane={s}", .{pane_id_key});
        }
    }
}
```

Insert this at line 1787 (inside `if (p.server) |server|`), before the
`server.pane_count -= 1` line. This covers the normal disconnect path.

`cleanupPane` does not need a separate close_tab because `cleanupPane` is only
called from `deinit` (global shutdown) where the entire server is being killed
anyway.

### Result: FAILURE

Closing one pane kills all tabs. The entire Chromium profile server exits when a
single tab is closed, destroying every tab on that profile.

**Root cause:** `Shell::Close()` cascades to `Shell::Shutdown()`. Content Shell
tracks all Shells in a global `windows_` vector. When a Shell is destroyed, its
destructor removes it from `windows_`. If `windows_` becomes empty, the platform
delegate calls `DidCloseLastWindow()` → `Shell::Shutdown()`, which closes ALL
remaining Shells and quits the run loop, killing the process.

In theory, closing one Shell out of N should leave N-1 in `windows_`, so
`DidCloseLastWindow` shouldn't fire. But the macOS path goes through
`performClose:` → `windowShouldClose:` → `delete shell`, which interacts with
the run loop. The destruction chain has side effects we don't fully control —
`Shell::Close()` was designed for Content Shell's single-window-per-tab model
where each Shell owns an NSWindow, not for our profile server where Shells share
a headless process.

**The deeper problem:** We're calling `Shell::Close()` — the standard Content
Shell close API — but Content Shell's lifecycle model doesn't match ours. In
standard Content Shell, closing the last window exits the process. In our
profile server, we want to close individual tabs without affecting others.
`Shell::Close()` is the wrong abstraction for us because it triggers
window-level lifecycle hooks (`performClose:`, `DidCloseLastWindow`) that assume
"one Shell = one window = one application instance."

**What we need instead:** A way to destroy the WebContents and its compositor
without going through Shell's window lifecycle. The tab's resources are: Shell
(WebContents owner), ShellTabObserver, per-tab compositor
(AcceleratedWidgetMac + Compositor + Layer + Bridge), and the per-tab XPC
connection. We need to tear these down individually without calling
`Shell::Close()`.

**Code reverted** — both Chromium and GUI changes from this experiment.

## Experiment 3: Defuse DidCloseLastWindow cascade

### Hypothesis

Experiment 2 failed because `Shell::Close()` cascades to `Shell::Shutdown()`
through Content Shell's window lifecycle. The exact chain:

```
Shell::Close()
  → performClose:nil                        (macOS window lifecycle)
    → windowShouldClose: → delete shell     (synchronous)
      → ~Shell()
        → remove from windows_
        → web_contents_.reset()
        → if windows_.empty()
          → DidCloseLastWindow()
            → Shell::Shutdown()             ← KILLS ALL SHELLS + QUITS
```

Our fork's `DidCloseLastWindow()` (in `shell_platform_delegate.cc:22`) calls
`Shell::Shutdown()` — the base class default. There is no macOS-specific
override. This is the root cause: when the last Shell in the global `windows_`
vector is destroyed, the platform delegate shuts down the entire process.

But we already have our OWN shutdown trigger:
`if (tabs_.empty()) Shell::Shutdown()` in `CloseTab` and `CloseTabByPaneId`.
This is the correct trigger — it fires when our `tabs_` vector (which we
control) is empty, not when Content Shell's `windows_` vector (which the Shell
destructor controls) is empty.

**The fix:** Make `DidCloseLastWindow()` a no-op. Remove `Shell::Shutdown()`
from it. Our explicit `tabs_.empty()` checks become the sole shutdown path. Then
re-apply Experiment 2's `close_tab` logic (which was correct except for the
cascade).

If this works, `web status` will show tab counts matching pane counts after
open/close cycles, and closing one tab will not affect others.

### Design

Three changes: one in Chromium's platform delegate (the root fix), two
re-applied from Experiment 2 (close_tab handler + GUI disconnect signal).

#### 1. Chromium: make `DidCloseLastWindow` a no-op (`shell_platform_delegate.cc`)

Current code (line 22–24):

```cpp
void ShellPlatformDelegate::DidCloseLastWindow() {
  Shell::Shutdown();
}
```

Change to:

```cpp
void ShellPlatformDelegate::DidCloseLastWindow() {
  // No-op. Our ShellBrowserMainParts::CloseTab / CloseTabByPaneId
  // call Shell::Shutdown() explicitly when tabs_ is empty.
  // The default DidCloseLastWindow → Shutdown cascade would kill
  // all tabs when closing any single tab (Issue 689 Experiment 2).
}
```

This is the root fix. Without it, `Shell::Close()` cascades.

#### 2. Chromium: re-add `CloseTabByPaneId` + `close_tab` handler

Same as Experiment 2. New method in `shell_browser_main_parts.cc` after
`HandleQueryTabs` (line ~1250):

```cpp
void ShellBrowserMainParts::CloseTabByPaneId(const std::string& pane_id) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);
  for (auto it = tabs_.begin(); it != tabs_.end(); ++it) {
    if ((*it)->pane_id == pane_id) {
      LOG(INFO) << "[ProfileServer] CloseTabByPaneId pane=" << pane_id
                << ", " << (tabs_.size() - 1) << " tab(s) remaining";
      (*it)->tab_observer.reset();
      (*it)->shell->Close();
      if ((*it)->tab_connection) {
        xpc_connection_cancel((*it)->tab_connection);
        xpc_release((*it)->tab_connection);
      }
      tabs_.erase(it);
      if (tabs_.empty()) {
        LOG(INFO) << "[ProfileServer] No tabs remaining, exiting";
        Shell::Shutdown();
      }
      return;
    }
  }
  LOG(WARNING) << "[ProfileServer] CloseTabByPaneId: no tab for pane="
               << pane_id;
}
```

New action handler in `StartDynamicMode`, after `query_tabs` (line ~342):

```cpp
} else if (action && std::string_view(action) == "close_tab") {
    const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
    std::string pane_id(pane_id_str ? pane_id_str : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::CloseTabByPaneId,
                       base::Unretained(self), pane_id));
}
```

Declaration in `shell_browser_main_parts.h`:

```cpp
void CloseTabByPaneId(const std::string& pane_id);
```

#### 3. GUI: re-add `close_tab` in `handleDisconnect` (`xpc.zig`)

Same as Experiment 2. Insert at line 1787 (inside `if (p.server) |server|`),
before `server.pane_count -= 1`:

```zig
// Close the Chromium tab (Issue 689).
if (p.tab_sent and server.peer != null) {
    const close_msg = xpc_dictionary_create(null, null, 0);
    defer xpc_release(close_msg);
    xpc_dictionary_set_string(close_msg, "action", "close_tab");
    var pane_z: [37]u8 = undefined;
    if (pane_id_key.len > 0 and pane_id_key.len <= 36) {
        @memcpy(pane_z[0..pane_id_key.len], pane_id_key);
        pane_z[pane_id_key.len] = 0;
        xpc_dictionary_set_string(close_msg, "pane_id", @ptrCast(&pane_z));
        xpc_connection_send_message(server.peer, close_msg);
        log.info("sent close_tab pane={s}", .{pane_id_key});
    }
}
```

### Verification

1. Build both: `cd gui && zig build`, then
   `cd chromium/src && autoninja -C out/Default chromium_profile_server`.
2. Open TermSurf, `web google.com`. Run `web status` — expect 1 tab, 1 pane.
3. Open a second browser tab (`web ryanxcharles.com`). `web status` — 2 tabs, 2
   panes.
4. Close the second pane (`:q`). `web status` — expect 1 tab, 1 pane. The closed
   tab should be gone, and the first tab should be unaffected.
5. Open DevTools (`web devtools`). `web status` — 2 tabs (1 browser + 1
   devtools), 2 panes.
6. Close DevTools pane. `web status` — expect 1 tab, 1 pane.
7. Reopen DevTools. Should work without crash (no duplicate
   `InspectorOverlayAgent`).
8. Resize the terminal window. Should not crash (the original Issue 686
   trigger).

### What Experiment 2 lacked

The only difference between this experiment and Experiment 2 is step 1 — making
`DidCloseLastWindow` a no-op. The `CloseTabByPaneId` code and the GUI disconnect
signal are identical. Experiment 2's code was correct; it just couldn't survive
Shell::Close() triggering the cascade.

### Result: FAILURE

Opened one tab (count=1), opened a second (count=2), closed one pane — count
dropped to 0. Closing one tab still kills all tabs, even with
`DidCloseLastWindow` as a no-op.

**Root cause:** The cascade does not come from `DidCloseLastWindow`. Making it a
no-op had no effect. `Shell::Close()` kills the entire profile server through a
different path.

The cascade must originate from inside the macOS window lifecycle that
`Shell::Close()` triggers. The chain is:

```
Shell::Close()
  → g_platform->DestroyShell(this) returns true
    → [window performClose:nil]
      → windowShouldClose: → _shell.ClearAndDelete() → delete shell
        → ~Shell()
          → g_platform->CleanUp(this)     (erases from shell_data_map_)
          → remove from windows_
          → web_contents_.reset()          ← destroys WebContents
          → if windows_.empty()
            → DidCloseLastWindow()         (now no-op — doesn't matter)
```

With two shells, closing one leaves one in `windows_`. `DidCloseLastWindow`
never fires. Yet the profile server still dies. This means the crash/cascade
happens BEFORE the empty check — somewhere in the destruction of the first
Shell's resources:

1. **`performClose:nil` on hidden borderless NSWindows.** Content Shell creates
   real NSWindows (even though ours are hidden with alpha=0). `performClose:nil`
   triggers macOS window lifecycle hooks that may behave unexpectedly for
   borderless windows without close buttons.
2. **`web_contents_.reset()` observer chain.** Destroying a WebContents fires
   `WebContentsDestroyed()` on all observers. If any observer (DevToolsAgent,
   BrowserContext, RenderProcessHost) has a side effect that cascades to other
   Shells, the entire process could crash or shut down.
3. **Process crash.** The profile server may simply crash during Shell
   destruction. A crash kills all tabs because the process dies. The LOG
   statements in `CloseTabByPaneId` may not flush before the crash.

**Conclusion:** `Shell::Close()` is fundamentally unsafe for our use case.
Defusing `DidCloseLastWindow` was necessary but not sufficient — the cascade has
a deeper source inside Shell's destruction. We cannot use `Shell::Close()` at
all.

**Next direction:** Bypass `Shell::Close()` entirely. Instead of going through
Content Shell's window lifecycle, directly tear down the tab's resources:

1. Reset the ShellTabObserver (stop observing)
2. Detach WebContents from Shell (need new Shell method or `delete shell` with
   the macOS window lifecycle bypassed)
3. Destroy the per-tab compositor (AcceleratedWidgetMac, Compositor, Layer,
   Bridge)
4. Cancel and release the per-tab XPC connection
5. Erase from `tabs_`

The key question for the next experiment: can we call `delete shell` directly
(bypassing `performClose:nil`) without crashing? The Shell destructor would
still run (CleanUp, remove from windows*, web_contents*.reset()), but the macOS
window lifecycle is skipped. If that path is stable, the fix is to replace
`shell->Close()` with `delete shell`.

**Code reverted** — all three files (Chromium platform delegate, Chromium main
parts, GUI xpc.zig).

## Experiment 4: Orphan the Shell (don't close it)

### Hypothesis

Three experiments have called `Shell::Close()` and all cascaded. But we don't
know WHERE in the destruction chain the cascade originates. This experiment
isolates the question by skipping Shell destruction entirely.

If we remove `shell->Close()` from `CloseTabByPaneId` and just orphan the Shell
(leave it alive in `windows_` with its WebContents intact), then closing one tab
should NOT cascade. The orphaned Shell leaks its resources (NSWindow,
WebContents, renderer), but the remaining tabs survive.

- **If this works** (count goes from 2 to 1): the cascade is confirmed to be
  inside `Shell::Close()` / Shell destruction. The next step is to narrow down
  whether the crash is in `performClose:nil`, the Shell destructor, or
  `web_contents_.reset()`.
- **If this fails** (count drops to 0): the cascade is NOT in Shell destruction.
  Something else kills the profile server — the XPC message itself, a GUI-side
  bug, or an observer triggered by erasing from `tabs_`.

### Design

Three changes: Chromium close_tab handler (without Shell::Close()), Chromium
CloseTabByPaneId (orphans instead of closes), GUI disconnect signal.

#### 1. Chromium: add `CloseTabByPaneId` — orphan only (`shell_browser_main_parts.cc`)

Same as Experiments 2/3 but with `shell->Close()` removed. The Shell stays alive
in `windows_`:

```cpp
void ShellBrowserMainParts::CloseTabByPaneId(const std::string& pane_id) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);
  for (auto it = tabs_.begin(); it != tabs_.end(); ++it) {
    if ((*it)->pane_id == pane_id) {
      LOG(INFO) << "[ProfileServer] CloseTabByPaneId (orphan) pane=" << pane_id
                << ", " << (tabs_.size() - 1) << " tab(s) remaining";
      (*it)->tab_observer.reset();
      // NOTE: Intentionally NOT calling shell->Close().
      // The Shell and its WebContents leak. This is a diagnostic
      // experiment to confirm the cascade is inside Shell destruction.
      if ((*it)->tab_connection) {
        xpc_connection_cancel((*it)->tab_connection);
        xpc_release((*it)->tab_connection);
      }
      tabs_.erase(it);
      if (tabs_.empty()) {
        LOG(INFO) << "[ProfileServer] No tabs remaining, exiting";
        Shell::Shutdown();
      }
      return;
    }
  }
  LOG(WARNING) << "[ProfileServer] CloseTabByPaneId: no tab for pane="
               << pane_id;
}
```

The Shell's raw*ptr in TabState is not freed. The Shell stays in `windows*`as an
orphan.`web*contents*` is intact. The NSWindow stays alive (hidden). This leaks,
but it's a diagnostic experiment — we need to know if the cascade is inside
Shell destruction.

#### 2. Chromium: add `close_tab` action handler (`shell_browser_main_parts.cc`)

Same as Experiment 2/3 — unchanged:

```cpp
} else if (action && std::string_view(action) == "close_tab") {
    const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
    std::string pane_id(pane_id_str ? pane_id_str : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::CloseTabByPaneId,
                       base::Unretained(self), pane_id));
}
```

#### 3. Chromium: declare in header (`shell_browser_main_parts.h`)

```cpp
void CloseTabByPaneId(const std::string& pane_id);
```

#### 4. GUI: send `close_tab` in `handleDisconnect` (`xpc.zig`)

Same as Experiment 2/3 — unchanged.

### Verification

1. Build Chromium and GUI.
2. Open one tab. `web status` — expect 1 tab, 1 pane.
3. Open a second tab. `web status` — expect 2 tabs, 2 panes.
4. Close one pane. `web status` — expect **2 tabs** (orphan stays), **1 pane**.
   The key test: the remaining tab and pane must still be alive.
5. If step 4 shows 1 tab, 1 pane — even better, means `tabs_` cleanup worked and
   Shell didn't cascade.
6. If step 4 shows 0 — the cascade is NOT in Shell destruction. Need to
   investigate the XPC message path and GUI-side cleanup.

### Why not `DidCloseLastWindow` no-op?

Not needed for this experiment. We never call `Shell::Close()`, so no Shell
destructor runs, so `DidCloseLastWindow` never fires. If this experiment
succeeds and we later add proper Shell destruction, we'll re-add the no-op.

### Result: FAILURE

Opened one tab (count=1), opened a second (count=2), closed one pane — count
dropped to 0. Closing one tab still kills all tabs, even though we never called
`Shell::Close()`.

**Root cause:** The cascade is NOT inside `Shell::Close()`. We never called it.
We never destroyed the Shell, never destroyed its WebContents, never touched the
NSWindow. The Shell was intentionally orphaned — left alive and leaking. Yet the
entire profile server still died.

The only operations `CloseTabByPaneId` performed were:

1. `(*it)->tab_observer.reset()` — destroy the ShellTabObserver
2. `xpc_connection_cancel((*it)->tab_connection)` — cancel the per-tab XPC
   connection
3. `tabs_.erase(it)` — erase the TabState from the vector

Erasing the TabState triggers its destructor, which destroys:

- `std::unique_ptr<PersistentCompositorBridge> bridge`
- `std::unique_ptr<ui::Layer> root_layer`
- `std::unique_ptr<ui::Compositor> compositor`
- `std::unique_ptr<ui::AcceleratedWidgetMac> widget_mac`

The most likely crash point is **compositor destruction**. The Shell's
`RenderWidgetHostView` is still alive and still connected to the compositor
pipeline. Destroying the compositor (AcceleratedWidgetMac + Compositor + Layer +
Bridge) while the rendering pipeline is active likely triggers a crash — a
dangling pointer, a DCHECK, or a use-after-free in the GPU process. The crash
kills the entire profile server, taking all tabs with it.

This reframes the entire problem. The cascade was never in `Shell::Close()` (Exp
2/3) or `DidCloseLastWindow()` (Exp 3). It's in the **TabState destructor** —
specifically, the compositor teardown. The per-tab compositor is tightly coupled
to the Shell's rendering pipeline and cannot be destroyed while the Shell is
alive.

**Correct teardown order:**

1. Destroy the Shell first (which destroys WebContents, which disconnects the
   RenderWidgetHostView from the compositor pipeline)
2. THEN destroy the compositor
3. THEN clean up XPC connections

But `Shell::Close()` also cascades (Exp 2/3). So the full fix requires:

1. Make `DidCloseLastWindow` a no-op (Exp 3 — still needed)
2. Call `Shell::Close()` or `delete shell` to destroy WebContents first
3. THEN let TabState destructor clean up the compositor
4. Handle the fact that the Shell may not synchronously delete (macOS window
   lifecycle is async)

The next experiment should try: keep the DidCloseLastWindow no-op from Exp 3,
call `shell->Close()`, and then DEFER the TabState erasure (via PostTask) to
give the Shell destruction time to complete before the compositor is torn down.
Alternatively: `delete shell` directly (bypass `performClose:nil`), which is
synchronous, then immediately erase TabState.

**Code reverted** — Chromium shell_browser_main_parts.cc/.h and GUI xpc.zig.

## Experiment 5: Log-only close_tab handler

### Hypothesis

Experiments 2–4 all crashed the profile server. Each experiment did different
things inside `CloseTabByPaneId`, yet all failed identically. We assumed the
crash was in our teardown code, but we haven't verified that the crash is even
inside `CloseTabByPaneId`. It could be in the XPC message delivery, the GUI-side
cleanup, or somewhere else entirely.

This experiment does the absolute minimum: receive the `close_tab` message, log
it, and return. No observer reset, no XPC cancel, no TabState erasure. Nothing
is destroyed. If the remaining tab survives, the crash is confirmed to be in our
teardown code. If it still dies, the crash is outside `CloseTabByPaneId` — in
the GUI, the XPC transport, or somewhere we haven't considered.

### Design

Two changes: Chromium log-only handler, GUI disconnect signal.

#### 1. Chromium: add log-only `CloseTabByPaneId` (`shell_browser_main_parts.cc`)

```cpp
void ShellBrowserMainParts::CloseTabByPaneId(const std::string& pane_id) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);
  LOG(INFO) << "[ProfileServer] CloseTabByPaneId (log-only) pane=" << pane_id
            << ", tabs=" << tabs_.size();
  // Intentionally do nothing. Diagnostic experiment to confirm
  // the crash is inside our teardown code, not in message delivery.
}
```

No observer reset. No XPC cancel. No tabs*.erase(). The tab stays fully alive in
every data structure — `tabs*`, `windows\_`, compositor pipeline. It becomes a
confirmed orphan, same as the pre-Issue-689 behavior but with a log message.

#### 2. Chromium: add `close_tab` action handler (`shell_browser_main_parts.cc`)

Same as Experiments 2–4:

```cpp
} else if (action && std::string_view(action) == "close_tab") {
    const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
    std::string pane_id(pane_id_str ? pane_id_str : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::CloseTabByPaneId,
                       base::Unretained(self), pane_id));
}
```

#### 3. Chromium: declare in header (`shell_browser_main_parts.h`)

```cpp
void CloseTabByPaneId(const std::string& pane_id);
```

#### 4. GUI: send `close_tab` in `handleDisconnect` (`xpc.zig`)

Same as Experiments 2–4.

### Verification

1. Build Chromium and GUI.
2. Open one tab. `web status` — expect 1 tab, 1 pane.
3. Open a second tab. `web status` — expect 2 tabs, 2 panes.
4. Close one pane. `web status` — expect **2 tabs** (orphan, same as before
   Issue 689), **1 pane**.
5. **If 2 tabs, 1 pane:** crash is in our teardown code. All prior experiments
   (2–4) crashed because of what they did INSIDE CloseTabByPaneId, not because
   of the message itself. The next experiment can try the correct teardown
   order.
6. **If 0:** the crash is outside CloseTabByPaneId. Investigate: does the GUI's
   `handleDisconnect` kill the server? Does `xpc_connection_send_message` on
   `server.peer` have a side effect? Does the pane_count decrement path have a
   bug?

### Result: SUCCESS

No crash. The remaining tab survived. `web status` showed 2 Chromium tabs, 1 GUI
pane — exactly the expected orphan state.

Log confirms the message was received and processed:

```
CloseTabByPaneId (log-only) pane=B33107D4-..., tabs=2
query_tabs: 2 tabs (2 browser, 0 devtools)
```

**Conclusion:** The crash in Experiments 2–4 was inside our teardown code, not
in the XPC message delivery or GUI-side cleanup. The profile server can receive
`close_tab` and survive — as long as `CloseTabByPaneId` doesn't destroy
anything.

The three operations that Exp 2–4 performed (and that crashed) were:

1. `tab_observer.reset()` — destroy ShellTabObserver
2. `xpc_connection_cancel(tab_connection)` — cancel per-tab XPC
3. `tabs_.erase(it)` — destroy TabState (compositor, bridge, layer, widget)

The next experiment should identify which operation crashes, and test the
correct teardown order: destroy the Shell/WebContents first (to disconnect the
rendering pipeline from the compositor), then destroy the TabState.

## Experiment 6: Correct teardown order

### Hypothesis

Experiments 2–4 crashed because they destroyed TabState resources (compositor,
bridge, layer) while the Shell's RenderWidgetHostView was still connected to
them. The compositor pipeline expects its layers and compositor to outlive the
RWHV, or at least to be disconnected before destruction.

The correct teardown order is:

1. Destroy the Shell (which destroys WebContents → RenderWidgetHostView,
   disconnecting the rendering pipeline from the compositor)
2. THEN destroy the TabState (compositor, bridge, layer, widget — now safe
   because nothing references them)
3. Clean up XPC connections

But `Shell::Close()` goes through `performClose:nil` (macOS window lifecycle),
which may be async or have side effects. `delete shell` bypasses the macOS
lifecycle and runs the destructor synchronously:

```
delete shell
  → ~Shell()
    → g_platform->CleanUp(this)     (erases from shell_data_map_)
    → remove from windows_
    → web_contents_.reset()          (destroys WebContents → RWHV)
    → if windows_.empty()
      → DidCloseLastWindow()         → Shell::Shutdown() ← MUST BE NO-OP
```

This requires the `DidCloseLastWindow` no-op from Experiment 3. Without it,
deleting the last Shell would call `Shutdown()` and kill the process before our
`tabs_.empty()` check runs.

### Design

Three changes in Chromium, one in GUI (same as Exp 2–4).

#### 1. Chromium: make `DidCloseLastWindow` a no-op (`shell_platform_delegate.cc`)

Same as Experiment 3:

```cpp
void ShellPlatformDelegate::DidCloseLastWindow() {
  // No-op (Issue 689). Our CloseTabByPaneId calls Shell::Shutdown()
  // explicitly when tabs_ is empty.
}
```

#### 2. Chromium: `CloseTabByPaneId` with correct order (`shell_browser_main_parts.cc`)

Key difference from Experiments 2–4: `delete shell` BEFORE `tabs_.erase()`.

```cpp
void ShellBrowserMainParts::CloseTabByPaneId(const std::string& pane_id) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);
  for (auto it = tabs_.begin(); it != tabs_.end(); ++it) {
    if ((*it)->pane_id == pane_id) {
      LOG(INFO) << "[ProfileServer] CloseTabByPaneId pane=" << pane_id
                << ", " << (tabs_.size() - 1) << " tab(s) remaining";

      // Step 1: Disconnect rendering pipeline.
      // Reset observer first (stops WebContents callbacks).
      (*it)->tab_observer.reset();
      // Delete the Shell synchronously. This destroys WebContents,
      // which destroys RenderWidgetHostView, disconnecting it from
      // our compositor. Bypasses performClose:nil (macOS window
      // lifecycle) — the destructor runs immediately.
      delete (*it)->shell.get();
      (*it)->shell = nullptr;

      // Step 2: Cancel per-tab XPC connection.
      if ((*it)->tab_connection) {
        xpc_connection_cancel((*it)->tab_connection);
        xpc_release((*it)->tab_connection);
        (*it)->tab_connection = nullptr;
      }

      // Step 3: Erase TabState. Compositor, bridge, layer, widget
      // are now safe to destroy — nothing references them.
      tabs_.erase(it);

      if (tabs_.empty()) {
        LOG(INFO) << "[ProfileServer] No tabs remaining, exiting";
        Shell::Shutdown();
      }
      return;
    }
  }
  LOG(WARNING) << "[ProfileServer] CloseTabByPaneId: no tab for pane="
               << pane_id;
}
```

#### 3. Chromium: `close_tab` action handler + header declaration

Same as Experiments 2–5 (unchanged).

#### 4. GUI: send `close_tab` in `handleDisconnect` (`xpc.zig`)

Same as Experiments 2–5 (unchanged). Already present from Experiment 5.

### Verification

1. Clear the log: `> ~/.local/state/termsurf/chromium-server.log`
2. Build Chromium and GUI.
3. Open one tab. `web status` — 1 tab, 1 pane.
4. Open a second tab. `web status` — 2 tabs, 2 panes.
5. Close one pane. `web status` — expect **1 tab, 1 pane**. The closed tab
   should be gone, the remaining tab should be alive.
6. Check the log for `CloseTabByPaneId pane=... 1 tab(s) remaining` — confirms
   the Shell was deleted and the TabState was erased without crashing.
7. Open DevTools (`web devtools`). `web status` — 2 tabs, 2 panes.
8. Close DevTools pane. `web status` — 1 tab, 1 pane.
9. Reopen DevTools — should work without crash (no duplicate
   InspectorOverlayAgent).

### What's different from prior experiments

| Experiment | Shell destruction  | TabState destruction    | DidCloseLastWindow |
| ---------- | ------------------ | ----------------------- | ------------------ |
| 2          | `shell->Close()`   | `tabs_.erase` after     | Default (Shutdown) |
| 3          | `shell->Close()`   | `tabs_.erase` after     | No-op              |
| 4          | None (orphan)      | `tabs_.erase`           | Default            |
| 5          | None               | None (log only)         | Default            |
| **6**      | **`delete shell`** | **`tabs_.erase` after** | **No-op**          |

Experiment 6 combines the DidCloseLastWindow no-op (Exp 3) with synchronous
Shell deletion (`delete` instead of `Close()`), and ensures the Shell is
destroyed BEFORE the TabState. This is the first experiment to get the teardown
order right.

### Result: SUCCESS

Closing one pane correctly closes its Chromium tab without affecting other tabs.

```
web status  →  1 tab, 1 pane     (opened first tab)
web status  →  2 tabs, 2 panes   (opened second tab)
web status  →  1 tab, 1 pane     (closed first tab — tab id=1 gone, id=2 alive)
```

The surviving tab (id=2) continued working normally. The profile server stayed
alive with 1 tab remaining.

Two things made this work:

1. **DidCloseLastWindow no-op** — prevents `Shell::Shutdown()` from killing all
   tabs when any Shell destructor runs and `windows_` happens to be empty.
2. **`delete shell` before `tabs_.erase()`** — destroys WebContents → RWHV
   first, disconnecting the rendering pipeline from the compositor, so the
   TabState destructor can safely tear down compositor resources.

## Conclusion

**Closed.** Tab lifecycle works. DevTools close/reopen confirmed — no crash.

### What we did

When a GUI pane closes, the TUI's XPC connection drops. The GUI's
`handleDisconnect` already cleaned up the pane overlay, focus state, and maps —
but it never told the Chromium profile server that the tab was gone. The tab
(Shell, WebContents, compositor, renderer process) stayed alive forever, leaking
memory and GPU resources. For DevTools tabs, the leak was visible: reopening
DevTools attached a second `InspectorOverlayAgent` to the same renderer, and on
resize, the duplicate triggered a `PaintController` DCHECK crash (Issue 686).

The fix has three parts across two codebases:

#### 1. GUI sends `close_tab` when a pane disconnects (`xpc.zig`)

In `handleDisconnect`, after the existing pane cleanup but before decrementing
the pane count, we send a `close_tab` XPC message to the profile server with the
pane's UUID:

```zig
// Close the Chromium tab (Issue 689 Exp 5).
if (p.server) |server| {
    if (p.tab_sent and server.peer != null) {
        const close_msg = xpc_dictionary_create(null, null, 0);
        defer xpc_release(close_msg);
        xpc_dictionary_set_string(close_msg, "action", "close_tab");
        // ... set pane_id, send message
    }
}
```

This is the signal the profile server was missing.

#### 2. Profile server tears down the tab in the correct order (`shell_browser_main_parts.cc`)

`CloseTabByPaneId` finds the tab by pane UUID and destroys it in a specific
order that took four failed experiments to discover:

```cpp
// 1. Reset observer (stops WebContents navigation callbacks).
(*it)->tab_observer.reset();
// 2. Delete Shell FIRST — destroys WebContents → RenderWidgetHostView,
//    disconnecting the rendering pipeline from the compositor.
delete (*it)->shell.get();
(*it)->shell = nullptr;
// 3. Cancel per-tab XPC connection.
xpc_connection_cancel((*it)->tab_connection);
// 4. Erase TabState LAST — compositor, bridge, layer, widget are now
//    safe to destroy because nothing references them.
tabs_.erase(it);
```

**The order is critical.** The Shell owns the WebContents, which owns the
RenderWidgetHostView (RWHV), which is connected to the compositor pipeline
(AcceleratedWidgetMac → Compositor → Layer → PersistentCompositorBridge). If you
destroy the TabState first (which owns the compositor resources), the RWHV tries
to use a destroyed compositor and the process crashes. Destroying the Shell
first disconnects the RWHV from the pipeline, making it safe to tear down the
compositor afterward.

When the last tab is erased, we explicitly call `Shell::Shutdown()` to exit the
profile server cleanly.

#### 3. `DidCloseLastWindow` becomes a no-op (`shell_platform_delegate.cc`)

Chromium's Shell destructor checks if `windows_` is empty after removing itself,
and if so, calls `DidCloseLastWindow()`. Our fork's base implementation calls
`Shell::Shutdown()`, which kills every remaining Shell — a cascade that destroys
all tabs when any single tab is closed.

We make `DidCloseLastWindow` a no-op because our `CloseTabByPaneId` handles the
"last tab" case explicitly. This prevents the cascade while still allowing clean
shutdown.

### How we got here

Six experiments, each isolating one variable:

| Exp | What it tried                     | Result  | What we learned                                     |
| --- | --------------------------------- | ------- | --------------------------------------------------- |
| 1   | `web status` command              | SUCCESS | Diagnostic tool works                               |
| 2   | `shell->Close()` + `tabs_.erase`  | FAILURE | Cascade kills all tabs                              |
| 3   | No-op DidCloseLastWindow + Close  | FAILURE | Cascade has a deeper source than DidCloseLastWindow |
| 4   | Orphan Shell, just erase TabState | FAILURE | Crash is in TabState destructor, not Shell          |
| 5   | Log only, no teardown             | SUCCESS | Proves crash is inside our teardown code            |
| 6   | `delete shell` THEN `tabs_.erase` | SUCCESS | Correct teardown order solves both problems         |

The key insight came from Experiment 4: even without touching the Shell at all,
just erasing the TabState crashed the process. That proved the problem was never
in `Shell::Close()` — it was in the TabState destructor destroying compositor
resources while the RWHV was still connected to them. Experiment 5 confirmed
this by doing nothing at all (just logging) and surviving. Experiment 6 applied
the fix: destroy the Shell first to disconnect the RWHV, then safely destroy the
TabState.

### Files changed

**Chromium** (branch `146.0.7650.0-issue-689-exp3`):

- `content/chromium_profile_server/browser/shell_platform_delegate.cc` —
  `DidCloseLastWindow` no-op
- `content/chromium_profile_server/browser/shell_browser_main_parts.cc` —
  `CloseTabByPaneId` implementation + `close_tab` action handler
- `content/chromium_profile_server/browser/shell_browser_main_parts.h` —
  `CloseTabByPaneId` declaration

**GUI**:

- `gui/src/apprt/xpc.zig` — send `close_tab` in `handleDisconnect`
