# Keybindings Architecture (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x (Ghostty + WKWebView).
> TermSurf 2.0 will have different keybinding architecture based on WezTerm.

## libghostty Keybindings

libghostty (the Zig core) owns the keybinding system for terminal operations:

1. **Config parsing** - Keybindings defined in `~/.config/ghostty/config` (e.g.,
   `keybind = ctrl+t=new_tab`)
2. **Action dispatch** - When a key is pressed in a terminal surface, libghostty
   matches it against bindings and fires an action
3. **App runtime handles actions** - Swift receives action callbacks (e.g.,
   `GHOSTTY_ACTION_NEW_TAB`) and implements the behavior

Key files:

- `src/config/Config.zig` - Keybinding config parsing
- `src/input/Binding.zig` - Trigger-to-action mapping
- `src/apprt/action.zig` - Action enum (quit, new_tab, goto_split, etc.)
- `termsurf-macos/Sources/Ghostty/Ghostty.App.swift` - Action handlers

This system assumes keyboard input flows through a terminal surface, which
passes events to libghostty.

## TermSurf Webview Keybindings

Webviews introduce a problem: when WKWebView is focused, keyboard events go to
the browser, not libghostty. We handle this with a **modal approach**:

### Three Modes

1. **Control mode** (terminal keybindings work)
   - SurfaceView is the first responder
   - All ghostty keybindings work naturally (pane navigation, splits, etc.)
   - Enter switches to browse mode
   - i switches to insert mode (edit URL)
   - ctrl+c closes the webview
   - ControlBar displays: "i to edit, Enter to browse, Ctrl+C to close"

2. **Browse mode** (browser has focus, ghostty keybindings still work)
   - WKWebView is the first responder
   - Most keys go to the browser
   - Ghostty keybindings are intercepted via local event monitor and processed
   - Ctrl+C (intercepted via local event monitor) switches to control mode
   - ControlBar displays: "Ctrl+C to control"

3. **Insert mode** (edit URL)
   - URL text field is the first responder
   - Normal text editing controls work (arrow keys, selection, etc.)
   - URL is selected by default when entering insert mode
   - Enter navigates to the URL and switches to browse mode
   - Esc cancels editing, restores original URL, switches to control mode
   - ControlBar displays: "Enter to go, Esc to cancel"

### Implementation

**Control mode** keybindings are handled in `SurfaceView_AppKit.swift`:

- At the start of `keyDown()`, check if a WebViewContainer subview exists
- If so, intercept Enter, i, and ctrl+c before passing to libghostty
- All other keys flow through to libghostty normally

**Browse mode** keybindings are handled via a local event monitor in
`WebViewContainer.swift`:

- `NSEvent.addLocalMonitorForEvents` intercepts all keyDown events
- When in browse mode, checks if the key matches a ghostty keybinding
- If it's a keybinding, routes to SurfaceView and consumes the event
- If not, lets the event pass through to WKWebView
- Ctrl+C is always intercepted to exit browse mode (switches to control mode)
- This is invisible to websites and cannot be overridden by them

**Insert mode** keybindings are handled in `ControlBar.swift`:

- ControlBar implements `NSTextFieldDelegate`
- `control(_:textView:doCommandBy:)` intercepts Enter and Esc
- Enter triggers `onURLSubmitted` callback with the edited URL string
- Esc triggers `onInsertCancelled` callback and restores the original URL
- WebViewContainer wires these callbacks to navigate and switch modes

**ControlBar** (`ControlBar.swift`):

- Displays URL on the left (monospace font, truncates with ellipsis)
- Displays mode-specific hint text on the right
- In insert mode, URL field becomes editable with text selected

### Focus State Synchronization

When switching between panes, ghostty makes the target pane's SurfaceView the
first responder. If returning to a pane with a webview, WebViewContainer's
internal `focusMode` may be stale (still set to `.browse` from before).

This is handled in `SurfaceView_AppKit.swift`:

```swift
if let container = subviews.first(where: { $0 is WebViewContainer }) ... {
    // If SurfaceView is receiving keys but container thinks it's in browse mode,
    // sync the state
    if !container.isControlMode {
        container.syncToControlMode()
    }
    // Then handle Enter/i/ctrl+c
}
```

`syncToControlMode()` updates the internal state and control bar text without
changing the first responder (since SurfaceView already has focus).

### Why Keep SurfaceView as First Responder?

The key insight: keeping SurfaceView as first responder in control mode means
**all ghostty keybindings work automatically**. Events flow through SurfaceView
→ libghostty → action dispatch, just like a normal terminal pane.

Previous attempts to make a separate view the first responder required
forwarding key events back to SurfaceView, which broke due to focus guards in
the responder chain.

### URL Normalization

When a URL is submitted from insert mode, it is normalized before navigation:

- URLs without a scheme (e.g., `example.com`) get `https://` prepended
- URLs with `http://`, `https://`, or `file://` are used as-is

### Current Hardcoded Bindings

| Context   | Key       | Action                            |
| --------- | --------- | --------------------------------- |
| Control   | Enter     | Switch to browse                  |
| Control   | i         | Switch to insert (edit URL)       |
| Control   | ctrl+c    | Close webview                     |
| Browse    | ctrl+c    | Switch to control                 |
| All modes | cmd+c     | Copy (via menu action)            |
| All modes | cmd+x     | Cut (via menu action)             |
| All modes | cmd+v     | Paste (via menu action)           |
| All modes | cmd+r     | Refresh webview                   |
| All modes | cmd+=     | Zoom in (10%)                     |
| All modes | cmd+-     | Zoom out (10%)                    |
| All modes | cmd+0     | Reset zoom to 100%                |
| All modes | cmd+alt+i | Open Safari Web Inspector         |
| Insert    | Enter     | Navigate to URL, switch to browse |
| Insert    | Esc       | Cancel edit, switch to control    |

These are not configurable via ghostty config. This may change in the future if
we add TermSurf-specific configuration.

## AppKit Keyboard Event Types

AppKit has two completely separate code paths for keyboard events:

1. **`keyDown`** - Regular key events (letters, arrows, shift+arrow, escape,
   etc.)
2. **`performKeyEquivalent`** - Command-key events (cmd+c, cmd+v, cmd+a, etc.)

Understanding this distinction is critical for handling keyboard input when both
terminal and webview need to coexist.

### keyDown Flow

```
User presses key (e.g., shift+arrow)
    ↓
First responder receives keyDown
    ↓
If not handled, bubbles up responder chain
```

### performKeyEquivalent Flow

```
User presses cmd+key (e.g., cmd+c)
    ↓
First responder receives performKeyEquivalent
    ↓
If returns true → event consumed
If returns false → bubbles up, eventually becomes menu action
```

### Local Event Monitors

For keys that need to be intercepted before the first responder sees them,
`performKeyEquivalent` may not be sufficient. The solution is
`NSEvent.addLocalMonitorForEvents`:

```
User presses key (e.g., Ctrl+C)
    ↓
Local event monitor intercepts BEFORE any view
    ↓
If returns nil → event consumed, no view sees it
If returns event → normal processing continues
```

This is how we handle Ctrl+C in browse mode: the monitor in `WebViewContainer`
intercepts Ctrl+C before WKWebView can see it, making it invisible to websites
and impossible for them to override.

### Critical: Two-Level Focus Check

Local event monitors are **app-global**—they fire for ALL keyDown events across
all windows. When multiple tabs have webviews, each WebViewContainer has its own
monitor running. Without proper guards, an inactive tab's monitor can intercept
keys meant for the active tab.

**Required checks before handling any key in a local event monitor:**

1. **Active window/tab check (`isKeyWindow`)** - Is our window the key window?
2. **Active pane check (`firstResponder`)** - Is the first responder in our
   hierarchy?

```swift
// In the local event monitor closure:

// CHECK 1: Only handle if our window is the key window (active tab/window)
guard self.window?.isKeyWindow ?? false else { return event }

// CHECK 2: Only handle if first responder is in our view hierarchy (active pane)
guard let firstResponder = self.window?.firstResponder as? NSView else { return event }
let isFocusedHierarchy =
  firstResponder === self.superview  // SurfaceView (control mode)
  || firstResponder.isDescendant(of: self)  // WebView, URL field, etc.
guard isFocusedHierarchy else { return event }
```

**Why `isKeyWindow` covers both tabs and windows:**

In TermSurf/Ghostty, each tab is a separate `NSWindow` grouped via
`NSWindowTabGroup`. The "visual window" you see is actually multiple NSWindows
in a tab group. There is only one key window across the entire application at
any time—the window receiving keyboard input. This means `isKeyWindow` will be
`false` for:

- Inactive tabs in the same tab group (visual window)
- Tabs/windows in a different window entirely
- All windows when the app is in the background

No separate "active window" check is needed beyond `isKeyWindow`.

**Why both checks are necessary:**

| Scenario                   | isKeyWindow | firstResponder in hierarchy | Should handle? |
| -------------------------- | ----------- | --------------------------- | -------------- |
| Active tab, active pane    | ✓           | ✓                           | Yes            |
| Active tab, different pane | ✓           | ✗                           | No             |
| Inactive tab (same window) | ✗           | (irrelevant)                | No             |
| Different window entirely  | ✗           | (irrelevant)                | No             |

**Bug history:** Originally only the pane check existed. This caused issues when
webviews were open on multiple tabs—the inactive tab's monitor would intercept
keys because its window still had a valid firstResponder pointing to its own
view hierarchy.

## Ghostty Keybindings and Mode Priority

Keybinding priority differs by mode:

| Mode        | Priority      | Behavior                                                   |
| ----------- | ------------- | ---------------------------------------------------------- |
| **Browse**  | Webview first | Webview keybindings work; ghostty gets unhandled keys      |
| **Control** | Ghostty first | All ghostty keybindings work; webview doesn't receive keys |
| **Insert**  | URL field     | Normal text editing in URL field                           |

**Special case**: Ctrl+C is ALWAYS intercepted in browse mode to exit to control
mode. This ensures the user can always regain full control of keybindings.

### Implementation

Two mechanisms work together:

**1. Local Event Monitor** (in `WebViewContainer.swift`)

Intercepts `keyDown` events before any view sees them:

```swift
// First: two-level focus check (see "Critical: Two-Level Focus Check" above)
guard self.window?.isKeyWindow ?? false else { return event }
guard let firstResponder = self.window?.firstResponder as? NSView else { return event }
let isFocusedHierarchy = firstResponder === self.superview || firstResponder.isDescendant(of: self)
guard isFocusedHierarchy else { return event }

// Then: mode-specific handling
switch self.focusMode {
case .browse:
    // Intercept Ctrl+C to exit browse mode
    if event.modifierFlags.contains(.control) && event.charactersIgnoringModifiers == "c" {
        self.focusControlBar()
        return nil
    }
    return event  // Let webview handle other keys

case .control:
    // Ghostty has priority - check keybindings first
    if let surfaceView = self.superview as? Ghostty.SurfaceView {
        if surfaceView.processKeyBindingIfMatched(event) {
            return nil  // Ghostty handled it
        }
    }
    return event  // Let SurfaceView handle (Enter, i, ctrl+c, etc.)

case .insert:
    return event  // URL field handles keys
}
```

**2. performKeyEquivalent Override** (in `WebViewContainer.swift`)

Catches modifier keys (ctrl+key, cmd+key) that webview doesn't handle:

```swift
override func performKeyEquivalent(with event: NSEvent) -> Bool {
    if focusMode == .browse {
        // Let webview try first
        if super.performKeyEquivalent(with: event) {
            return true  // Webview handled it
        }

        // Webview didn't handle - check ghostty keybinding
        if let surfaceView = superview as? Ghostty.SurfaceView {
            if surfaceView.processKeyBindingIfMatched(event) {
                return true  // Ghostty handled it
            }
        }
    }
    return super.performKeyEquivalent(with: event)
}
```

**3. processKeyBindingIfMatched** (in `SurfaceView_AppKit.swift`)

Checks if a key matches a ghostty keybinding and processes it:

```swift
func processKeyBindingIfMatched(_ event: NSEvent) -> Bool {
    guard let surface = self.surface else { return false }

    var ghosttyEvent = event.ghosttyKeyEvent(GHOSTTY_ACTION_PRESS)
    let isBinding = (event.characters ?? "").withCString { ptr in
        ghosttyEvent.text = ptr
        return ghostty_surface_key_is_binding(surface, ghosttyEvent)
    }

    guard isBinding else { return false }

    _ = (event.characters ?? "").withCString { ptr in
        ghosttyEvent.text = ptr
        return ghostty_surface_key(surface, ghosttyEvent)
    }

    return true
}
```

### Why This Architecture?

- **Browse mode**: Webview keybindings work correctly. Ghostty only handles keys
  the webview doesn't use.
- **Control mode**: User has full control of ghostty keybindings regardless of
  what the webview might want.
- **Ctrl+C guarantee**: User can always exit browse mode via Ctrl+C, ensuring
  they're never "trapped" in a webview that consumes all keys.

## SurfaceView Key Handling Implementation

When a webview is visible, SurfaceView must decide which keys to handle itself
(for terminal) vs which to let the webview handle.

### Regular Keys (keyDown)

In `SurfaceView_AppKit.swift`, `keyDown` checks for webview presence:

```swift
if let container = subviews.last(where: { $0 is WebViewContainer }) ... {
    if container.isControlMode {
        // Handle control mode special keys: Enter, i, ctrl+c
        // ...
    }
    // Webview visible - return early, let first responder (webview) handle
    return
}
// No webview - send to terminal via libghostty
```

This works because WKWebView correctly handles regular key events when it's the
first responder.

### Command Keys (performKeyEquivalent)

Command keys are trickier due to a **WKWebView quirk**:

> WKWebView's `performKeyEquivalent` claims cmd+c/x/v (returns `true`) but
> doesn't actually execute the copy/cut operation. However, WKWebView's `copy:`
> action method works correctly when triggered via the Edit menu.

The workaround is to intercept cmd+c/x/v and convert them to menu actions. We
also intercept ctrl+c here: in control mode it closes the webview, in browse
mode the local event monitor handles switching to control mode:

```swift
override func performKeyEquivalent(with event: NSEvent) -> Bool {
    if let container = subviews.last(where: { $0 is WebViewContainer }) ... {
        // Handle cmd+c/x/v via menu actions (WKWebView bug workaround)
        if hasCmd && !hasOpt {
            switch char {
            case "c":
                NSApp.sendAction(#selector(NSText.copy(_:)), to: nil, from: self)
                return true
            case "x":
                NSApp.sendAction(#selector(NSText.cut(_:)), to: nil, from: self)
                return true
            case "v":
                NSApp.sendAction(#selector(NSText.paste(_:)), to: nil, from: self)
                return true
            default:
                break
            }
        }

        // Handle ctrl+c: close in control mode, mode switch handled by local monitor
        let hasCtrl = event.modifierFlags.contains(.control)
        if hasCtrl && !hasCmd && !hasOpt && char == "c" {
            if container.isControlMode {
                container.onClose?(container.webviewId, 0)
            }
            return true  // Consume event in both modes
        }

        // Other keys: return false to let menu system handle (for Ghostty keybindings)
        return false
    }
    // No webview - send to terminal
}
```

### Menu Item Validation

To ensure the menu action reaches WKWebView (not SurfaceView), we also return
`false` from `validateMenuItem` for copy/cut/paste when a webview is visible:

```swift
func validateMenuItem(_ item: NSMenuItem) -> Bool {
    if subviews.contains(where: { $0 is WebViewContainer }) {
        switch item.action {
        case #selector(copy(_:)), #selector(cut(_:)), #selector(paste(_:)):
            return false  // Don't claim these - let webview handle
        default:
            break
        }
    }
    // ... rest of validation
}
```

This tells AppKit "SurfaceView can't handle copy/cut/paste right now" so the
action continues down the responder chain to WKWebView.

## Pattern for Future Keybindings

When adding new keybindings that need to work in webviews:

1. **Regular keys** - Let first responder handle by returning early from
   `keyDown`
2. **Command keys that WKWebView handles correctly** - Return `false` from
   `performKeyEquivalent` to let them flow normally
3. **Command keys that WKWebView breaks** - Intercept in `performKeyEquivalent`
   and convert to `NSApp.sendAction` to trigger the menu action directly
4. **Keys that must be intercepted before any view** - Use a local event monitor
   (`NSEvent.addLocalMonitorForEvents`). This intercepts events before any view
   sees them. Use sparingly—only when the key absolutely must not reach the
   first responder (e.g., Ctrl+C in browse mode to switch modes).

**Critical:** When using local event monitors, always implement the two-level
focus check (see "Critical: Two-Level Focus Check" above). Monitors are
app-global and will fire for events in ALL windows/tabs. Without the
`isKeyWindow` and `firstResponder` checks, inactive tabs will incorrectly
intercept keystrokes.
