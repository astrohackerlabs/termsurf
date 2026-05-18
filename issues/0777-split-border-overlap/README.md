+++
status = "open"
opened = "2026-04-11"
+++

# Issue 777: Split border overlaps pane content and blocks mouse resize

## Goal

The `split_border_width` border must not overlap pane content or block mouse-driven pane resizing.

## Background

Wezboard has a config option `split_border_width = 4` that draws a border around each terminal pane. This border has two problems:

### 1. Border overlaps pane content

The border is drawn on top of the pane's content area rather than outside it. With `split_border_width = 4`, the outermost 4 pixels of terminal content are hidden behind the border. The pane needs padding or margin equal to the border width so content is inset and fully visible.

### 2. Border covers the mouse resize handle

WezTerm uses a thin invisible hit region between panes for mouse-driven resizing (click and drag to resize splits). The border is drawn on top of this region, visually covering it and — more critically — intercepting or blocking mouse events. With the border enabled, it is impossible to resize panes with the mouse.

## Analysis

Both problems stem from the same root cause: the border is drawn inward, consuming space that belongs to the pane content area and the resize hit region, rather than being accounted for in the layout.

Possible fixes:

1. **Inset pane content** — Add padding/margin to the pane's renderable area equal to `split_border_width`, so the border frames the content without overlapping it.
2. **Expand resize hit region** — Ensure the mouse resize handle extends over or through the border area, or position it outside the border, so drag-to-resize still works with any border width.
3. **Draw border outside content** — Alternatively, allocate the border width as extra space between panes in the layout calculation, rather than drawing it on top of existing pane space.

The fix should ensure that both issues are resolved together, since they share the same layout cause.
