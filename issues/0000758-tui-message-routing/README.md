+++
status = "open"
opened = "2026-03-16"
+++

# Issue 758: TUI processes messages for all tabs, not just its own

## Goal

Each TUI instance only processes browser state messages (UrlChanged,
LoadingState, TitleChanged) for its own tab. Navigating in one TUI does not
affect the URL bar or state of another TUI.

## Background

### The bug

When two TUIs are connected to the same Roamium process (same profile, different
tabs), navigating in one TUI causes the URL to change in both. The title and
loading state also bleed across.

### How messages flow

1. TUI connects to Wezboard, sends `SetOverlay` with a URL
2. Wezboard sends `BrowserReady` back to the TUI with a `tab_id` and a
   `browser_socket` path
3. TUI connects directly to the Roamium process via `browser_socket`
4. Roamium sends `UrlChanged`, `LoadingState`, `TitleChanged` over this socket

The problem: a single Roamium process serves one profile, which can have
multiple tabs (one per TUI). When any tab navigates, Roamium sends the state
change to ALL connections on the socket. Every TUI connected to that profile
receives every message.

### Why it bleeds

In `webtui/src/ipc.rs` (~line 391), the TUI dispatches `UrlChanged` without
checking the `tab_id`:

```rust
Some(Msg::UrlChanged(m)) => {
    let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::UrlChanged {
        url: m.url.clone(),
    }));
}
```

The protobuf message includes `tab_id`, but the TUI drops it. Same for
`LoadingState` and `TitleChanged`.

### The fix

The TUI already knows its own `tab_id` — it receives it in the `BrowserReady`
message. The fix: when dispatching `UrlChanged`, `LoadingState`, and
`TitleChanged`, check `m.tab_id` against the TUI's own tab_id and ignore
mismatches.

This is a TUI-side fix only. No changes needed in Wezboard or Roamium.
