+++
status = "closed"
opened = "2026-02-19"
closed = "2026-02-19"
+++

# Issue 601: XPC in Ghost

## Goal

Add XPC support to Ghost so it can receive messages from `web`. No Chromium, no
IOSurface — just the XPC connection lifecycle and message handling, all in Zig.

## Background

In ts5, all XPC communication lives in Swift (CompositorXPC.swift, ~500 lines).
Ghost moves this to Zig. This issue proves the integration works by handling the
`web` TUI's messages — the simplest half of the XPC protocol.

### The `web` protocol

The `web` TUI connects to Ghost through the xpc-gateway:

```
web                          xpc-gateway              Ghost
 │                               │                      │
 │  connect to Mach service      │                      │
 │──────────────────────────────▶│                      │
 │                               │                      │
 │                               │◀─── register_app ────│
 │                               │     (endpoint)       │
 │                               │                      │
 │  { action: "connect" }        │                      │
 │──────────────────────────────▶│                      │
 │                               │                      │
 │  reply: { endpoint }          │                      │
 │◀──────────────────────────────│                      │
 │                               │                      │
 │  connect to endpoint ─────────────────────────────▶  │
 │                                                      │
 │  { action: "set_overlay",                            │
 │    pane_id, col, row,                                │
 │    width, height, url,                               │
 │    profile, browsing }                               │
 │─────────────────────────────────────────────────────▶│
 │                                                      │
 │  { action: "mode_changed",                           │
 │    pane_id, browsing }                               │
 │─────────────────────────────────────────────────────▶│
 │                                                      │
 │  disconnect                                          │
 │─────────────────────────────────────────────────────▶│
```

Messages from `web`:

| Action         | Fields                                                   |
| -------------- | -------------------------------------------------------- |
| `set_overlay`  | pane_id, col, row, width, height, url, profile, browsing |
| `mode_changed` | pane_id, browsing                                        |

Messages from Ghost to `web`:

| Action         | Fields   |
| -------------- | -------- |
| `mode_changed` | browsing |
| `url_changed`  | url      |

### Blocks in Zig

XPC uses C blocks for event handlers. Ghostty's `zig-objc` dependency
(`objc.Block`) already handles this — it constructs the block struct (isa,
flags, invoke, descriptor, captures) in pure Zig. The Metal renderer uses it for
command buffer completion and IOSurface layer callbacks. No C shim needed.

### Where the code lives

In ts5, CompositorXPC.swift is a standalone class instantiated by AppDelegate.
In Ghost, the XPC logic belongs in Zig's core — likely as a new module alongside
the existing Surface, renderer, and apprt code. The exact file structure will be
determined in Experiment 1.

## Ideas for experiments

1. **Gateway connection and anonymous listener** — Ghost connects to the
   xpc-gateway, creates a listener, registers its endpoint. `web` connects and
   Ghost logs the connection. Proves XPC plumbing and `objc.Block` work from
   Zig.

2. **Parse `set_overlay` and `mode_changed`** — Ghost parses incoming XPC
   dictionaries from `web` and logs the values. Proves dictionary reading works.

3. **Send messages to `web`** — Ghost sends `mode_changed` and `url_changed`
   back to `web`. Proves bidirectional communication.

## Experiments

### Experiment 1: Gateway connection and anonymous listener

#### Goal

Ghost connects to the xpc-gateway on startup, creates an anonymous XPC listener,
registers its endpoint, and accepts connections from `web`. Ghost logs peer
connect/disconnect events. No message parsing — just proving XPC plumbing works
from Zig using `objc.Block`.

#### Changes

##### `ghost/src/apprt/xpc.zig` (new file)

A new module that handles all XPC communication. Uses `@cImport` for the XPC C
API and `objc.Block` for event handlers.

```zig
const xpc = @cImport({
    @cInclude("xpc/xpc.h");
});
```

Public interface:

```zig
pub fn init() void    // Connect to gateway, create listener, register endpoint
pub fn deinit() void  // Clean up connections
```

`init()` does:

1. Create a named XPC connection to `com.termsurf.xpc-gateway`:
   ```zig
   const gateway = xpc.xpc_connection_create_mach_service(
       "com.termsurf.xpc-gateway", null, 0);
   ```

2. Set an event handler on the gateway using `objc.Block`. The block type needs
   no captures and one argument (`xpc.xpc_object_t`):
   ```zig
   const EventBlock = objc.Block(struct {}, .{xpc.xpc_object_t}, void);
   var block = EventBlock.init(.{}, gatewayEventHandler);
   ```
   The handler just logs errors.

3. Resume the gateway connection.

4. Create an anonymous listener:
   ```zig
   const listener = xpc.xpc_connection_create(null, null);
   ```

5. Set an event handler on the listener. When a peer connects
   (`xpc_get_type(event) == xpc.XPC_TYPE_CONNECTION`):
   - Set an event handler on the peer connection (for messages and disconnect).
   - Resume the peer connection.
   - Log "Peer connected". On peer disconnect, log "Peer disconnected".

6. Resume the listener.

7. Create an endpoint from the listener and send `register_app` to the gateway:
   ```zig
   const endpoint = xpc.xpc_endpoint_create(listener);
   const msg = xpc.xpc_dictionary_create(null, null, 0);
   xpc.xpc_dictionary_set_string(msg, "action", "register_app");
   xpc.xpc_dictionary_set_value(msg, "endpoint", endpoint);
   xpc.xpc_connection_send_message(gateway, msg);
   ```

##### `ghost/src/apprt/embedded.zig`

In `App.init()`, after the existing initialization, call `xpc.init()`. In
`App.terminate()`, call `xpc.deinit()`.

##### `ghost/src/build/SharedDeps.zig`

May need to link the XPC framework. Check if `@cImport("xpc/xpc.h")` works
without explicit linking (XPC is part of libSystem on macOS, so it likely does).

#### Key unknowns

1. Does `@cImport` handle `<xpc/xpc.h>` cleanly? The header uses C blocks in
   function signatures — Zig may ignore or error on these.
2. Does `objc.Block` work with XPC's `xpc_connection_set_event_handler`? The
   function expects an `xpc_handler_t` block, not an Objective-C block. They use
   the same runtime, but the type signature may differ.
3. Can we compare `xpc_get_type(event)` with `XPC_TYPE_CONNECTION` from Zig?
   These are extern pointer constants.

If any of these fail, we'll need workarounds (C shim, different API, etc.).

#### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log

# In another terminal:
export TERMSURF_PANE_ID=$(uuidgen)
cargo run -p web -- https://example.com
```

Pass: Ghost logs show "Peer connected" when `web` starts and "Peer disconnected"
when `web` exits. No crashes, no block-related errors. The `web` TUI renders
normally.

Note: Launch with `GHOSTTY_LOG=stderr` — the embedded macOS build disables
stderr logging by default.

#### Result

Pass. All four lifecycle messages appear in the log:

```
info(xpc): connecting to xpc-gateway
info(xpc): registered endpoint with xpc-gateway
info(xpc): peer connected
info(xpc): peer disconnected
```

All three key unknowns resolved:

1. **`extern "c"` declarations work** for XPC functions — no `@cImport` needed.
   Manual declarations with `?*anyopaque` avoid C block type translation issues.
2. **`objc.Block` works with XPC event handlers** — the block ABI is identical
   for Objective-C and XPC. `xpc_connection_set_event_handler` copies the block
   correctly.
3. **XPC type constant comparison works** — `extern const` symbols with
   `@constCast` for identity comparison against `xpc_get_type()` return values.

#### Files changed

| File                           | Change                                    |
| ------------------------------ | ----------------------------------------- |
| `ghost/src/apprt/xpc.zig`      | New file — XPC gateway, listener, handler |
| `ghost/src/apprt/embedded.zig` | Call `xpc.init()` / `xpc.deinit()`        |

### Experiment 2: Parse `set_overlay` and `mode_changed`

#### Goal

Ghost parses incoming XPC dictionary messages from `web` and logs the field
values. Proves `xpc_dictionary_get_string`, `xpc_dictionary_get_uint64`, and
`xpc_dictionary_get_bool` work from Zig.

#### Changes

##### `ghost/src/apprt/xpc.zig`

Add three extern declarations:

```zig
extern "c" fn xpc_dictionary_get_string(xdict: xpc_object_t, key: [*:0]const u8) ?[*:0]const u8;
extern "c" fn xpc_dictionary_get_uint64(xdict: xpc_object_t, key: [*:0]const u8) u64;
extern "c" fn xpc_dictionary_get_bool(xdict: xpc_object_t, key: [*:0]const u8) bool;
```

Add the dictionary type constant:

```zig
extern const _xpc_type_dictionary: anyopaque;
```

Update `peerHandler` to detect dictionaries and dispatch:

```zig
fn peerHandler(_: *const EventBlock.Context, event: xpc_object_t) callconv(.c) void {
    if (xpc_get_type(event) == xpcPtr(&_xpc_type_dictionary)) {
        handleMessage(event);
    } else if (xpc_get_type(event) == xpcPtr(&_xpc_type_error)) {
        if (event == xpcPtr(&_xpc_error_connection_invalid)) {
            log.info("peer disconnected", .{});
        }
    }
}
```

Add `handleMessage` that reads the `action` field and dispatches:

- `"set_overlay"` → log pane_id, col, row, width, height, url, profile, browsing
- `"mode_changed"` → log pane_id, browsing
- anything else → log unknown action

The `xpc_dictionary_get_string` return type is `?[*:0]const u8` — null if the
key is missing. Convert to a Zig slice with `std.mem.span` for logging.

#### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log

# In a Ghost pane:
export TERMSURF_PANE_ID=$(uuidgen)
cargo run -p web -- https://example.com
```

Pass: Ghost logs show the parsed field values from `set_overlay` (pane_id, col,
row, width, height, url, profile, browsing) and `mode_changed` (pane_id,
browsing). Values match what `web` sends.

#### Result

Pass. Both message types parsed correctly:

```
info(xpc): set_overlay pane=9F96D529-... col=1 row=4 width=120 height=32 url=https://example.com profile=default browsing=true
info(xpc): mode_changed pane=9F96D529-... browsing=false
```

All dictionary accessor functions work from Zig: `xpc_dictionary_get_string`
(returns `?[*:0]const u8`), `xpc_dictionary_get_uint64`, and
`xpc_dictionary_get_bool`. String comparison via `std.mem.span` + `std.mem.eql`
for action dispatch.

#### Files changed

| File                      | Change                                         |
| ------------------------- | ---------------------------------------------- |
| `ghost/src/apprt/xpc.zig` | Parse set_overlay and mode_changed, log values |

### Experiment 3: Send messages to `web`

#### Goal

Ghost sends `url_changed` back to `web` when it receives `set_overlay`. This
proves bidirectional XPC communication from Zig. The `web` TUI already handles
`url_changed` by updating its URL bar — so the round trip is visually
confirmable.

#### Changes

##### `ghost/src/apprt/xpc.zig`

Add `xpc_retain` (needed to retain the peer connection beyond the event handler
callback):

```zig
extern "c" fn xpc_retain(object: xpc_object_t) xpc_object_t;
```

Add module-level state for the peer connection:

```zig
var web_peer: xpc_object_t = null;
```

In `listenerHandler`, store the peer:

```zig
web_peer = xpc_retain(event);
```

In `peerHandler`, on disconnect clear it:

```zig
web_peer = null;
```

In `handleSetOverlay`, after logging, send `url_changed` back to confirm the
round trip. Append " [ghost]" to the URL so the change is visible:

```zig
if (web_peer) |peer| {
    const reply = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(reply, "action", "url_changed");
    xpc_dictionary_set_string(reply, "url", "<the url> [ghost]");
    xpc_connection_send_message(peer, reply);
}
```

Building the modified URL string requires a small buffer — use a stack-allocated
array and `std.fmt.bufPrint` to append " [ghost]".

In `deinit`, release the stored peer if non-null.

#### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log

# In a Ghost pane:
export TERMSURF_PANE_ID=$(uuidgen)
cargo run -p web -- https://example.com
```

Pass: The `web` TUI's URL bar changes from `https://example.com` to
`https://example.com [ghost]` after connecting. Ghost logs show the outgoing
message.

#### Result

Pass. The `web` URL bar updated to `https://example.com [ghost]`, confirming
bidirectional XPC communication from Zig. Ghost log:

```
info(xpc): set_overlay pane=5BFBF112-... col=1 row=4 width=120 height=32 url=https://example.com profile=default browsing=true
info(xpc): sent url_changed: https://example.com [ghost]
```

`xpc_retain` / `xpc_release` work correctly for storing peer connections beyond
the event handler callback. `std.fmt.bufPrintZ` produces null-terminated strings
suitable for `xpc_dictionary_set_string`.

#### Files changed

| File                      | Change                                   |
| ------------------------- | ---------------------------------------- |
| `ghost/src/apprt/xpc.zig` | Store peer, send url_changed back to web |

## Conclusion

XPC works from Zig. The full `web` protocol — gateway connection, anonymous
listener, endpoint registration, message parsing, and bidirectional
communication — runs in ~160 lines of Zig with zero Swift involvement.

Three techniques made this possible:

1. **Manual `extern "c"` declarations** instead of `@cImport("xpc/xpc.h")`. The
   XPC header uses C block types in function signatures that Zig's translate-c
   may not handle. Declaring each function with `?*anyopaque` for all XPC opaque
   types sidesteps this entirely.

2. **`objc.Block` for event handlers.** XPC's `xpc_connection_set_event_handler`
   expects a C block, which has the same ABI as an Objective-C block. Ghostty's
   `zig-objc` dependency constructs these in pure Zig. The XPC runtime copies
   them correctly.

3. **`extern const` symbols for type comparison.** XPC type constants
   (`XPC_TYPE_CONNECTION`, `XPC_TYPE_DICTIONARY`, `XPC_TYPE_ERROR`) and error
   sentinels (`XPC_ERROR_CONNECTION_INVALID`) are extern globals compared by
   address identity. `@constCast` bridges the const/mutable pointer mismatch.

This validates the core premise of Ghost: all browser integration logic belongs
in Zig. CompositorXPC.swift's 500 lines of Swift were never necessary — XPC is a
C API, and Zig calls C natively.

### Files changed

| File                                         | Change                                       |
| -------------------------------------------- | -------------------------------------------- |
| `ghost/src/apprt/xpc.zig`                    | New — full web XPC protocol in Zig           |
| `ghost/src/apprt/embedded.zig`               | Call `xpc.init()` / `xpc.deinit()`           |
| `ghost/xpc-gateway/`                         | Copied from ts5 (Package.swift + main.swift) |
| `ghost/macos/com.termsurf.xpc-gateway.plist` | Dev launchd plist for ghost                  |

### What's next

Issue 602+ will add the Chromium half of the XPC protocol: `server_register`,
`create_tab`, `tab_ready`, `display_surface`, and IOSurface Mach port transfer —
all in Zig.
