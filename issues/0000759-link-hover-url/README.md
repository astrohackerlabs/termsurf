+++
status = "open"
opened = "2026-03-17"
+++

# Issue 759: Show link URL on hover

## Goal

When the user hovers over a link in the browser overlay, the TUI displays the
destination URL. When the mouse leaves the link, the URL disappears.

## Background

### The problem

Currently, hovering over a link gives the user no indication of where it leads.
In a normal browser, the destination URL appears in a status bar at the bottom
of the window. TermSurf has no equivalent.

### How Chromium handles it

Chromium has a well-defined pipeline for link hover URLs:

1. **Blink** detects the hover via hit testing in
   `ChromeClientImpl::ShowMouseOverURL()`. It extracts the URL from
   `HitTestResult::AbsoluteLinkURL()`.

2. **WebViewImpl** rate-limits updates using an async request-reply pattern. It
   won't send the next update until the browser process ACKs the previous one.
   This prevents flooding the IPC channel when the user moves the mouse quickly
   across many links.

3. **Mojo IPC** carries the URL from renderer to browser via
   `LocalMainFrameHost::UpdateTargetURL(url) => ()`.

4. **RenderFrameHostImpl** receives the message, notifies `WebContentsImpl`,
   which routes to `WebContentsDelegate::UpdateTargetURL(source, url)`.

5. **The embedder** decides what to do. Chrome sends it to `StatusBubble` (the
   translucent overlay in the bottom-left corner). Content Shell does nothing —
   it doesn't override `UpdateTargetURL()`.

### What we need

Three changes across three layers:

1. **Chromium (`shell.cc`)** — Override `UpdateTargetURL()` on the Shell class.
   Send a new protobuf message through the existing IPC socket to the GUI.

2. **Protocol (`termsurf.proto`)** — Add a `TargetUrlChanged` message. Follows
   the same pattern as `UrlChanged`, `TitleChanged`, etc. Sent from Chromium to
   the GUI, and from the GUI (via the browser socket) to the TUI.

3. **TUI (`webtui`)** — Receive the message and display the URL in the browser
   chrome. The URL bar area or a dedicated status line at the bottom are natural
   places.

### Message design

```protobuf
message TargetUrlChanged {
  int64 tab_id = 1;
  string url = 2;    // empty string = mouse left the link
}
```

An empty `url` means the hover ended. The TUI clears the display.

This follows the existing pattern: `UrlChanged` for the page URL, `TitleChanged`
for the page title, `TargetUrlChanged` for the link hover URL. All three are
Chromium → GUI → TUI state messages with the same shape.

### Message flow

```
Blink (hover detected)
  → Mojo IPC → RenderFrameHostImpl → WebContentsImpl
  → Shell::UpdateTargetURL(source, url)
  → protobuf TargetUrlChanged → Unix socket → GUI (Wezboard)
  → protobuf TargetUrlChanged → browser socket → TUI (webtui)
  → TUI displays URL in status area
```

### Rate limiting

Blink already rate-limits hover updates (one in-flight at a time, queues the
next). We don't need additional throttling — the messages arrive at a reasonable
rate.

## Experiments

### Experiment 1: Wire target URL from Chromium to TUI viewport border

#### Description

Add the full pipeline: Chromium callback → C library → Roamium → protobuf → TUI
display. The TUI shows the hover URL in the bottom-left corner of the viewport
border, using ratatui's `title_bottom()` — the same mechanism already used for
the engine label (bottom-right) and command error text.

#### Chromium branch

Create `146.0.7650.0-issue-759` from `146.0.7650.0-issue-750` (the most recent
branch). Add it to the Branches table in `chromium/README.md`.

#### Changes

**1. Chromium: `content/shell/browser/shell.h`**

Add `UpdateTargetURL` override (~line 170, near `NavigationStateChanged`):

```cpp
void UpdateTargetURL(WebContents* source, const GURL& url) override;
```

**2. Chromium: `content/shell/browser/shell.cc`**

Add the override (~after `NavigationStateChanged`, line 673):

```cpp
void Shell::UpdateTargetURL(WebContents* source, const GURL& url) {
  TsNotifyTargetUrlChanged(source, url.spec().c_str());
}
```

This passes the `WebContents*` as the handle (same pattern as
`TsTabObserver::DidFinishNavigation` which passes `handle_`). An empty URL
string means the hover ended.

**3. Chromium: `content/libtermsurf_chromium/ts_browser_main_parts.h`**

Add the notification function declaration (~line 40, after
`TsNotifyCursorChanged`):

```cpp
void TsNotifyTargetUrlChanged(void* wc_handle, const char* url);
```

**4. Chromium: `content/libtermsurf_chromium/libtermsurf_chromium.h`**

Add the callback registration function (~line 169, after
`ts_set_on_cursor_changed`):

```c
TS_EXPORT void ts_set_on_target_url_changed(
    void (*cb)(ts_web_contents_t wc, const char* url, void* user_data),
    void* user_data);
```

Same signature as `ts_set_on_url_changed` — the callback receives the
WebContents handle and a URL string.

**5. Chromium: `content/libtermsurf_chromium/libtermsurf_chromium.cc`**

Add global callback state (~line 48, after cursor changed):

```cpp
// Target URL changed callback.
void (*g_on_target_url_changed)(ts_web_contents_t, const char*, void*) = nullptr;
void* g_on_target_url_changed_data = nullptr;
```

Add the notify function (~line 93, after `TsNotifyCursorChanged`):

```cpp
void TsNotifyTargetUrlChanged(void* wc_handle, const char* url) {
  if (g_on_target_url_changed)
    g_on_target_url_changed(wc_handle, url, g_on_target_url_changed_data);
}
```

Add the C API registration (~line 263, after `ts_set_on_cursor_changed`):

```cpp
void ts_set_on_target_url_changed(
    void (*cb)(ts_web_contents_t wc, const char* url, void* user_data),
    void* user_data) {
  g_on_target_url_changed = cb;
  g_on_target_url_changed_data = user_data;
}
```

**6. Protocol: `proto/termsurf.proto`**

Add the message definition (~line 183, after `CursorChanged`):

```protobuf
message TargetUrlChanged {
  int64 tab_id = 1;
  string url = 2;    // empty = hover ended
}
```

Add it to the `TermSurfMessage` oneof (~line 18, after `cursor_changed`):

```protobuf
TargetUrlChanged target_url_changed = 32;
```

**7. Roamium: `roamium/src/ffi.rs`**

Add the FFI declaration (~line 119, after `ts_set_on_cursor_changed`):

```rust
pub fn ts_set_on_target_url_changed(
    cb: Option<unsafe extern "C" fn(TsWebContents, *const c_char, *mut c_void)>,
    user_data: *mut c_void,
);
```

**8. Roamium: `roamium/src/dispatch.rs`**

Add the callback handler (~after `on_cursor_changed`):

```rust
pub unsafe extern "C" fn on_target_url_changed(
    wc: TsWebContents,
    url: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let url_str = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    let msg = TermSurfMessage {
        msg: Some(Msg::TargetUrlChanged(proto::termsurf::TargetUrlChanged {
            tab_id: t.tab_id,
            url: url_str,
        })),
    };
    crate::ipc::send(&msg);
}
```

**9. Roamium: `roamium/src/main.rs`**

Register the callback (~line 89, after `ts_set_on_cursor_changed`):

```rust
ffi::ts_set_on_target_url_changed(Some(dispatch::on_target_url_changed), ptr::null_mut());
```

**10. TUI: `webtui/src/ipc.rs`**

Add to `CompositorMessage` enum (~line 29, after `TitleChanged`):

```rust
TargetUrlChanged { url: String },
```

Add dispatch case in `dispatch_message` (~line 419, after `TitleChanged` case):

```rust
Some(Msg::TargetUrlChanged(m)) => {
    if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
        return;
    }
    let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::TargetUrlChanged {
        url: m.url.clone(),
    }));
}
```

Uses the same tab_id filter as UrlChanged/TitleChanged/LoadingState (Issue 758).

**11. TUI: `webtui/src/main.rs`**

Add state variable (~line 390, near `page_title`):

```rust
let mut target_url = String::new();
```

Add IPC handler (~line 709, after `TitleChanged` handler):

```rust
ipc::CompositorMessage::TargetUrlChanged { url: new_target } => {
    target_url = new_target;
}
```

Pass `target_url` to `ui()` function — add parameter `target_url: &str` to the
function signature (~line 810).

In the viewport block construction (~line 951), add a `title_bottom()` with left
alignment for the target URL (only when non-empty):

```rust
let mut viewport_block = Block::default()
    .borders(Borders::ALL)
    .title(viewport_title)
    .title_top(profile_title.alignment(Alignment::Right))
    .title_bottom(engine_label.alignment(Alignment::Right))
    .border_style(Style::default().fg(viewport_border).bg(BG))
    .title_style(Style::default().fg(viewport_border))
    .style(Style::default().bg(BG));
if !target_url.is_empty() {
    let hover_label = Line::from(
        Span::raw(target_url).style(Style::default().fg(DIM)),
    );
    viewport_block = viewport_block.title_bottom(hover_label);
}
```

The left-aligned `title_bottom()` sits at the bottom-left of the viewport
border. The right-aligned engine label stays at the bottom-right. Both can
coexist because ratatui supports multiple `title_bottom()` calls with different
alignments.

#### Verification

```bash
scripts/build.sh chromium
scripts/build.sh roamium
cd webtui && cargo build
scripts/build.sh wezboard
```

| #   | Test                     | Steps                           | Expected                                          |
| --- | ------------------------ | ------------------------------- | ------------------------------------------------- |
| 1   | Hover shows URL          | Hover mouse over a link         | URL appears in viewport bottom-left border        |
| 2   | Leave link clears URL    | Move mouse off the link         | Bottom-left text disappears                       |
| 3   | Different links update   | Move mouse across several links | URL updates to each link's destination            |
| 4   | Engine label still works | Check viewport bottom-right     | Engine name still shows at bottom-right           |
| 5   | No bleed across TUIs     | Two TUIs, hover in one          | Only that TUI shows the hover URL (tab_id filter) |
