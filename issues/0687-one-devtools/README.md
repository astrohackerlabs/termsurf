+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 687: One DevTools Per Tab

Enforce that only one DevTools session can be open per inspected browser tab.
Opening a second DevTools for the same tab should be rejected with an error
message instead of silently creating a duplicate that crashes the renderer.

## Background

Issue 686 found that opening two DevTools panes for the same inspected tab
causes a DCHECK crash in Chromium's `PaintController`. Both DevTools sessions
attach an `InspectorOverlayAgent` to the same renderer, producing duplicate
`DisplayItem::Id` entries during overlay painting. Chromium enforces one
DevTools frontend per inspected page internally — TermSurf bypasses that by
creating independent `ShellDevToolsFrontend` instances.

## Where to Enforce

The check can happen at three levels:

1. **Chromium (`CreateDevToolsTab`)** — before creating the
   `ShellDevToolsFrontend`, check if any existing tab already has
   `inspected_tab_id == N`. If so, reject the request and send an error back.
   This is the safest level — it's impossible to bypass.

2. **GUI (`handleSetDevtoolsOverlay`)** — before forwarding
   `create_devtools_tab` to Chromium, check if any pane already has
   `inspected_tab_id == N` for the same server. Faster feedback — no round-trip
   to Chromium.

3. **TUI** — before sending the XPC message. Requires the TUI to know what
   DevTools sessions are already open, which it currently doesn't.

## Design Decision: DevTools Is a Locked Mode

DevTools panes do not support URL navigation. The URL bar is not editable. You
cannot navigate to a different page, and you cannot press "back" to return to a
previous URL. DevTools is a special-purpose inspector — one per browser tab,
displayed alongside that tab. If you want to browse somewhere else, close the
DevTools pane and open a new browser.

### Why lock navigation entirely?

The alternative — allowing navigation from a DevTools pane — reintroduces the
duplicate-DevTools problem in a harder form:

1. **User types `devtools://4` in the URL bar.** We'd have to intercept every
   navigation, check whether the target is a `devtools://` URL, query the GUI
   for whether that tab already has DevTools, and handle the error — all inside
   the running TUI event loop.
2. **User navigates from DevTools to a normal URL, then presses "back".** The
   browser would navigate back to the `devtools://` URL, potentially recreating
   the duplicate overlay without any check.
3. **User navigates to a normal URL.** Now we have a pane that was created as
   DevTools but is displaying a regular page. The `inspected_tab_id` tracking
   becomes inconsistent.

All three cases require runtime error handling inside the TUI (async error
messages from the GUI, error display in the UI, state reconciliation). By
locking navigation entirely, we reduce the problem to a single check at launch
time. The TUI validates before it starts, prints an error to stderr if rejected,
and never enters the UI. No async error handling, no runtime checks, no UI error
display needed.

## Relevant Code

- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — `CreateDevToolsTab`, `tabs_` vector
- `gui/src/apprt/xpc.zig` — `handleSetDevtoolsOverlay`, `handleQueryLast`,
  `panes` map, `inspected_tab_id` field on `Pane`
- `tui/src/main.rs` — DevTools detection (line 240), `send_set_devtools_overlay`
  (line 329), Control mode keybindings (line 400), Edit mode navigation
  (line 469)
- `tui/src/xpc.rs` — `send_query_last` (synchronous request-reply pattern)

## Experiment 1: Launch-time validation + locked DevTools mode

### Hypothesis

If the TUI sends a synchronous `query_devtools` message before entering the UI,
and the GUI checks for duplicate `inspected_tab_id` across all panes, then:

- Opening a second DevTools for the same tab prints an error and exits
  immediately
- The TUI never reaches ratatui, so no UI error handling is needed
- Locking all navigation keys in DevTools mode prevents the user from creating
  duplicates after launch

### Changes

#### 1. GUI: New `handleQueryDevtools` handler (`xpc.zig`)

Add a new synchronous handler following the `handleQueryLast` pattern. Register
it in `handleMessage` as `"query_devtools"`.

**Request fields:**

- `inspected_tab_id` (i64) — 0 for auto-target, N for explicit
- `profile` (string) — browser profile name

**Handler logic:**

1. **Resolve auto-target.** If `inspected_tab_id == 0`, look up
   `last_browser_pane` → get its `tab_id`. If `last_browser_pane` is null or has
   no `tab_id`, reply with error `"No browser tab found"`.
2. **Check for duplicates.** Iterate all entries in `panes`. If any pane has
   `inspected_tab_id == resolved_tab_id` (meaning it's a DevTools pane
   inspecting the same tab), reply with error
   `"Tab N already has DevTools open"`.
3. **Success.** Reply with the resolved `tab_id`.

**Reply fields (success):**

- `tab_id` (i64) — the resolved tab ID to inspect

**Reply fields (error):**

- `error` (string) — human-readable error message

#### 2. TUI: New `send_query_devtools` function (`xpc.rs`)

Add a synchronous request-reply function following the `send_query_last`
pattern.

```rust
pub fn send_query_devtools(
    &self,
    pane_id: &str,
    inspected_tab_id: i64,
    profile: &str,
) -> Result<i64, String>
```

- Build XPC dictionary with `action = "query_devtools"`, `pane_id`,
  `inspected_tab_id`, `profile`
- Send with `xpc_connection_send_message_with_reply_sync`
- Check reply for `error` field → return `Err(error_string)`
- Otherwise read `tab_id` field → return `Ok(tab_id)`

#### 3. TUI: Pre-check before entering ratatui (`main.rs`)

After detecting `is_devtools` (line 255) but before entering raw mode (line
263), add a validation block:

```rust
if is_devtools {
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        match conn.send_query_devtools(pid, inspected_tab_id, &profile) {
            Ok(resolved_tab_id) => {
                inspected_tab_id = resolved_tab_id;
            }
            Err(err) => {
                eprintln!("Error: {}", err);
                return Ok(());
            }
        }
    }
}
```

This requires changing `inspected_tab_id` from `let` to `let mut`.

If the query succeeds, the resolved `tab_id` is used for all subsequent
`send_set_devtools_overlay` calls — this handles the auto-target case where
`inspected_tab_id` was 0 and needs to be replaced with the actual tab ID.

If the query fails, the error prints to the terminal and the TUI exits
immediately. The user sees the error in their terminal pane without ratatui ever
starting.

#### 4. TUI: Lock navigation in DevTools mode (`main.rs`)

In the event loop's `Mode::Control` branch (line 400), skip all keys that enter
Edit mode when `is_devtools` is true:

- `i`, `A`, `I` — enter Insert mode (navigate URL)
- `n` — enter Normal mode (edit URL)
- `v`, `V` — enter Visual mode (select URL text)

These keys are simply ignored. The user can still:

- `Enter` — switch to Browse mode (interact with DevTools)
- `Esc` — switch back to Control mode
- `:` — enter Command mode (`:q` to quit, etc.)
- `Ctrl+C` — quit

In `Mode::Edit` (line 469), add a safety guard: if `is_devtools`, pressing Enter
does nothing (no `send_navigate`). This is a belt-and-suspenders check — Edit
mode should be unreachable in DevTools, but if it's somehow entered, it can't
trigger navigation.

#### 5. TUI: DevTools viewport title and status hints (`main.rs`)

Three visual signals reinforce that this is a locked DevTools pane:

**A. Viewport title.** Override the viewport title when `is_devtools`. Instead
of showing the page title from Chromium (e.g., "DevTools - google.com"), show
`DevTools · default/4` — the profile name and tab ID. This is stable and
unambiguous. The format is `DevTools · {profile}/{inspected_tab_id}`.

**B. Status bar hints.** In Control mode, the normal browser shows
`:q⏎ quit  i edit url  ⏎ browse`. In DevTools, show only the keys that work:
`:q⏎ quit  ⏎ browse`. The reduced hint set signals "this is a limited mode"
without any explicit explanation.

**C. URL bar.** No change — it already shows `devtools://4` or `devtools`, which
is the third reinforcing signal.

### Test

1. Open a browser: `web google.com`
2. Open DevTools: `web devtools` → should work
3. In DevTools TUI, press `i` → should do nothing (no Edit mode)
4. In DevTools TUI, press Enter → should enter Browse mode normally
5. Open a second DevTools: `web devtools` → should print
   `"Error: Tab N already has DevTools open"` and exit immediately
6. Close the first DevTools, try again → should work
7. `web devtools://999` (nonexistent tab) → should print error and exit
8. `web devtools` with no browser open → should print
   `"Error: No browser tab found"` and exit

### Result: SUCCESS

All test cases pass. The launch-time `query_devtools` check correctly rejects
duplicate DevTools and missing tabs with immediate error messages. Navigation
keys are locked in DevTools mode — `i`, `A`, `I`, `n`, `v`, `V` are ignored. The
viewport title shows `DevTools · profile/tab_id`, and the status bar shows only
the available keys. The URL bar remains read-only with the `devtools://N`
indicator.

## Conclusion

The crash from Issue 686 — two DevTools sessions on the same tab causing a
`PaintController` DCHECK — is now prevented. The fix enforces Chromium's
one-DevTools-per-page invariant at the GUI level, before the TUI ever starts.

### What was built

- **`query_devtools` synchronous XPC message.** The TUI sends this before
  entering ratatui. The GUI resolves auto-targeting, checks for duplicate
  `inspected_tab_id` across all panes, and replies with the resolved `tab_id` or
  an error string. This is the first XPC message that returns structured errors
  — `query_last` returns empty replies on failure, but `query_devtools` returns
  `{ error: "..." }`.
- **Launch-time rejection.** If the check fails, the TUI prints the error to
  stderr and exits immediately. The ratatui UI never starts. No async error
  handling, no runtime checks.
- **Locked DevTools mode.** DevTools panes cannot navigate. All URL editing keys
  (`i`, `A`, `I`, `n`, `v`, `V`) are disabled in Control mode. Enter in Edit
  mode is a no-op safety guard. This eliminates the entire class of problems
  where a user might navigate to a `devtools://` URL from within a running pane.
- **Visual indicators.** Viewport title shows `DevTools · profile/tab_id`,
  status bar shows only `:q⏎ quit  ⏎ browse`, URL bar shows `devtools://N`.

### Design rationale

We considered three alternatives before settling on the locked-mode approach:

1. **Runtime error handling** — intercept every navigation, check for
   duplicates, display errors in the TUI. Too complex: requires async GUI→TUI
   error messages, a new error display area in the UI, and state reconciliation.
2. **Separate `devtools` binary** — cleaner `web` code, but duplicates ~300
   lines of XPC/event loop/ratatui infrastructure to avoid 6 conditionals.
3. **Immutable URL bar only** — prevents typing `devtools://N` but doesn't
   prevent back-navigation or other edge cases.

The locked-mode approach eliminates all edge cases by reducing the problem to a
single validation at launch time.

### Changes

- `gui/src/apprt/xpc.zig` — `handleQueryDevtools` handler, registered in
  `handleMessage`
- `tui/src/xpc.rs` — `send_query_devtools` synchronous request-reply
- `tui/src/main.rs` — pre-check before ratatui, `if !is_devtools` guards on edit
  keys, safety guard on Enter in Edit mode, viewport title override, reduced
  status bar hints
