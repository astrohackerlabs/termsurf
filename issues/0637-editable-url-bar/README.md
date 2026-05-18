+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

# Issue 637: Editable URL Bar

## Goal

Make the URL bar in the `web` TUI editable using edtui, a Vim-inspired text
editor widget for ratatui. Users should be able to edit the URL with full Vim
keybindings and press Enter to navigate.

## Background

The URL bar (`tui/src/main.rs`) is currently a read-only `Paragraph` widget that
displays the current URL. It updates via `CompositorMessage::UrlChanged` from
the compositor but cannot be edited by the user. There is no way to navigate to
a new URL by typing — the only navigation is via links clicked in the browser
pane.

[edtui](https://github.com/preiter93/edtui) is a Vim-inspired text editor widget
for ratatui. It provides full Vim keybindings (normal, insert, visual modes),
horizontal scrolling for long lines, undo/redo, cursor management, and a
customizable key handler. A local copy lives at `vendor/edtui/`.

## Current state

- **URL bar**: `Paragraph` widget in `layout[0]`, displays `url: String`
- **Modes**: `Browse` and `Control` (enum at line 24)
- **Mode switching**: `Esc` exits Browse, `Enter` enters Browse
- **Key dispatch**: lines 136–155, per-mode match
- **Compositor sync**: `send_set_overlay()` sends URL, `UrlChanged` receives it
- **No text input state**: no cursor, no edit buffer, no text input crate

## Mode design

This is the hardest part. The TUI currently has two modes. Adding an editable
URL bar introduces a third mode that itself contains Vim sub-modes. The
transitions must feel natural to a Vim user.

### Current modes

```
Browse ──Esc──> Control ──Enter──> Browse
```

- **Browse**: Keys forwarded to Chromium. URL bar is read-only.
- **Control**: Keys handled by TUI. Can quit (`q`), enter Browse (`Enter`).

### New mode: UrlEdit

```
Browse ──Ctrl+Esc──> Control ──e──> UrlEdit ──Enter──> Browse
                         ^                       │
                         └───────Ctrl+Esc────────┘
```

- **UrlEdit**: Keys handled by edtui. URL bar is editable with Vim keybindings.

Inside UrlEdit, edtui manages its own Vim modes (Normal, Insert, Visual). The
TUI does not need to track these — edtui handles them internally. The TUI only
needs to intercept two keys at the boundary:

- **Enter** (from any edtui mode): Navigate to the edited URL. Switch to Browse.
- **Ctrl+Esc** (from any edtui mode): Cancel edit. Switch to Control.

`Ctrl+Esc` is already the universal "escape to Control" key — it exits Browse
mode the same way. Using it in UrlEdit too keeps one consistent key for "get me
back to Control from anywhere." Escape (without Ctrl) always goes to edtui,
which uses it for its own Vim mode transitions (Insert → Normal, Visual →
Normal). No conditional interception needed.

### Entering UrlEdit

From Control mode, pressing `e` enters UrlEdit. The edit buffer is initialized
with the current URL. edtui starts in Normal mode with the cursor at the end.

A typical flow:

1. Press Esc to leave Browse and enter Control
2. Press `e` to enter UrlEdit (edtui Normal mode, cursor at end of URL)
3. Press `A` to append (edtui Insert mode)
4. Type the new URL
5. Press Enter to navigate (switches to Browse)

Or to edit a URL already in the bar:

1. Esc → `e` → use `w`/`b`/`h`/`l` to navigate → `ciw` to change a word → type →
   Enter

Or to cancel:

1. Esc → `e` → edit some text → Ctrl+Esc (back to Control)

### Mode summary

| Mode    | URL bar          | Keys go to | Enter           | Ctrl+Esc  | Escape        |
| ------- | ---------------- | ---------- | --------------- | --------- | ------------- |
| Browse  | read-only        | Chromium   | —               | → Control | —             |
| Control | read-only        | TUI        | → Browse        | —         | —             |
| UrlEdit | editable (edtui) | edtui      | navigate+Browse | → Control | → edtui (Vim) |

## edtui configuration

### Single-line mode

edtui has no built-in single-line mode. We enforce it by:

1. **Removing newline keybindings** from the `KeyEventHandler`:
   - Remove Enter → `LineBreak(1)` (Insert mode)
   - Remove `o` → `AppendNewline(1)` (Normal mode)
   - Remove `O` → `InsertNewline(1)` (Normal mode)
2. **Rebinding Enter** to trigger navigation (handled by the TUI, not edtui).
3. **Patching `insert_char`** in edtui's `helper.rs` to strip `\n` from pasted
   text. This prevents multi-line pastes from creating new lines.
4. **Setting `wrap(false)`** on `EditorView` to enable horizontal scrolling
   instead of line wrapping.

### Horizontal scrolling

With `wrap(false)`, edtui's `update_viewport_horizontal()` in `state/view.rs`
shifts the viewport to keep the cursor visible. Long URLs that don't fit in the
bar will scroll left/right as the cursor moves. This works out of the box.

### Theme

edtui's `EditorView` accepts a `theme()` to match the Tokyo Night palette used
by the TUI. Set `base`, `cursor_style`, and `selection_style`:

- **Base**: `fg(FG)` / `bg(BG)` — matches the TUI's text/background
- **Cursor**: `fg(BG)` / `bg(FG)` — inverted block cursor
- **Selection**: `fg(FG)` / `bg(#283457)` — Tokyo Night visual selection blue

## Rendering

In UrlEdit mode, replace the `Paragraph` widget with edtui's `EditorView` widget
in `layout[0]`. The `EditorView` renders the edit buffer with cursor and handles
all display internally (cursor position, horizontal scroll, selection
highlighting).

In Browse and Control modes, continue rendering the read-only `Paragraph` as
before.

## Navigation

When Enter is pressed in UrlEdit mode:

1. Extract the edited URL from edtui's `EditorState`
2. Update the TUI's `url` string
3. Send the new URL to the compositor (either via `send_set_overlay()` with the
   new URL, or via a new `send_navigate()` XPC action)
4. Switch to Browse mode and notify the compositor (`send_mode_changed(true)`)

## Dependencies

Add edtui as a path dependency to `vendor/edtui/` (so we can patch `insert_char`
later for paste-with-newline). This requires upgrading the TUI from
`ratatui = "0.29"` + `crossterm = "0.28"` to `ratatui = "0.30"` +
`crossterm = "0.29"` to match edtui's dependency versions.

## Experiment 1: edtui URL bar with mode transitions

### Hypothesis

Adding edtui as the URL bar editor with a `UrlEdit` mode, Ctrl+Esc/Enter
interception, and single-line enforcement will produce a working editable URL
bar with Vim keybindings. The user can press `e` from Control mode, edit the
URL, and press Enter to navigate.

### Changes

#### 1. Upgrade dependencies (`tui/Cargo.toml`)

edtui 0.11.1 depends on `crossterm = "0.29"` and `ratatui-core = "0.1"`. The TUI
currently uses `ratatui = "0.29"` + `crossterm = "0.28"`. Upgrade to match:

```toml
[dependencies]
crossterm = "0.29"
ratatui = "0.30"
edtui = { path = "../vendor/edtui", default-features = false, features = ["arboard"] }
libc = "0.2"
block2 = "0.6"
```

Disable `default-features` to skip `syntax-highlighting` (not needed for a URL
bar). Keep `arboard` for system clipboard support.

#### 2. Add `UrlEdit` mode (`tui/src/main.rs`)

Add a third variant to the `Mode` enum:

```rust
#[derive(PartialEq)]
enum Mode {
    Browse,
    Control,
    UrlEdit,
}
```

Add edtui state alongside existing state in `main()`:

```rust
use edtui::{EditorEventHandler, EditorMode, EditorState, EditorTheme,
            EditorView, Lines};
use edtui::events::{KeyEventHandler, KeyEventRegister, KeyInput};

let mut editor_state = EditorState::new(Lines::from(url.as_str()));
let mut editor_handler = {
    let mut kh = KeyEventHandler::vim_mode();
    // Remove newline keybindings for single-line mode.
    kh.remove(&KeyEventRegister::i(vec![KeyInput::new(KeyCode::Enter)]));
    kh.remove(&KeyEventRegister::n(vec![KeyInput::new('o')]));
    kh.remove(&KeyEventRegister::n(vec![KeyInput::shift('O')]));
    EditorEventHandler::new(kh)
};
```

#### 3. Key dispatch for UrlEdit mode

In the event loop, add a `Mode::UrlEdit` arm. The TUI intercepts Enter and
Ctrl+Esc. Everything else (including plain Escape) goes to edtui:

```rust
Mode::UrlEdit => {
    match key.code {
        KeyCode::Enter => {
            // Extract URL from editor, navigate, switch to Browse.
            let new_url: String = editor_state.lines
                .get(jagged::index::RowIndex::new(0))
                .map(|line| line.iter().collect())
                .unwrap_or_default();
            url = new_url;
            mode = Mode::Browse;
            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                conn.send_mode_changed(pid, true);
            }
            // Force viewport update to send new URL.
            last_viewport = Rect::default();
        }
        _ => {
            // Pass everything else to edtui (including Escape).
            editor_handler.on_key_event(key, &mut editor_state);
        }
    }
}
```

Note: `Ctrl+Esc` is already handled before the mode dispatch (it always switches
to Control from any mode). See the existing `Ctrl+C` pattern — `Ctrl+Esc` is
added at the same level.

#### 4. Enter UrlEdit from Control mode

Add `e` keybinding in Control mode:

```rust
Mode::Control => match key.code {
    KeyCode::Char('q') => break,
    KeyCode::Char('e') => {
        // Initialize editor with current URL, cursor at end.
        editor_state = EditorState::new(Lines::from(url.as_str()));
        let len = url.len();
        editor_state.cursor = edtui::Index2::new(0, len.saturating_sub(1));
        mode = Mode::UrlEdit;
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            conn.send_mode_changed(pid, false);
        }
    }
    KeyCode::Enter => {
        mode = Mode::Browse;
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            conn.send_mode_changed(pid, true);
        }
    }
    _ => {}
},
```

#### 5. Render edtui in UrlEdit mode (`ui()` function)

Pass `editor_state` to `ui()` and render `EditorView` when in UrlEdit mode:

```rust
fn ui(
    frame: &mut Frame,
    url: &str,
    profile: &str,
    mode: &Mode,
    editor_state: &mut EditorState,
) -> Rect {
    // ... layout unchanged ...

    let (url_border, viewport_border) = match mode {
        Mode::Browse => (BORDER, CYAN),
        Mode::Control => (CYAN, BORDER),
        Mode::UrlEdit => (CYAN, BORDER),
    };

    if *mode == Mode::UrlEdit {
        const SELECTION: Color = Color::Rgb(0x28, 0x34, 0x57);
        let theme = EditorTheme::default()
            .base(Style::default().fg(FG).bg(BG))
            .cursor_style(Style::default().fg(BG).bg(FG))
            .selection_style(Style::default().fg(FG).bg(SELECTION))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Chromium ")
                    .title_top(profile_title.alignment(Alignment::Right))
                    .border_style(Style::default().fg(url_border).bg(BG))
                    .title_style(Style::default().fg(url_border))
                    .style(Style::default().bg(BG)),
            )
            .hide_status_line();
        EditorView::new(editor_state)
            .theme(theme)
            .wrap(false)
            .render(layout[0], frame.buffer_mut());
    } else {
        // Existing Paragraph rendering.
        let url_bar = Paragraph::new(url)
            .style(Style::default().fg(FG))
            .block(/* ... existing block ... */);
        frame.render_widget(url_bar, layout[0]);
    }

    // ... rest unchanged ...
}
```

#### 6. Update status bar hints

Add UrlEdit hints:

```rust
Mode::UrlEdit => Line::from(vec![
    Span::styled("<", d),
    Span::styled("enter", f),
    Span::styled("> ", d),
    Span::styled("navigate  ", f),
    Span::styled("<", d),
    Span::styled("ctrl+esc", f),
    Span::styled("> ", d),
    Span::styled("control", f),
]),
```

And a mode label:

```rust
let label = match mode {
    Mode::Browse => "\u{F059F} BROWSE",
    Mode::Control => "\u{F11C} CONTROL",
    Mode::UrlEdit => "\u{F040} EDIT",
};
```

### Verification

1. `cd tui && cargo build` — must compile with upgraded deps
2. Launch TermSurf, open a `web` pane
3. Press Esc to enter Control mode
4. Press `e` — URL bar becomes editable, cursor visible at end of URL
5. Press `A` then type a new URL — characters appear
6. Press Enter — browser navigates to the new URL, mode switches to Browse
7. Press Ctrl+Esc from UrlEdit — cancels edit, returns to Control
8. Ctrl+Esc → `e` → `0` → `w` → `ciw` → type → Enter — Vim motions work
9. Long URL: horizontal scrolling keeps cursor visible

### Success criteria

- `e` from Control enters UrlEdit with edtui rendering
- Vim keybindings work (h/l/w/b/i/a/A/x/dd/ciw etc.)
- Enter extracts the URL and navigates
- Ctrl+Esc exits UrlEdit to Control from any edtui sub-mode
- Escape handled by edtui for Vim mode transitions (Insert → Normal)
- No newlines can be inserted (Enter and o/O removed)
- Horizontal scrolling works for long URLs
- Status bar shows correct hints and mode label

### Result: Success

The editable URL bar works. All success criteria pass except "Enter extracts the
URL and navigates" — the URL is extracted and the mode switches to Browse, but
the browser does not navigate to the new URL. This is not a bug in the URL bar
itself. The URL bar correctly edits, extracts, and hands off the URL. The
problem is that no `navigate` XPC action exists anywhere in the stack — the GUI,
Chromium server, and TUI have never implemented "go to a URL." The
`send_set_overlay()` workaround (resetting `last_viewport` to force an overlay
update with the new URL) does not trigger navigation because `set_overlay` only
stores the URL for existing panes — it never forwards it to Chromium.

Navigation is a separate feature that requires changes at three levels: a
`navigate` action in the Chromium server (`shell_browser_main_parts.cc`), a
`handleNavigate` handler in the GUI (`xpc.zig`), and a `send_navigate` method in
the TUI (`xpc.rs`).

## Experiment 2: Navigate XPC action

### Hypothesis

Adding a `navigate` action that flows from the TUI through the GUI to the
Chromium server will make the editable URL bar actually navigate the browser
when Enter is pressed.

### XPC messages

```
TUI → GUI:
{ action: "navigate", pane_id: "<uuid>", url: "<url>" }

GUI → Chromium server:
{ action: "navigate", pane_id: "<uuid>", url: "<url>" }
```

The message is identical on both hops. The GUI receives it from the TUI, looks
up the pane's Chromium server, and forwards it.

### Changes

#### 1. Chromium server: add `navigate` action (`shell_browser_main_parts.cc`)

The server already navigates in three places:

- `CreateTab` loads the initial URL via `Shell::CreateNewWindow(ctx, url, ...)`
- `HandleKeyEvent` intercepts Cmd+[ / Cmd+] / Cmd+R and calls
  `GetController().GoBack()`, `GoForward()`, `Reload()`

For explicit URL navigation, use `GetController().LoadURLWithParams()`. This is
the standard Chromium API for programmatic navigation — it's what the omnibox
uses internally.

Add to the XPC message dispatch (after `key_event`):

```cpp
} else if (action && std::string_view(action) == "navigate") {
  const char* pane = xpc_dictionary_get_string(event, "pane_id");
  const char* url_str = xpc_dictionary_get_string(event, "url");
  std::string s_pane(pane ? pane : "");
  std::string s_url(url_str ? url_str : "");
  content::GetUIThreadTaskRunner({})->PostTask(
      FROM_HERE,
      base::BindOnce(&ShellBrowserMainParts::NavigateTab,
                     base::Unretained(self), s_pane, GURL(s_url)));
}
```

Add the method declaration to the header:

```cpp
void NavigateTab(const std::string& pane_id, const GURL& url);
```

Implement `NavigateTab`:

```cpp
void ShellBrowserMainParts::NavigateTab(const std::string& pane_id,
                                        const GURL& url) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);
  for (auto& tab : tabs_) {
    if (tab->pane_id == pane_id) {
      content::NavigationController::LoadURLParams params(url);
      params.transition_type = ui::PAGE_TRANSITION_TYPED;
      tab->shell->web_contents()->GetController().LoadURLWithParams(params);
      LOG(INFO) << "[ProfileServer] Navigate pane " << pane_id
                << " to " << url.spec();
      return;
    }
  }
  LOG(WARNING) << "[ProfileServer] Navigate: no tab for pane " << pane_id;
}
```

`PAGE_TRANSITION_TYPED` matches the semantics — the user typed a URL in the
address bar.

#### 2. GUI: add `handleNavigate` (`gui/src/apprt/xpc.zig`)

The GUI receives `navigate` from the TUI and forwards it to the Chromium server
for the matching pane. This follows the same pattern as `sendFocusChanged` —
look up the pane, get the server peer, send an XPC message.

Add to `handleMessage` dispatch:

```zig
} else if (std.mem.eql(u8, action_str, "navigate")) {
    handleNavigate(msg);
}
```

Implement `handleNavigate`:

```zig
fn handleNavigate(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const url = str(xpc_dictionary_get_string(msg, "url"));

    log.info("navigate pane={s} url={s}", .{ pane_id, url });

    const p = panes.get(pane_id) orelse {
        log.warn("navigate: no pane for {s}", .{pane_id});
        return;
    };
    const server = p.server orelse return;
    if (server.peer == null) return;

    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "navigate");

    // Null-terminate pane_id.
    var pane_z: [37]u8 = undefined;
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        xpc_dictionary_set_string(fwd, "pane_id", @ptrCast(&pane_z));
    }

    // Null-terminate URL.
    var url_z: [2049]u8 = undefined;
    if (url.len > 0 and url.len < url_z.len) {
        @memcpy(url_z[0..url.len], url);
        url_z[url.len] = 0;
        xpc_dictionary_set_string(fwd, "url", @ptrCast(&url_z));
    }

    xpc_connection_send_message(server.peer, fwd);
}
```

#### 3. TUI: add `send_navigate` (`tui/src/xpc.rs`)

Add a new method to `CompositorConnection`:

```rust
/// Tell the compositor to navigate to a new URL.
pub fn send_navigate(&self, pane_id: &str, url: &str) {
    let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
    if dict.is_null() {
        return;
    }

    unsafe {
        let action_key = CString::new("action").unwrap();
        let action_val = CString::new("navigate").unwrap();
        xpc_dictionary_set_string(dict, action_key.as_ptr(), action_val.as_ptr());

        let pane_key = CString::new("pane_id").unwrap();
        let pane_val = CString::new(pane_id).unwrap();
        xpc_dictionary_set_string(dict, pane_key.as_ptr(), pane_val.as_ptr());

        let url_key = CString::new("url").unwrap();
        let url_val = CString::new(url).unwrap();
        xpc_dictionary_set_string(dict, url_key.as_ptr(), url_val.as_ptr());

        xpc_connection_send_message(self.raw, dict);
        xpc_release(dict);
    }
}
```

#### 4. TUI: call `send_navigate` on Enter (`tui/src/main.rs`)

Replace the `last_viewport` reset workaround with an explicit navigate call:

```rust
Mode::UrlEdit => match key.code {
    KeyCode::Enter => {
        let new_url: String = editor_state
            .lines
            .get(RowIndex::new(0))
            .map(|line| line.iter().collect())
            .unwrap_or_default();
        url = new_url;
        mode = Mode::Browse;
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            conn.send_navigate(pid, &url);
            conn.send_mode_changed(pid, true);
        }
    }
    // ...
}
```

Remove the `last_viewport = Rect::default()` line — it was the workaround that
didn't work.

### Verification

1. `cd tui && cargo build` — TUI compiles
2. Build Chromium (`autoninja -C out/Default chromium_profile_server`)
3. Build GUI (`cd gui && zig build`)
4. Launch TermSurf, open a `web` pane with any URL
5. Ctrl+Esc → `e` → clear URL → type a new URL → Enter
6. Browser navigates to the new URL
7. URL bar updates when Chromium reports the final URL via `url_changed`

### Success criteria

- Enter in UrlEdit sends `navigate` action through all three levels
- Browser navigates to the edited URL
- Loading indicator appears during navigation
- URL bar updates to the final URL (after redirects, etc.)

### Result: Success

All success criteria pass. The navigate action flows from TUI → GUI → Chromium
server. The browser navigates to the edited URL on Enter.

## Conclusion

Issue 637 is closed. The URL bar is editable with full Vim keybindings via
edtui, and Enter navigates the browser to the edited URL via a new `navigate`
XPC action that flows through all three processes.
