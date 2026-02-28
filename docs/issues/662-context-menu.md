# Issue 662: Browser Context Menu

Add a right-click context menu to the browser pane with Back, Forward, and
Reload options.

## Problem

Right-clicking in the browser pane forwards the mouse event to Chromium, but
Chromium renders headlessly via CALayerHost and has no window to attach a
context menu to. The context menu request goes nowhere. Users have no way to
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
