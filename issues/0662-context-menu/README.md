+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 662: Browser Context Menu

Add a right-click context menu to the browser pane with Back, Forward, and
Reload options.

## Problem

Right-clicking in the browser pane forwards the mouse event to Chromium, but
Chromium's context menu was intentionally disabled in Issue 616 Experiment 9
because it opened as a separate window and stole focus. Users have no way to
navigate back/forward or reload without keyboard shortcuts.

## Architecture

The context menu spans three layers:

### C++ (Chromium fork)

Chromium's content layer generates context menu requests via
`WebContentsDelegate::HandleContextMenu()`. The Chromium fork needs to intercept
this callback and send an XPC message back to TermSurf with the right-click
coordinates. The menu items (Back, Forward, Reload) can be hardcoded on the
TermSurf side initially — no need to serialize Chromium's full menu model.

### Zig (gui/src/)

Receives the XPC message from Chromium indicating a context menu was requested.
Calls a C API export to tell Swift to display the menu at the given coordinates.
Receives the user's selection back and acts on it (e.g., sends a navigation
command back to Chromium via XPC).

### Swift (gui/macos/)

Displays a native `NSMenu` via `NSMenu.popUp(positioning:at:in:)` on the surface
view. This is the simplest path — native macOS context menu with zero custom
rendering. Fits the "thin wrapper" pattern: Zig says "show this menu at these
coordinates," Swift shows it, returns the selection.

## Flow

1. User right-clicks in browser pane
2. Zig forwards mouse event to Chromium via XPC (already works)
3. Chromium's `HandleContextMenu` fires → sends XPC message back to TermSurf
4. Zig receives context menu XPC message → calls C API export
5. Swift pops native `NSMenu` at click coordinates
6. User selects an item (Back / Forward / Reload)
7. Swift returns selection to Zig via callback
8. Zig sends navigation command to Chromium via XPC

## Menu Items

Initial menu (hardcoded on TermSurf side):

- **Back** — navigate back
- **Forward** — navigate forward
- **Reload** — reload current page

Future additions (not in scope): Copy Link, Open in New Tab, Inspect Element.

## Experiment 1: Research

### Hypothesis

Before implementation, we need to understand how each layer handles context
menus today and what modifications are required.

### Research needed

1. **Chromium `HandleContextMenu`** — find where Content Shell implements (or
   stubs) `WebContentsDelegate::HandleContextMenu()`. Determine what parameters
   it receives (coordinates, menu model) and what the simplest intercept looks
   like.

2. **Existing XPC message types** — review the current XPC protocol between
   Chromium and TermSurf. Determine how to add a new message type for context
   menu requests (format, keys, naming convention).

3. **Zig → Swift context menu path** — check whether Ghostty already has any
   context menu or `NSMenu` infrastructure in the Swift layer. Determine what C
   API export is needed for Zig to trigger a menu popup.

4. **Coordinate mapping** — understand how Chromium's content coordinates map to
   the macOS window coordinates needed by `NSMenu.popUp(positioning:at:in:)`.
   Check whether the existing mouse coordinate transform can be reused.

5. **Right-click suppression** — determine whether we need to suppress
   Chromium's default right-click behavior (since it can't display its own menu)
   or whether it already fails silently.

### Findings

#### 1. Chromium `HandleContextMenu`

**Method signature** (`content/public/browser/web_contents_delegate.h`):

```cpp
virtual bool HandleContextMenu(RenderFrameHost& render_frame_host,
                               const ContextMenuParams& params);
```

Returns `true` to consume (suppresses default menu), `false` to pass through.
Base implementation returns `false`.

**Three-tier call flow** (`web_contents_impl.cc` lines 8731–8754):

1. Guest embedders get first chance (`GuestHandleContextMenu`)
2. `WebContentsDelegate::HandleContextMenu` gets second chance — **return true
   to consume**
3. `WebContentsViewDelegate::ShowContextMenu` gets final chance (platform UI)

**`ContextMenuParams` key fields** (inherited from
`blink::UntrustworthyContextMenuParams`):

- `int x, y` — click coordinates relative to RenderView origin
- `GURL link_url, page_url, frame_url` — URLs
- `std::u16string selection_text` — selected text
- `bool is_editable` — whether on an editable field
- `blink::mojom::ContextMenuDataMediaType media_type` — what was right-clicked

**Content Shell macOS implementation**
(`shell_web_contents_view_delegate_mac.mm` lines 99–224): builds a full NSMenu
with Back/Forward/Reload, Copy/Paste, Open Link, Inspect. Uses `params_.x`,
`params_.y` for positioning.

**Simplest intercept**: override `HandleContextMenu` in the WebContentsDelegate
subclass, extract coordinates from `params`, send via XPC, return `true` to
suppress the default macOS shell menu.

#### 2. Existing XPC message types

All XPC messages use a `"action"` key to identify the message type.

**Chromium → GUI:**

- `"server_register"` — process registration
- `"tab_ready"` — tab init complete
- `"ca_context"` — CALayerHost context ID
- `"cursor_changed"` — cursor type
- `"loading_state"` — load progress
- `"url_changed"` — URL update
- `"title_changed"` — page title

**GUI → Chromium:**

- `"create_tab"` — tab creation (url, pane_id, pixel_width, pixel_height)
- `"navigate"` — navigation (pane_id, url)
- `"resize"` — resize (pane_id, pixel_width, pixel_height)
- `"mouse_event"` — mouse input (pane_id, type, button, x, y, click_count,
  modifiers)
- `"scroll_event"` — scroll input
- `"mouse_move"` — mouse movement
- `"key_event"` — keyboard input
- `"focus_changed"` — focus state

**Convention**: action strings are snake_case, coordinates are doubles, IDs are
strings, dimensions are uint64. All dispatched in `handleMessage()` via
`std.mem.eql(u8, action_str, "...")`.

**New message**: `"context_menu_request"` from Chromium → GUI with `pane_id`
(string), `x` (double), `y` (double).

#### 3. Zig → Swift context menu path

**Ghostty already has a context menu.** `SurfaceView_AppKit.swift` overrides
`menu(for:)` (lines 1416–1490) to show an NSMenu with Copy, Paste, Split, Reset,
Inspector items. AppKit positions it automatically from the event.

**C API export pattern**: Zig exports functions in `embedded.zig` (e.g.,
`termsurf_surface_binding_action`). Swift calls these from menu item handlers.
The reverse path uses `termsurf_runtime_config_s.action_cb` — a callback from
Zig to Swift's `App.action()` dispatcher, which routes `TERMSURF_ACTION_*`
constants to Swift handler functions.

**No menu-specific C API exists today.** Menus are handled entirely in Swift.
For a browser context menu, the simplest path is: Zig receives XPC context menu
request → calls a new C API export (e.g., `termsurf_surface_show_context_menu`)
→ Swift builds and displays the NSMenu → user selects item → Swift calls back to
Zig with the selection.

#### 4. Coordinate mapping

**Three coordinate spaces:**

1. **Physical pixels** — macOS window coordinates, Y-flipped in Swift
   (`frame.height - pos.y`)
2. **Grid coordinates** — terminal cell positions, stored in
   `overlay_grid_col/row/width/height`
3. **Logical pixels** — physical pixels ÷ content scale, sent to Chromium via
   XPC

**`hitTestOverlay()`** (`Surface.zig` lines 2456–2478) converts physical →
overlay-relative logical: subtracts overlay origin (grid × cell size), divides
by content scale.

**Reverse transform for NSMenu**: multiply logical coordinates by content scale,
add overlay origin (grid × cell size), flip Y for macOS. The existing cell
dimensions and content scale are all available in the renderer state.

**Alternative**: since Ghostty's existing `menu(for:)` uses the NSEvent
automatically, we could skip explicit coordinate mapping by intercepting the
right-click at the Swift level before forwarding to Chromium, and showing the
menu from the original NSEvent. This avoids the round-trip coordinate transform
entirely.

#### 5. Right-click suppression

**Zig forwards right-clicks to Chromium unconditionally.** `mouseButtonCallback`
(`Surface.zig` lines 4028–4087) calls `xpc.sendMouseEvent()` for all button
types including `.right`. The button is sent as `"right"` with modifier flag
`256` (1 << 8).

**Chromium's context menu is already disabled.** In
`chromium_profile_server/browser/shell_web_contents_view_delegate_mac.mm` line
104, `ShowContextMenu` returns immediately with the comment:

```cpp
// Context menu disabled — input routed via TUI/XPC (Issue 616 Experiment 9).
return;
```

The entire original menu body is commented out. This was done intentionally in
Issue 616 because Chromium's NSMenu opened as a separate window, stealing focus
from TermSurf.

**No further suppression needed** — the context menu is already disabled in the
Chromium fork.

### Result

Pass. All five research areas answered. Key insight: Ghostty already has NSMenu
infrastructure via `menu(for:)` in Swift, and Chromium's context menu is already
intentionally disabled (Issue 616 Exp 9). The simplest implementation path is to
intercept right-clicks at the Swift/Zig level (before forwarding to Chromium)
rather than round-tripping through Chromium's `HandleContextMenu` → XPC → Swift.
This avoids C++ changes and coordinate transform complexity entirely.

## Experiment 2: Browser context menu via Zig

### Hypothesis

`menu(for:)` is NOT the right intercept point. When the browser overlay is
active, `rightMouseDown` calls `termsurf_surface_mouse_button()`, Zig's
`mouseButtonCallback` hits the overlay, forwards to Chromium via XPC, and
returns `true`. Swift never calls `super.rightMouseDown()`, so AppKit never
calls `menu(for:)`. This is the same mechanism that lets neovim suppress the
terminal context menu — the event is consumed before AppKit's menu machinery
activates.

To show a browser context menu, we need to intercept the right-click in Zig's
overlay hit-test path (where the event is already consumed) and trigger a native
NSMenu from there via a new C API export.

Navigation actions won't work yet (Back/Forward/Reload need new XPC message
types and C++ handlers). This experiment proves the menu appears correctly.
Wiring up the actions is a follow-up experiment.

### Changes

1. **Zig: intercept right-click in overlay path** — in `Surface.zig`
   `mouseButtonCallback()`, inside the overlay hit-test block (line ~4047), when
   `button == .right` and `action == .press`, instead of forwarding to Chromium
   via `xpc.sendMouseEvent()`, call a new C API export to show the browser
   context menu. Still return `true` to suppress the terminal menu.

2. **Zig: add C API export** — in `embedded.zig`, add a new export function
   (e.g., `termsurf_surface_show_browser_context_menu`) that Swift implements.
   Pass the surface pointer so Swift can find the correct view.

3. **C header** — declare the new function in `termsurf.h`.

4. **Swift: implement the C API callback** — in `SurfaceView_AppKit.swift`, add
   a method that builds and displays an NSMenu with three items:
   - Back (SF Symbol: `chevron.left`)
   - Forward (SF Symbol: `chevron.right`)
   - Reload (SF Symbol: `arrow.clockwise`) Use
     `NSMenu.popUp(positioning:at:in:)` or post a synthetic right-click event to
     trigger AppKit's menu display at the correct position.

5. **Swift: add placeholder action handlers** — `@objc` methods on SurfaceView
   that log the action for now (e.g., `browserBack`, `browserForward`,
   `browserReload`). These will be wired to XPC navigation commands in a later
   experiment.

### Test

1. Launch TermSurf, navigate to a page in browse mode
2. Right-click in the browser pane — see Back, Forward, Reload menu (not the
   terminal Copy/Paste/Split menu)
3. Right-click in the terminal pane — see the normal terminal context menu
4. Select a menu item — no crash, action logs to console
5. Menu appears at the cursor position

## Conclusion (Deferred)

Deferred. The research is complete but the implementation is surprisingly
complex due to AppKit's event ordering. Key findings for when this is revisited:

### Why Chromium can't show its own menu

Chromium runs as a separate process. On macOS, showing an NSMenu activates the
process that owns it. Chromium's NSMenu was disabled in Issue 616 Experiment 9
(`shell_web_contents_view_delegate_mac.mm` line 104) because it opened as a
separate window and stole focus from TermSurf. The context menu must be shown by
TermSurf's own process.

### How Ghostty's right-click event chain works

1. AppKit calls `menu(for:)` on SurfaceView **before** any mouse events
2. If `menu(for:)` returns non-nil → AppKit shows the menu and **suppresses**
   `rightMouseDown`/`rightMouseUp` — Zig never sees the event
3. If `menu(for:)` returns nil → AppKit calls `rightMouseDown`
4. `rightMouseDown` calls `termsurf_surface_mouse_button()` → Zig's
   `mouseButtonCallback()`
5. If Zig returns `true` (consumed) → Swift returns without calling
   `super.rightMouseDown()` → no terminal context menu
6. If Zig returns `false` → Swift calls `super.rightMouseDown()` → AppKit calls
   `menu(for:)` → terminal context menu appears

This is how neovim suppresses the terminal context menu: neovim enables mouse
reporting, so Zig consumes the right-click (returns `true`), `super` is never
called, and `menu(for:)` is never reached.

### The timing problem

For the browser overlay, the right-click goes through `mouseButtonCallback` →
`hitTestOverlay` → `xpc.sendMouseEvent()` → returns `true`. This means
`menu(for:)` is never called because `super.rightMouseDown()` is never called.
Setting a flag in Zig and checking it in `menu(for:)` doesn't work because
`menu(for:)` runs before `rightMouseDown` in AppKit's normal flow, but only runs
via `super.rightMouseDown()` in Ghostty's override.

### Two viable approaches

**Approach A: Swift-only via `menu(for:)`.** Check
`termsurf_surface_is_overlay_forwarding(surface)` inside `menu(for:)` at the top
of the `.rightMouseDown` case. If browsing, return a browser-specific NSMenu.
AppKit calls `menu(for:)` before `rightMouseDown`, so the check happens before
the overlay consumes the event. No Zig changes needed. The right-click would
show the browser menu and suppress `rightMouseDown` entirely (the overlay never
sees it). Downside: Chromium never receives the right-click event.

**Approach B: Zig intercept + C API callback.** In `mouseButtonCallback`, when
`button == .right` and the overlay is hit, don't forward to Chromium. Instead
call a new C API export that tells Swift to show a browser NSMenu. Challenge:
Zig runs on a callback thread, NSMenu must be shown on the main thread, and the
menu positioning needs the original NSEvent coordinates.

### What's needed for navigation

Back/Forward/Reload require new XPC message types (`"go_back"`, `"go_forward"`,
`"reload"`) and corresponding C++ handlers in the Chromium Profile Server to
call `WebContents::GetController().GoBack()`, `GoForward()`, and `Reload()`.

### Existing infrastructure

- `termsurf_surface_is_overlay_forwarding()` — already exported to Swift, can be
  called from `menu(for:)`
- `SurfaceView_AppKit.swift` `menu(for:)` — already builds terminal context
  menus, can be extended with a browser branch
- NSMenu extensions and SF Symbol helpers already exist in the codebase
