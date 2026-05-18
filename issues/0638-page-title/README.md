+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

# Issue 638: Page Title in Viewport

## Goal

Display the current page title in the viewport border. When no title is
available, show "Viewport" as the default.

## Background

The viewport block in the `web` TUI (`tui/src/main.rs`) currently has a
hardcoded title of " Viewport ". The browser knows the page title — Chromium
fires `WebContentsObserver::TitleWasSet` whenever it changes — but this
information is never sent to the TUI.

The infrastructure for sending per-pane notifications from Chromium to the TUI
already exists and was used for `url_changed` (Issue 618) and `loading_state`
(Issue 616). The pattern is:

1. `ShellTabObserver` (Chromium) observes the `WebContents` event
2. Sends an XPC message to the GUI app via the tab connection
3. GUI forwards the message to the `web` TUI via the pane's `web_peer`
4. TUI receives the message and updates its state

## Current state

- **Viewport title**: Hardcoded `" Viewport "` in `ui()` function
  (`tui/src/main.rs:350`)
- **ShellTabObserver**: Implements `DidFinishNavigation`, `DidStartLoading`,
  `DidStopLoading`, `LoadProgressChanged`, `DidFailLoad`. Does NOT implement
  `TitleWasSet`.
- **GUI XPC handler**: Forwards `url_changed` and `loading_state` from Chromium
  to TUI. No `title_changed` handler.
- **TUI XPC client**: Receives `UrlChanged` and `LoadingState` messages. No
  `TitleChanged` message.

## XPC messages

```
Chromium server → GUI:
{ action: "title_changed", pane_id: "<uuid>", title: "<page title>" }

GUI → TUI:
{ action: "title_changed", pane_id: "<uuid>", title: "<page title>" }
```

Same shape on both hops, identical to how `url_changed` and `loading_state`
work.

## Experiment 1: Title sync via TitleWasSet

### Hypothesis

Implementing `TitleWasSet` in `ShellTabObserver`, forwarding through the GUI,
and displaying in the TUI viewport border will show the page title in real time.

### Changes

#### 1. Chromium server: `TitleWasSet` (`shell_tab_observer.cc/.h`)

Add `TitleWasSet` override to `ShellTabObserver`. Chromium calls this whenever
the page title changes (initial load, SPA navigation, `document.title =`).

In the header, add to the `WebContentsObserver:` section:

```cpp
void TitleWasSet(NavigationEntry* entry) override;
```

In the `.cc`, add the implementation (follows the `DidFinishNavigation` pattern
exactly):

```cpp
void ShellTabObserver::TitleWasSet(NavigationEntry* entry) {
#if BUILDFLAG(IS_MAC)
  if (!xpc_connection_ || !entry)
    return;

  std::string title = base::UTF16ToUTF8(entry->GetTitleForDisplay());

  LOG(INFO) << "[ShellTabObserver] TitleWasSet pane=" << pane_id_
            << " title=" << title;

  xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
  xpc_dictionary_set_string(msg, "action", "title_changed");
  xpc_dictionary_set_string(msg, "pane_id", pane_id_.c_str());
  xpc_dictionary_set_string(msg, "title", title.c_str());
  xpc_connection_send_message(xpc_connection_, msg);
  xpc_release(msg);
#endif
}
```

Add the include for `NavigationEntry`:

```cpp
#include "content/public/browser/navigation_entry.h"
```

#### 2. GUI: forward `title_changed` (`gui/src/apprt/xpc.zig`)

Add to `handleMessage` dispatch:

```zig
} else if (std.mem.eql(u8, action_str, "title_changed")) {
    handleTitleChanged(msg);
}
```

Implement `handleTitleChanged` (same pattern as `handleUrlChanged`):

```zig
fn handleTitleChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const p = panes.get(pane_id) orelse return;
    if (p.web_peer == null) return;

    const title = xpc_dictionary_get_string(msg, "title") orelse return;

    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "title_changed");
    xpc_dictionary_set_string(fwd, "title", title);
    xpc_connection_send_message(p.web_peer, fwd);
}
```

#### 3. TUI: receive `TitleChanged` (`tui/src/xpc.rs`)

Add a new variant to `CompositorMessage`:

```rust
pub enum CompositorMessage {
    ModeChanged { browsing: bool },
    UrlChanged { url: String },
    LoadingState { state: String, _progress: u8 },
    TitleChanged { title: String },
}
```

Add parsing in the event handler (after the `loading_state` block):

```rust
} else if action == "title_changed" {
    let title_key = CString::new("title").unwrap();
    let title_ptr = unsafe { xpc_dictionary_get_string(event, title_key.as_ptr()) };
    if !title_ptr.is_null() {
        let title = unsafe { std::ffi::CStr::from_ptr(title_ptr) }
            .to_str()
            .unwrap_or("")
            .to_string();
        let _ = tx.send(CompositorMessage::TitleChanged { title });
    }
}
```

#### 4. TUI: display title in viewport (`tui/src/main.rs`)

Add a `page_title: String` variable in `main()`, initialized to empty:

```rust
let mut page_title = String::new();
```

Handle the new message in the compositor drain loop:

```rust
xpc::CompositorMessage::TitleChanged { title } => {
    page_title = title;
}
```

Pass `page_title` to `ui()` and use it in the viewport block title:

```rust
let viewport_title = if page_title.is_empty() {
    " Viewport ".to_string()
} else {
    format!(" {} ", page_title)
};
let viewport_block = Block::default()
    .borders(Borders::ALL)
    .title(viewport_title)
    .border_style(Style::default().fg(viewport_border).bg(BG))
    .title_style(Style::default().fg(viewport_border))
    .style(Style::default().bg(BG));
```

### Verification

1. `cd tui && cargo build` — TUI compiles
2. Build Chromium (`autoninja -C out/Default chromium_profile_server`)
3. Build GUI (`cd gui && zig build`)
4. Launch TermSurf, `web google.com`
5. Viewport border shows "Google" after page loads
6. Click a link — title updates to the new page's title
7. Edit URL bar → Enter to navigate — title updates
8. SPA navigation (e.g. Gmail) — title updates without full page reload

### Success criteria

- Viewport title shows "Viewport" before any title is received
- Viewport title updates to the page title after load
- Title updates on navigation (link click, URL bar, back/forward)
- Long titles are truncated by ratatui's block rendering (no crash)

### Result: Success

All success criteria pass. The page title flows from Chromium's `TitleWasSet`
through XPC to the TUI viewport border.

## Conclusion

Issue 638 is closed. The viewport border displays the current page title,
falling back to "Viewport" when no title is available.
