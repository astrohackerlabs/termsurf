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
