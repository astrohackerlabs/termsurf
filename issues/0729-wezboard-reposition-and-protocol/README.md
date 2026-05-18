+++
status = "closed"
opened = "2026-03-08"
closed = "2026-03-08"
+++

# Issue 729: Overlay reposition on resize and remaining protocol

## Goal

Fix overlay positioning during window resize so multi-pane layouts stay aligned,
and implement the remaining unhandled TermSurf protocol messages (DevTools and
OpenSplit).

## Background

Issue 728 brought Wezboard to interactive parity with Ghostboard for single-pane
browsing — input forwarding, cursor changes, and focus management all work. But
a positioning bug remains: when the window is resized with two side-by-side
browser panes, both panes resize correctly but the second pane's x/y origin
doesn't track its terminal pane. The overlay stays anchored to its original
pixel position instead of moving with the pane.

### Root cause: resize path skips repositioning

The `SetOverlay` handler in `conn.rs` has two paths:

1. **New overlay** (line 506+) — Creates CALayerHost, calls
   `update_ca_layer_frame()` which computes pixel x/y from grid coordinates +
   cell metrics + padding + border. Correct.

2. **Resize** (line 472-503) — Updates `pane.pixel_width`, `pane.pixel_height`,
   `pane.col`, `pane.row`, sends `Resize` to Chromium, then **returns early**.
   It never calls `update_ca_layer_frame()`, so the positioning layer's frame
   stays at the old x/y values.

When the window resizes, the TUI detects viewport changes and sends a new
`SetOverlay` with updated cell dimensions. This hits the resize path, which
updates pixel dimensions but not the frame position. For pane 1 (at column 0),
this is invisible — x stays at 0. For pane 2 (at column N), the x position
should shift because cell metrics changed, but it doesn't.

### How Ghostboard handles this

Ghostboard stores grid coordinates in the renderer and recomputes pixel
positions dynamically in `updateCALayerHostFrame()` every render frame:

```zig
const x: f64 = @as(f64, grid_col) * cw / scale + pl / scale;
const y: f64 = @as(f64, grid_row) * ch / scale + pt / scale;
```

Wezboard's `update_ca_layer_frame()` does the same math but is only called on
new overlay creation, not on resize.

### Remaining protocol messages

After Issue 728, two functional areas remain unimplemented:

| Message            | Direction        | What it does                           |
| ------------------ | ---------------- | -------------------------------------- |
| SetDevtoolsOverlay | TUI → Board      | Create DevTools pane linked to tab     |
| CreateDevtoolsTab  | Board → Chromium | Send DevTools tab creation to Chromium |
| OpenSplit          | TUI → Board      | Create a split pane in the terminal    |

These are feature extensions beyond core browsing. DevTools requires
coordinating a second overlay with an `inspected_tab_id`. OpenSplit requires
calling WezTerm's split pane API.

## Analysis

### The reposition fix

The resize path in `handle_set_overlay()` needs to call
`update_ca_layer_frame()` after updating pane state, just like the new-overlay
path does. The function already handles all the math — grid-to-pixel conversion
using cell metrics, padding, border, scale, and pane cell position from the mux.
It just isn't called.

The challenge is that `update_ca_layer_frame()` requires:

1. A mutable reference to the `Pane`
2. The root layer pointer (stored in the pane as `ca_layer_root`)
3. The state mutex to be held (for the pane lookup)

The resize path already has the state mutex locked and the pane available, so
the fix should be straightforward — call `update_ca_layer_frame()` before
returning.

### DevTools

Ghostboard's `handleSetDevtoolsOverlay` creates a pane with `inspected_tab_id`
set, then sends `CreateDevtoolsTab` to Chromium instead of `CreateTab`. The TUI
triggers this via the `:devtools` command. This requires understanding how
WezTerm creates new panes and how to associate a DevTools overlay with an
existing tab.

### OpenSplit

The TUI sends `OpenSplit` with a direction (horizontal/vertical) to create a new
terminal split pane. The board needs to call WezTerm's split pane API.
Ghostboard implements this by spawning a new terminal pane in the specified
direction.

## Experiments

### Experiment 1: Reposition overlay on resize

#### Goal

When the window resizes with multiple browser panes, each overlay's x/y position
must track its terminal pane. Currently only dimensions update; position stays
stale.

#### Root cause

The resize path in `handle_set_overlay()` (line 472-503) updates
`pane.pixel_width`, `pane.pixel_height`, `pane.col`, and `pane.row`, sends
`Resize` to Chromium, then returns at line 503 without calling
`update_ca_layer_frame()`. The positioning layer's frame keeps its old x/y.

#### Design

After sending `Resize` to Chromium, look up the mux window for this pane, get
the root layer via `get_or_create_overlay()`, get a mutable pane reference, and
call `update_ca_layer_frame()`. This mirrors exactly what `handle_ca_context`
does at lines 954-1053.

The `get_or_create_overlay(&mut st, mux_window_id)` call returns a raw pointer,
ending the mutable borrow on `st`, so `st.panes.get_mut()` can borrow again
afterward — same pattern as `handle_ca_context`.

#### Changes

**1. `termsurf/conn.rs` — Call `update_ca_layer_frame` in resize path**

Replace the early return at line 503 with:

```rust
// Reposition the overlay (x/y may have changed due to cell metric changes)
#[cfg(target_os = "macos")]
{
    if let Some(mux_window_id) = get_pane_mux_window(&overlay.pane_id) {
        if let Some(root_layer) = get_or_create_overlay(&mut st, mux_window_id) {
            if let Some(pane) = st.panes.get_mut(&overlay.pane_id) {
                unsafe {
                    update_ca_layer_frame(pane, root_layer);
                }
            }
        }
    }
}
return Ok(());
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Open two side-by-side panes: `web lite.duckduckgo.com` in both
3. Resize the window horizontally — both overlays track their panes (second
   pane's left edge stays aligned with its terminal pane)
4. Resize vertically — overlays stay aligned
5. Single pane still works (regression check)

**Result:** Fail

Slow window resizes don't reposition the overlays. Fast resizes do. The
`SetOverlay` path only fires when the TUI detects a viewport change and sends a
new message — during slow resizes, the TUI may not send `SetOverlay` on every
frame, so the repositioning doesn't happen. The hook is in the wrong place. The
reposition needs to happen in WezTerm's own resize handler (where cell metrics
are updated), not in the `SetOverlay` message handler.

#### Conclusion

The `SetOverlay` message path is the wrong place to reposition overlays during
window resize. The TUI only sends `SetOverlay` when it detects a viewport
change, which doesn't happen on every resize increment. The reposition needs to
be driven by WezTerm's resize event directly — the same place that calls
`metrics::set()` — so overlays track pane positions on every frame, not just
when the TUI happens to send a message.

### Experiment 2: Reposition from window resize handler

#### Goal

Same as Experiment 1 — overlays must track pane positions during window resize.
This time, drive repositioning from WezTerm's resize handler instead of the
`SetOverlay` message path. The `SetOverlay` resize path continues to handle
resizing (pixel dimensions sent to Chromium); repositioning is a separate
concern driven by the window.

#### Design

1. Add a public `reposition_all_overlays()` function in `conn.rs` that iterates
   all panes with active CALayers and calls `update_ca_layer_frame()` for each.
2. Export it from `termsurf/mod.rs`.
3. Call it from `TermWindow::resize()` in `resize.rs`, right after
   `metrics::set()` updates cell metrics.
4. Revert the Experiment 1 change (remove `update_ca_layer_frame` from the
   `SetOverlay` resize path).

#### Changes

**1. `termsurf/conn.rs` — Add `reposition_all_overlays()`**

New public function after `update_ca_layer_frame`:

```rust
#[cfg(target_os = "macos")]
pub fn reposition_all_overlays() {
    let Some(state) = super::state::global() else { return };
    let mut st = state.lock().unwrap();
    let pane_ids: Vec<String> = st.panes.iter()
        .filter(|(_, p)| p.ca_layer_positioning != 0)
        .map(|(id, _)| id.clone())
        .collect();
    for pane_id in &pane_ids {
        let Some(mux_window_id) = get_pane_mux_window(pane_id) else { continue };
        let Some(root_layer) = get_or_create_overlay(&mut st, mux_window_id) else { continue };
        let Some(pane) = st.panes.get_mut(pane_id) else { continue };
        unsafe { update_ca_layer_frame(pane, root_layer); }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn reposition_all_overlays() {}
```

**2. `termsurf/mod.rs` — Export the function**

```rust
pub use conn::reposition_all_overlays;
```

**3. `termwindow/resize.rs` — Call after `metrics::set()`**

```rust
crate::termsurf::metrics::set(/* ... */);
crate::termsurf::reposition_all_overlays();
```

**4. `termsurf/conn.rs` — Revert Experiment 1**

Remove the `update_ca_layer_frame` block from the `SetOverlay` resize path,
restoring the original `return Ok(());`.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Open two side-by-side panes: `web lite.duckduckgo.com` in both
3. Slowly resize the window horizontally — both overlays track their panes
4. Resize vertically — overlays stay aligned
5. Single pane still works (regression check)

**Result:** Success

Overlays track pane positions during slow and fast window resizes. Driving
repositioning from WezTerm's resize handler (right after `metrics::set()`)
ensures every resize increment updates overlay positions, regardless of whether
the TUI sends a `SetOverlay` message.

#### Conclusion

Repositioning overlays from the window resize handler is the correct approach.
The resize handler fires on every resize increment and has fresh cell metrics,
so `reposition_all_overlays()` always computes correct pixel positions. This
decouples overlay positioning from TUI message timing — the window owns
positioning, the TUI owns dimensions.

### Experiment 3: Remove split border padding

#### Goal

Remove the content inset/padding that was added alongside `split_border_width`
in Issue 723 Experiment 4. The padding shifts pane content inward on all 4 sides
to prevent the border from overlapping text. This causes two problems:

1. The webview overlay doesn't know about the inset, so it renders at the wrong
   position/size — it fills the full pane region while terminal content is
   shifted inward.
2. The right/bottom padding doesn't work correctly — content still bleeds under
   the border on those edges.

The border itself (4 colored rectangles drawn on layer 2 by `paint_pane_border`)
is correct and stays. Only the padding logic in `paint_pane` is removed.

#### Design

All changes in `wezboard/wezboard-gui/src/termwindow/render/pane.rs`.

**1. Remove `bw` computation and background rect inset (lines 155-172)**

Delete the `bw` variable and the `background_rect` inset block. The background
rect goes back to its original size — the border draws on top of it on layer 2.

Before:

```rust
let bw = if num_panes > 1 && !pos.is_zoomed {
    self.config
        .split_border_width
        .evaluate_as_pixels(config::DimensionContext {
            dpi: self.dimensions.dpi as f32,
            pixel_max: self.dimensions.pixel_width as f32,
            pixel_cell: self.render_metrics.cell_size.width as f32,
        }) as f32
} else {
    0.0
};
let mut background_rect = background_rect;
if bw > 0.0 {
    background_rect.origin.x += bw;
    background_rect.origin.y += bw;
    background_rect.size.width -= bw * 2.0;
    background_rect.size.height -= bw * 2.0;
}
```

After: (deleted entirely)

**2. Remove `+ bw` from text position offsets (lines 361-371)**

Remove `+ bw` from `left_pixel_x` (line 364) and `top_pixel_y` (line 371). Text
renders at its original position — the border overlaps the outermost pixels of
text, which at 2px is barely visible.

Before:

```rust
let left_pixel_x = padding_left
    + border.left.get() as f32
    + (pos.left as f32 * self.render_metrics.cell_size.width as f32)
    + bw;

let mut render = LineRender {
    ...
    top_pixel_y: top_pixel_y + bw,
    ...
    border_inset: bw,
    ...
};
```

After:

```rust
let left_pixel_x = padding_left
    + border.left.get() as f32
    + (pos.left as f32 * self.render_metrics.cell_size.width as f32);

let mut render = LineRender {
    ...
    top_pixel_y: top_pixel_y,
    ...
    // border_inset field removed
    ...
};
```

**3. Remove `border_inset` field from `LineRender` struct (line 357)**

Delete the `border_inset: f32` field declaration.

**4. Remove `border_inset` from pixel_width calculation (line 520)**

Before:

```rust
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32
    - self.border_inset * 2.0,
```

After:

```rust
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32,
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Open two side-by-side panes with browser overlays — overlays should align
   with pane edges (no gap from inset)
3. Terminal text in split panes renders edge-to-edge — no inward shift
4. Colored borders still render on top of content on layer 2
5. Single pane — no borders, no change from before

**Result:** Success

Removing the padding fixes both problems. Webview overlays align with pane edges
(no gap from inset), and terminal text renders edge-to-edge without shifting.
The 2px border draws on layer 2 on top of content, overlapping the outermost
pixels — visually clean and correct.

#### Conclusion

The content inset/padding was unnecessary. Drawing borders on layer 2 (on top of
content) is sufficient — the border overlaps a thin strip of the pane edge,
which is imperceptible at 2px. Removing the padding eliminates the
webview/terminal misalignment and the broken right/bottom inset.

### Experiment 4: OpenSplit — create split pane with command

#### Goal

Handle the `OpenSplit` protocol message so the TUI's `:devtools` command can
create a new split pane running a specified command. This is the foundation for
DevTools support — the TUI sends `OpenSplit` with a direction and command (e.g.,
`"/path/to/web devtools"`), and the board creates a split pane running that
command.

#### Background

The TUI's `:devtools [direction]` command (webtui/src/main.rs:617-634):

1. Calls `query_devtools` to validate (no duplicates, resolve tab ID)
2. Sends `OpenSplit { pane_id, direction, command }` where:
   - `direction` is `"right"`, `"down"`, `"left"`, or `"up"`
   - `command` is `"/path/to/web devtools"` (full path to the `web` binary +
     "devtools" argument)

Ghostboard handles this by calling `termsurf_surface_split_with_input()`, which
stores the command as "pending input" typed into the new shell. Wezboard can do
better: use `SplitSource::Spawn` with a `CommandBuilder` to run the command
directly as the new pane's process.

#### Proto definition

```protobuf
message OpenSplit {
  string pane_id = 1;
  string direction = 2;  // "right", "down", "left", "up"
  string command = 3;     // full executable path + args
}
```

#### Direction mapping

| TUI direction | `SplitDirection` | `target_is_second` |
| ------------- | ---------------- | ------------------ |
| `"right"`     | `Horizontal`     | `true`             |
| `"left"`      | `Horizontal`     | `false`            |
| `"down"`      | `Vertical`       | `true`             |
| `"up"`        | `Vertical`       | `false`            |

#### Design

**1. `termsurf/conn.rs` — Add `OpenSplit` to message dispatch**

Add to `msg_type_name`:

```rust
Some(Msg::OpenSplit(_)) => "OpenSplit",
```

Add to `handle_message`:

```rust
Some(Msg::OpenSplit(o)) => {
    log::info!("OpenSplit: pane_id={} direction={} command={}",
        o.pane_id, o.direction, o.command);
    handle_open_split(o, state);
}
```

**2. `termsurf/conn.rs` — Add `handle_open_split` function**

```rust
fn handle_open_split(split: proto::OpenSplit, state: &SharedState) {
    use mux::tab::{SplitDirection, SplitRequest, SplitSize};

    // Parse direction
    let (direction, target_is_second) = match split.direction.as_str() {
        "right" => (SplitDirection::Horizontal, true),
        "left" => (SplitDirection::Horizontal, false),
        "down" => (SplitDirection::Vertical, true),
        "up" => (SplitDirection::Vertical, false),
        other => {
            log::warn!("OpenSplit: unknown direction '{}'", other);
            return;
        }
    };

    // Find the mux pane ID
    let numeric_pane_id: usize = match split.pane_id.parse() {
        Ok(id) => id,
        Err(_) => {
            log::warn!("OpenSplit: invalid pane_id '{}'", split.pane_id);
            return;
        }
    };

    // Find which mux window owns this pane
    let Some(mux_window_id) = get_pane_mux_window(&split.pane_id) else {
        log::warn!("OpenSplit: pane {} not in any mux window", split.pane_id);
        return;
    };

    // Build the command
    let args: Vec<String> = split.command
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if args.is_empty() {
        log::warn!("OpenSplit: empty command");
        return;
    }

    let request = SplitRequest {
        direction,
        target_is_second,
        top_level: false,
        size: SplitSize::Percent(50),
    };

    // Spawn async split
    let command = split.command.clone();
    promise::spawn::spawn(async move {
        let mux = mux::Mux::get();
        let cmd_builder = portable_pty::CommandBuilder::from_argv(
            args.iter().map(|s| s.as_str().into()).collect(),
        );
        match mux.split_pane(
            numeric_pane_id,
            request,
            mux::domain::SplitSource::Spawn {
                command: Some(cmd_builder),
                command_dir: None,
            },
            config::keyassignment::SpawnTabDomain::CurrentPaneDomain,
        ).await {
            Ok((pane, _size)) => {
                log::info!("OpenSplit: created pane {} with command '{}'",
                    pane.pane_id(), command);
            }
            Err(e) => {
                log::error!("OpenSplit: split_pane failed: {:#}", e);
            }
        }
    }).detach();
}
```

Key points:

- Runs the split asynchronously via `promise::spawn::spawn` since
  `mux.split_pane()` is async
- Uses `CommandBuilder::from_argv` to run the command directly (no shell)
- Uses `SplitSize::Percent(50)` for a 50/50 split
- The new pane automatically gets `TERMSURF_SOCKET` and `TERMSURF_PANE_ID` from
  WezTerm's environment, so the `web devtools` process connects back to the
  board

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Open `web lite.duckduckgo.com`
3. Type `:devtools right` — a new split pane should appear to the right
4. The new pane should run `web devtools` and connect to the board
5. Type `:devtools down` — split below
6. Type `:devtools left` — split to the left
7. Type `:devtools up` — split above

**Result:** Partial success

`OpenSplit` itself works correctly — the split pane is created in the right
direction and `web devtools` runs in it. The new TUI connects to the board,
sends `HelloRequest` (pane_id=1), and sends `QueryDevtoolsRequest` (which
resolves `inspected_tab_id=0` to tab 1 via `last_browser_pane`). All of this
works.

The failure: the DevTools TUI then sends `SetDevtoolsOverlay` — not `SetOverlay`
— to request a DevTools overlay. Wezboard doesn't handle `SetDevtoolsOverlay`
yet; it falls through to the `Some(other)` catch-all and is logged as
`type=Other`. No overlay is created, no `CreateDevtoolsTab` is sent to Chromium,
so nothing renders. The TUI sits idle showing "DevTools" with no webview.

#### Conclusion

`OpenSplit` is implemented and working. The missing piece is
`SetDevtoolsOverlay` handling (which creates a pane with `inspected_tab_id` set
and sends `CreateDevtoolsTab` to Chromium instead of `CreateTab`) — this is the
next experiment.

### Experiment 5: Handle SetDevtoolsOverlay and CreateDevtoolsTab

#### Goal

Handle `SetDevtoolsOverlay` so the DevTools TUI gets a browser overlay showing
Chrome DevTools for the inspected tab. This is the last missing piece —
Experiment 4 proved `OpenSplit` works and the DevTools TUI connects, but its
`SetDevtoolsOverlay` message is ignored.

#### Background

`SetDevtoolsOverlay` is nearly identical to `SetOverlay`:

| Field            | SetOverlay | SetDevtoolsOverlay |
| ---------------- | ---------- | ------------------ |
| pane_id          | ✓          | ✓                  |
| col, row         | ✓          | ✓                  |
| width, height    | ✓          | ✓                  |
| profile          | ✓          | ✓                  |
| browsing         | ✓          | ✓                  |
| browser          | ✓          | ✓                  |
| url              | ✓          | ✗                  |
| inspected_tab_id | ✗          | ✓                  |

The only difference: `SetOverlay` has a `url` to navigate to;
`SetDevtoolsOverlay` has an `inspected_tab_id` to open DevTools for.

Similarly, `CreateDevtoolsTab` is like `CreateTab` but with `inspected_tab_id`
instead of `url`:

```protobuf
message CreateTab {
  string url = 1;
  string pane_id = 2;
  uint64 pixel_width = 3;
  uint64 pixel_height = 4;
  bool dark = 5;
}

message CreateDevtoolsTab {
  string pane_id = 1;
  int64 inspected_tab_id = 2;
  uint64 pixel_width = 3;
  uint64 pixel_height = 4;
  bool dark = 5;
}
```

The Roamium/Chromium side already handles `CreateDevtoolsTab` — it opens
DevTools targeting the specified tab. The board just needs to send it.

#### Design

**1. `conn.rs` — Add `SetDevtoolsOverlay` to `msg_type_name`**

```rust
Some(Msg::SetDevtoolsOverlay(_)) => "SetDevtoolsOverlay",
```

**2. `conn.rs` — Add `SetDevtoolsOverlay` to `handle_message`**

```rust
Some(Msg::SetDevtoolsOverlay(d)) => {
    handle_set_devtools_overlay(d, tx.clone(), state)?;
}
```

**3. `conn.rs` — Add `handle_set_devtools_overlay` function**

This mirrors `handle_set_overlay` with two differences:

- Stores `inspected_tab_id` on the pane (instead of `url`)
- Calls `send_create_devtools_tab` (instead of `send_create_tab`)

```rust
fn handle_set_devtools_overlay(
    overlay: proto::SetDevtoolsOverlay,
    tui_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    let browser = if overlay.browser.is_empty() {
        "roamium".to_string()
    } else {
        overlay.browser.clone()
    };

    let (cell_w, cell_h, _, _, _, _) = super::metrics::get();
    let pixel_w = if cell_w > 0 { overlay.width * cell_w as u64 }
                  else { overlay.width * 10 };
    let pixel_h = if cell_h > 0 { overlay.height * cell_h as u64 }
                  else { overlay.height * 20 };

    let is_new = !st.panes.contains_key(&overlay.pane_id);

    if !is_new {
        // Resize path — same as SetOverlay
        let (tab_id, profile, browser_name) = {
            let pane = st.panes.get_mut(&overlay.pane_id).unwrap();
            pane.pixel_width = pixel_w;
            pane.pixel_height = pixel_h;
            pane.col = overlay.col;
            pane.row = overlay.row;
            (pane.tab_id, pane.profile.clone(), pane.browser.clone())
        };
        if tab_id != 0 {
            // Send Resize to Chromium
            let key = TermSurfState::server_key(&profile, &browser_name);
            if let Some(server) = st.servers.get(&key) {
                if let Some(ref server_tx) = server.tx {
                    let msg = TermSurfMessage {
                        msg: Some(Msg::Resize(proto::Resize {
                            tab_id, pixel_width: pixel_w, pixel_height: pixel_h,
                        })),
                    };
                    let _ = server_tx.try_send(msg.encode_to_vec());
                }
            }
        }
        return Ok(());
    }

    // Create new pane — like SetOverlay but with inspected_tab_id, no url
    let pane = Pane {
        pane_id: overlay.pane_id.clone(),
        profile: overlay.profile.clone(),
        browser: browser.clone(),
        url: String::new(),  // DevTools has no URL
        col: overlay.col,
        row: overlay.row,
        pixel_width: pixel_w,
        pixel_height: pixel_h,
        tab_id: 0,
        tui_tx,
        browsing: overlay.browsing,
        dark: false,
        inspected_tab_id: overlay.inspected_tab_id,
        // ... CA layer fields initialized to 0/defaults
    };
    st.panes.insert(overlay.pane_id.clone(), pane);

    // Reuse existing server (DevTools uses the same Chromium process)
    let key = TermSurfState::server_key(&overlay.profile, &browser);
    if !st.servers.contains_key(&key) {
        drop(st);
        let server = spawn_server(&overlay.profile, &browser)?;
        let mut st = state.lock().unwrap();
        st.servers.insert(key.clone(), server);
        let server = st.servers.get(&key).unwrap();
        if let Some(ref server_tx) = server.tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_devtools_tab(server_tx, pane)?;
        }
    } else {
        let server = st.servers.get_mut(&key).unwrap();
        server.pane_count += 1;
        let server_tx = server.tx.clone();
        if let Some(ref stx) = server_tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_devtools_tab(stx, pane)?;
        }
    }
    Ok(())
}
```

**4. `conn.rs` — Add `send_create_devtools_tab` function**

```rust
fn send_create_devtools_tab(
    server_tx: &Sender<Vec<u8>>,
    pane: &Pane,
) -> anyhow::Result<()> {
    let msg = TermSurfMessage {
        msg: Some(Msg::CreateDevtoolsTab(proto::CreateDevtoolsTab {
            pane_id: pane.pane_id.clone(),
            inspected_tab_id: pane.inspected_tab_id,
            pixel_width: pane.pixel_width,
            pixel_height: pane.pixel_height,
            dark: pane.dark,
        })),
    };
    let payload = msg.encode_to_vec();
    server_tx.try_send(payload)?;
    log::info!(
        "sent CreateDevtoolsTab: pane_id={} inspected_tab_id={}",
        pane.pane_id, pane.inspected_tab_id
    );
    Ok(())
}
```

**5. `conn.rs` — Update `handle_server_register` pending flush**

When a server registers, it flushes pending panes (those with `tab_id == 0`).
Currently it always calls `send_create_tab`. It needs to check
`inspected_tab_id` and call `send_create_devtools_tab` for DevTools panes:

```rust
for pane_id in pending {
    let pane = st.panes.get(&pane_id).unwrap();
    if pane.inspected_tab_id != 0 {
        send_create_devtools_tab(&server_tx, pane)?;
    } else {
        send_create_tab(&server_tx, pane)?;
    }
}
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Open `web lite.duckduckgo.com`
3. Type `:devtools right` — DevTools panel opens in a split to the right
4. DevTools renders and shows the inspected page's DOM/elements
5. Type `:devtools down` from the original pane — DevTools opens below
6. Close DevTools pane — original pane unaffected

**Result:** Success

`SetDevtoolsOverlay` is handled and `CreateDevtoolsTab` is sent to Chromium. The
DevTools TUI spawns via `OpenSplit`, connects, sends `SetDevtoolsOverlay`, and
the board creates the pane with `inspected_tab_id` and sends `CreateDevtoolsTab`
to Chromium. DevTools renders in the split pane.

#### Conclusion

`SetDevtoolsOverlay` and `CreateDevtoolsTab` complete the DevTools protocol path
in Wezboard. The handler mirrors `handle_set_overlay` with two differences:
`inspected_tab_id` instead of `url`, and `send_create_devtools_tab` instead of
`send_create_tab`. The `handle_server_register` pending flush also checks
`inspected_tab_id` to dispatch the correct create message. Combined with
Experiment 4's `OpenSplit`, the full `:devtools` flow now works end-to-end.

## Conclusion

Issue 729 fixed overlay repositioning during window resize and implemented the
remaining TermSurf protocol messages for Wezboard.

**Experiment 1** attempted to reposition overlays from the `SetOverlay` message
handler. This failed because the TUI doesn't send `SetOverlay` on every resize
increment — slow resizes left overlays misaligned.

**Experiment 2** moved repositioning to WezTerm's window resize handler, calling
`reposition_all_overlays()` right after `metrics::set()`. This works on every
resize increment regardless of TUI message timing.

**Experiment 3** removed the split border content padding added in Issue 723.
The padding shifted pane content inward, misaligning webview overlays. Drawing
borders on layer 2 (on top of content) is sufficient — the 2px overlap is
imperceptible.

**Experiment 4** implemented `OpenSplit`, which creates a split pane running a
specified command. The TUI's `:devtools` command sends `OpenSplit` with a
direction and command, and the board uses `mux.split_pane()` with
`CommandBuilder::from_argv` to spawn the process directly.

**Experiment 5** implemented `SetDevtoolsOverlay` and `CreateDevtoolsTab`,
completing the DevTools protocol path. The handler mirrors `handle_set_overlay`
with `inspected_tab_id` instead of `url`, and the server register flush
dispatches the correct create message based on `inspected_tab_id`.

With these changes, Wezboard handles all TermSurf protocol messages needed for
multi-pane browsing with DevTools support. The remaining unhandled messages are
forwarded or logged as debug output.
