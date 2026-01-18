# CEF MVP3: Precise Pane Matching

## Goal

The browser must always match the precise position and dimensions of its pane.

## Requirements

1. **Position**: The browser's top-left corner must be at the pane's top-left
   corner. Always.

2. **Size**: The browser's width and height must equal the pane's width and
   height. Always.

3. **Responsiveness**: When the pane changes (window resize, split, unsplit,
   drag divider), the browser must update to match near-instantaneously.

4. **No overflow**: The browser must never extend beyond its pane boundaries. It
   must never cover adjacent panes or UI elements.

5. **Temporary stretching is acceptable**: During resize transitions, the
   browser texture may be temporarily stretched or compressed. This is fine as
   long as it immediately corrects itself to match the new pane dimensions at
   1:1 scale.

6. **Unconditional stretching**: The current texture is always stretched to
   match the pane dimensions exactly. No gaps, no bands, no overflow. The
   viewport is set to the pane bounds, period.

7. **Acceptable transient state**: Stretching is acceptable only as a brief
   transient state while waiting for CEF to produce a correctly-sized texture.

## Technical Approach

1. **Single source of truth**: All pane bounds come from
   `calculate_pane_pixel_bounds()`. This function is the sole authority for
   browser viewport position and size. Do not introduce alternative calculations.

2. **Edge extension**: Edge panes extend to the window edge (covering
   padding/borders). Interior panes extend half-cell into dividers. See the
   "Source of Truth" section below for the exact formulas.

3. **HiDPI support**: Both Retina and standard displays must work correctly.
   Retina currently works—preserve this. The browser must render at the correct
   scale factor for the display.

4. **Debounced re-render**: When pane size changes, request a CEF re-render
   using debounced resize with trailing edge. See detailed section below.

## Debounced Resize with Trailing Edge

The re-render loop uses **debounced resize with trailing edge** to avoid
overwhelming CEF with resize requests during continuous dragging.

### The Pattern

1. At most one resize render is in flight at any time
2. At most one resize is queued (the "trailing edge")
3. New resize requests replace the queued one, not append to it
4. The final size is always rendered (hence "trailing edge")

### State Machine

```
States:
  Idle                          - No render in progress
  Rendering(size)               - Render in progress, nothing queued
  RenderingWithQueued(in_flight, queued)  - Render in progress, trailing edge queued

Transitions:

  [Pane size changes to S]
    Idle                        → Rendering(S)           // Start immediately
    Rendering(X)                → RenderingWithQueued(X, S)  // Queue trailing edge
    RenderingWithQueued(X, _)   → RenderingWithQueued(X, S)  // Replace trailing edge

  [CEF finishes rendering]
    Rendering(X)                → Idle                   // Done
    RenderingWithQueued(X, Q)   → Rendering(Q)           // Start trailing edge
```

### Example Flow

1. User drags window edge
2. Paint loop detects pane size changed (800×600 → 850×600)
3. State is `Idle` → call `browser.resize(850, 600)` → `Rendering(850×600)`
4. Viewport stretches current texture to pane bounds (temporary)
5. User keeps dragging → pane is now 900×600
6. State is `Rendering` → queue trailing edge →
   `RenderingWithQueued(850×600, 900×600)`
7. User keeps dragging → pane is now 950×600
8. Replace trailing edge → `RenderingWithQueued(850×600, 950×600)`
9. CEF delivers 850×600 texture
10. Start trailing edge → `Rendering(950×600)`
11. User stopped, pane stays at 950×600
12. CEF delivers 950×600 texture
13. No trailing edge queued → `Idle`
14. Viewport shows 950×600 texture in 950×600 pane = 1:1

### Detection Point

Hybrid approach:

- **Detection** in `paint_browser_overlay`: compare pane size to last-requested
  size, update trailing edge queue
- **Execution** in `on_paint` callback: when CEF finishes, start trailing edge
  if queued

### Data Structure

```rust
struct DebounceState {
    in_flight: Option<(u32, u32)>,      // Size currently being rendered
    trailing_edge: Option<(u32, u32)>,  // Queued size (replaces, not appends)
}
```

### Source of Truth

**The physical pixel dimensions passed to `set_pane_bounds()` are the source of
truth for re-renders.** Convert these to logical pixels and pass to
`browser.resize()`. Do not introduce a different calculation.

The values are calculated in `paint_browser_overlay` as follows:

```
# Edge detection
is_left   = pos.left == 0
is_top    = pos.top == 0
is_right  = pos.left + pos.width >= terminal_cols
is_bottom = pos.top + pos.height >= terminal_rows

# Position
x = 0                                                  if is_left
    padding_left + border.left + pos.left*cell_w - cell_w/2    otherwise

y = border.top + tab_bar_height                        if is_top
    border.top + tab_bar_height + padding_top + pos.top*cell_h - cell_h/2    otherwise

# Size
pane_width  = window_width - x                         if is_right
              pos.width*cell_w + width_delta           otherwise

pane_height = window_height - y                        if is_bottom
              pos.height*cell_h + height_delta         otherwise

# Deltas (extend into padding/dividers)
width_delta  = padding_left + border.left + cell_w/2   if is_left
               cell_w                                  otherwise

height_delta = padding_top + cell_h/2                  if is_top
               cell_h                                  otherwise
```

The key principle: **edge panes extend to window edge; interior panes extend
half-cell into dividers.**

## Non-Goals for MVP3

- Perfect frame-by-frame synchronization (minor lag is acceptable)
- Avoiding all visual artifacts during resize (temporary stretch is fine)
- Input handling improvements
- Navigation controls

## Success Criteria

The implementation is complete when:

- You can resize the window and the browser fills the pane exactly
- You can split the pane and the browser shrinks to match the new smaller pane
  exactly
- You can close a split and the browser grows to match the larger pane exactly
- You can drag pane dividers and the browser resizes to match exactly
- At no point does the browser overflow into adjacent panes or leave gaps
