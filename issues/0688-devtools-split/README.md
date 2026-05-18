+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 688: DevTools Split Command

Add a `:devtools` command to the TUI command bar that opens DevTools in a new
split pane. Typing `:devtools right` in a browser pane creates a split to the
right and runs `web devtools` in it, automatically inspecting the current tab.

## Background

Issue 687 enforced one DevTools per tab and locked DevTools panes from
navigating. DevTools now works reliably, but opening it requires manually
creating a split pane and typing `web devtools`. This issue automates that
workflow into a single command.

The end goal is a keyboard shortcut like `Opt+Cmd+I` that means "devtools right"
— but the shortcut is a separate issue. This issue builds the underlying command
infrastructure.

## How It Works

1. User is browsing in a `web` pane (e.g., `web google.com`)
2. User presses `:` to enter Command mode
3. User types `devtools right` and presses Enter
4. The TUI sends an XPC message to the GUI: "create a split to the right of my
   pane, and run this command in it"
5. The GUI creates the split using existing Ghostty split infrastructure
6. The new pane runs `web devtools`, which auto-targets the browser tab from
   step 1

### Why the full executable path matters

The command sent to the new pane must use the full path of the currently running
`web` binary (`std::env::current_exe()`), not just `web`. In development, the
user may have multiple builds — a release `web` in `$PATH` and a debug `web` in
the cargo target directory. Running the wrong one leads to confusing version
mismatches. Using the exact path of the current process guarantees the same
binary runs in the new pane.

## Design

### New TUI command: `:devtools <direction>`

Extend the command dispatcher in `main.rs` (the `dispatch()` function) to
recognize `devtools` commands:

- `:devtools right` — open DevTools in a split to the right
- `:devtools down` — open DevTools in a split below
- `:devtools left` — open DevTools in a split to the left
- `:devtools up` — open DevTools in a split above
- `:devtools` (no direction) — default to `right`

The command returns a new `CommandResult` variant that carries the direction.

### New XPC message: `open_split`

The TUI sends a new XPC message to the GUI:

```
{
  action: "open_split",
  pane_id: "...",
  direction: "right",          // "right", "down", "left", "up"
  command: "/full/path/to/web devtools"
}
```

The GUI receives this, finds the surface for the pane, and triggers a split
using the existing `termsurf_surface_split` API with a custom command in the
`SurfaceConfiguration`.

### GUI handler: `handleOpenSplit`

The GUI handler needs to:

1. Look up the surface for `pane_id` (via `surface_to_pane` / `panes` map)
2. Map the direction string to `SplitDirection` (right=0, down=1, left=2, up=3)
3. Create a split on that surface with the given command

The existing split flow is:

```
termsurf_surface_split(surface, direction)
  → TermSurf.App.newSplit() posts notification with SurfaceConfiguration
  → BaseTerminalController.newSplit() creates new SurfaceView
```

The `SurfaceConfiguration` has a `command` field. The challenge is threading a
custom command through this flow — `termsurf_surface_split()` doesn't currently
accept a command parameter. Options:

1. **Use `initialInput`** — create a normal split, then "type" the command into
   it via the `initialInput` field on `SurfaceConfiguration`. This sends the
   command text as keyboard input after the shell starts. Simpler but depends on
   shell prompt timing.
2. **Use `command`** — set `SurfaceConfiguration.command` to the full command
   string. The new pane runs `web devtools` directly without a shell. Cleaner
   but requires modifying the split flow to accept a custom command.
3. **Use a new XPC-to-surface path** — bypass `termsurf_surface_split` and
   create the split directly from the XPC handler, posting the notification with
   a custom `SurfaceConfiguration`.

Option 3 is the most direct — the XPC handler already has access to the surface
and can post the same notification that `termsurf_surface_split` would, but with
a custom `SurfaceConfiguration` that includes the command.

### Getting the executable path

In `main.rs`, capture the current executable path early:

```rust
let current_exe = std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(String::from))
    .unwrap_or_else(|| "web".to_string());
```

When building the `open_split` command, construct: `"{current_exe} devtools"`

### Error cases

Two cases must be caught before sending the `open_split` XPC message:

1. **`:devtools` typed in a DevTools pane.** You can't open DevTools for
   DevTools. Checked locally — `is_devtools` is already a flag in the TUI. No
   XPC needed.
2. **`:devtools` typed in a browser tab that already has DevTools open.** The
   `query_devtools` message (Issue 687) already checks for duplicates. The TUI
   calls it before sending `open_split`. If it returns an error, the command bar
   shows the error instead of splitting.

Both cases are validated before any split is attempted.

### Command bar error display

When a command fails, the command bar turns red and shows an error message on a
footer line below the input. This is a general-purpose error mechanism for
command mode — any command can use it.

**Visual behavior:**

- The command bar border turns red (replacing the normal yellow)
- A single-line error message appears below the command input, inside the bar's
  bottom border area (e.g., `"Tab 4 already has DevTools open"`)
- The error persists until the user types another character or exits command
  mode (Esc)

**Implementation:**

Add a `CommandResult::Error(String)` variant to the `CommandResult` enum. When
`dispatch()` returns an error, the event loop stores the error message in a
`command_error: Option<String>` variable. The `ui()` function checks this
variable — if set, it renders the command bar with a red border and the error
text as a bottom title. Any subsequent keystroke in command mode clears the
error.

This pattern generalizes beyond DevTools — unrecognized commands, invalid
arguments, or any future command that can fail will use the same red-bar
mechanism.

## Relevant Code

- `tui/src/main.rs` — `dispatch()` function (command mode), `CommandResult` enum
- `tui/src/xpc.rs` — XPC message sending
- `gui/src/apprt/xpc.zig` — XPC message handling, `panes` map, `surface_to_pane`
  map
- `gui/src/apprt/embedded.zig` — `termsurf_surface_split`, C API exports
- `gui/src/apprt/action.zig` — `SplitDirection` enum
- `gui/macos/Sources/TermSurf/TermSurf.App.swift` — `newSplit()`, notification
  posting
- `gui/macos/Sources/Features/Terminal/BaseTerminalController.swift` —
  `newSplit()`, `SurfaceConfiguration`
- `gui/macos/Sources/TermSurf/Surface View/SurfaceView.swift` —
  `SurfaceConfiguration` struct with `command` and `initialInput` fields

## Experiment 1: End-to-end `:devtools` command

### Hypothesis

If the TUI parses `:devtools [direction]`, validates the request, sends an
`open_split` XPC message, and the GUI creates a split with `initialInput` set to
the `web devtools` command, then DevTools opens in a new split with one command.

### Changes

#### 1. TUI: `CommandResult::Error` and `CommandResult::DevTools` (`main.rs`)

Add two new variants to the `CommandResult` enum:

```rust
enum CommandResult {
    Quit,
    SetColorScheme(String),
    DevTools(String),   // direction: "right", "down", "left", "up"
    Error(String),      // error message to display
    None,
}
```

Add a `devtools` command to the `COMMANDS` array:

```rust
Command {
    name: "devtools",
    exec: |args| match args.first() {
        Some(&"right" | &"r") | None => CommandResult::DevTools("right".into()),
        Some(&"down" | &"d") => CommandResult::DevTools("down".into()),
        Some(&"left" | &"l") => CommandResult::DevTools("left".into()),
        Some(&"up" | &"u") => CommandResult::DevTools("up".into()),
        Some(other) => CommandResult::Error(
            format!("Unknown direction: {}", other),
        ),
    },
},
```

#### 2. TUI: Command error display (`main.rs`)

Add state variable and pass to `ui()`:

```rust
let mut command_error: Option<String> = None;
```

In the `Mode::Command` Enter handler, after calling `dispatch()`:

- `CommandResult::Error(msg)` → store in `command_error`, stay in Command mode
  (don't switch to Control)
- All other results → clear `command_error`, proceed as normal

In the `Mode::Command` key handler, clear the error on any keystroke that isn't
Enter (so the user sees the error until they start typing again).

Add `command_error: &Option<String>` parameter to `ui()`. In the command bar
rendering:

- If `command_error.is_some()`, use red border color instead of yellow
- Add `.title_bottom()` with the error text styled red

#### 3. TUI: DevTools validation and `open_split` (`main.rs`)

In the `Mode::Command` Enter handler, when `dispatch()` returns
`CommandResult::DevTools(direction)`:

1. **Check `is_devtools`.** If true, set `command_error` to
   `"Cannot open DevTools from a DevTools pane"` and stay in Command mode.
2. **Call `query_devtools`.** Send `query_devtools(pane_id, 0, &profile)` to
   check if the current tab already has DevTools. If it returns `Err(msg)`, set
   `command_error` to the error message and stay in Command mode.
3. **Send `open_split`.** If both checks pass, call
   `send_open_split(pane_id, &direction, &command_string)` where
   `command_string` is `"{current_exe} devtools"`.
4. Switch to Control mode.

Capture the executable path early in `main()`:

```rust
let current_exe = std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(String::from))
    .unwrap_or_else(|| "web".to_string());
```

#### 4. TUI: `send_open_split` function (`xpc.rs`)

Add a fire-and-forget XPC send function:

```rust
pub fn send_open_split(
    &self,
    pane_id: &str,
    direction: &str,
    command: &str,
)
```

Sends:

```
{
  action: "open_split",
  pane_id: "...",
  direction: "right",
  command: "/full/path/to/web devtools"
}
```

#### 5. GUI: `handleOpenSplit` handler (`xpc.zig`)

Register `"open_split"` in `handleMessage`. The handler:

1. Extract `pane_id`, `direction`, `command` from the XPC message.
2. Look up the pane in `panes`, get its `overlay_surface`.
3. Find the `Surface` (apprt surface) from the core surface. The existing
   pattern is `app.findSurfaceByPaneId(pane_id)` — use the same lookup.
4. Map direction string to `SplitDirection` enum (right=0, down=1, left=2,
   up=3).
5. Call `termsurf_surface_split_with_input(surface, direction, command)` — a new
   C API export.

#### 6. GUI: `termsurf_surface_split_with_input` C API (`embedded.zig`)

Add a new export that behaves like `termsurf_surface_split` but also stores a
pending `initialInput` string for the new surface:

```zig
export fn termsurf_surface_split_with_input(
    ptr: *Surface,
    direction: apprt.action.SplitDirection,
    input: [*:0]const u8,
) void
```

This function:

1. Stores the input string in a module-level `pending_initial_input` variable.
2. Calls `termsurf_surface_split(ptr, direction)` — the normal split path.
3. The Swift notification handler picks up the pending input.

#### 7. Swift: Read pending initial input (`TermSurf.App.swift`)

In the `newSplit` case of the action dispatcher (line 838–850), after creating
the `SurfaceConfiguration` from the inherited config:

1. Check if `termsurf_surface_get_pending_input()` returns a non-null string.
2. If so, set `config.initialInput` to that string + `"\n"` (the newline
   triggers execution).
3. Clear the pending input.

Add a new C export in `embedded.zig`:

```zig
export fn termsurf_surface_get_pending_input() ?[*:0]const u8
```

This returns and clears the pending input string. It's a one-shot: the first
call returns the string, subsequent calls return null until a new
`split_with_input` is called.

### Why `initialInput` over `command`

Using `initialInput` (typing into the shell) rather than `command` (replacing
the shell):

- The new pane has a real shell. If `web devtools` exits (user quits DevTools),
  the pane stays open with a shell prompt — the user can run another command.
- With `command`, the pane would close when `web devtools` exits (or show
  "Process exited" if `wait_after_command` is set). Less useful.
- `initialInput` is typed after the shell starts, so shell configuration
  (.zshrc, aliases, etc.) is fully loaded.

The timing concern (shell not ready when input arrives) is mitigated by
Ghostty's existing `initialInput` infrastructure — it buffers the input and
sends it after the PTY is ready.

### Test

1. Open a browser: `web google.com`
2. Press `:`, type `devtools right`, press Enter
3. A split should open to the right, running `web devtools`
4. The DevTools pane should auto-target the google.com tab
5. Press `:`, type `devtools right` again → red command bar:
   `"Tab N already has DevTools open"`
6. Close DevTools, try again → should work
7. In the DevTools pane, press `:`, type `devtools right` → red command bar:
   `"Cannot open DevTools from a DevTools pane"`
8. `:devtools down` → split below
9. `:devtools` (no direction) → defaults to right
10. `:devtools banana` → red command bar: `"Unknown direction: banana"`
11. Type any character after seeing error → error clears, bar returns to yellow

### Result: FAILURE

`:devtools right` works on the first invocation — the split opens,
`web devtools` runs, and DevTools auto-targets the browser tab correctly. The
command bar error display also works: `:devtools` in a DevTools pane shows the
red bar, `:devtools banana` shows the direction error, and errors clear on the
next keystroke.

The failure is in the close → reopen cycle. After closing the DevTools pane and
typing `:devtools left`, Chromium crashes with runaway audio (GPU process dies
mid-frame, audio buffers loop), requiring a force kill of the profile server.

**Root cause:** Closing a DevTools pane removes it from the GUI's `panes` map
(`cleanupPane` clears the overlay and deletes the pane entry), but does not tell
Chromium's profile server to close the DevTools tab. The orphaned DevTools
session stays alive inside Chromium, still attached to the browser tab's
renderer via its `InspectorOverlayAgent`.

When a new `:devtools` command runs, `query_devtools` checks the `panes` map for
duplicates — but the old pane was already removed, so no duplicate is detected.
The new `web devtools` creates a second DevTools tab for the same inspected tab.
Two `InspectorOverlayAgent` instances attach to one renderer, triggering the
same `PaintController` DCHECK crash from Issue 686.

This is the Issue 686 crash resurfacing through a code path that Issue 687's
validation cannot catch: the duplicate isn't visible in the `panes` map because
the tracking was cleaned up while the Chromium session persisted.

**What needs to happen:** `cleanupPane` must send a "close DevTools tab" message
to Chromium's profile server when a pane with `inspected_tab_id != 0` is
removed. The Chromium-side DevTools session must be fully torn down before a new
one can be created. This is a prerequisite for the `:devtools` command to work
reliably.

## Experiment 2: Close Chromium tab on pane cleanup

### Hypothesis

If the GUI retains the profile server's per-tab XPC connection on the Pane
struct, and cancels it during pane cleanup, then the profile server's existing
`CloseTab` error handler fires, properly destroying the DevTools tab (Shell,
WebContents, ShellDevToolsFrontend, InspectorOverlayAgent). Reopening DevTools
for the same browser tab should work without crashing.

### Background

There are two independent XPC connections per tab:

- **Connection A** (TUI ↔ GUI): The TUI creates this via the gateway. Stored as
  `web_peer` on the Pane struct. When the TUI exits, this drops, triggering
  `handleDisconnect` → `cleanupPane`.
- **Connection B** (Profile Server → GUI): The profile server creates this
  inside `CreateDevToolsTab` (and `CreateTab`) via
  `xpc_connection_create_from_endpoint(app_endpoint_)`. Stored as
  `tab_connection` in the profile server's `TabState`. Messages like
  `tab_ready`, `ca_context`, `loading_state` arrive on this connection.

When the TUI exits, only Connection A drops. Connection B stays alive because
nobody cancels it. The profile server has no idea the pane is gone — its
DevTools tab persists with a live `InspectorOverlayAgent` attached to the
inspected renderer. Opening a new DevTools creates a duplicate, crashing
Chromium.

The profile server already handles Connection B closure correctly — its XPC
error handler calls `CloseTab`, which destroys the Shell, cancels the
connection, and removes the tab from `tabs_`. The GUI just needs to cancel its
end of Connection B during pane cleanup.

Currently, `handleTabReady` does not store the server peer. The connection
reference is available via `xpc_dictionary_get_remote_connection(msg)` on any
message from the profile server (e.g., `tab_ready`), but it's never retained.

**This is not just a DevTools problem.** Every Chromium tab — browser and
DevTools alike — has the same two-connection architecture. When any `web` pane
closes, Connection A drops but Connection B survives. The Chromium tab persists
as an orphan inside the profile server: its Shell, WebContents, compositor, and
renderer all stay alive, consuming memory and GPU resources. This has been true
since tabs were introduced but was never noticed because orphaned browser tabs
don't conflict with each other — they just silently leak. The only reason it
surfaced now is that DevTools orphans crash when a second inspector attaches to
the same renderer.

The orphan problem is masked by `killServer`: when the last pane on a profile
closes, `handleDisconnect` kills the entire profile server process, which
destroys all tabs (orphaned or not). So if a user opens one tab, closes it, and
opens another, the server is killed and restarted — no orphan accumulates. But
if a user has multiple panes on the same profile, closing one pane leaks its
Chromium tab for the lifetime of the server.

This fix closes all Chromium tabs properly, not just DevTools tabs.

### Changes

#### 1. Pane struct: add `server_peer` field (`xpc.zig`)

Add a new field to the Pane struct:

```zig
server_peer: xpc_object_t = null, // Profile server's tab connection (Issue 688).
```

This holds a retained reference to Connection B.

#### 2. `handleTabReady`: retain and store server peer (`xpc.zig`)

After storing `tab_id`, retain the remote connection and store it on the pane:

```zig
fn handleTabReady(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");

    if (panes.get(pane_id)) |p| {
        p.tab_id = tab_id;
        if (tab_id > 0) {
            last_browser_pane = p.pane_id_key;
        }

        // Retain the profile server's tab connection for cleanup (Issue 688).
        const peer = xpc_dictionary_get_remote_connection(msg);
        if (peer != null and p.server_peer == null) {
            p.server_peer = xpc_retain(peer);
        }
    }

    log.info("tab_ready pane={s} tab_id={d}", .{ pane_id, tab_id });
}
```

The `p.server_peer == null` guard ensures we only retain once (the first
`tab_ready` message).

#### 3. `cleanupPane`: cancel server peer (`xpc.zig`)

After releasing `web_peer`, cancel and release `server_peer`:

```zig
fn cleanupPane(pane_id_key: []const u8) void {
    if (panes.get(pane_id_key)) |p| {
        if (p.overlay_surface) |surface| {
            surface.clearOverlay();
            _ = surface_to_pane.remove(@intFromPtr(surface));
        }
        if (p.web_peer) |peer| {
            _ = peer_to_pane.remove(@intFromPtr(peer));
            xpc_release(peer);
        }
        // Cancel profile server's tab connection (Issue 688).
        // Triggers CloseTab in the profile server via its error handler.
        if (p.server_peer) |peer| {
            xpc_connection_cancel(peer);
            xpc_release(peer);
        }
        _ = panes.remove(pane_id_key);
        alloc.destroy(p);
        alloc.free(pane_id_key);
    }
}
```

#### 4. `handleDisconnect`: cancel server peer (`xpc.zig`)

Add the same cancel/release in the web peer disconnect handler, inside the
`if (panes.get(pane_id_key))` block, before releasing `web_peer`:

```zig
// Cancel profile server's tab connection (Issue 688).
if (p.server_peer) |peer| {
    xpc_connection_cancel(peer);
    xpc_release(peer);
}
```

Also add it in the server peer disconnect handler's pane cleanup loop (the
`if (p.server == server)` block), before releasing `web_peer`.

### Test

1. Open a browser: `web google.com`
2. Press `:`, type `devtools right`, press Enter → split opens with DevTools
3. Close the DevTools pane (`:q` or close the split)
4. Press `:`, type `devtools left`, press Enter → should open DevTools again
   without crashing
5. Close and reopen multiple times → no crash, no orphaned tabs
6. Test with regular browser tabs too — close a `web` pane, verify the Chromium
   tab is cleaned up (check profile server logs for "Closing tab" messages)
7. All error cases from Experiment 1 still work (DevTools-in-DevTools, duplicate
   detection, invalid direction)

### Result: FAILURE

Closing the DevTools pane destroys the browser tab too. The main `web` pane
stays alive (the TUI is still running), but the Chromium content is gone. When
the user presses `:` or any key in the browser pane, the TUI redraws, sends
`set_overlay`, and the GUI creates a new tab from scratch — causing a full page
reload.

**Root cause:** `xpc_dictionary_get_remote_connection(msg)` in `handleTabReady`
returns the same connection object for both the browser tab's `tab_ready` and
the DevTools tab's `tab_ready`. This is most likely the profile server's control
connection — the single bidirectional channel between the GUI and the profile
server — not a per-tab connection. Both pane A (browser) and pane B (DevTools)
store the same connection as `server_peer`.

When pane B (DevTools) closes and its `server_peer` is cancelled, it cancels the
shared control connection. This severs ALL communication between the GUI and the
profile server. The profile server's error handler fires, destroying all tabs
(browser and DevTools alike). The browser pane's Chromium content vanishes.

The experiment's assumption was wrong: we assumed each tab has its own dedicated
XPC connection from the profile server back to the GUI (Connection B). In
reality, the profile server may send all tab events (`tab_ready`, `ca_context`,
`loading_state`, etc.) on a single shared connection, not on the per-tab
`tab_connection` created in `CreateTab`/`CreateDevToolsTab`.

**What needs to happen:** Before fixing pane cleanup, we need to understand
which connection `tab_ready` actually arrives on. If it's the control
connection, we cannot cancel it to close individual tabs — we need an explicit
`close_tab` XPC message instead. The profile server already has a `CloseTab`
function; it just needs a new action in its control handler that accepts a
`pane_id` and calls `CloseTab` for the matching tab.

## Experiment 3: Explicit `close_tab` XPC message

### Hypothesis

If the GUI sends an explicit `close_tab` message (with `pane_id`) on the profile
server's control connection during pane cleanup, the profile server can look up
and destroy the correct tab without affecting other tabs or the shared
connection. This avoids the Experiment 2 failure (cancelling the shared
connection) and the Experiment 1 failure (orphaned Chromium tabs).

### Background

Experiments 1 and 2 established:

1. **Orphaned tabs are universal.** Every Chromium tab leaks when its GUI pane
   closes while other panes on the same profile exist. The profile server never
   learns the pane is gone — Connection B (per-tab `tab_connection` created in
   `CreateTab`/`CreateDevToolsTab`) stays alive.
2. **Connection cancellation is too coarse.** Experiment 2 tried cancelling what
   `xpc_dictionary_get_remote_connection(msg)` returned in `handleTabReady`, but
   that returns the shared control connection. Cancelling it kills all tabs.
3. **The control connection is the right channel.** The GUI already sends
   `create_tab`, `resize`, `navigate`, `key_event`, etc. on `server.peer` (the
   control connection). A `close_tab` message belongs here too.
4. **`pane_id` is the right key.** Both sides already track it — the GUI stores
   it in the `panes` map, and the profile server stores it in
   `TabState.pane_id`. No new identifiers needed.
5. **`CloseTab` already works.** The profile server's existing `CloseTab`
   function properly destroys the Shell, cancels the per-tab `tab_connection`,
   and removes the tab from `tabs_`. We just need a new entry point that finds
   the tab by `pane_id` instead of by connection pointer.

### Changes

#### 1. Revert Experiment 2's `server_peer` approach (`xpc.zig`)

Remove the `server_peer` field from the Pane struct and all code that retains,
cancels, or releases it (in `handleTabReady`, `cleanupPane`, and
`handleDisconnect`). The connection cancellation approach doesn't work.

#### 2. Profile server: add `close_tab` action handler (Chromium patch)

In the control connection's message handler (the `StartDynamicMode` handler in
`shell_browser_main_parts.cc`), add a new action:

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

#### 3. Profile server: add `CloseTabByPaneId` method (Chromium patch)

New method on `ShellBrowserMainParts`:

```cpp
void ShellBrowserMainParts::CloseTabByPaneId(const std::string& pane_id) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);

    for (auto it = tabs_.begin(); it != tabs_.end(); ++it) {
        if ((*it)->pane_id == pane_id) {
            LOG(INFO) << "[ProfileServer] Closing tab for pane " << pane_id
                      << ", " << (tabs_.size() - 1) << " tab(s) remaining";
            (*it)->shell->Close();
            if ((*it)->tab_connection) {
                xpc_connection_cancel((*it)->tab_connection);
                xpc_release((*it)->tab_connection);
            }
            tabs_.erase(it);
            return;
        }
    }

    LOG(WARNING) << "[ProfileServer] close_tab: no tab found for pane "
                 << pane_id;
}
```

This mirrors the existing `CloseTab(xpc_connection_t conn)` but matches by
`pane_id` instead of connection pointer.

#### 4. GUI: send `close_tab` in `cleanupPane` (`xpc.zig`)

In `cleanupPane`, after releasing `web_peer` and before removing the pane from
the map, send a fire-and-forget `close_tab` message on the profile server's
control connection:

```zig
// Tell the profile server to close this tab (Issue 688).
if (p.server) |server| {
    if (server.peer) |peer| {
        const msg = xpc_dictionary_create(null, null, 0);
        defer xpc_release(msg);
        xpc_dictionary_set_string(msg, "action", "close_tab");
        xpc_dictionary_set_string(msg, "pane_id", pane_id_key.ptr);
        xpc_connection_send_message(peer, msg);
    }
}
```

This goes in `cleanupPane` only — not in `handleDisconnect`'s server-peer
disconnect path (when the server itself dies, all tabs are already gone).

### Why this works

- **No connection cancellation.** The control connection stays alive. Other tabs
  are unaffected.
- **Matches by `pane_id`.** Each tab has a unique `pane_id` stored in
  `TabState`. The profile server finds exactly the right tab.
- **Fires before pane removal.** `cleanupPane` sends the message while
  `p.server` is still valid, then proceeds to remove the pane from the map.
- **Idempotent.** If the tab was already closed (server died, profile killed),
  the message arrives on a dead connection or finds no matching tab — no crash.
- **Fixes the universal leak.** Every pane cleanup (browser and DevTools) now
  tells the profile server to close the associated tab.

### Test

1. Open a browser: `web google.com`
2. `:devtools right` → split opens with DevTools
3. Close the DevTools pane (`:q`)
4. Check profile server logs — should show "Closing tab for pane ..."
5. `:devtools left` → DevTools reopens without crash
6. Close and reopen 5 times → stable, no orphaned tabs
7. Open two browser panes on the same profile (`web a.com`, `web b.com`)
8. Close one pane → only that tab closes in the profile server, the other
   continues working
9. All error cases from Experiment 1 still work
10. Existing `killServer` still works when the last pane on a profile closes

### Result: FAILURE

Crashed on first invocation of `:devtools right` — before any close/reopen
cycle. All profile server processes were killed before testing. The crash is not
from orphaned tabs or stale servers.

The approach of sending `close_tab` on the control connection is sound in
theory, but something in the end-to-end flow — split creation, DevTools tab
setup, or XPC message routing — is broken in a way that three experiments have
failed to isolate.

All code changes (Zig, Swift, Chromium) reverted. The Chromium
`146.0.7650.0-issue-688` branch was deleted.

## Conclusion

Three experiments, three failures:

1. **Experiment 1** built the `:devtools` command end-to-end. It worked on first
   invocation but crashed on close → reopen because the GUI never told Chromium
   to close the DevTools tab — the orphaned `InspectorOverlayAgent` caused a
   duplicate-inspector crash.
2. **Experiment 2** tried cancelling the profile server's XPC connection per-tab
   during pane cleanup. It cancelled the shared control connection instead,
   destroying all tabs on the profile.
3. **Experiment 3** added an explicit `close_tab` XPC message on the control
   connection with `CloseTabByPaneId` on the Chromium side. Crashed on first
   invocation — the root cause was never identified.

The `:devtools <direction>` command is blocked by a more fundamental problem:
**we cannot reliably close individual Chromium tabs when panes are removed.**
This affects all tabs, not just DevTools — every closed pane leaks its Chromium
tab until the profile server process is killed (which only happens when the last
pane on a profile closes).

**Next step:** Before attempting the split command again, we need a dedicated
issue to solve tab lifecycle management: reliably closing individual Chromium
tabs (both browser and DevTools) when their GUI panes are removed. Once that
works, the `:devtools` command from Experiment 1 can be re-implemented on top of
it.
