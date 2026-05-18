+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 682: Direct TUI → Chromium XPC Connection

The TUI (`web`) sends several messages to Chromium that the GUI simply relays
without touching. Adding a direct XPC connection from the TUI to the Chromium
profile server would eliminate these relay hops and simplify the architecture.

## Background

### Current Architecture

All XPC flows through the GUI as a hub:

```
TUI ←→ GUI ←→ Chromium
```

The GUI dispatches TUI messages in `xpc.zig` and Chromium messages in the same
file. Some messages require the GUI (they touch GUI state like the renderer,
focus, or input coordinates). Others are pure pass-through — the GUI reads
fields, rebuilds the message, and forwards it unchanged.

### Complete XPC Message Inventory

#### TUI → GUI (6 messages)

| Message            | Fields                                                   | Purpose                    |
| ------------------ | -------------------------------------------------------- | -------------------------- |
| `connect`          | —                                                        | Gateway handshake (sync)   |
| `hello`            | pane_id                                                  | Get config/homepage (sync) |
| `set_overlay`      | pane_id, col, row, width, height, url, profile, browsing | Viewport position updates  |
| `navigate`         | pane_id, url                                             | URL navigation             |
| `set_color_scheme` | pane_id, scheme                                          | `:colorscheme` command     |
| `mode_changed`     | pane_id, browsing                                        | Browse/control toggle      |

#### GUI → Chromium (10 messages)

| Message            | Fields                                                                     | Purpose                                |
| ------------------ | -------------------------------------------------------------------------- | -------------------------------------- |
| `register_app`     | endpoint                                                                   | Gateway registration                   |
| `create_tab`       | url, pane_id, pixel_width, pixel_height, dark                              | New browser tab                        |
| `resize`           | pane_id, pixel_width, pixel_height                                         | Pane dimension change                  |
| `navigate`         | pane_id, url                                                               | URL navigation (forwarded)             |
| `set_color_scheme` | pane_id, dark                                                              | Color scheme (forwarded or system KVO) |
| `focus_changed`    | pane_id, focused                                                           | Pane focus state                       |
| `mouse_event`      | pane_id, type, button, x, y, click_count, modifiers                        | Mouse clicks                           |
| `scroll_event`     | pane_id, x, y, delta_x, delta_y, phase, momentum_phase, precise, modifiers | Scroll wheel                           |
| `mouse_move`       | pane_id, x, y, modifiers                                                   | Hover/drag                             |
| `key_event`        | pane_id, type, windows_key_code, utf8, modifiers                           | Keyboard input                         |

#### Chromium → GUI (8 messages)

| Message           | Fields                                            | Purpose                     |
| ----------------- | ------------------------------------------------- | --------------------------- |
| `connect`         | —                                                 | Gateway handshake (sync)    |
| `server_register` | profile                                           | Profile server ready        |
| `tab_ready`       | pane_id                                           | Tab created                 |
| `ca_context`      | pane_id, ca_context_id, pixel_width, pixel_height | GPU surface for compositing |
| `cursor_changed`  | pane_id, cursor_type                              | Cursor type update          |
| `url_changed`     | pane_id, url                                      | Navigation committed        |
| `loading_state`   | pane_id, state, progress                          | Page load progress          |
| `title_changed`   | pane_id, title                                    | Page title update           |

#### GUI → TUI (4 messages)

| Message         | Fields          | Purpose                          |
| --------------- | --------------- | -------------------------------- |
| `mode_changed`  | browsing        | Browse/control state sync        |
| `url_changed`   | url             | URL change from Chromium         |
| `loading_state` | state, progress | Page load progress from Chromium |
| `title_changed` | title           | Page title from Chromium         |

Plus the `hello` reply (sync): `homepage`.

### Pass-Through Analysis

**TUI → GUI → Chromium** (GUI just relays):

| Message            | What GUI does                              |
| ------------------ | ------------------------------------------ |
| `navigate`         | Reads pane_id/url, rebuilds, forwards      |
| `set_color_scheme` | Reads scheme, resolves dark bool, forwards |

**Chromium → GUI → TUI** (GUI just relays):

| Message         | What GUI does                        |
| --------------- | ------------------------------------ |
| `url_changed`   | Reads url, rebuilds, forwards to TUI |
| `loading_state` | Reads state/progress, forwards       |
| `title_changed` | Reads title, forwards                |

Note: `mode_changed` (GUI→TUI) is NOT a relay — it's generated by GUI events
(overlay click, Esc, non-overlay click). See experiment 1 analysis.

That's 5 relay hops that add latency and code for no functional reason.

### Messages That Need the GUI

These messages genuinely touch GUI state and must stay in the GUI:

| Message          | Why it needs GUI                                       |
| ---------------- | ------------------------------------------------------ |
| `create_tab`     | GUI creates the pane, manages server lifecycle         |
| `resize`         | GUI calculates pixel dimensions from cell grid         |
| `focus_changed`  | GUI tracks focus across surfaces                       |
| `mouse_event`    | GUI translates surface coordinates to overlay-relative |
| `scroll_event`   | GUI translates coordinates, reads NSEvent fields       |
| `mouse_move`     | GUI translates coordinates                             |
| `key_event`      | GUI translates key codes, handles Cmd bypass           |
| `ca_context`     | GUI creates CALayerHost in the renderer                |
| `cursor_changed` | GUI sets the NSCursor on the window                    |
| `set_overlay`    | GUI positions the overlay in the surface               |

### Proposed Architecture

```
TUI ←→ GUI ←→ Chromium
 ↑                ↑
 └────────────────┘
    direct link
```

The TUI connects to Chromium directly for messages that don't need the GUI. The
GUI connection remains for everything that touches rendering, input, or pane
lifecycle.

### Open Questions

1. **How does the TUI discover the Chromium server?** Currently, the GUI manages
   server lifecycle — it launches Chromium, receives `server_register`, and
   stores the server's peer connection. The TUI would need either:
   - The GUI to forward the server's XPC endpoint to the TUI
   - The TUI to connect to the same gateway and receive the endpoint directly
   - A new mach service name for the Chromium server

2. **Pane ID mapping.** The TUI knows its `TERMSURF_PANE_ID` but Chromium uses
   the same pane ID. As long as both use the same ID, routing works. But the GUI
   currently creates the tab (via `create_tab`) before the TUI can talk to
   Chromium. The TUI would need to wait for the tab to exist.

3. **`set_color_scheme` resolution.** The TUI currently sends `scheme` as a
   string (`"dark"`, `"light"`, `"system"`). The GUI resolves `"system"` by
   reading the surface's `config_conditional_state.theme`. If the TUI sends
   directly to Chromium, it would need to resolve `"system"` itself — either by
   querying the GUI first, or by receiving system theme notifications directly.

4. **Message ordering.** If `navigate` goes direct but `create_tab` goes through
   the GUI, there's a race: the TUI might send `navigate` before the GUI's
   `create_tab` arrives. The TUI would need to know the tab is ready before
   sending direct messages.

5. **Is it worth the complexity?** The relay adds microseconds of latency. The
   code savings are modest (removing ~60 lines of forwarding in xpc.zig). The
   architectural benefit is cleaner separation of concerns, but the cost is a
   second XPC connection to manage and new synchronization requirements.

## Experiment 1: Code analysis

### Hypothesis

A static analysis of the relay code paths will reveal how much work the GUI
actually does per forwarded message, whether a direct connection is feasible,
and what the simplest migration path would be.

### Plan

1. For each pass-through message, read the GUI handler and document exactly what
   it does beyond forwarding (field transformation, state reads, side effects)
2. For each Chromium→TUI relay, check whether Chromium could send directly to
   the TUI with its current XPC connection model
3. Identify which messages could move to a direct connection with zero changes
   to Chromium, which would need Chromium changes, and which are blocked by
   architectural constraints
4. Recommend: direct connection, hybrid, or keep current architecture

### Analysis

#### TUI → GUI → Chromium relays

**`navigate`** (`handleNavigate`, xpc.zig:530)

The GUI does: pane lookup, server lookup, null-terminate pane_id and url into
stack buffers, rebuild message, send. Zero state changes, zero side effects
beyond logging. Pure relay with string copying.

Verdict: **trivially movable** to direct. The TUI would just need the Chromium
server's XPC peer.

**`set_color_scheme`** (`handleSetColorScheme`, xpc.zig:565)

The GUI does: pane lookup, server lookup, **resolve `"system"` to dark bool** by
reading `surface.config_conditional_state.theme`. For `"dark"`/`"light"` this is
pure relay. For `"system"` it reads GUI state that the TUI doesn't have.

Verdict: **partially movable**. `dark`/`light` could go direct if the TUI sends
`dark` bool instead of scheme string (matching what Chromium already expects).
`system` requires knowing the current system theme — the TUI doesn't have this
without asking the GUI or receiving system theme notifications.

#### Chromium → GUI → TUI relays

**`url_changed`** (`handleUrlChanged`, xpc.zig:504)

The GUI does: pane lookup, `web_peer` null check, rebuild message dropping
`pane_id`, send. Zero state changes. Pure relay.

Verdict: **trivially movable** if Chromium had a direct connection to the TUI.

**`loading_state`** (`handleLoadingState`, xpc.zig:489)

The GUI does: pane lookup, `web_peer` null check, rebuild message dropping
`pane_id`, send. Zero state changes. Pure relay.

Verdict: **trivially movable**.

**`title_changed`** (`handleTitleChanged`, xpc.zig:517)

The GUI does: pane lookup, `web_peer` null check, rebuild message dropping
`pane_id`, send. Zero state changes. Pure relay.

Verdict: **trivially movable**.

#### Correction: `mode_changed` is NOT a relay

The background section listed `mode_changed` (GUI→TUI) as a pass-through. It is
not. `sendModeToWeb` is called only from GUI-initiated events:

- `notifyOverlayClicked` — user clicks inside the browser overlay
- `notifyEsc` — user presses Esc
- `notifyNonOverlayClicked` — user clicks outside the overlay

These are GUI events that the GUI itself generates. Chromium never sends
`mode_changed`.

Similarly, `mode_changed` (TUI→GUI) in `handleModeChanged` is NOT a pure relay.
It updates `p.browsing` state and sends `focus_changed` to Chromium — it has
side effects.

**Actual pass-through count: 5, not 6.**

#### Connection model constraints

Currently Chromium doesn't know the TUI exists. The connection topology is:

```
TUI ──web_peer──→ GUI ──server.peer──→ Chromium
                  GUI ←──tab_conn──── Chromium
```

Chromium sends messages (url_changed, loading_state, title_changed) on per-tab
`tab_conn` connections that it creates from the GUI's app endpoint. These
connections go to the GUI's anonymous listener. Chromium has no endpoint for the
TUI.

For Chromium→TUI direct, either:

- **Option A**: The GUI forwards the TUI's `web_peer` connection to Chromium
  (not possible — XPC connections aren't transferable between processes)
- **Option B**: The TUI registers its own anonymous XPC listener and the GUI
  sends the TUI's endpoint to Chromium after `server_register`
- **Option C**: Chromium creates a second per-tab connection to the TUI's
  endpoint (received from the GUI alongside `create_tab`)

For TUI→Chromium direct, the GUI would need to forward the server's XPC endpoint
to the TUI after `server_register` + `tab_ready`. The TUI creates its own
connection to Chromium from this endpoint.

#### Migration complexity

**TUI→Chromium direct** (navigate, set_color_scheme dark/light):

- GUI sends server endpoint to TUI after tab is ready (new message)
- TUI creates XPC connection to Chromium from endpoint
- TUI sends navigate/set_color_scheme directly
- Race condition: TUI must wait for endpoint before sending direct messages
- Chromium: zero changes (it already accepts these messages from any connection)

**Chromium→TUI direct** (url_changed, loading_state, title_changed):

- TUI creates anonymous listener, sends endpoint to GUI
- GUI includes TUI endpoint in `create_tab` message to Chromium
- Chromium creates per-tab connection to TUI from endpoint
- Chromium: moderate changes (new connection creation, new dispatch target)

### Recommendation

**Keep the current relay architecture.** The analysis shows:

1. Only 5 messages are true pass-through (not 6 as initially estimated)
2. The relay handlers are 5–15 lines each — minimal code
3. `set_color_scheme` with `"system"` arg requires GUI state, so it can't fully
   go direct without additional plumbing
4. Chromium→TUI direct requires Chromium changes (new connection target) and a
   TUI listener — significant new complexity
5. TUI→Chromium direct requires endpoint forwarding and tab-readiness
   synchronization — a new message and wait state
6. XPC message relay latency is sub-millisecond on macOS (same-machine Mach
   messages). No user-visible benefit from eliminating it.
7. The hub topology keeps connection management in one place (the GUI). A
   triangle topology (TUI↔GUI↔Chromium + TUI↔Chromium) doubles the connection
   surface area and creates new failure modes.

The relay is cheap, simple, and working. The direct connection saves negligible
latency at the cost of real architectural complexity. Not worth it.

### Result: Keep current architecture

The relay pattern is the right design. The pass-through code is minimal (5
handlers, ~50 lines total), the latency is negligible, and eliminating it would
require endpoint forwarding, tab-readiness synchronization, and Chromium changes
for the reverse direction. The hub topology through the GUI is simpler to reason
about and maintain.

## Conclusion

A direct TUI→Chromium XPC connection is not worth the complexity. Static
analysis of all 28 XPC messages across all four directions found only 5 true
relays (~50 lines of code), each doing trivial field copying with zero state
changes. The `set_color_scheme "system"` path requires GUI state the TUI doesn't
have. The reverse direction (Chromium→TUI) would require Chromium changes and a
new TUI listener. XPC relay latency is sub-millisecond. The hub topology through
the GUI is the right architecture — simple, working, and cheap to maintain.
