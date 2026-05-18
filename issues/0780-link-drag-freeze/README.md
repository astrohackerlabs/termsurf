+++
status = "open"
opened = "2026-04-17"
+++

# Issue 780: Link drag freezes the browser

## Goal

When the user clicks a link and accidentally drags the mouse, the browser
should behave normally — either cancel the drag or complete it cleanly.
Currently, any drag initiated on a link (or other draggable element) puts the
webview into a stuck state that appears as a freeze.

## Background

Web pages use the HTML5 drag-and-drop API for links, images, and any element
with `draggable=true`. On a regular Chromium browser, clicking a link and
moving the mouse before release starts a drag: the cursor changes, a drag
"ghost" follows the mouse, and the drag ends on mouse-up (drop, or cancel if
released outside a drop target).

In TermSurf, the user reports that doing this freezes the browser — it stops
responding to subsequent input until some recovery action is taken. That
strongly suggests we start a drag session (Chromium enters drag-state
internally) but never deliver the events that would end it, so Chromium sits
forever waiting for a drop/cancel.

## Analysis

Our mouse input pipeline forwards `mousedown`, `mousemove`, and `mouseup` into
Chromium via the TermSurf protocol (see Issues 514, 515, 695). That pipeline
is sufficient for text selection and ordinary clicks, but HTML5 drag-and-drop
on macOS requires a separate interaction model:

1. Chromium detects a drag gesture and calls its platform delegate to **start
   a native drag session**. On macOS this normally goes through
   `NSView`/`NSDraggingSource` APIs on the `RenderWidgetHostView`.
2. While dragging, the OS drives the interaction: it sends
   `draggingEntered:` / `draggingUpdated:` / `draggingExited:` / `performDragOperation:`
   to potential targets.
3. On mouse-up, the OS delivers a drop or cancel to the source, which ends the
   drag session inside Chromium.

Because the webview is composited via CALayerHost and is not a normal
`NSView` in Wezboard's window (the underlying `RenderWidgetHostView` lives in
the Roamium process), the native macOS drag machinery cannot see the correct
window or hit-test. Possible failure modes:

- **Drag starts but never ends.** Chromium starts a drag session, but the
  mouse-up we forward as a protocol message doesn't end the drag because
  Chromium is waiting on the native drag loop, not on our synthetic
  `mouseup`. Until a cancel/drop arrives, input is effectively frozen.
- **The drag ghost window never appears** because the `NSDraggingSource` is
  in the Roamium process, not in Wezboard's window.
- **Synthetic mouse events during drag are ignored** because Chromium has
  switched to the drag-event state machine.

## Proposed Solutions

Options to investigate, roughly in order of effort:

1. **Suppress drag-start entirely.** The simplest fix: stop Chromium from
   initiating a drag for draggable elements in overlay mode. The page still
   sees `click` on release, which is what the user wanted anyway. This keeps
   us in a known-good state but loses drag-and-drop as a feature.

2. **Synthesize a drag cancel on mouse-up.** If Chromium has entered a drag
   state, deliver a cancel signal (ESC-equivalent / cancel drag) when we see
   `mouseup`, so the drag session always ends with the button release. This
   prevents the freeze without implementing full drag support.

3. **Implement drag-and-drop properly.** Bridge the macOS drag machinery
   across the Wezboard/Roamium process boundary: Roamium initiates the drag,
   Wezboard receives the `NSDraggingSource` events and forwards them over
   the protocol, and Roamium drives Chromium's drag state with them. This
   is the "real" fix but a substantial chunk of work.

Start with option 1 or 2 to unfreeze the browser; file a follow-up for
option 3 if/when we want real drag-and-drop support.

## Reproduction

1. Build and run Wezboard + Roamium.
2. Load any page with links (e.g., a news site).
3. Press the mouse button on a link, move the cursor a few pixels, release.
4. Observe: the overlay stops responding to clicks/scrolls/keyboard until
   recovered (reload, tab switch, etc.).
