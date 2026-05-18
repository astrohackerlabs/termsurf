+++
status = "closed"
opened = "2026-02-18"
closed = "2026-02-18"
+++

# Issue 515: Text Selection (Drag-to-Select)

## Goal

Click and drag to select text on a webpage in browse mode. The selection
highlight must be visible in the rendered overlay.

## Background

Issue 514 established the full mouse input pipeline: clicks, scrolling, hover,
and cursor sync all work. But text selection via click-and-drag does not.
Experiments 10–12 in Issue 514 attempted three different fixes and all failed.
The Swift changes were reverted; the Chromium-side button derivation fix (commit
`084a805`) was kept because it's correct.

This issue isolates the problem and solves it.

## Two Independent Problems

Research reveals two problems that must both be solved for visible text
selection:

### Problem 1: Drag events never reach Chromium

Issue 514's logs showed mouseDown and mouseUp at different coordinates with **no
mouse move events in between** — the user dragged 160+ pixels but Chromium only
saw down and up. Without intermediate `kMouseMove` events carrying
`kLeftButtonDown`, Chromium's selection controller never fires
`HandleMouseDraggedEvent()` and no selection is created.

The root cause is how macOS generates drag events:

- When a local event monitor **consumes** `.leftMouseDown` (returns `nil`), no
  view in the responder chain receives it.
- macOS only generates `.leftMouseDragged` events when a view's `mouseDown:`
  handler has started a drag tracking session.
- With mouseDown consumed, no tracking session exists, so macOS generates
  **neither** `.leftMouseDragged` **nor** `.mouseMoved` while the button is
  physically held. Total silence until mouseUp.

Issue 514 Experiment 11 tried letting mouseDown propagate (return event) so
macOS would start drag tracking. Experiment 12 also let `.leftMouseDragged`
propagate. Both still failed — but Experiment 12's conclusion noted: "We're
debugging blind — `HandleMouseMove` has no logging." It is possible that
Experiment 12 actually delivered drags to Chromium but the selection wasn't
visible due to Problem 2.

### Problem 2: Chromium may not render selection without focus

Chromium's selection rendering depends on focus state. The chain:

```
FrameSelection → SelectionBoundsRecorder::ShouldRecordSelection()
    → checks FocusController::FocusedFrame() matches
    → checks FocusController::IsActive()

FocusController::IsActive()
    → returns is_active_ || is_emulating_focus_
    → is_active_ set by RenderWidgetHostViewMac::SetActive(bool)
```

If the page isn't "active" (no focused window), Blink skips painting the
selection highlight entirely. The caret is also disabled:

```cpp
// frame_selection.cc:1067
frame_caret_->SetCaretEnabled(active_and_focused);
```

The Chromium Profile Server runs headless — no visible NSWindow. Chromium has
headless-mode overrides in `RenderWidgetHostViewMac` that allow focus
propagation without a window:

```cpp
// render_widget_host_view_mac.mm:1782
if (IsHeadless() || is_getting_focus_ || is_window_key_) {
    host()->GotFocus();
}
```

But we never call `Focus()` or `SetActive(true)` on the view. Without that, the
`FocusController` thinks the page is inactive and selection highlights are not
painted — even if the selection exists internally.

This means Experiment 12 may have actually created a selection in Chromium, but
it was invisible because the frame lacked focus. The FrameSinkVideoCapturer
captures the rendered frame faithfully — if Blink doesn't paint the highlight,
the capturer doesn't capture it.

## Ideas for Experiments

### Approach A: Propagate mouseDown + diagnostic logging

Re-attempt the Experiment 12 approach (let mouseDown and mouseDragged
propagate), but this time with diagnostic logging on both sides to verify events
are flowing. This has known side effects — the terminal receives mouse escape
sequences — but they're harmless because the `web` TUI has mouse capture enabled
and ignores them.

Steps:

1. Add logging to the Swift move monitor: log when `.leftMouseDragged` arrives,
   whether `hitTestOverlay` succeeds, and the coordinates/modifiers sent.
2. Add logging to Chromium's `HandleMouseMove`: log every call with coordinates,
   button, and modifiers.
3. Let mouseDown propagate (return event) in the click monitor.
4. Let `.leftMouseDragged` propagate (return event) in the move monitor.
5. Reproduce and read logs.

Advantage: minimal code change, directly tests the Experiment 12 hypothesis.
Disadvantage: terminal side effects (harmless but messy).

### Approach B: Synthesized drag state (no propagation)

Track button state ourselves and bypass macOS drag tracking entirely. The click
monitor records when mouseDown fires over the overlay; the move monitor checks
this state and adds button-down modifiers to regular `.mouseMoved` events.

Steps:

1. Add shared state: `dragPaneUUID: UUID?`, `dragButton: String?`.
2. Click monitor: on mouseDown over overlay, set `dragPaneUUID` and
   `dragButton`. Continue consuming (return nil).
3. Move monitor: when `dragPaneUUID` is set, add `kLeftButtonDown` (32) to
   modifiers on every `.mouseMoved` event. Forward as `mouse_move` with the
   button-down flag.
4. Click monitor: on mouseUp, clear `dragPaneUUID`.

**Critical unknown:** Does macOS generate `.mouseMoved` events while a button is
physically held down and mouseDown was consumed? If not (total silence as the
Issue 514 evidence suggests), this approach cannot work. The experiments in
Issue 514 suggest `.mouseMoved` events are suppressed too, but this was never
directly confirmed with logging.

Advantage: no terminal side effects, events fully consumed. Disadvantage: may
not work if `.mouseMoved` is suppressed during button-down.

### Approach C: Focus state fix for selection rendering

Independent of how drags are delivered, Chromium needs focus to render selection
highlights. Call `Focus()` on the `RenderWidgetHostView` when creating tabs:

```cpp
// In CreateTab, after ObserveContents and cursor callback:
if (auto* view = shell->web_contents()->GetRenderWidgetHostView()) {
    view->Focus();
}
```

This should be done regardless of which drag approach (A or B) is used.

### Recommended experiment order

1. **Experiment 1: Diagnostic logging** — Add logging to both the Swift move
   monitor and Chromium's `HandleMouseMove`. Reproduce with the current code
   (mouseDown consumed, move consumed) to establish baseline: what events does
   the monitor actually receive during a physical drag?

2. **Experiment 2: Focus state** — Add `view->Focus()` in `CreateTab`. Test with
   a simple click (not drag) on a text input — if focus is working, the input
   should show a blinking cursor and focus ring. This validates Problem 2
   independently.

3. **Experiment 3: Propagate + logging** — Combine Approach A with the logging
   from Experiment 1. Let mouseDown and mouseDragged propagate, read logs,
   verify the full chain. If drags reach Chromium and focus is set, text
   selection should appear.

4. **Experiment 4: Clean up** — Remove diagnostic logging, finalize the
   approach. If propagation works and terminal side effects are acceptable, ship
   it. If not, explore Approach B with the knowledge gained from logging.

## Experiments

### Experiment 1: Focus state for input feedback

Tell Chromium the page is focused and active so Blink renders selection
highlights, blinking carets, and focus rings. Verify with google.com — the
search input auto-focuses on load; when the page is "active" it shows a blue
focus ring and blinking cursor.

#### Root cause

The Chromium Profile Server never tells the renderer it has focus. On Mac,
`Shell::ActivateContents` normally does three things:

```cpp
top_contents->Focus();                        // focus the RenderWidgetHost
[window makeKeyAndOrderFront:nil];            // make window key (active)
[NSApp activateIgnoringOtherApps:YES];        // make app active
```

Steps 2 and 3 drive `RenderWidgetHostViewMac::SetActive(true)`, which calls
`UpdateActiveState(true)` → `host()->delegate()->SendActiveState(true)` →
`RenderWidgetHostImpl::SetActive(true)` → Blink's `FocusController::SetActive`.
Without this, `FocusController::IsActive()` returns false and Blink skips
painting selection highlights and disables the caret.

In headless mode (no NSWindow), `RenderWidgetHostViewMac` has special handling:
calling `Focus()` triggers `OnFirstResponderChanged(true)` asynchronously, which
calls `host()->GotFocus()` when `IsHeadless()` is true. But `GotFocus` only sets
the focused frame — it doesn't set the page active. We need both.

#### Changes

##### shell_browser_main_parts.cc

In `CreateTab`, after the cursor callback registration (line ~367), add:

```cpp
// Tell the renderer this page is focused and active so Blink renders
// selection highlights, blinking carets, and focus rings.
if (auto* view = shell->web_contents()->GetRenderWidgetHostView()) {
    view->Focus();
    view->SetActive(true);
}
```

`Focus()` makes the main frame the focused frame (headless mode handles the
missing NSWindow). `SetActive(true)` sets `FocusController::IsActive()` to true
so Blink paints interactive feedback.

No Swift or Rust changes needed. Single file, two lines.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- https://google.com
```

1. Enter browse mode. The Google search input should show a blue focus ring and
   blinking text cursor — proving the page is active and focused.
2. Click inside the search input — focus ring should remain, cursor should be at
   the click position.
3. Move mouse over "Gmail" or "Images" links — they should show hover highlights
   (already working from Issue 514, but confirms focus didn't break anything).

Pass: Google search input shows focus ring and blinking cursor on page load.

#### Result: Partial pass

The Google search input shows a blinking cursor on page load — proving
`Focus()` + `SetActive(true)` correctly propagate through `FocusController` to
enable carets and focus rings. Blink now considers the page active and focused.

However, when pressing Esc to exit browse mode, the blinking cursor persists.
The page stays focused because we only set focus once at tab creation and never
send unfocus. The focus lifecycle needs to track mode changes:

- **Enter browse mode** (or focus a pane already in browse mode) → tell Chromium
  the view is focused and active.
- **Exit browse mode** (or focus a different pane) → tell Chromium the view lost
  focus and is inactive.

This requires a new XPC message (`focus_changed`) from CompositorXPC to the
Chromium server, triggered by mode transitions and pane focus changes. The
`CreateTab` focus call should be removed — focus should only be set when the
user is actually interacting with the page, not unconditionally at creation.

### Experiment 2: Focus/unfocus lifecycle

Add a `focus_changed` XPC message so CompositorXPC can tell the Chromium server
when a pane gains or loses focus. Four trigger points in CompositorXPC, one new
handler in the Chromium server.

#### Trigger points

Focus should be sent (`focused: true`) when:

1. **`mode_changed` from web with `browsing=true`** — The TUI entered browse
   mode. The user is now interacting with the page.
2. **`mode_changed` from web with `browsing=false`** — The TUI exited browse
   mode (e.g., user pressed `i` to enter control mode). Send `focused: false`.
3. **Ctrl+Esc exit** — The Ctrl+Esc handler in CompositorXPC sets
   `paneBrowsing[uuid] = false`. Send `focused: false` here too.

Pane focus changes (switching between split panes) are not handled yet — this
would require observing Ghostty's first-responder changes, which is a separate
concern. For now, mode transitions cover the primary use case.

#### XPC message

```
{
    action: "focus_changed",
    pane_id: "<uuid>",
    focused: <bool>
}
```

Sent from CompositorXPC to the Chromium server on the control connection.

#### Changes

##### CompositorXPC.swift

Add a helper method to send the focus message:

```swift
private func sendFocusChanged(paneUUID: UUID, focused: Bool) {
    guard let profile = paneProfiles[paneUUID],
          let controlConn = serverControlConnections[profile] else { return }
    let msg = xpc_dictionary_create(nil, nil, 0)
    xpc_dictionary_set_string(msg, "action", "focus_changed")
    xpc_dictionary_set_string(msg, "pane_id", paneUUID.uuidString)
    xpc_dictionary_set_bool(msg, "focused", focused)
    xpc_connection_send_message(controlConn, msg)
}
```

Call it from three places:

1. **`handleModeChanged`** — after updating `paneBrowsing[uuid]`:
   ```swift
   sendFocusChanged(paneUUID: uuid, focused: browsing)
   ```

2. **Ctrl+Esc handler** — inside the `xpcQueue.sync` block, after setting
   `paneBrowsing[uuid] = false`:
   ```swift
   self.sendFocusChanged(paneUUID: uuid, focused: false)
   ```

3. **`handleSetOverlay`** — after storing initial `paneBrowsing[uuid]`, if
   browsing is true at connection time:
   ```swift
   if browsing {
       sendFocusChanged(paneUUID: uuid, focused: true)
   }
   ```
   This handles the case where the TUI connects already in browse mode.

##### shell_browser_main_parts.cc

Remove the `Focus()` + `SetActive(true)` calls from `CreateTab` (added in
Experiment 1). Focus is now driven by the XPC message, not by tab creation.

Add `"focus_changed"` case to the XPC handler in `StartDynamicMode`:

```cpp
} else if (action && std::string_view(action) == "focus_changed") {
    const char* pane = xpc_dictionary_get_string(event, "pane_id");
    bool focused = xpc_dictionary_get_bool(event, "focused");
    std::string s_pane(pane ? pane : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::HandleFocusChanged,
                       base::Unretained(self), s_pane, focused));
}
```

New method `HandleFocusChanged`:

```cpp
void ShellBrowserMainParts::HandleFocusChanged(
    const std::string& pane_id, bool focused) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);

    TabState* tab = nullptr;
    for (auto& t : tabs_) {
        if (t->pane_id == pane_id) { tab = t.get(); break; }
    }
    if (!tab) return;

    auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
    if (!view) return;

    if (focused) {
        view->Focus();
        view->SetActive(true);
    } else {
        view->SetActive(false);
    }

    LOG(INFO) << "[ProfileServer] Focus " << (focused ? "gained" : "lost")
              << " for pane " << pane_id;
}
```

##### shell_browser_main_parts.h

Add inside the `#if BUILDFLAG(IS_MAC)` block:

```cpp
void HandleFocusChanged(const std::string& pane_id, bool focused);
```

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- https://google.com
```

1. Enter browse mode — Google search input shows blinking cursor and focus ring.
2. Press Esc (exit browse mode) — blinking cursor and focus ring disappear.
3. Enter browse mode again — blinking cursor returns.
4. Press Ctrl+Esc — blinking cursor disappears.

Pass: focus ring and caret appear on browse entry, disappear on browse exit.

#### Result: Partial pass

Mode-driven focus works correctly within a single pane:

- Enter browse mode → blinking cursor appears (focus gained).
- Press Esc → blinking cursor stops (focus lost).
- Press Ctrl+Esc → blinking cursor stops (focus lost).
- Re-enter browse mode → blinking cursor returns.

However, pane switching does not trigger focus changes:

- **Leave a pane in browse mode** (click another pane or use keybinding to
  switch focus) → the webview keeps its blinking cursor. Chromium still thinks
  it's focused.
- **Return to a pane in browse mode** → the webview doesn't regain focus because
  no `focus_changed` message is sent. The TUI didn't toggle browse mode — it was
  already browsing — so `handleModeChanged` never fires.

The three current trigger points (Ctrl+Esc, `handleModeChanged`,
`handleSetOverlay`) all key off mode transitions. Pane focus changes happen
outside the mode system — they're a Ghostty first-responder change that
CompositorXPC doesn't observe.

#### Ideas for fixing pane focus

1. **Observe `NSWindow.firstResponder` changes.** When the first responder
   changes to a different SurfaceView, check if the old pane was browsing (send
   unfocus) and if the new pane is browsing (send focus). This could use KVO on
   the window's `firstResponder` property, or a periodic check on the mouse move
   monitor (which already runs on every mouse event).

2. **Use the mouse move monitor as a proxy.** The move monitor already knows
   which pane the mouse is over (`hitTestOverlay` returns the UUID). If the pane
   under the mouse changes and the new pane is browsing, send focus. If the old
   pane was browsing and no longer under the mouse, send unfocus. This doesn't
   cover keyboard-driven pane switches but handles the common case.

3. **Hook into Ghostty's focus notification.** Ghostty calls
   `ghostty_surface_mouse_button` and similar C API functions when a surface
   gains focus. If there's a C API callback for surface focus changes, we could
   observe it from Swift. This would be the most architecturally clean approach
   but requires understanding Ghostty's focus propagation.

### Experiment 3: Pane-switch focus via first-responder tracking

Track which pane currently has Chromium focus. On every mouse click and mouse
move, check `NSApp.keyWindow?.firstResponder` — if the focused SurfaceView
changed, send focus/unfocus accordingly. This covers mouse-driven pane switches
immediately and keyboard-driven switches on the next mouse event.

#### Design

New state property:

```swift
/// The pane that currently has Chromium focus (at most one at a time).
private var chromiumFocusedPane: UUID? = nil
```

New helper method (runs on xpcQueue, called from main thread context):

```swift
/// Check the current first responder and update Chromium focus if needed.
/// Must be called with first-responder UUID already resolved on main thread.
private func updatePaneFocus(currentResponderUUID: UUID?) {
    // Nothing changed.
    if chromiumFocusedPane == currentResponderUUID { return }

    // Unfocus the old pane (if it was browsing).
    if let old = chromiumFocusedPane {
        sendFocusChanged(paneUUID: old, focused: false)
    }

    // Focus the new pane (only if it's browsing).
    if let new_ = currentResponderUUID, paneBrowsing[new_] == true {
        sendFocusChanged(paneUUID: new_, focused: true)
        chromiumFocusedPane = new_
    } else {
        chromiumFocusedPane = nil
    }
}
```

The first-responder check happens on the main thread (where `NSApp.keyWindow` is
valid). The UUID is passed into `updatePaneFocus` on the xpcQueue:

```swift
/// Resolve the focused pane UUID from the current first responder.
/// Must be called on the main thread.
private func focusedPaneUUID() -> UUID? {
    guard let window = NSApp.keyWindow,
          let surfaceView = window.firstResponder as? Ghostty.SurfaceView
    else { return nil }
    return surfaceView.id
}
```

#### Call sites

**1. Click monitor** — at the very top, before `hitTestOverlay`. Every click
(including clicks outside the overlay that switch pane focus) triggers a check:

```swift
// Check pane focus on every click (covers pane switching).
let responderUUID = self.focusedPaneUUID()
self.xpcQueue.async { self.updatePaneFocus(currentResponderUUID: responderUUID) }
```

This goes right after `guard event.window != nil else { return event }`, before
the `hitTestOverlay` call. It doesn't consume the event or change the click
routing — it's a side effect that runs asynchronously.

**2. Move monitor** — at the very top, before `hitTestOverlay`. Every mouse move
updates the focus check:

```swift
let responderUUID = self.focusedPaneUUID()
self.xpcQueue.async { self.updatePaneFocus(currentResponderUUID: responderUUID) }
```

Same placement: after `guard let self = self else { return event }`, before the
`hitTestOverlay` call.

#### Interaction with existing triggers

The mode-change triggers from Experiment 2 still work. When the TUI toggles
browse mode, `handleModeChanged` sends focus/unfocus AND now also sets
`chromiumFocusedPane`. The `updatePaneFocus` helper needs to stay in sync:

- `handleModeChanged` with `browsing=true`: also set
  `chromiumFocusedPane = uuid`.
- `handleModeChanged` with `browsing=false`: also set
  `chromiumFocusedPane = nil`.
- Ctrl+Esc handler: also set `chromiumFocusedPane = nil`.
- `handleSetOverlay` with `browsing=true`: also set
  `chromiumFocusedPane = uuid`.

This ensures `updatePaneFocus` sees the correct state and doesn't send duplicate
or conflicting messages.

#### Limitations

Keyboard-only pane switches (e.g., Cmd+] to move to next split) won't trigger
until the next mouse event. This is acceptable for now — the mouse is the
primary input device in browse mode.

#### Changes

##### CompositorXPC.swift

- Add `chromiumFocusedPane` state property.
- Add `focusedPaneUUID()` (main thread) and `updatePaneFocus()` (xpcQueue).
- Add focus check at top of click monitor and move monitor.
- Update `handleModeChanged`, Ctrl+Esc handler, and `handleSetOverlay` to
  maintain `chromiumFocusedPane`.

No Chromium changes — the `focus_changed` XPC handler from Experiment 2 is
reused as-is.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# Open two panes side by side, both running:
cargo run -p web -- https://google.com
```

1. Enter browse mode in pane A — blinking cursor appears.
2. Click on pane B (also in browse mode) — pane A's cursor stops blinking, pane
   B's cursor starts blinking.
3. Click back on pane A — pane B stops, pane A starts.
4. Click on a terminal pane (not browsing) — browsing pane's cursor stops.
5. Move mouse back to the browsing pane — cursor starts again on next move.

Pass: at most one pane has Chromium focus at a time, and it tracks pane
switches.

### Experiment 3: Pane focus via NSNotification

Use Ghostty's existing `focusDidChange` hook to broadcast an NSNotification when
a SurfaceView gains or loses focus. CompositorXPC observes it and sends
`focus_changed` to Chromium. This covers all pane-switch mechanisms — mouse
clicks, keyboard shortcuts, splits, focus-follows-mouse — because they all flow
through `becomeFirstResponder`/`resignFirstResponder` → `focusDidChange`.

#### How Ghostty tracks focus

```
User clicks pane B / presses Ctrl+L / etc.
    ↓
macOS calls pane A: resignFirstResponder() → focusDidChange(false)
macOS calls pane B: becomeFirstResponder() → focusDidChange(true)
    ↓
ghostty_surface_set_focus(surface, focused)
    ↓
Zig Surface.focusCallback() updates renderer + app state
```

`focusDidChange` (SurfaceView_AppKit.swift:430) is the single point where all
focus changes are processed. It's called after the `focused` guard check, so it
only fires on actual state changes (no duplicates).

#### Changes

##### SurfaceView_AppKit.swift (Ghostty modification)

Post an NSNotification from `focusDidChange`, right after
`ghostty_surface_set_focus`:

```swift
func focusDidChange(_ focused: Bool) {
    guard let surface = self.surface else { return }
    guard self.focused != focused else { return }
    self.focused = focused
    ghostty_surface_set_focus(surface, focused)

    // Notify observers (e.g. CompositorXPC) of pane focus changes.
    NotificationCenter.default.post(
        name: .surfaceFocusDidChange,
        object: self,
        userInfo: ["focused": focused])

    // ... rest of existing code unchanged ...
}
```

Add the notification name as an extension (at the top of the file or in a shared
location):

```swift
extension Notification.Name {
    static let surfaceFocusDidChange = Notification.Name("SurfaceFocusDidChange")
}
```

Two lines of functional code added to Ghostty. The notification carries the
SurfaceView as `object` (so the observer can read its `id`) and the focus state
in `userInfo`.

##### CompositorXPC.swift

New state property:

```swift
/// The pane that currently has Chromium focus (at most one at a time).
private var chromiumFocusedPane: UUID? = nil
```

Register the observer in `start()`, alongside the existing event monitors:

```swift
NotificationCenter.default.addObserver(
    forName: .surfaceFocusDidChange,
    object: nil,
    queue: nil
) { [weak self] notification in
    guard let self = self else { return }
    guard let surfaceView = notification.object as? Ghostty.SurfaceView,
          let focused = notification.userInfo?["focused"] as? Bool
    else { return }

    let uuid = surfaceView.id
    self.xpcQueue.async {
        guard self.paneBrowsing[uuid] != nil else { return }
        self.updatePaneFocus(paneUUID: uuid, focused: focused)
    }
}
```

The `paneBrowsing[uuid] != nil` guard ensures we only react to panes that have a
web overlay. Focus changes on terminal-only panes are ignored.

New helper on xpcQueue:

```swift
private func updatePaneFocus(paneUUID: UUID, focused: Bool) {
    if focused {
        // Unfocus the old pane first (at most one at a time).
        if let old = chromiumFocusedPane, old != paneUUID {
            sendFocusChanged(paneUUID: old, focused: false)
        }
        // Only focus if the pane is actually in browse mode.
        if paneBrowsing[paneUUID] == true {
            sendFocusChanged(paneUUID: paneUUID, focused: true)
            chromiumFocusedPane = paneUUID
        } else {
            chromiumFocusedPane = nil
        }
    } else {
        // Pane lost focus — unfocus if it was the active one.
        if chromiumFocusedPane == paneUUID {
            sendFocusChanged(paneUUID: paneUUID, focused: false)
            chromiumFocusedPane = nil
        }
    }
}
```

Update the existing mode-change triggers to also maintain `chromiumFocusedPane`:

- **`handleModeChanged`**: after `sendFocusChanged`, set
  `chromiumFocusedPane = browsing ? uuid : nil`.
- **Ctrl+Esc handler**: after `sendFocusChanged`, set
  `chromiumFocusedPane = nil`.
- **`handleSetOverlay`**: after `sendFocusChanged` (when `browsing` is true),
  set `chromiumFocusedPane = uuid`.

No Chromium changes — the `focus_changed` XPC handler from Experiment 2 is
reused as-is.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# Open two panes side by side (Cmd+D), both running:
cargo run -p web -- https://google.com
```

1. Enter browse mode in pane A — blinking cursor appears.
2. Press Ctrl+L (or your keybinding) to switch to pane B — pane A's cursor stops
   blinking.
3. Enter browse mode in pane B — pane B's cursor starts blinking.
4. Press Ctrl+H to switch back to pane A — pane B stops, pane A starts (already
   in browse mode).
5. Press Esc in pane A — cursor stops (mode exit, same as Experiment 2).
6. Click on a terminal-only pane — browsing pane's cursor stops.

Pass: focus tracks pane switches via keyboard shortcuts, mouse clicks, and all
other mechanisms. At most one pane has Chromium focus at a time.

#### Result: Pass

Focus now correctly tracks pane switches across all mechanisms — keyboard
shortcuts (Ctrl+H/J/K/L), mouse clicks, splits, and tab switches. The blinking
cursor appears when entering browse mode, disappears when exiting, and transfers
correctly between panes. At most one pane has Chromium focus at any time.

The key insight was hooking into Ghostty's existing `focusDidChange` via
NSNotification rather than trying to independently detect pane switches. This
covers every possible focus change path with just two lines of Ghostty
modification.

Combined with Experiments 1–2, the full focus lifecycle is now:

- **Mode transitions** (enter/exit browse): `handleModeChanged` + Ctrl+Esc
- **Pane switches** (any mechanism): NSNotification from `focusDidChange`
- **Initial connection** (already browsing): `handleSetOverlay`

All three feed into `updatePaneFocus`, which enforces single-pane-at-a-time
Chromium focus via the `chromiumFocusedPane` state variable.

### Experiment 4: Focus on initial tab creation

#### Problem

When first opening a web page (`cargo run -p web -- https://google.com`), the
webview doesn't receive focus. The user must press Esc then re-enter browse mode
for the Google search box to show a blinking cursor.

Root cause: in `handleSetOverlay`, the `sendFocusChanged` call fires before the
Chromium server has registered. The guard in `sendFocusChanged` checks
`paneProfiles[uuid]` (not yet set) and `serverControlConnections[profile]` (not
yet connected), so the message is silently dropped. `chromiumFocusedPane` is set
to the UUID, but the focus never reaches Chromium.

#### Changes

##### CompositorXPC.swift

In `handleServerRegister`, after sending `create_tab` for each pending tab, also
send focus if that pane is the `chromiumFocusedPane`:

```swift
// Flush all pending tabs for this profile.
for (uuid, pending) in pendingTabs {
    if pending.profile == profile {
        sendCreateTab(peer, paneId: uuid.uuidString, url: pending.url, uuid: uuid)
        // Send deferred focus if this pane was supposed to be focused.
        if chromiumFocusedPane == uuid {
            sendFocusChanged(paneUUID: uuid, focused: true)
        }
    }
}
```

This is the first moment the server can receive the message — right after
`create_tab`. No other changes needed.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a pane:
cargo run -p web -- https://google.com
```

Pass: Google search box shows blinking cursor immediately after the page loads,
without needing to toggle browse mode.

#### Result: Partial pass

Single pane works perfectly — opens focused, Esc loses focus, Enter regains it.
But opening a second pane on the same profile reveals deeper issues:

1. **Second pane blinks once then stops.** The `sendFocusChanged` call in
   `handleSetOverlay` fires before `paneProfiles[uuid]` is set (line ~449), so
   it always silently fails — for every pane, not just the first. The Experiment
   4 fix only covered the `handleServerRegister` path (first tab on a new
   server). When a second tab uses an already-registered server, it takes the
   direct `sendCreateTab` path in `handleSetOverlay`, which has no deferred
   focus. The one blink comes from the NSNotification (pane B gains AppKit
   focus), but the focus state is confused because...

2. **Pane A is never unfocused.** `chromiumFocusedPane` was set to B (in the
   broken early assignment) before A lost AppKit focus, so `updatePaneFocus`'s
   guard (`chromiumFocusedPane == A`) fails and the unfocus message is skipped.
   Both tabs end up focused simultaneously in Chromium.

3. **Double-speed blinking after closing pane A.** After the first pane's web
   process disconnects, the second pane blinks at 2x speed — likely two
   focus/blink sources applying to the remaining tab because pane A was never
   properly cleaned up in Chromium.

Root cause: the `sendFocusChanged` + `chromiumFocusedPane` assignment in
`handleSetOverlay` is fundamentally broken — it runs before `paneProfiles` is
set so the message never reaches Chromium, but `chromiumFocusedPane` is updated,
corrupting the focus tracking state.

Fix for next experiment: remove the broken early `sendFocusChanged` from
`handleSetOverlay` entirely. Instead, call `updatePaneFocus` after every
`sendCreateTab` (both the direct path in `handleSetOverlay` and the deferred
path in `handleServerRegister`). This sends focus only after the tab exists,
with proper single-pane enforcement.

### Experiment 5: Fix focus after create_tab

#### Problem

The `sendFocusChanged` + `chromiumFocusedPane` assignment in `handleSetOverlay`
fires before `paneProfiles` is set, so the focus message always silently fails.
The Experiment 4 fix only covered the `handleServerRegister` path (first tab on
a new server). When subsequent tabs use an already-registered server, the direct
`sendCreateTab` path in `handleSetOverlay` has no deferred focus. This corrupts
the `chromiumFocusedPane` state and causes multi-pane focus bugs.

#### Changes

##### CompositorXPC.swift

**1. Remove the broken early focus from `handleSetOverlay`.**

Replace:

```swift
// If already in browse mode at connection time, tell Chromium to focus.
if browsing {
    sendFocusChanged(paneUUID: uuid, focused: true)
    chromiumFocusedPane = uuid
}
```

With nothing — delete these lines entirely. Focus will be sent after
`create_tab` instead.

**2. Add `updatePaneFocus` after the direct `sendCreateTab` in
`handleSetOverlay`.**

```swift
if let controlConn = serverControlConnections[profile] {
    // Server already registered — send create_tab immediately.
    sendCreateTab(controlConn, paneId: paneIdStr, url: url, uuid: uuid)
    // Focus after tab exists, with proper single-pane enforcement.
    if paneBrowsing[uuid] == true {
        updatePaneFocus(paneUUID: uuid, focused: true)
    }
}
```

**3. Replace the raw `sendFocusChanged` in `handleServerRegister` with
`updatePaneFocus`.**

```swift
for (uuid, pending) in pendingTabs {
    if pending.profile == profile {
        sendCreateTab(peer, paneId: uuid.uuidString, url: pending.url, uuid: uuid)
        // Focus after tab exists, with proper single-pane enforcement.
        if paneBrowsing[uuid] == true {
            updatePaneFocus(paneUUID: uuid, focused: true)
        }
    }
}
```

This replaces the `chromiumFocusedPane == uuid` check with
`paneBrowsing[uuid]
== true`, which is the actual source of truth for whether a
pane should be focused. `updatePaneFocus` handles unfocusing the old pane and
setting `chromiumFocusedPane`.

No Chromium changes needed.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# Open two panes side by side (Cmd+D), both running:
cargo run -p web -- https://google.com
```

1. Pane A opens — blinking cursor appears immediately (no toggle needed).
2. Pane B opens — pane A's cursor stops, pane B's cursor starts.
3. Switch panes via Ctrl+H/L — focus transfers correctly.
4. Close pane A — pane B blinks at normal speed.
5. Esc/Enter in remaining pane — focus toggles correctly.

Pass: focus is correctly sent after `create_tab`, with proper single-pane
enforcement, for both new-server and existing-server paths.

#### Result: Pass

Focus now works correctly for multi-pane scenarios. Pane A receives focus
immediately on open, pane B steals focus from A when it opens, pane switches
transfer focus, closing a pane leaves the remaining one at normal blink speed,
and Esc/Enter toggles work as expected.

The fix was moving focus delivery from the broken pre-`paneProfiles` location in
`handleSetOverlay` to after `sendCreateTab` in both code paths, using
`updatePaneFocus` for proper single-pane enforcement. Combined with Experiments
1–4, the full focus lifecycle is now complete:

- **Tab creation** (new server or existing): `updatePaneFocus` after
  `sendCreateTab`
- **Mode transitions** (enter/exit browse): `handleModeChanged` + Ctrl+Esc
- **Pane switches** (any mechanism): NSNotification from `focusDidChange`

### Experiment 6: Drag events via flag suppression

#### Problem

The mouse click monitor consumes `leftMouseDown` (returns `nil`), which prevents
macOS from generating `.leftMouseDragged` events. Without drag events, Chromium
cannot track text selection. The logs confirm: only `Mouse down` and `Mouse up`
arrive at Chromium — no move events between them.

The root cause is architectural: returning `nil` from a local event monitor
removes the event from the responder chain entirely. macOS only generates drag
events when a view's `mouseDown:` handler has started a drag tracking session.
With mouseDown consumed, no view receives it, no tracking session exists.

#### Approach

Stop consuming left mouse events. Instead, return them so the responder chain
processes normally and drag tracking starts. But suppress the terminal from
acting on them by setting a flag on the SurfaceView.

The sequence:

1. Monitor fires on `leftMouseDown` (main thread)
2. Hit-test finds overlay match → get SurfaceView, set
   `suppressMouseForOverlay = true`
3. Forward to Chromium via XPC
4. **Return the event** (don't consume) → event enters responder chain
5. `SurfaceView.mouseDown` fires → checks flag → returns early (terminal
   ignores)
6. macOS starts drag tracking (view received mouseDown)
7. `.leftMouseDragged` events generated → existing drag monitor forwards to
   Chromium, consumes them (terminal never sees drags)
8. `leftMouseUp` arrives → monitor forwards to Chromium, returns event
9. `SurfaceView.mouseUp` fires → checks flag → clears flag, returns early

Right mouse events keep the existing consume behavior (no drag tracking needed).
Scroll events unchanged. The `.leftMouseDragged` in the move/drag monitor stays
consumed — drag events are generated regardless of what the monitor does with
them; only the initial `mouseDown` delivery matters.

#### Changes

##### SurfaceView_AppKit.swift (Ghostty modification)

Add a property to SurfaceView (alongside `focused`, `prevPressureStage`, etc.):

```swift
/// Set by CompositorXPC when a mouse event targets the browser overlay.
/// Suppresses terminal mouse handling so the event can drive drag tracking.
var suppressMouseForOverlay: Bool = false
```

Guard in `mouseDown`:

```swift
override func mouseDown(with event: NSEvent) {
    guard !suppressMouseForOverlay else { return }
    guard let surface = self.surface else { return }
    // ... existing code ...
}
```

Guard in `mouseUp` (clear the flag):

```swift
override func mouseUp(with event: NSEvent) {
    if suppressMouseForOverlay {
        suppressMouseForOverlay = false
        return
    }
    // ... existing code ...
}
```

Guard in `mouseDragged` (in case macOS delivers any before the monitor):

```swift
override func mouseDragged(with event: NSEvent) {
    guard !suppressMouseForOverlay else { return }
    self.mouseMoved(with: event)
}
```

Three one-line guards added to Ghostty. The flag is set/cleared on the main
thread (monitors and view handlers both run on main), so no synchronization
needed.

##### CompositorXPC.swift

In the mouse click monitor, split left and right mouse handling. For left mouse
events over the overlay, set the flag and return the event instead of nil:

```swift
// For left mouse events: set flag and return event (preserve drag tracking).
if event.type == .leftMouseDown || event.type == .leftMouseUp {
    let surfaceView: Ghostty.SurfaceView? = self.xpcQueue.sync {
        self.paneSurfaceViews[hit.uuid]
    }
    if event.type == .leftMouseDown {
        surfaceView?.suppressMouseForOverlay = true
    }

    // Forward to Chromium (async).
    self.xpcQueue.async { /* ... existing XPC send ... */ }

    return event  // DON'T consume — allows drag tracking
}

// For right mouse events: consume as before.
// ... existing code returning nil ...
```

No changes to the move/drag monitor — `.leftMouseDragged` stays consumed. No
Chromium changes.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a pane:
cargo run -p web -- https://google.com
```

1. Enter browse mode, click and drag across "About" link text at the bottom of
   Google's page.
2. Check logs for `mouse_move` events between `Mouse down` and `Mouse up`.
3. Check screen for blue selection highlight.

Pass: drag events reach Chromium (visible in logs), and selection highlight
appears on the rendered page.

#### Result: Pass

Text selection via click-and-drag works. Blue selection highlights appear on the
rendered page.

The experiment required fixing two independent bugs:

1. **Flag suppression works.** Returning left mouse events from the monitor
   (instead of consuming) preserves macOS drag tracking. The
   `suppressMouseForOverlay` flag on SurfaceView prevents the terminal from
   processing the click. `.leftMouseDragged` events are generated by macOS and
   intercepted by the existing move/drag monitor.

2. **Wrong modifier values.** The button-down flags in the move/drag monitor
   used incorrect values: `32` for `kLeftButtonDown` and `512` for
   `kRightButtonDown`. The actual Blink values are
   `kLeftButtonDown = 1 << 6 =
   64` and `kRightButtonDown = 1 << 8 = 256`. The
   old values mapped to `kIsAutoRepeat` and `kCapsLockOn`, so Chromium treated
   every drag as a plain mouse move with no button held — never initiating
   selection.

Combined with the focus lifecycle (Experiments 1–5), all prerequisites for text
selection are now met: focus is active (Blink paints highlights), drag events
are delivered (macOS generates them), and button state is correct (Blink knows
the left button is held).

## Conclusion

Text selection via click-and-drag is working. Both problems identified at the
start of this issue are solved.

### What we built

**Focus lifecycle** (Experiments 1–5): Chromium's `FocusController::IsActive()`
must be true for Blink to paint selection highlights, carets, and focus rings.
We implemented a complete focus lifecycle that tracks:

- **Tab creation**: `updatePaneFocus` after `sendCreateTab` (both new-server and
  existing-server paths)
- **Mode transitions**: `handleModeChanged` sends focus on enter, unfocus on
  exit; Ctrl+Esc handler does the same
- **Pane switches**: NSNotification from Ghostty's `focusDidChange` (covers
  keyboard shortcuts, mouse clicks, splits, and all other mechanisms)

At most one pane has Chromium focus at a time, enforced by
`updatePaneFocus`/`chromiumFocusedPane`.

**Drag event delivery** (Experiment 6): Left mouse events are no longer consumed
by the monitor. Instead, they're returned to the responder chain with a
`suppressMouseForOverlay` flag on SurfaceView that prevents the terminal from
processing them. This preserves macOS drag tracking — `.leftMouseDragged` events
are generated naturally and forwarded to Chromium by the existing move/drag
monitor.

**Modifier fix** (Experiment 6): The button-down flags for drag events used
wrong values (`32`/`512` instead of `64`/`256`). This mapped to `kIsAutoRepeat`
and `kCapsLockOn` instead of `kLeftButtonDown` and `kRightButtonDown`, so
Chromium treated every drag as a plain move.

### Experiments

| # | Description                              | Result                              |
| - | ---------------------------------------- | ----------------------------------- |
| 1 | Focus + SetActive on tab creation        | Partial (never unfocuses)           |
| 2 | Focus lifecycle tied to mode changes     | Partial (pane switches not tracked) |
| 3 | Pane focus via NSNotification            | Pass                                |
| 4 | Deferred focus after create_tab          | Partial (only first tab)            |
| 5 | Fix focus after create_tab (both paths)  | Pass                                |
| 6 | Drag via flag suppression + modifier fix | Pass                                |

### Files changed

**Ghostty (2 files):**

- `SurfaceView_AppKit.swift` — `Notification.Name.surfaceFocusDidChange`,
  notification post in `focusDidChange`, `suppressMouseForOverlay` flag with
  guards in `mouseDown`/`mouseUp`/`mouseDragged`
- `CompositorXPC.swift` — `chromiumFocusedPane` state, NSNotification observer,
  `updatePaneFocus` helper, deferred focus after `sendCreateTab`, flag
  suppression for left mouse events, corrected Blink modifier values

**Chromium (branch `146.0.7650.0-issue-515`):**

- `shell_browser_main_parts.cc` — `focus_changed` XPC handler,
  `HandleFocusChanged` method
- `shell_browser_main_parts.h` — `HandleFocusChanged` declaration

### What's next

- **Keyboard input** (Issue 516?) — typing in input fields, keyboard shortcuts
- **Copy/paste** — Cmd+C to copy selected text
- **Triple-click** — select full line/paragraph
- **Drag outside overlay bounds** — selection while dragging past the edge
