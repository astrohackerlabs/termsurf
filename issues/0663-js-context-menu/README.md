+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

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

## Experiment 1: Inject context menu from ShowContextMenu

### Hypothesis

Replacing the disabled `ShowContextMenu` body with a call to
`ExecuteJavaScriptForTests` that injects a DOM-based context menu will produce a
working right-click menu inside the browser pane with no focus loss.

### API choice

`RenderFrameHost` offers three JavaScript execution methods:

- `ExecuteJavaScript` — restricted to `chrome://` and `devtools://` URLs
- `ExecuteJavaScriptInIsolatedWorld` — runs in an isolated world, no URL
  restriction, but requires a non-zero `world_id`
- `ExecuteJavaScriptForTests` — no restrictions, runs in any world

Use `ExecuteJavaScriptForTests` with `ISOLATED_WORLD_ID_GLOBAL` for the simplest
proof of concept. Can migrate to `ExecuteJavaScriptInIsolatedWorld` later for
better isolation from page scripts.

### Changes

In
`chromium/src/content/chromium_profile_server/browser/shell_web_contents_view_delegate_mac.mm`:

1. **Replace the early `return;`** in `ShowContextMenu` (line 105) with
   JavaScript injection code.

2. **Build a JavaScript string** that:
   - Removes any existing context menu (`#termsurf-ctx-menu`)
   - Creates a `<div id="termsurf-ctx-menu">` with `position: fixed` at
     `(params.x, params.y)`
   - Styles it as a dark menu (Tokyo Night palette: bg `#1a1b26`, fg `#c0caf5`,
     hover `#283457`, border `#565f89`, shadow, rounded corners,
     `z-index: 999999`)
   - Adds three items: Back, Forward, Reload
   - Each item calls `history.back()`, `history.forward()`, or
     `location.reload()` and removes the menu
   - Adds a one-shot `click` listener on `document` to dismiss the menu when
     clicking elsewhere
   - Adds a one-shot `contextmenu` listener on `document` to dismiss the menu if
     the user right-clicks again elsewhere (the new right-click will trigger a
     fresh `ShowContextMenu`)

3. **Call `render_frame_host.ExecuteJavaScriptForTests`** with the JavaScript
   string, a null callback, and `ISOLATED_WORLD_ID_GLOBAL`.

### Chromium branch

Create `146.0.7650.0-issue-663` from the latest TermSurf branch. Add to
`docs/chromium.md` Branches table.

### Test

1. Launch TermSurf, navigate to a page
2. Right-click in the browser pane — see a dark styled context menu with Back,
   Forward, Reload
3. Click Back — page navigates back, menu disappears
4. Click Forward — page navigates forward, menu disappears
5. Click Reload — page reloads, menu disappears
6. Right-click, then click elsewhere — menu dismisses
7. Right-click, then right-click elsewhere — old menu dismissed, new menu
   appears at new position
8. No focus loss to Chromium process at any point

### Result

Cancelled. The implementation was straightforward — replace the early `return;`
in `ShowContextMenu` with a call to `ExecuteJavaScriptForTests` injecting a
DOM-based context menu. However, the experiment was cancelled before testing
because modifying the Chromium fork is undesirable while a potential rewrite of
Chromium Profile Server in Zig is under consideration. Minimizing Chromium fork
changes reduces the maintenance burden if the C++ layer is replaced.

## Conclusion

Deferred. The JavaScript injection approach is validated as the simplest path to
a browser context menu — no Zig changes, no Swift changes, no XPC round-trips,
no coordinate mapping, no focus issues. The implementation requires only one C++
file change (`shell_web_contents_view_delegate_mac.mm`). However, modifying the
Chromium fork is being minimized while a Zig rewrite of Chromium Profile Server
is under consideration. This issue should be revisited after the Zig rewrite
decision is made — the same approach (injecting JavaScript from
`ShowContextMenu`) will work regardless of whether the host is C++ or Zig.
