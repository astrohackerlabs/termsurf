+++
status = "closed"
opened = "2026-03-19"
closed = "2026-03-19"
+++

# Issue 761: Browser engine label missing when using default

## Goal

The viewport bottom-right should always show the browser engine name, even when
the user doesn't pass `--browser`.

## Background

### The problem

When the user runs `web --browser /path/to/roamium`, the TUI extracts the last
path component ("roamium") and displays it in the viewport bottom-right. But
when the user runs just `web localhost:3000` without `--browser`, the label is
empty — the user gets no indication of which engine is rendering the page.

### Root cause

In `webtui/src/main.rs`, the `browser` variable is set from the CLI argument:

```rust
let mut browser = cli.browser.unwrap_or_default();  // "" when omitted
```

The TUI passes this empty string to the GUI in `SetOverlay.browser`. The GUI
resolves the default engine internally and launches it, but never tells the TUI
what it chose. The `BrowserReady` message (GUI → TUI) contains `pane_id`,
`tab_id`, and `browser_socket` — but not the browser name.

### Fix

Add a `browser` field to the `BrowserReady` message:

```protobuf
message BrowserReady {
  string pane_id = 1;
  int64 tab_id = 2;
  string browser_socket = 3;
  string browser = 4;          // resolved browser binary path
}
```

When the GUI sends `BrowserReady`, it includes the actual browser path it
launched. The TUI updates its `browser` variable from this field, and the
viewport label updates on the next render.

### Scope

Three layers:

1. **Protocol (`termsurf.proto`)** — Add `browser` field to `BrowserReady`.
2. **Wezboard (GUI)** — Populate the new field when sending `BrowserReady`.
3. **TUI (`webtui`)** — Read the field from `BrowserReady` and update the
   `browser` variable.

## Experiments

### Experiment 1: Add browser field to BrowserReady

#### Description

Add `browser` to the `BrowserReady` protobuf message. The GUI already has the
resolved browser name on `pane.browser` when it constructs `BrowserReady` — it
just doesn't include it. The TUI already has `let mut browser` — it just never
gets updated after initialization.

#### Changes

**1. Protocol: `proto/termsurf.proto`**

Add `browser` field to `BrowserReady` (~line 233):

```protobuf
message BrowserReady {
  string pane_id = 1;
  int64 tab_id = 2;
  string browser_socket = 3;
  string browser = 4;
}
```

**2. Wezboard: `wezboard-gui/src/termsurf/conn.rs`**

In `handle_tab_ready()` (~line 747), add `pane.browser` to the `BrowserReady`
construction:

```rust
let browser_ready = TermSurfMessage {
    msg: Some(Msg::BrowserReady(proto::BrowserReady {
        pane_id: ready.pane_id.clone(),
        tab_id: ready.tab_id,
        browser_socket: listen_socket.clone(),
        browser: pane.browser.clone(),
    })),
};
```

**3. TUI: `webtui/src/ipc.rs`**

Add `browser` to `CompositorMessage::BrowserReady` (~line 30):

```rust
BrowserReady { tab_id: i64, browser_socket: String, browser: String },
```

Update the dispatch case (~line 429):

```rust
Some(Msg::BrowserReady(m)) => {
    let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::BrowserReady {
        tab_id: m.tab_id,
        browser_socket: m.browser_socket.clone(),
        browser: m.browser.clone(),
    }));
}
```

**4. TUI: `webtui/src/main.rs`**

Update the `BrowserReady` handler (~line 715) to also update the `browser`
variable:

```rust
ipc::CompositorMessage::BrowserReady {
    tab_id,
    browser_socket,
    browser: resolved_browser,
} => {
    if !resolved_browser.is_empty() {
        browser = resolved_browser;
    }
    // Connect directly to the browser engine.
    if let Some(conn) = ipc::BrowserConnection::connect(
        &browser_socket,
        tab_id,
        browser_tx.clone(),
    ) {
        browser_conn = Some(conn);
    }
}
```

Only updates `browser` if the field is non-empty, so an explicit `--browser`
flag is never overwritten by a blank.

#### Verification

```bash
scripts/build.sh wezboard
cd webtui && cargo build
```

| #   | Test                 | Steps                                                    | Expected                                   |
| --- | -------------------- | -------------------------------------------------------- | ------------------------------------------ |
| 1   | Default shows label  | Run `web localhost:3000` (no --browser)                  | "roamium" appears in viewport bottom-right |
| 2   | Explicit still works | Run `web --browser roamium localhost`                    | "roamium" appears in viewport bottom-right |
| 3   | Absolute path works  | Run `web --browser /usr/local/roamium/roamium localhost` | "roamium" in bottom-right                  |

**Result:** Pass

All three tests pass. The engine label now appears in the viewport bottom-right
regardless of whether `--browser` is specified.

#### Conclusion

Threading `pane.browser` through `BrowserReady` was the only change needed. The
GUI already resolved the default; the TUI just wasn't told.

## Conclusion

The browser engine label now always appears in the viewport bottom-right. The
fix adds a `browser` field to the `BrowserReady` protobuf message. The GUI
populates it from `pane.browser` (which holds the resolved engine name), and the
TUI updates its local `browser` variable on receipt.
