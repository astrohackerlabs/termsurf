+++
status = "open"
opened = "2026-04-11"
+++

# Issue 774: Zooming non-browser pane leaves webview overlay visible

## Goal

When zooming a non-browser pane, the browser webview overlay must be hidden so the zoomed pane is fully visible without obstruction.

## Background

Wezboard supports "zooming" a pane, which temporarily hides all other panes in the same tab and expands the selected pane to fill the entire tab area. This works correctly when zooming a browser pane — the webview scales to fill the space.

However, when zooming a non-browser pane (e.g. a terminal pane) while a browser pane exists in the same tab, the other panes vanish as expected but the browser's webview overlay remains visible on top of the zoomed pane. The CALayerHost overlay is not hidden or repositioned when its owning pane is hidden by the zoom.

This makes zooming useless for any non-webview pane in a tab that also contains a browser pane, because the webview covers part or all of the zoomed content.

## Analysis

The zoom operation hides panes at the mux/layout level, but the webview overlay (rendered via CALayerHost compositing) is managed separately from the pane layout system. When a pane is hidden by zoom, the corresponding browser overlay must also be hidden. When zoom is exited, the overlay must be restored.

The fix likely involves:

1. Detecting when a zoom hides a browser pane and sending a message to hide/show the overlay.
2. Or updating the overlay position/visibility during the zoom layout recalculation so overlays for non-visible panes are hidden.
