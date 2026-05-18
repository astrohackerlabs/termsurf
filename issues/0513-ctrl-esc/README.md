+++
status = "closed"
opened = "2026-02-17"
closed = "2026-02-17"
+++

# Issue 513: Ctrl+Esc and Window-Side Mode Tracking

## Background

The `web` TUI has two modes: Browse and Control. Pressing Esc switches from
Browse to Control mode. The status bar displays
`[ctrl+esc] force exit browse
mode` as a hint — the intent was always for
Ctrl+Esc to be the primary mode switch, with bare Esc as a temporary
convenience.

## Problem

Ctrl+Esc works in WezTerm but not in TermSurf (Ghostty fork). Bare Esc works in
both. The `web` code hasn't changed — the issue is in how the terminal encodes
Ctrl+Esc and how crossterm parses it.

### What Ghostty sends

Ghostty's `function_keys.zig` (line 226) encodes Ctrl+Escape as:

```
\x1b[27;5;27~
```

This is the xterm "modify-other-keys" format (`CSI 27 ; modifier ; keycode ~`).
The entry has `modify_other_keys: .any`, meaning Ghostty sends this sequence
regardless of whether the application has requested modify-other-keys mode.

Bare Escape sends `\x1b` — a single byte that crossterm handles fine.

### What crossterm expects

The `web` TUI uses crossterm 0.28.1 **without enabling keyboard enhancement
flags**. Without `PushKeyboardEnhancementFlags`, crossterm's legacy parser
handles standard CSI sequences (arrows, function keys, etc.) but does not
recognize the `CSI 27 ; 5 ; 27 ~` format — key number 27 is not in the standard
function key table. The sequence is either silently dropped or misinterpreted.

### Why WezTerm works

WezTerm likely sends a different encoding for Ctrl+Esc — either the same bare
`\x1b` as unmodified Escape, or a sequence that crossterm's legacy parser
recognizes. The exact encoding WezTerm uses has not been verified.

## Architectural decision: window handles input in browse mode

The original Options section proposed fixing this inside the `web` TUI (enabling
the kitty keyboard protocol, parsing raw sequences, etc.). But a broader
analysis of the input forwarding architecture changes the picture.

### Why the window must handle keyboard in browse mode

When browser input forwarding is implemented, the window (TermSurf) will need to
forward keypresses to the Chromium Profile Server. The window has access to
`NSEvent`, which provides:

- KeyDown and KeyUp events (terminals only signal "key pressed")
- Left Shift vs Right Shift distinction
- Key repeat vs separate presses
- IME composition sequences
- Dead keys for accented characters

The terminal PTY is a lossy channel — it cannot faithfully transmit the full
range of keyboard events that browsers need. Issue 513 itself is proof: Ctrl+Esc
doesn't survive the Ghostty → PTY → crossterm encoding.

Mouse input must also go through the window, because the terminal lacks sub-cell
pixel coordinates needed for fine-grained mouse control. Since both keyboard and
mouse must go through the window in browse mode, the window is the single input
authority for browser interaction.

### Mode must be shared

Both the window and the `web` TUI need to know the current mode:

- **The window** needs the mode to decide whether to forward keypresses to the
  Chromium Profile Server (browse mode) or let them pass through to the terminal
  (control mode).
- **The `web` TUI** needs the mode to render the correct UI state (border
  colors, status bar hints, URL bar focus).

Mode state is shared between the two processes. When the mode changes, the
window must notify `web` (via the existing XPC connection through
CompositorXPC), and vice versa.

### How Ctrl+Esc works under this architecture

1. User presses Ctrl+Esc while in browse mode.
2. The window intercepts the keypress via `NSEvent` (before it reaches the PTY).
3. The window recognizes Ctrl+Esc as the "exit browse mode" keybinding.
4. The window transitions its mode state from Browse to Control.
5. The window notifies `web` of the mode change via XPC.
6. The window stops forwarding keypresses to Chromium and lets them pass through
   to the terminal.
7. `web` updates its UI to reflect control mode (border colors, status bar).

Bare Esc continues to work as it does today — `web` receives it via crossterm
and transitions to control mode locally, then notifies the window.

## Implementation scope

This issue requires two things:

### 1. Window-side mode tracking and Ctrl+Esc handling

Add mode state (Browse/Control) to CompositorXPC, per pane. When the window
receives a Ctrl+Esc keypress in browse mode:

- Transition mode to Control
- Notify `web` via XPC
- Stop intercepting keypresses (let them flow to the terminal)

When the window receives a mode change notification from `web` (e.g., `web`
detected bare Esc or Enter):

- Update mode state
- Start or stop intercepting keypresses accordingly

### 2. Mode synchronization protocol

Add XPC messages for mode changes on the existing `web` ↔ CompositorXPC
connection:

- `mode_changed` (from window to `web`): window changed mode (e.g., Ctrl+Esc)
- `mode_changed` (from `web` to window): `web` changed mode (e.g., bare Esc,
  Enter)

Both sides update their local mode state on receipt.

## Future: full input forwarding

This issue lays the groundwork for full browser input forwarding (keyboard and
mouse). Once the window can intercept keypresses in browse mode, forwarding them
to the Chromium Profile Server is a natural next step. The XPC channel from the
window to the profile server already exists (CompositorXPC manages it). Adding
`key_event` and `mouse_event` messages completes the input pipeline.

The `web` TUI remains responsible for browser chrome (URL bar, status bar,
viewport border) and control mode keybindings (`q` to quit, Enter to browse).
The window is responsible for browser input (all keypresses and mouse events in
browse mode).

## Experiments

### Experiment 1: Ctrl+Esc and bidirectional mode sync

Make Ctrl+Esc work by intercepting it at the window level. Establish
bidirectional mode synchronization between CompositorXPC and `web` so both sides
always agree on the current mode.

#### Key event flow

macOS dispatches key events through local event monitors before the normal
responder chain (performKeyEquivalent → keyDown). AppDelegate already registers
a local event monitor for app-level shortcuts. CompositorXPC will register its
own monitor **first** (it runs before AppDelegate's registration in
`applicationDidFinishLaunching`). When the monitor returns `nil`, the event is
consumed — subsequent monitors and the responder chain never see it.

```
NSEvent (Ctrl+Esc)
    ↓
CompositorXPC local event monitor (registered first)
    ├─ Ctrl+Esc + focused pane in browse mode → consume (return nil)
    └─ Anything else → pass through (return event)
    ↓
AppDelegate local event monitor (registered second)
    ↓
SurfaceView.performKeyEquivalent() → keyDown() → PTY
```

When CompositorXPC consumes Ctrl+Esc, it never reaches the PTY. The terminal
encoding problem from the Problem section is bypassed entirely — `NSEvent` gives
us `keyCode == 0x35` (Escape) and `modifierFlags.contains(.control)` directly.

#### Change 1: CompositorXPC.swift — mode tracking and Ctrl+Esc interception

**Mode model: browse vs not-browse.** The window only needs to know whether a
pane is in browse mode. It does not care about control mode, insert mode, or any
other mode `web` may add in the future — those are internal to the TUI. The
window's only question is: "should I intercept keypresses for this pane?" The
answer is yes when browsing, no otherwise. The mode state is therefore a boolean
(`browsing`), not a string enum.

**New state properties** (after `pendingPixelSizes`):

```swift
/// Panes currently in browse mode (window intercepts keys).
/// Absent or false = not browsing (keys pass through to terminal).
private var paneBrowsing: [UUID: Bool] = [:]

/// Maps pane UUID → web peer connection (for sending mode_changed back).
private var webPeersForPane: [UUID: xpc_connection_t] = [:]
```

**Store the XPC queue as a property** (replace the local `let queue` in
`start()`):

```swift
private let xpcQueue = DispatchQueue(label: "com.termsurf.compositor.xpc")
```

Use `xpcQueue` everywhere `queue` was used in `start()`.

**Register the local event monitor** in `start()`, before the anonymous listener
setup.

ts1 discovered that local event monitors are app-global — they fire for ALL
keyDown events across all windows and tabs. Without proper guards, an inactive
tab's monitor intercepts keys meant for the active tab. ts1's fix was a
two-level focus check (documented in Issue 104). We apply the same pattern:

1. **Active window check** — `NSApp.keyWindow` ensures we only act when our
   window is the key window. In Ghostty, each tab is a separate `NSWindow`
   grouped via `NSWindowTabGroup`, so `isKeyWindow` is false for inactive tabs.
2. **Active pane check** — `firstResponder as? Ghostty.SurfaceView` ensures we
   only act on the focused pane, not a different split in the same window.

```swift
NSEvent.addLocalMonitorForEvents(matching: [.keyDown]) { [weak self] event in
    guard let self = self else { return event }

    // Only intercept Ctrl+Esc.
    guard event.keyCode == 0x35,
          event.modifierFlags.contains(.control) else { return event }

    // Two-level focus check (from ts1 Issue 104):
    // 1. Our window must be the key window (covers inactive tabs too).
    guard let window = NSApp.keyWindow, window.isKeyWindow else { return event }
    // 2. The first responder must be a SurfaceView (the focused pane).
    guard let surfaceView = window.firstResponder
            as? Ghostty.SurfaceView else { return event }
    let uuid = surfaceView.id

    // Check and update mode on the XPC queue (where all state lives).
    let consumed = self.xpcQueue.sync { () -> Bool in
        guard self.paneBrowsing[uuid] == true else { return false }
        self.paneBrowsing[uuid] = false
        self.sendModeChanged(paneUUID: uuid, browsing: false)
        fputs("[Compositor] Ctrl+Esc: exit browse for pane \(uuid)\n", stderr)
        return true
    }

    return consumed ? nil : event
}
```

The `xpcQueue.sync` dispatch is safe: main → XPC queue, no deadlock risk.

**Read initial mode from `set_overlay`** (when URL is present, after storing
`peerPaneIds`). The `browsing` field is included in the `set_overlay` message
itself — no separate mode message needed on startup:

```swift
let browsing = xpc_dictionary_get_bool(msg, "browsing")
paneBrowsing[uuid] = browsing
webPeersForPane[uuid] = peer
```

**Handle `mode_changed` from `web`** (add case to `handleMessage` switch):

```swift
case "mode_changed":
    handleModeChanged(msg, from: peer)
```

```swift
private func handleModeChanged(_ msg: xpc_object_t, from peer: xpc_connection_t) {
    guard let paneIdPtr = xpc_dictionary_get_string(msg, "pane_id") else { return }
    let paneIdStr = String(cString: paneIdPtr)
    let browsing = xpc_dictionary_get_bool(msg, "browsing")
    guard let uuid = UUID(uuidString: paneIdStr) else { return }

    paneBrowsing[uuid] = browsing
    fputs("[Compositor] mode_changed from web: browsing=\(browsing) for pane \(paneIdStr)\n", stderr)
}
```

**Send mode_changed to `web`** (new helper):

```swift
private func sendModeChanged(paneUUID: UUID, browsing: Bool) {
    guard let peer = webPeersForPane[paneUUID] else { return }
    let msg = xpc_dictionary_create(nil, nil, 0)
    xpc_dictionary_set_string(msg, "action", "mode_changed")
    xpc_dictionary_set_bool(msg, "browsing", browsing)
    xpc_connection_send_message(peer, msg)
}
```

**Clean up on disconnect** (add to the `if let uuid` block in
`handleDisconnect`):

```swift
paneBrowsing.removeValue(forKey: uuid)
webPeersForPane.removeValue(forKey: uuid)
```

#### Change 2: web/src/xpc.rs — initial mode in set_overlay, receive messages

**Add `browsing` to `send_set_overlay`.** The initial mode is sent as part of
the existing `set_overlay` message, not as a separate message. Add a `browsing`
parameter and set it in the dictionary:

```rust
pub fn send_set_overlay(&self, pane_id: &str, col: u16, row: u16,
                        width: u16, height: u16, url: &str, profile: &str,
                        browsing: bool) {
    // ... existing dictionary setup ...

    let browsing_key = CString::new("browsing").unwrap();
    xpc_dictionary_set_bool(dict, browsing_key.as_ptr(), browsing);

    // ... send message ...
}
```

**Replace the no-op event handler** with one that parses incoming `mode_changed`
messages and sends them through an `mpsc` channel to the event loop.

**Add message enum and channel receiver to the struct:**

```rust
pub enum CompositorMessage {
    ModeChanged { browsing: bool },
}

pub struct CompositorConnection {
    raw: XpcConnectionT,
    rx: std::sync::mpsc::Receiver<CompositorMessage>,
}
```

**In `connect()`, create the channel and wire up the event handler:**

```rust
let (tx, rx) = std::sync::mpsc::channel();

let block2 = block2::RcBlock::new(move |event: XpcObjectT| {
    if event.is_null() { return; }
    let event_type = unsafe { xpc_get_type(event) };
    let dict_type = unsafe { &XPC_TYPE_DICTIONARY as *const c_void };
    if event_type != dict_type { return; }

    let action_key = CString::new("action").unwrap();
    let action_ptr = unsafe { xpc_dictionary_get_string(event, action_key.as_ptr()) };
    if action_ptr.is_null() { return; }
    let action = unsafe { std::ffi::CStr::from_ptr(action_ptr) }
        .to_str()
        .unwrap_or("");

    if action == "mode_changed" {
        let browsing_key = CString::new("browsing").unwrap();
        let browsing = unsafe { xpc_dictionary_get_bool(event, browsing_key.as_ptr()) };
        let _ = tx.send(CompositorMessage::ModeChanged { browsing });
    }
});
```

Add `xpc_dictionary_get_bool` to the FFI declarations:

```rust
fn xpc_dictionary_get_bool(dict: XpcObjectT, key: *const c_char) -> bool;
```

Replace the existing no-op `block2` with this. Store `rx` in the struct:

```rust
Some(Self { raw: app_conn, rx })
```

**Add `try_recv` method:**

```rust
pub fn try_recv(&self) -> Option<CompositorMessage> {
    self.rx.try_recv().ok()
}
```

**Add `send_mode_changed` method:**

```rust
pub fn send_mode_changed(&self, pane_id: &str, browsing: bool) {
    let dict = unsafe {
        xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0)
    };
    if dict.is_null() { return; }

    unsafe {
        let action_key = CString::new("action").unwrap();
        let action_val = CString::new("mode_changed").unwrap();
        xpc_dictionary_set_string(dict, action_key.as_ptr(), action_val.as_ptr());

        let pane_key = CString::new("pane_id").unwrap();
        let pane_val = CString::new(pane_id).unwrap();
        xpc_dictionary_set_string(dict, pane_key.as_ptr(), pane_val.as_ptr());

        let browsing_key = CString::new("browsing").unwrap();
        xpc_dictionary_set_bool(dict, browsing_key.as_ptr(), browsing);

        xpc_connection_send_message(self.raw, dict);
        xpc_release(dict);
    }
}
```

Add `xpc_dictionary_set_bool` to the FFI declarations:

```rust
fn xpc_dictionary_set_bool(dict: XpcObjectT, key: *const c_char, value: bool);
```

#### Change 3: web/src/main.rs — initial mode and mode changes

**Pass initial mode in `send_set_overlay`.** The existing call site (line 93)
adds `true` for `browsing` since `web` starts in browse mode:

```rust
conn.send_set_overlay(
    pid,
    viewport_rect.x, viewport_rect.y,
    viewport_rect.width, viewport_rect.height,
    &url, &profile,
    mode == Mode::Browse,
);
```

**Send mode changes to compositor when `web` changes mode locally.** Any
transition out of browse mode sends `browsing: false`. Any transition into
browse mode sends `browsing: true`. The window doesn't care which non-browse
mode `web` is in — only whether it should intercept keys.

In the `Mode::Browse` match arm:

```rust
Mode::Browse => {
    if key.code == KeyCode::Esc {
        mode = Mode::Control;
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            conn.send_mode_changed(pid, false);
        }
    }
}
```

In the `Mode::Control` match arm:

```rust
KeyCode::Enter => {
    mode = Mode::Browse;
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        conn.send_mode_changed(pid, true);
    }
}
```

**Receive mode changes from compositor.** After the `event::poll` block, drain
incoming messages. When the window sends `browsing: false` (e.g., Ctrl+Esc),
`web` transitions to control mode. `web` may have other non-browse modes in the
future (insert, search, etc.) — but when the window says "stop browsing,"
control mode is always the correct landing state.

```rust
if let Some(ref conn) = compositor {
    while let Some(msg) = conn.try_recv() {
        match msg {
            xpc::CompositorMessage::ModeChanged { browsing } => {
                mode = if browsing { Mode::Browse } else { Mode::Control };
            }
        }
    }
}
```

#### Initial mode agreement

The initial mode is explicit in the `set_overlay` message:

- `web` sends `browsing: true` in `set_overlay` (derived from
  `mode ==
  Mode::Browse`)
- CompositorXPC reads `browsing` from the message and sets `paneBrowsing[uuid]`

No separate sync message needed. The initial state is part of the same message
that establishes the overlay.

#### Thread safety

- **CompositorXPC state** lives on the `xpcQueue` serial queue. All XPC handlers
  fire there. The local event monitor (main thread) dispatches to
  `xpcQueue.sync` for reads and writes.
- **`web` state** lives on the main thread (single-threaded event loop). The
  `mpsc::Receiver` is polled synchronously each iteration. The `mpsc::Sender` is
  called from the XPC connection's callback queue — this is safe because
  `mpsc::Sender` is `Send`.

#### Verification

```bash
cd ts4/box-demo && bun run server.ts &
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- http://localhost:9407
```

Test matrix:

| Action                           | Expected                                                                                                        |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Press Ctrl+Esc in browse mode    | Window exits browse. `web` receives `browsing: false`, switches to control. Log: `exit browse for pane`.        |
| Press Ctrl+Esc in control mode   | No effect (passes through to terminal — `paneBrowsing` is false).                                               |
| Press bare Esc in browse mode    | `web` switches to control, sends `browsing: false`. Window updates `paneBrowsing`.                              |
| Press Enter in control mode      | `web` switches to browse, sends `browsing: true`. Window updates `paneBrowsing`.                                |
| Press Ctrl+Esc again after Enter | Window exits browse again (intercepted at NSEvent level).                                                       |
| Multiple panes with overlays     | Only the focused pane responds to Ctrl+Esc (two-level focus check).                                             |
| Run `web` in WezTerm             | Bare Esc and Enter still work. Ctrl+Esc depends on WezTerm's encoding (may or may not work — not a regression). |
| Quit `web` with `q`              | Clean disconnect. Mode state cleaned up. No crash.                                                              |

Pass: Ctrl+Esc switches from browse to control in TermSurf, and mode stays
synchronized between window and TUI across all transitions.

#### Result: Pass

Ctrl+Esc exits browse mode. The NSEvent local monitor intercepts Ctrl+Esc before
it reaches the PTY, completely bypassing the terminal encoding problem. The
two-level focus check ensures only the focused pane responds.

Bidirectional mode sync works: bare Esc in browse mode sends `browsing: false`
to the window, Enter in control mode sends `browsing: true`, and Ctrl+Esc sends
`browsing: false` from the window back to `web`. Both sides stay in agreement.

## Conclusion

Experiment 1 resolves the issue. Ctrl+Esc works reliably in TermSurf by
intercepting it at the NSEvent level — before the lossy PTY encoding that caused
the original problem. The bidirectional mode synchronization protocol
(`mode_changed` messages over the existing XPC connection) keeps the window and
`web` TUI in agreement across all mode transitions.

This also lays the groundwork for full browser input forwarding. The window now
knows whether a pane is in browse mode, which is the prerequisite for deciding
whether to forward keypresses and mouse events to the Chromium Profile Server.
Adding `key_event` and `mouse_event` messages on the existing XPC channel is the
natural next step.
