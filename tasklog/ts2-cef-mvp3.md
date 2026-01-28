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
   browser viewport position and size. Do not introduce alternative
   calculations.

2. **Edge extension**: Edge panes extend to the window edge (covering
   padding/borders). Interior panes extend half-cell into dividers. See the
   "Source of Truth" section below for the exact formulas.

3. **HiDPI support**: Both Retina and standard displays must work correctly.
   Retina currently works—preserve this. The browser must render at the correct
   scale factor for the display.

4. **Settle-and-rerender**: When pane size changes, resize immediately. After
   10ms of no changes, trigger one final render to fix any texture mismatch. See
   detailed section below.

## Settle-and-Rerender

CEF renders asynchronously. When resize requests arrive during an in-progress
render, the delivered texture may not match the current pane size. This causes
visual distortion (stretching/compression artifacts).

The solution: after resizing stops, wait briefly, then render one more time.

### How It Works

1. **Immediate resize**: When pane size changes, call `browser.resize()`
   immediately. The viewport stretches the current texture to fit (acceptable
   temporary state).

2. **Mark time**: Record when the resize happened (`last_resize_time`).

3. **Keep painting**: While waiting to settle, keep the paint loop running via
   `window.invalidate()`.

4. **Settle render**: After 10ms with no size changes, trigger one final
   `browser.resize()` at the current size. This ensures the texture matches the
   pane exactly.

### Why 10ms?

- Fast enough to be imperceptible to humans
- Long enough to catch the "settle" point after rapid resize
- Empirically validated to fix the texture mismatch issue

### Manual Fallback

If the browser ever gets into a bad state, **Ctrl+Shift+R** forces an immediate
re-render at the current size. This is a debugging aid and should rarely be
needed with the automatic settle logic.

### Implementation

In `BrowserState`:

- `last_resize_time: RefCell<Option<Instant>>` - when last resize was requested
- `mark_resize_time()` - called after each resize
- `clear_resize_time()` - called after settle render
- `time_since_last_resize()` - returns elapsed time if waiting

In `paint_browser_overlay`:

- If size changed: resize + mark time
- If waiting and 10ms elapsed: settle render + clear time
- If waiting and <10ms: invalidate window to keep painting

## Source of Truth

**The physical pixel dimensions passed to `set_pane_bounds()` are the source of
truth for re-renders.** Convert these to logical pixels and pass to
`browser.resize()`. Do not introduce a different calculation.

The values are calculated in `calculate_pane_pixel_bounds()` as follows:

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
