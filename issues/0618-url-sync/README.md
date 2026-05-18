+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 618: URL sync

## Goal

When the browser navigates to a new page (link click, JavaScript redirect,
back/forward), update the URL bar in the `web` TUI to reflect the current URL.

## Background

The URL currently flows in one direction: the TUI sends the initial URL to the
GUI via `set_overlay`, the GUI forwards it to the Chromium server via
`create_tab`, and Chromium loads the page. After that initial load, the URL bar
is static — it always shows whatever the user typed on the command line.

The TUI already handles incoming `url_changed` messages (`tui/src/xpc.rs:190`,
`tui/src/main.rs:170`). The `UrlChanged` variant in `CompositorMessage` updates
the `url` variable and triggers a redraw. This was built during Issue 616 in
anticipation of this feature.

What's missing is the other end of the pipeline:

1. **Chromium server** — Does not notify the GUI when the URL changes after a
   navigation. The server needs to detect URL changes (via
   `DidNavigateMainFrame` or equivalent Content API callback) and send a message
   to the GUI.
2. **GUI (xpc.zig)** — Does not forward URL change messages from the Chromium
   server to the TUI. The GUI needs to receive the URL change from the server
   and relay it to the correct TUI pane.

### Current message flow

```
TUI → GUI → Chromium    (set_overlay with URL, create_tab with URL)
Chromium → GUI           (loading_state, iosurface frames)
GUI → TUI                (mode_changed, loading_state)
```

### Desired message flow

```
Chromium → GUI → TUI     (url_changed when navigation occurs)
```

## Architecture

Three changes are needed across three processes:

1. **Chromium server** (`chromium/src/`): Send a `url_changed` XPC message to
   the GUI whenever the main frame navigates to a new URL. The Content API
   provides `WebContentsObserver::DidFinishNavigation` for this.
2. **GUI** (`gui/src/apprt/xpc.zig`): Handle the incoming `url_changed` message
   from the Chromium server, look up which TUI pane owns the tab, and forward
   the message to that pane's XPC connection.
3. **TUI** (`tui/src/`): Already implemented — `UrlChanged` message handler
   updates the URL bar and redraws.

## Chromium branch

`146.0.7650.0-issue-616` — the `url_changed` message is already sent from
`ShellVideoConsumer::DidFinishNavigation()` in this branch. No new Chromium
branch is needed for this issue.

## Experiments

### Experiment 1: Forward url_changed from Chromium to TUI

The Chromium server already sends `url_changed` XPC messages from
`ShellVideoConsumer::DidFinishNavigation()` (`shell_video_consumer.cc:90-143`).
The TUI already handles them (`xpc.rs:182-190`, `main.rs:170-171`). The only
missing piece is the GUI relay in `xpc.zig`.

#### Changes

**`gui/src/apprt/xpc.zig`:**

1. Add dispatch in `handleMessage()` (after the `loading_state` branch at line
   258-259):

   ```zig
   } else if (std.mem.eql(u8, action_str, "url_changed")) {
       handleUrlChanged(msg);
   ```

2. Add `handleUrlChanged()` function (after `handleLoadingState` at line 477),
   following the same forwarding pattern:

   ```zig
   fn handleUrlChanged(msg: xpc_object_t) void {
       const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
       const p = panes.get(pane_id) orelse return;
       if (p.web_peer == null) return;

       const url = xpc_dictionary_get_string(msg, "url") orelse return;

       const fwd = xpc_dictionary_create(null, null, 0);
       xpc_dictionary_set_string(fwd, "action", "url_changed");
       xpc_dictionary_set_string(fwd, "url", url);
       xpc_connection_send_message(p.web_peer, fwd);
   }
   ```

No Chromium or TUI changes needed — both ends already exist.

#### Verification

1. Launch TermSurf, run `web google.com`
2. Click a link on the page (e.g., a search result)
3. The URL bar in the TUI should update to the new page's URL
4. Press Cmd+[ (back) — URL bar should revert to the previous URL
5. Press Cmd+] (forward) — URL bar should show the navigated URL again

**Result:** Pass

The URL bar updates on every navigation — link clicks, back/forward, and
redirects.

## Conclusion

The URL sync pipeline was already 2/3 complete. The Chromium server sent
`url_changed` (Issue 616) and the TUI handled it. The only missing piece was an
8-line relay function in `gui/src/apprt/xpc.zig` to forward the message from the
server to the TUI. One dispatch line + one function, following the existing
`handleLoadingState` pattern.
