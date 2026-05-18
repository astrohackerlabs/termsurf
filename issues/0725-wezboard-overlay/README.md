+++
status = "closed"
opened = "2026-03-07"
closed = "2026-03-07"
+++

# Issue 725: Wezboard browser overlay rendering

## Goal

Make browser content visible in the Wezboard terminal window. The TermSurf
protocol plumbing works end-to-end (Issue 724), but the CALayerHost overlay
renders nothing on screen.

## Background

Issue 724 implemented the first three layers of the TermSurf protocol in
Wezboard across three experiments:

1. **State management** (Exp 1) — `Pane`, `Server`, and `TermSurfState` structs
   with pane registry, server registry, tab-to-pane mappings. Browser process
   spawning with `--ipc-socket`. Tab lifecycle (`CreateTab`, `TabReady`,
   `CloseTab`).
2. **Message forwarding** (Exp 2) — Board routes messages between TUI and
   Chromium. Navigate, UrlChanged, LoadingState, TitleChanged, SetColorScheme,
   ModeChanged, and Resize forwarding. Disconnect cleanup with server pane
   counting.
3. **CALayerHost rendering** (Exp 3) — CaContext message handling, three-layer
   CALayerHost hierarchy (flipped -> positioning -> host), cleanup on
   disconnect.

Everything works except visibility. Logs confirm:

- `CaContext: tab_id=1 context_id=4223629142` (valid nonzero context ID)
- `created CALayerHost contextId=4223629142`
- TUI disconnect cleans up layers without crash

But no browser content appears on screen. Not mispositioned, not half-sized —
completely invisible.

## The problem

The CALayerHost is added as a sublayer of the terminal view's backing layer
(`[ns_view layer]`). The backing layer is a `CAMetalLayer` created by
`make_backing_layer()` in `window.rs`. ANGLE (OpenGL ES via Metal) also creates
its own `CAMetalLayer` as a sublayer of this same backing layer for terminal
rendering.

The layer tree looks like:

```
NSView [layer-backed, wantsLayer=YES]
  └─ CAMetalLayer [backing layer from make_backing_layer()]
       ├─ CAMetalLayer [ANGLE's sublayer, renders terminal]
       └─ flipped_layer [our code]
            └─ positioning_layer
                 └─ CALayerHost [contextId set correctly]
```

## Hypotheses considered

### 1. Z-order: CALayerHost behind ANGLE's content

**Status: excluded.**

Core Animation draws sublayers on top of the parent layer's content. Sibling
sublayers are composited in insertion order — later additions go on top. ANGLE's
sublayer is added during EGL init (window creation). Our flipped_layer is added
later (when CaContext arrives). Our CALayerHost is on top.

### 2. ANGLE's opaque rendering covers the CALayerHost

**Status: excluded.**

Even if ANGLE renders a fully opaque terminal background (it does — `glClear`
then a full-window filled rectangle), our CALayerHost is above it in z-order, so
it would paint on top of the opaque content.

### 3. contentsScale mismatch (1.0 vs 2.0)

**Status: excluded as sole cause.**

WezTerm hardcodes `contentsScale = 1.0` on the backing layer. Ghostboard sets
`contentsScale = scaleFactor` (2.0 on Retina). A scale mismatch could cause
incorrect sizing or positioning, but not complete invisibility. A factor of 2
error would make the overlay half-sized or doubled, not zero-sized.

### 4. Zero-sized frames

**Status: excluded.**

The flipped_layer frame is set to `backing_layer.bounds` (the full view size).
The positioning_layer frame is set to `pixel_width / contentsScale` by
`pixel_height / contentsScale` — with placeholder values of ~800x700 points.
Both are non-zero.

### 5. CALayerHost not receiving content from Chromium

**Status: excluded.**

The same Roamium binary with the same CAContext mechanism works in Ghostboard.
The context ID is valid and nonzero. CALayerHost doesn't need special
entitlements — Window Server handles cross-process compositing natively.

### 6. Wrong view or wrong layer

**Status: excluded.**

`first_ns_view()` gets the NSView via `HasWindowHandle` ->
`RawWindowHandle::AppKit` -> `ns_view`. This is the same view that ANGLE renders
into. `[ns_view layer]` returns its backing CAMetalLayer.

### 7. Layer-backed view doesn't composite manual sublayers

**Status: current best hypothesis.**

WezTerm creates a **layer-backed** view: `setWantsLayer: true` is called without
first assigning a layer (window.rs line 627). In a layer-backed view, AppKit
owns the layer tree. Apple's documentation states: "In a layer-backed view, you
should never interact directly with the layer." Manually added sublayers may not
be composited.

Ghostboard creates a **layer-hosting** view: it assigns a custom
`IOSurfaceLayer` to `view.layer` _before_ setting `view.wantsLayer = true`
(Metal.zig lines 124-125). In a layer-hosting view, the app owns the layer tree
and manually added sublayers composite correctly.

This is the only hypothesis that explains complete invisibility with correct
z-order, correct context ID, non-zero frames, and working protocol plumbing.

## Proposed solution

Create a transparent **overlay NSView** as a subview on top of the terminal
view. Make the overlay view layer-hosting (assign its layer before setting
`wantsLayer`). Put the CALayerHost in the overlay view's layer tree:

```
NSWindow
  └─ contentView
       ├─ terminalView (layer-backed, ANGLE renders here — unchanged)
       └─ overlayView (new, layer-hosting, transparent)
            └─ CALayer [root, assigned before wantsLayer]
                 └─ flipped_layer (geometryFlipped=YES, auto-fills parent)
                      └─ positioning_layer (explicit frame)
                           └─ CALayerHost (contextId from Chromium)
```

This sidesteps the layer-backed restriction without modifying WezTerm's ANGLE
rendering pipeline. The overlay NSView is a sibling subview composited by AppKit
on top of the terminal view.

The overlay NSView would be:

- Created once on the main thread (when first CaContext arrives, or at window
  init)
- Same frame as the terminal view, with autoresizing mask to follow resizes
- Layer-hosting: `view.layer = CALayer.new(); view.wantsLayer = true` (in that
  order)
- Transparent: no background color, `layer.opaque = false`
- Non-interactive: `hitTest:` returns nil so all input passes through to the
  terminal view beneath

## Experiments

### Experiment 1: Transparent overlay NSView

Create a transparent, layer-hosting NSView as a sibling subview on top of the
terminal view. Move the CALayerHost layer tree from the terminal view's backing
layer into this overlay view's layer tree.

The key insight: WezTerm's terminal view is layer-backed (AppKit owns the layer
tree), so manually added sublayers are not composited. By creating a separate
layer-hosting view, we own the layer tree and CALayerHost compositing works.

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termsurf/state.rs`

Add a field to `TermSurfState` to store the overlay view pointer:

```rust
/// Overlay NSView for CALayerHost rendering (macOS only).
/// Stored as usize (0 = not created yet).
pub overlay_view: usize,
```

Initialize to `0` in `TermSurfState::new()`.

##### 2. EDIT `wezboard/wezboard-gui/src/termsurf/conn.rs`

**Create `TermSurfOverlayView` ObjC subclass** — A custom NSView subclass that
overrides `hitTest:` to return nil, making it transparent to mouse events. All
input passes through to the terminal view beneath.

Register the class once (using `std::sync::Once`) with two methods:

- `hitTest:` → returns null (pass-through)
- `acceptsFirstResponder` → returns NO

**Create `get_or_create_overlay(state)` function** — Lazily creates the overlay
view on first CaContext arrival:

1. Get the terminal NSView via `first_ns_view()`
2. Get its superview (the window's contentView)
3. Create a `TermSurfOverlayView` instance with `initWithFrame:` using the
   terminal view's frame
4. Set autoresizing mask to width+height sizable (18) so it follows resizes
5. Create a root `CALayer`, set `opaque = false`
6. Assign root layer to overlay view BEFORE setting `wantsLayer = true`
   (layer-hosting order)
7. Add overlay view as subview of contentView (goes on top of terminal view)
8. Store the overlay view pointer in `state.overlay_view`
9. Return the root layer pointer

**Modify `handle_ca_context()`** — Instead of calling `get_backing_layer()` to
get the terminal's backing layer, call `get_or_create_overlay()` to get the
overlay view's root layer. Add the flipped_layer as a sublayer of this root
layer instead.

**Modify `get_backing_layer()`** — Rename to `get_overlay_root_layer()` or
remove entirely, since we no longer add sublayers to the terminal view's backing
layer.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com`
3. Confirm log: `CaContext: tab_id=1 context_id=...` (nonzero)
4. Confirm log: `created overlay NSView` (new)
5. Confirm log: `created CALayerHost contextId=...`
6. **Browser content visible** as overlay in terminal window
7. Terminal text still visible beneath (overlay is transparent where no browser
   content exists)
8. Mouse clicks pass through overlay to terminal (hitTest: returns nil)
9. Close pane — layers cleaned up, no crash

**Result:** Pass

Browser content is visible in the Wezboard terminal window. The hypothesis was
correct: WezTerm's layer-backed view does not composite manually added
sublayers. The transparent layer-hosting overlay NSView fixes this — CALayerHost
content now renders on screen.

Logs confirm the full sequence:

- `created overlay NSView`
- `created CALayerHost contextId=2673521954`
- `UrlChanged`, `TitleChanged` — page loaded successfully
- TUI disconnect cleaned up pane without crash

One crash was encountered during implementation: `setAutoresizingMask:` on
NSView expects `NSUInteger` (u64 on 64-bit macOS), not `u32` like CALayer's
`CAAutoresizingMask`. Fixed by passing `18u64`.

**Remaining issues:**

- **Wrong position** — The webview renders at the top-left corner of the window,
  not inside the terminal pane. The positioning layer's frame needs to account
  for the pane's grid position (row/column offset + padding).
- **Wrong size** — The webview is smaller than the window. The pixel dimensions
  come from placeholder values (`overlay.width * 10`, `overlay.height * 20`) in
  `handle_set_overlay()`, not actual cell metrics. Accurate cell dimensions are
  needed.

Both issues are sizing/positioning problems, not rendering problems. The core
overlay architecture works.

#### Conclusion

The layer-backed vs layer-hosting hypothesis (Hypothesis 7) was confirmed. The
overlay NSView approach successfully renders browser content in Wezboard. Next
steps: fix positioning and sizing to place the webview correctly within the
terminal pane.

### Experiment 2: Accurate pixel dimensions from cell metrics

Fix the webview's size and position by using WezTerm's actual cell dimensions
instead of placeholder values. Currently `handle_set_overlay()` computes pixel
dimensions as `width * 10` and `height * 20` — hardcoded approximations that
produce wrong sizes. The positioning layer's frame is at (0,0) — no offset for
where the pane sits within the window.

WezTerm knows the real cell dimensions (`render_metrics.cell_size`), padding
(`config.window_padding`), and pane grid positions (`PositionedPane.left/top`).
The challenge is that these live in `TermWindow` (main thread, Rc<RefCell>),
while `conn.rs` runs in async tasks. We need to bridge this gap.

#### Approach

Store cell metrics in global atomics. TermWindow updates them during resize (and
at startup). conn.rs reads them when computing pixel dimensions.

This is simpler than passing TermSurfState into TermWindow or using
`TermWindowNotif::Apply` callbacks — it's a one-way data flow with no locking.

#### Changes

##### 1. CREATE `wezboard/wezboard-gui/src/termsurf/metrics.rs`

A small module with global atomic cell metrics:

```rust
use std::sync::atomic::{AtomicU32, Ordering};

static CELL_WIDTH: AtomicU32 = AtomicU32::new(0);
static CELL_HEIGHT: AtomicU32 = AtomicU32::new(0);
static PADDING_LEFT: AtomicU32 = AtomicU32::new(0);
static PADDING_TOP: AtomicU32 = AtomicU32::new(0);

pub fn set(cell_width: u32, cell_height: u32, padding_left: u32, padding_top: u32) {
    CELL_WIDTH.store(cell_width, Ordering::Relaxed);
    CELL_HEIGHT.store(cell_height, Ordering::Relaxed);
    PADDING_LEFT.store(padding_left, Ordering::Relaxed);
    PADDING_TOP.store(padding_top, Ordering::Relaxed);
}

pub fn get() -> (u32, u32, u32, u32) {
    (
        CELL_WIDTH.load(Ordering::Relaxed),
        CELL_HEIGHT.load(Ordering::Relaxed),
        PADDING_LEFT.load(Ordering::Relaxed),
        PADDING_TOP.load(Ordering::Relaxed),
    )
}
```

##### 2. EDIT `wezboard/wezboard-gui/src/termsurf/mod.rs`

Add `pub mod metrics;` to expose the new module.

##### 3. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

At the end of the `resize()` method (after cell dimensions and padding are
computed), call `crate::termsurf::metrics::set(...)` with the current cell
width, cell height, padding left, and padding top. These values are available
from `self.render_metrics.cell_size` and `self.padding_left_top()`.

Also call this in the TermWindow constructor (`spawn_window_impl` or equivalent)
so metrics are available before the first resize.

##### 4. EDIT `wezboard/wezboard-gui/src/termsurf/conn.rs`

Modify `handle_set_overlay()` to use real cell metrics:

```rust
let (cell_w, cell_h, pad_left, pad_top) = super::metrics::get();
let pixel_w = if cell_w > 0 {
    overlay.width * cell_w as u64
} else {
    overlay.width * 10  // fallback
};
let pixel_h = if cell_h > 0 {
    overlay.height * cell_h as u64
} else {
    overlay.height * 20  // fallback
};
```

Modify `update_ca_layer_frame()` to position the overlay using padding. For now,
use (padding_left, padding_top) as the origin — correct for a single full-size
pane. Multi-pane grid offsets can come later:

```rust
let (_, _, pad_left, pad_top) = super::metrics::get();
let x = pad_left as f64 / scale;
let y = pad_top as f64 / scale;
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com`
3. Webview fills the terminal pane area (not smaller than expected)
4. Webview is positioned inside the pane (not at window origin)
5. Resize the window — webview resizes proportionally
6. Close pane — clean shutdown, no crash

**Result:** Partial pass

The cell metrics bridge works correctly. Webview size is now accurate — it fills
the terminal pane area using real cell dimensions instead of placeholder values.
Resize works: the webview resizes with the window. Build is clean with zero
errors.

However, the vertical position is wrong. The webview is offset too high by
exactly the height of the WezTerm tab bar. The metrics bridge captures cell
padding (`padding_left`, `padding_top` from `config.window_padding`), but the
tab bar is rendered above the terminal content area and is not part of the
window padding. The `update_ca_layer_frame()` function positions the overlay at
`(pad_left / scale, pad_top / scale)`, which is correct relative to the terminal
content area but does not account for the tab bar pushing that content area down
within the window.

**What works:**

- Cell metrics atomics update on resize and at startup
- `handle_set_overlay()` computes correct pixel dimensions from real cell size
- Webview width and height match the terminal pane
- Window resize triggers metric updates and the webview follows

**What doesn't work:**

- Vertical position ignores tab bar height — webview is too high by one tab bar
  height

**Next step:** Add the tab bar height to the metrics bridge so
`update_ca_layer_frame()` can offset the y-origin correctly.

### Experiment 3: Fix y-offset with top_pixel_y

The overlay's y-position uses only `padding_top` from window config, but
WezTerm's actual content origin is computed as:

```rust
let top_pixel_y = top_bar_height + padding_top + border.top.get() as f32;
```

Three components are missing from our offset:

1. **`top_bar_height`** — tab bar pixel height (when tab bar is at top, not
   bottom)
2. **`border.top`** — OS window frame border

The simplest fix: replace `PADDING_LEFT`/`PADDING_TOP` atomics with
`CONTENT_ORIGIN_X`/`CONTENT_ORIGIN_Y` — the fully computed pixel offsets where
terminal content starts. This matches what WezTerm already computes in its
render code (`top_pixel_y` in `render/pane.rs:79`, `padding_left + border.left`
in `render/pane.rs:787`).

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termsurf/metrics.rs`

Rename `PADDING_LEFT`/`PADDING_TOP` to `CONTENT_ORIGIN_X`/`CONTENT_ORIGIN_Y`.
Update `set()` and `get()` parameter names to match:

```rust
static CONTENT_ORIGIN_X: AtomicU32 = AtomicU32::new(0);
static CONTENT_ORIGIN_Y: AtomicU32 = AtomicU32::new(0);
```

The `set()` signature becomes:

```rust
pub fn set(cell_width: u32, cell_height: u32, origin_x: u32, origin_y: u32)
```

##### 2. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Replace the current metrics push (which passes `pad_left` and `pad_top`) with
the full content origin calculation:

```rust
let (pad_left, pad_top) = self.padding_left_top();
let tab_bar_height = if self.show_tab_bar {
    self.tab_bar_pixel_height().unwrap_or(0.)
} else {
    0.
};
let top_bar_height = if self.config.tab_bar_at_bottom {
    0.0
} else {
    tab_bar_height
};
let border = self.get_os_border();
let origin_x = pad_left + border.left.get() as f32;
let origin_y = top_bar_height + pad_top + border.top.get() as f32;
crate::termsurf::metrics::set(
    self.render_metrics.cell_size.width as u32,
    self.render_metrics.cell_size.height as u32,
    origin_x as u32,
    origin_y as u32,
);
```

This mirrors `render/pane.rs:79` exactly.

##### 3. EDIT `wezboard/wezboard-gui/src/termwindow/mod.rs`

Update the initial metrics push in `spawn_window_impl` to also compute the full
origin. At this point in the constructor, `show_tab_bar` and `os_parameters` are
not yet set, so use the config defaults:

```rust
let tab_bar_height = if config.enable_tab_bar {
    TermWindow::tab_bar_pixel_height_impl(&config, &fontconfig, &render_metrics)
        .unwrap_or(0.)
} else {
    0.
};
let top_bar_height = if config.tab_bar_at_bottom {
    0.0
} else {
    tab_bar_height
};
crate::termsurf::metrics::set(
    render_metrics.cell_size.width as u32,
    render_metrics.cell_size.height as u32,
    (padding_left as f32) as u32,
    (top_bar_height + padding_top as f32) as u32,
);
```

Note: `border.top` is omitted here because `os_parameters` is `None` at
construction time (border defaults to 0). The first resize will set the correct
value.

##### 4. EDIT `wezboard/wezboard-gui/src/termsurf/conn.rs`

No changes needed. `update_ca_layer_frame()` already reads the third and fourth
values from `metrics::get()` and uses them as `x` and `y` offsets. The renamed
atomics now contain the correct full origin instead of just padding.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com`
3. Webview top edge aligns with terminal content (below tab bar)
4. Webview left edge aligns with terminal content (inside padding + border)
5. Resize window — webview stays aligned
6. Close pane — clean shutdown

**Result:** Fail

The webview is now one row too low. The `border.top` term in the origin formula
double-counts the OS window frame offset. The overlay NSView is a subview of the
window's `contentView` — the same coordinate space as the terminal view.
WezTerm's render code adds `border.top` because it offsets content within the
contentView for configured window frame styling. But our overlay already shares
that coordinate space, so adding `border.top` pushes the webview down by an
extra row.

The correct formula should be `top_bar_height + padding_top` without
`border.top`. Same for x: `padding_left` without `border.left`.

### Experiment 4: Remove border from origin formula

Experiment 3 added `border.top` and `border.left` to the content origin, but the
overlay NSView shares the contentView's coordinate space — the border offset is
already implicit. Remove the border terms from the origin calculation.

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Remove the `border` variable and simplify the origin calculation:

```rust
// before:
let border = self.get_os_border();
let origin_x = pad_left + border.left.get() as f32;
let origin_y = top_bar_height + pad_top + border.top.get() as f32;

// after:
let origin_x = pad_left;
let origin_y = top_bar_height + pad_top;
```

No other files need changes — `metrics.rs`, `mod.rs` (constructor), and
`conn.rs` are already correct. The constructor in `mod.rs` already omits border
(it was noted that `os_parameters` is `None` at construction time).

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com`
3. Webview top edge aligns with terminal content (below tab bar)
4. Webview left edge aligns with first terminal column
5. Resize window — webview stays aligned
6. Close pane — clean shutdown

**Result:** Fail

Removing `border.top` and `border.left` made no difference — the webview is
still offset one row too low. This means `border.top` was already 0 (as expected
with default macOS window decorations). The extra row of offset comes from
somewhere else in the formula.

The current origin formula is `top_bar_height + padding_top`. Since removing
border didn't help, the error must be in either `top_bar_height` or
`padding_top` — one of them contributes an extra cell height that shouldn't be
there.

### Experiment 5: Debug log all metric values

Two experiments failed because we guessed at the formula instead of inspecting
actual values. Add debug logging at both write and read sites to see every
component's value at runtime.

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Add a `log::info!` after computing the origin values, logging every component:

```rust
log::info!(
    "termsurf metrics: cell={}x{} pad_left={} pad_top={} \
     tab_bar_height={} top_bar_height={} origin=({}, {})",
    self.render_metrics.cell_size.width,
    self.render_metrics.cell_size.height,
    pad_left,
    pad_top,
    tab_bar_height,
    top_bar_height,
    origin_x,
    origin_y,
);
```

##### 2. EDIT `wezboard/wezboard-gui/src/termsurf/conn.rs`

Add a `log::info!` in `update_ca_layer_frame()` after computing the frame
values, logging the raw atomic values and the final frame coordinates:

```rust
log::info!(
    "termsurf frame: origin_x={} origin_y={} scale={} \
     frame=({}, {}, {}, {})",
    pad_left, pad_top, scale, x, y, w, h,
);
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com`
3. Check `~/dev/termsurf/logs/wezboard.log` for the logged values
4. Compare `top_bar_height` vs `cell_height` — are they equal? Is
   `top_bar_height` one cell too many?
5. Check `pad_top` — is it nonzero when it shouldn't be?
6. Check `scale` — is it 1.0 or 2.0?
7. Use the logged values to determine which component adds the extra row

**Result:** Fail

The debug logs provided no useful diagnostic value. The `termsurf metrics` log
in `resize.rs` never fired — `resize()` is not called during initial window
creation. The metrics come entirely from the `mod.rs` constructor, which has no
logging. The only log that fired was in `conn.rs`, which showed the final atomic
values (`origin_x=13 origin_y=65 scale=1`) but not the component breakdown (cell
size, tab bar height, padding) needed to identify the extra row.

The log should have been placed in `mod.rs` where the metrics are actually set
at startup, not only in `resize.rs` which runs later.

### Experiment 6: Debug logs in constructor via eprintln

Add `eprintln!` debug logging in the `mod.rs` constructor (the only code path
that actually runs at startup) and in `conn.rs` where the frame is set. Use
`eprintln!` instead of `log::info!` so output goes to stderr and is visible when
piping from the CLI. Remove the `log::info!` calls from experiments 3-5 that
didn't help.

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termwindow/mod.rs`

Add `eprintln!` after the `metrics::set()` call in the constructor, logging
every component:

```rust
eprintln!(
    "termsurf metrics: cell={}x{} padding_left={} padding_top={} \
     tab_bar_height={} top_bar_height={} origin=({}, {})",
    render_metrics.cell_size.width,
    render_metrics.cell_size.height,
    padding_left,
    padding_top,
    tab_bar_height,
    top_bar_height,
    padding_left,
    top_bar_height + padding_top,
);
```

##### 2. EDIT `wezboard/wezboard-gui/src/termsurf/conn.rs`

Replace the `log::info!` in `update_ca_layer_frame()` with `eprintln!`:

```rust
eprintln!(
    "termsurf frame: origin_x={} origin_y={} scale={} \
     frame=({}, {}, {}, {})",
    pad_left, pad_top, scale, x, y, w, h,
);
```

##### 3. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Remove the `log::info!` call added in experiment 5. Keep the metrics computation
and `metrics::set()` call.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Run from CLI:
   `cargo run -p wezboard-gui 2>&1 | tee ~/dev/termsurf/logs/wezboard.log`
3. Run `web google.com` in the terminal
4. Check output for `termsurf metrics` line with component breakdown
5. Check output for `termsurf frame` line with final frame values
6. Compare `top_bar_height` to `cell_height` — identify the extra row

**Result:** Pass (diagnostic)

The logs revealed the actual values:

```
cell=13x30  padding_left=13  padding_top=15
tab_bar_height=50  top_bar_height=50
origin=(13, 65)  scale=1
frame=(13, 65, 2054, 1980)
```

The webview is 30px (one `cell_height`) too low. Correct `origin_y` should be
35, not 65. That means
`origin_y = tab_bar_height - cell_height + padding_top = 50 - 30 + 15 = 35`.

### Experiment 7: Subtract cell_height from tab bar offset

The tab bar height of 50 overshoots by exactly one cell_height (30). Try
`origin_y = top_bar_height - cell_height + padding_top` and log the terminal
view and overlay view NSView frames to understand why the extra cell_height
exists.

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termwindow/mod.rs`

Change the origin_y calculation to subtract one cell_height:

```rust
crate::termsurf::metrics::set(
    render_metrics.cell_size.width as u32,
    render_metrics.cell_size.height as u32,
    padding_left as u32,
    (top_bar_height + padding_top)
        .saturating_sub(render_metrics.cell_size.height as usize) as u32,
);
```

Update the `eprintln!` to show the new formula.

##### 2. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Same change — subtract cell_height from origin_y:

```rust
let origin_y = (top_bar_height + pad_top
    - self.render_metrics.cell_size.height as f32)
    .max(0.0);
```

##### 3. Keep `eprintln!` in conn.rs and mod.rs

Keep the debug logs to verify the new values.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Run from CLI, pipe output
3. Confirm `origin_y` is now 35 (= 50 - 30 + 15)
4. Webview top edge aligns with terminal content
5. Resize window — webview stays aligned
6. Close pane — clean shutdown

**Result:** Fail

The webview moved but overshot — now almost one row too high. The logs confirm
`origin_y=35`. Bracketing the correct value:

- `origin_y=65` (exp 3-4): one row too low
- `origin_y=35` (exp 7): almost one row too high
- Correct value: ~50 = `top_bar_height` alone

The correct formula is just `origin_y = top_bar_height` — no `padding_top`, no
`cell_height` subtraction. The padding positions terminal text within the
content area, but the webview overlay should fill the entire content area
starting right where the tab bar ends, including the padding region.

### Experiment 8: Use tab bar height only

Set `origin_y = top_bar_height` — just the tab bar height, nothing else. The
webview should start immediately below the tab bar and fill the rest of the
window, covering the padding area (which is correct — the webview replaces the
terminal content, padding and all).

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termwindow/mod.rs`

Simplify origin_y to just `top_bar_height`:

```rust
crate::termsurf::metrics::set(
    render_metrics.cell_size.width as u32,
    render_metrics.cell_size.height as u32,
    padding_left as u32,
    top_bar_height as u32,
);
```

Update `eprintln!` accordingly.

##### 2. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Same change:

```rust
let origin_y = top_bar_height;
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Run from CLI, pipe output
3. Confirm `origin_y` is now 50 (= `top_bar_height`)
4. Webview top edge aligns immediately below the tab bar
5. Resize window — webview stays aligned
6. Close pane — clean shutdown

**Result:** Pass

The webview is correctly positioned immediately below the tab bar. The correct
formula is `origin_y = top_bar_height` — just the tab bar pixel height, nothing
else.

The key insight: the webview overlay replaces the entire terminal content area,
including the padding region. Terminal padding positions _text_ within the
content area, but the webview fills from the tab bar's bottom edge to the
window's bottom edge. Adding `padding_top` pushed the webview below where it
should start; subtracting `cell_height` overcorrected in the other direction.

#### What experiments 2-8 accomplished

Experiment 2 introduced global atomic cell metrics (`metrics.rs`) bridging
TermWindow's render data to the TermSurf connection code. The webview now has
correct pixel dimensions computed from real cell metrics instead of placeholder
`width*10`/`height*20` values.

Experiments 3-8 fixed the y-offset through iterative debugging:

- Exp 3: Added `top_bar_height + padding_top + border.top` — one row too low
- Exp 4: Removed `border.top` (was already 0) — still one row too low
- Exp 5: Added `log::info!` in resize.rs — never fired (wrong code path)
- Exp 6: Added `eprintln!` in mod.rs constructor — revealed actual values:
  `cell=13x30 tab_bar=50 padding_top=15`
- Exp 7: Subtracted `cell_height` — almost one row too high
- Exp 8: Used `top_bar_height` alone — correct

The x-offset (`origin_x = padding_left`) was correct from experiment 2 and
didn't need the tab bar treatment.

#### Conclusion

The cell metrics bridge works. Webview has correct size (real cell dimensions)
and correct position (below tab bar, at left padding). The overlay fills the
terminal content area including padding — correct behavior since the webview
replaces terminal content entirely.

### Experiment 9: Clean up debug code

Remove all debug `eprintln!` calls, unused variables, and dead code paths left
over from experiments 3-8.

#### Changes

##### 1. EDIT `wezboard/wezboard-gui/src/termwindow/mod.rs`

Remove the `eprintln!` block (lines 664-675). Remove the `top_bar_height`
variable — inline it into the `metrics::set()` call since it's only used once.

##### 2. EDIT `wezboard/wezboard-gui/src/termwindow/resize.rs`

Remove `pad_top` (unused since experiment 8). Remove intermediate variables
`origin_x` and `origin_y` — inline directly into `metrics::set()`. Remove the
`pad_left` binding and call `self.padding_left_top()` inline or bind only what's
needed. Simplify to:

```rust
// Push cell metrics to TermSurf overlay code
let tab_bar_height = if self.show_tab_bar {
    self.tab_bar_pixel_height().unwrap_or(0.)
} else {
    0.
};
let top_bar_height = if self.config.tab_bar_at_bottom {
    0.0
} else {
    tab_bar_height
};
let (pad_left, _) = self.padding_left_top();
crate::termsurf::metrics::set(
    self.render_metrics.cell_size.width as u32,
    self.render_metrics.cell_size.height as u32,
    pad_left as u32,
    top_bar_height as u32,
);
```

##### 3. EDIT `wezboard/wezboard-gui/src/termsurf/conn.rs`

Remove the `eprintln!` in `update_ca_layer_frame()`. Rename `pad_left`/`pad_top`
to `origin_x`/`origin_y` to match the atomic names.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors, zero warnings
   from our code
2. Launch Wezboard, run `web google.com`
3. Webview still correctly positioned (no regressions)
4. No debug output on stderr

**Result:** Pass

All debug `eprintln!` calls removed. Unused variables cleaned up. Variable names
in `conn.rs` renamed to match the atomic names (`origin_x`/`origin_y`). Build
produces zero warnings from our code. Webview still correctly positioned — no
regressions.

## Conclusion

Issue 725 solved browser overlay rendering in Wezboard across nine experiments.

**Experiment 1** identified why the overlay was invisible: WezTerm's terminal
view is layer-backed (AppKit owns the layer tree), so manually added CALayerHost
sublayers don't composite. The fix was a transparent layer-hosting NSView as a
sibling subview — the overlay renders on top of the terminal view with its own
layer tree that we control.

**Experiments 2-9** fixed overlay sizing and positioning. The cell metrics
bridge (`metrics.rs`) uses global atomics to pass TermWindow's render data to
the async connection code. The webview gets correct pixel dimensions from real
cell metrics (`cell_width * columns`, `cell_height * rows`). The y-offset is
just `top_bar_height` — the tab bar pixel height when the tab bar is at the top.
Six experiments of trial and error (and one critical debug logging experiment)
revealed that the webview replaces the entire content area below the tab bar,
including padding — so `padding_top` and `border.top` don't belong in the
offset.

**What works now:**

- Browser content visible in the Wezboard terminal window
- Correct overlay size from real cell metrics
- Correct overlay position (below tab bar, at left padding)
- Tab lifecycle: CreateTab, TabReady, CloseTab
- Navigation: URL forwarding between TUI and Chromium
- State: URL changes, loading state, title changes, color scheme
- CALayerHost compositing with zero-copy GPU rendering
- Clean shutdown on pane close

**Remaining work for the Wezboard PoC:**

Wezboard handles 11 of 30 TermSurf protocol messages. The remaining messages
fall into four categories:

1. **Input forwarding** — `KeyEvent`, `MouseEvent`, `MouseMove`, `ScrollEvent`.
   Without these, the browser overlay is view-only. This is the most important
   missing piece.
2. **Dynamic resize** — The overlay doesn't resize when the window resizes. The
   metrics bridge updates on resize, but `conn.rs` doesn't re-read them or call
   `update_ca_layer_frame()`. Need a notification path from TermWindow to the
   connection code.
3. **Tab queries** — `QueryLastRequest`, `QueryDevtoolsRequest`,
   `QueryTabsRequest`. The TUI uses these for session restore and DevTools
   discovery.
4. **Auxiliary features** — `FocusChanged`, `CursorChanged`,
   `SetDevtoolsOverlay`, `CreateDevtoolsTab`, `OpenSplit`. Nice-to-have for a
   full browser experience but not essential for the PoC.
