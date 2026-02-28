# Issue 663: JavaScript Context Menu

Display a browser context menu by injecting HTML/CSS/JS into the page, avoiding
the focus-stealing NSMenu problem entirely.

## Problem

Issue 662 explored showing a native NSMenu for the browser pane but was deferred
due to complexity: Chromium can't show its own NSMenu without stealing focus
(Issue 616 Exp 9), and intercepting right-clicks in Ghostty's AppKit event chain
has tricky timing issues between `menu(for:)` and `rightMouseDown`.

## Solution

Inject JavaScript into the page to create a DOM-based context menu. The menu is
a positioned `<div>` rendered as part of the page content — no separate window,
no process activation, no focus loss. Chromium renders it via CALayerHost like
everything else. All mouse clicks are already forwarded to the browser, so
interacting with the menu just works.

### Why this is simpler

- **No Zig changes.** No new C API exports, no flag passing, no event
  interception.
- **No Swift changes.** No `menu(for:)` modifications, no NSMenu construction.
- **No XPC round-trip for navigation.** Menu items call `window.history.back()`,
  `window.history.forward()`, and `location.reload()` directly in JavaScript.
- **No coordinate mapping.** The click coordinates are already in the page's
  coordinate space (`ContextMenuParams.x`, `ContextMenuParams.y`).
- **No focus issues.** The menu is a DOM element inside the page, composited by
  Window Server via CALayerHost like all other page content.

### Changes

In `chromium/src/content/chromium_profile_server/browser/`:

1. **Re-enable `ShowContextMenu`** — remove the early `return;` added in Issue
   616 Experiment 9. Replace it with a call to inject JavaScript.

2. **Inject context menu JavaScript** — call
   `WebContents::GetMainFrame()->ExecuteJavaScript()` with code that:
   - Creates a positioned `<div>` at `(params.x, params.y)` styled as a context
     menu (shadow, rounded corners, appropriate colors)
   - Adds menu items: Back, Forward, Reload
   - Each item calls its JavaScript equivalent (`history.back()`,
     `history.forward()`, `location.reload()`)
   - Dismisses itself on click-away via
     `document.addEventListener('click', ...)`
   - Removes itself from the DOM after an item is selected

### Concerns

- **`history.back()` limitations** — may not work if no history exists (first
  page loaded). Could fall back to C++ `GoBack()` via a custom message channel
  if needed.
- **Page CSP** — Content Security Policy on some pages might block inline
  scripts. `ExecuteJavaScript()` runs in the main world and should bypass CSP,
  but needs verification.
- **Styling conflicts** — the injected menu's CSS could theoretically conflict
  with the page's styles. Using highly specific selectors or shadow DOM would
  mitigate this.
- **Scroll position** — `params.x`/`params.y` are relative to the viewport, so
  the menu should be positioned with `position: fixed` to stay at the click
  point regardless of scroll.
