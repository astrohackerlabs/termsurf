+++
status = "closed"
opened = "2026-03-16"
closed = "2026-03-16"
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

## Experiments

### Experiment 1: Filter messages by tab_id in the TUI

#### Description

Pass the TUI's `tab_id` into `reader_loop` and `dispatch_message`. Filter
`UrlChanged`, `LoadingState`, and `TitleChanged` messages — only dispatch them
if `m.tab_id` matches the TUI's own tab_id.

#### Changes

**`webtui/src/ipc.rs`**

1. Add `tab_id: i64` parameter to `reader_loop` (~line 279):

   ```rust
   fn reader_loop(
       mut stream: UnixStream,
       event_tx: mpsc::Sender<super::LoopEvent>,
       reply_tx: mpsc::Sender<TermSurfMessage>,
       tab_id: i64,
   )
   ```

2. Pass `tab_id` through to `dispatch_message` (~line 303):

   ```rust
   dispatch_message(msg, &event_tx, &reply_tx, tab_id);
   ```

3. Add `tab_id: i64` parameter to `dispatch_message` (~line 369):

   ```rust
   fn dispatch_message(
       msg: TermSurfMessage,
       event_tx: &mpsc::Sender<super::LoopEvent>,
       reply_tx: &mpsc::Sender<TermSurfMessage>,
       tab_id: i64,
   )
   ```

4. In `dispatch_message`, add tab_id checks for the three affected messages
   (~lines 391–406):

   ```rust
   Some(Msg::UrlChanged(m)) => {
       if m.tab_id != 0 && m.tab_id != tab_id { return; }
       // ... existing dispatch
   }
   Some(Msg::LoadingState(m)) => {
       if m.tab_id != 0 && m.tab_id != tab_id { return; }
       // ... existing dispatch
   }
   Some(Msg::TitleChanged(m)) => {
       if m.tab_id != 0 && m.tab_id != tab_id { return; }
       // ... existing dispatch
   }
   ```

   The `m.tab_id != 0` check allows messages with no tab_id (0) to pass through,
   in case any code path sends them without a tab_id.

5. Update the `BrowserConnection::connect` call site (~line 328) to pass
   `tab_id`:

   ```rust
   let id = tab_id;
   std::thread::spawn(move || {
       reader_loop(reader, tx, reply_tx, id);
   });
   ```

6. Update the Wezboard connection's `reader_loop` call (~line 56) to pass `0` as
   tab_id (Wezboard messages don't need filtering):

   ```rust
   reader_loop(reader, tx, reply_tx, 0);
   ```

#### Verification

```bash
cd webtui && cargo build
scripts/build.sh wezboard
```

| #   | Test                          | Steps                                              | Expected                          |
| --- | ----------------------------- | -------------------------------------------------- | --------------------------------- |
| 1   | Navigate doesn't bleed        | Two TUIs same profile, navigate in one             | Only that TUI's URL changes       |
| 2   | Title doesn't bleed           | Two TUIs same profile, navigate to different pages | Each TUI shows its own page title |
| 3   | Loading state doesn't bleed   | Navigate in one TUI while other is idle            | Only navigating TUI shows loading |
| 4   | Single TUI still works        | One TUI, navigate normally                         | URL, title, loading all update    |
| 5   | Different profiles still work | Two TUIs different profiles, navigate in each      | Each works independently          |

**Result:** Pass

All five tests pass.

#### Conclusion

Filtering by `tab_id` in `dispatch_message` prevents browser state messages from
bleeding across TUIs. Each TUI now only processes messages for its own tab.

## Conclusion

The TUI was processing all browser state messages regardless of which tab they
belonged to. The fix: pass the TUI's `tab_id` into `reader_loop` and
`dispatch_message`, and filter `UrlChanged`, `LoadingState`, and `TitleChanged`
by matching `m.tab_id` against the TUI's own tab_id. TUI-side fix only.
