+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 675: XPC Hello Message

Add a `hello` message to the TUI→GUI XPC protocol so the TUI can request live
config from the GUI at startup, replacing the env var approach for settings like
`homepage`.

## Background

Currently the TUI gets config via environment variables set at shell spawn time
(`TERMSURF_HOMEPAGE` from Issue 674). This means config changes don't take
effect until the user opens a new terminal session. A hello message lets the TUI
get the latest config from the GUI every time `web` is run, without restarting
the pane.

### Current flow

```
1. GUI spawns shell → sets TERMSURF_HOMEPAGE env var (baked at spawn time)
2. User types `web`
3. TUI resolves URL from CLI arg → env var → hardcoded default
4. TUI connects to XPC
5. TUI sends set_overlay (first message)
```

The URL is resolved BEFORE the XPC connection is established. There is no init
handshake — the first message is `set_overlay`.

### Proposed flow

```
1. User types `web`
2. TUI connects to XPC
3. TUI sends { action: "hello", pane_id: "..." }     ← NEW
4. GUI replies { homepage: "..." }                     ← NEW
5. TUI resolves URL from CLI arg → hello response → env var → hardcoded default
6. TUI sends set_overlay
```

The hello message uses XPC's built-in request-reply mechanism
(`xpc_connection_send_message_with_reply_sync` on TUI side,
`xpc_dictionary_create_reply` on GUI side) — the same pattern the gateway
already uses for the `connect` message.

## Experiment 1: Hello message with homepage

### Hypothesis

Adding a synchronous `hello` request-reply to the XPC protocol will let the TUI
receive the latest `homepage` config from the GUI without relying on the env
var.

### Changes

#### 1. xpc.zig — declare `xpc_dictionary_create_reply` + add handler

Add the extern declaration alongside the existing XPC C API declarations:

```zig
extern "c" fn xpc_dictionary_create_reply(original: xpc_object_t) xpc_object_t;
```

Add `"hello"` to the `handleMessage` dispatch table:

```zig
} else if (std.mem.eql(u8, action_str, "hello")) {
    handleHello(msg);
}
```

Implement `handleHello`:

```zig
fn handleHello(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    log.info("hello pane={s}", .{pane_id});

    const reply = xpc_dictionary_create_reply(msg);
    if (reply == null) return;

    // Look up surface to read its config.
    if (app.findSurfaceByPaneId(pane_id)) |surface| {
        const homepage = surface.core().config.homepage;
        xpc_dictionary_set_string(reply, "homepage", homepage);
    }

    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) {
        xpc_connection_send_message(conn, reply);
    }
}
```

#### 2. xpc.rs — add `send_hello` method

Add a new public method on `CompositorConnection` that sends a synchronous
`hello` message and returns the homepage:

```rust
pub fn send_hello(&self, pane_id: &str) Option<String> {
    let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
    if dict.is_null() { return None; }

    unsafe {
        let key = CString::new("action").unwrap();
        let val = CString::new("hello").unwrap();
        xpc_dictionary_set_string(dict, key.as_ptr(), val.as_ptr());

        let pk = CString::new("pane_id").unwrap();
        let pv = CString::new(pane_id).unwrap();
        xpc_dictionary_set_string(dict, pk.as_ptr(), pv.as_ptr());
    }

    let reply = unsafe { xpc_connection_send_message_with_reply_sync(self.raw, dict) };
    unsafe { xpc_release(dict) };

    if reply.is_null() { return None; }

    let homepage_key = CString::new("homepage").unwrap();
    let hp = unsafe { xpc_dictionary_get_string(reply, homepage_key.as_ptr()) };
    let result = if !hp.is_null() {
        Some(unsafe { std::ffi::CStr::from_ptr(hp) }.to_str().unwrap_or("").to_string())
    } else {
        None
    };
    unsafe { xpc_release(reply) };
    result
}
```

#### 3. main.rs — reorder startup, use hello response

Move XPC connection BEFORE URL resolution. Use the hello response as a fallback:

```rust
// Connect to the TermSurf compositor via XPC (Issue 505).
let pane_id = std::env::var("TERMSURF_PANE_ID").ok();
let (tx, rx) = std::sync::mpsc::channel();
let compositor = pane_id
    .as_ref()
    .and_then(|_| xpc::CompositorConnection::connect(tx.clone()));

// Send hello to get live config from the GUI (Issue 675).
let hello_homepage = compositor.as_ref().and_then(|conn| {
    pane_id.as_ref().and_then(|pid| conn.send_hello(pid))
});

let mut url = match cli.command {
    Some(Commands::Url { url }) => url,
    None => cli.url.unwrap_or_else(|| {
        hello_homepage.unwrap_or_else(|| {
            std::env::var("TERMSURF_HOMEPAGE")
                .unwrap_or_else(|_| "https://termsurf.com/welcome".to_string())
        })
    }),
};
```

This gives four levels of precedence:

1. `web <url>` — explicit CLI arg wins
2. `web` inside TermSurf — hello response (live config)
3. `web` outside TermSurf — `TERMSURF_HOMEPAGE` env var
4. No env var — hardcoded `https://termsurf.com/welcome`

### Test

1. `cd gui && zig build` — compiles without errors
2. `cd tui && cargo build` — compiles without errors
3. Set `homepage = https://example.com` in config
4. Open TermSurf, type `web` — opens `https://example.com`
5. Change config to `homepage = https://termsurf.com`
6. Type `web` in the SAME pane (no restart) — opens `https://termsurf.com`
7. `web google.com` — still opens google.com (CLI arg wins)
8. Run `web` outside TermSurf — falls back to env var or hardcoded default

### Result: PASS

Both GUI and TUI build successfully. `web` without arguments receives the
homepage from the GUI via the hello message. Config changes take effect
immediately without restarting the terminal pane — each `web` invocation gets
the latest config. CLI args still take precedence. Outside TermSurf (no XPC),
falls back to env var then hardcoded default.

## Experiment 2: Remove TERMSURF_HOMEPAGE env var

### Hypothesis

The `TERMSURF_HOMEPAGE` env var is now redundant. Inside TermSurf, the hello
message provides live config. Outside TermSurf, there's no GUI to set the env
var anyway, so it always falls through to the hardcoded default. Removing it
simplifies the precedence chain without losing functionality.

### Changes

#### 1. Surface.zig — remove `env.put("TERMSURF_HOMEPAGE", ...)`

Delete the line that propagates the homepage env var to child processes.

#### 2. main.rs — remove `TERMSURF_HOMEPAGE` fallback

Simplify the URL resolution from four levels to three:

```rust
let mut url = match cli.command {
    Some(Commands::Url { url }) => url,
    None => cli.url.unwrap_or_else(|| {
        hello_homepage
            .unwrap_or_else(|| "https://termsurf.com/welcome".to_string())
    }),
};
```

Precedence becomes:

1. `web <url>` — explicit CLI arg wins
2. `web` inside TermSurf — hello response (live config)
3. `web` outside TermSurf — hardcoded `https://termsurf.com/welcome`

### Test

1. `cd gui && zig build` — compiles without errors
2. `cd tui && cargo build` — compiles without errors
3. `web` inside TermSurf — still opens configured homepage (via hello)
4. `web google.com` — CLI arg still wins
5. `env | grep TERMSURF_HOMEPAGE` — no longer set in shell environment

### Result: PASS

Both builds succeed. New panes no longer have `TERMSURF_HOMEPAGE` in their
environment. Homepage still works via the hello message. Existing panes retain
the old env var (expected — env vars are baked at spawn time), but it's harmless
since `send_hello` takes precedence.

## Conclusion

`web` now gets live config from the GUI via an XPC hello message at startup.
Three-level precedence:

1. `web <url>` — explicit CLI arg wins
2. `web` inside TermSurf — hello response (live from config, no restart needed)
3. `web` outside TermSurf — hardcoded `https://termsurf.com/welcome`

The `TERMSURF_HOMEPAGE` env var has been removed — the hello message supersedes
it entirely.

Changes across 3 files:

- `gui/src/apprt/xpc.zig` — `handleHello` handler with synchronous reply
- `gui/src/Surface.zig` — removed `TERMSURF_HOMEPAGE` env var propagation
- `tui/src/xpc.rs` — `send_hello` method (synchronous request-reply)
- `tui/src/main.rs` — reordered startup, simplified fallback chain
