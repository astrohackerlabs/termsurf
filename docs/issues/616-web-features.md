# Issue 616: Implement missing web features

## Goal

Systematically identify and implement browser features that are missing from the
current gui/ generation. High-impact features are implemented first. Features
that can't be implemented yet are logged for future work.

## Background

ts1 (WKWebView generation) implemented a comprehensive set of browser features
from v0.3 through v1.0. The current gui/ generation (Chromium via Content API)
has the core streaming pipeline working — live rendering, mouse input, keyboard
input, multi-pane multi-profile — but is missing many user-facing browser
features that ts1 had.

Some ts1 features translate directly to Chromium (downloads, file uploads, JS
dialogs). Others are WKWebView-specific and either aren't needed with Chromium
or require a different approach. This issue catalogs everything and prioritizes
what to build.

### Feature inventory

Features are organized by priority. Priority is based on how often a user would
encounter the missing feature during normal browsing.

#### High priority

These features affect common browsing scenarios. A user will hit these within
minutes of browsing.

| # | Feature                      | ts1 status                       | gui/ status     | Notes                                                                                                               |
| - | ---------------------------- | -------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------- |
| 1 | **target="_blank" handling** | Loads in same view               | Not implemented | Links requesting new windows (OAuth, "Open in new tab") silently fail without this. Very common on modern websites. |
| 2 | **JavaScript dialogs**       | alert/confirm/prompt via NSAlert | Not implemented | Many sites use confirm() for destructive actions, prompt() for input. Sites break without these.                    |
| 3 | **Downloads**                | WKDownloadDelegate + NSSavePanel | Not implemented | Any file download link currently does nothing.                                                                      |
| 4 | **File uploads**             | NSOpenPanel via WKUIDelegate     | Not implemented | `<input type="file">` does nothing without this. Common for profile pictures, attachments, etc.                     |
| 5 | **Page zoom**                | Cmd+=/-/0 via pageZoom           | Not implemented | Users expect standard zoom keybindings.                                                                             |
| 6 | **HTTP Basic Auth**          | NSAlert with username/password   | Not implemented | Password-protected pages show blank or error without this.                                                          |
| 7 | **URL normalization**        | Prepend https://                 | Not implemented | Users type `google.com`, not `https://google.com`. The `web` TUI or Chromium server should handle this.             |

#### Medium priority

These features matter but are encountered less frequently or have workarounds.

| #  | Feature                      | ts1 status                     | gui/ status     | Notes                                                                                                              |
| -- | ---------------------------- | ------------------------------ | --------------- | ------------------------------------------------------------------------------------------------------------------ |
| 8  | **Crash recovery**           | Reload/close dialog            | Not implemented | Chromium renderer crashes are rare but should be handled gracefully.                                               |
| 9  | **Camera/mic permissions**   | Permission prompt              | Not implemented | Only needed for video calls, media recording. Can defer.                                                           |
| 10 | **Console capture**          | JS injection → stdout/stderr   | Not implemented | Useful for developers. The `web` TUI could display console output. Requires Chromium DevTools protocol or similar. |
| 11 | **Web Inspector / DevTools** | Safari Inspector via Cmd+Alt+I | Not implemented | Chromium has DevTools built in, but we need a way to open them (remote debugging port, or in-process).             |

#### Lower priority

These are nice-to-have or may not apply to the Chromium architecture.

| #  | Feature                            | ts1 status                     | gui/ status         | Notes                                                                                      |
| -- | ---------------------------------- | ------------------------------ | ------------------- | ------------------------------------------------------------------------------------------ |
| 12 | **User-Agent spoofing**            | Custom Safari UA string        | Probably not needed | Chromium sends a real browser UA by default. Unlikely to get mobile layouts.               |
| 13 | **Header injection**               | Upgrade-Insecure-Requests      | Probably not needed | Chromium sends this header natively. Was a WKWebView-specific workaround.                  |
| 14 | **Blob download workaround**       | JS interceptor for WebKit bug  | Not needed          | This was a WebKit bug. Chromium handles blob: downloads natively.                          |
| 15 | **Session isolation (incognito)**  | Ephemeral WKWebsiteDataStore   | Not implemented     | Chromium supports incognito via BrowserContext. Low urgency — named profiles already work. |
| 16 | **Bookmarking**                    | Cmd+B, file-based JSON storage | Not implemented     | Useful but not critical for initial release.                                               |
| 17 | **JavaScript API (--js-api)**      | window.termsurf.exit(code)     | Not implemented     | Niche feature for scripting. Defer.                                                        |
| 18 | **Hide/show webviews (ctrl+z/fg)** | isHidden property              | Not implemented     | Terminal backgrounding support. Defer.                                                     |
| 19 | **Multi-webview stacking**         | Stack per pane with indicator  | Not implemented     | Multiple webviews per pane. Current architecture is one-per-pane. Defer.                   |
| 20 | **Dynamic tab titles**             | KVO on WKWebView.title         | Not implemented     | Tab shows page title. Requires Chromium to send title updates via XPC.                     |

#### Already implemented (in gui/ or differently)

| Feature               | Notes                                                                |
| --------------------- | -------------------------------------------------------------------- |
| **Profile isolation** | Multi-profile via separate Chromium Profile Servers (Issues 604–605) |
| **Three-mode focus**  | Browse/Control modes with Esc/Enter switching (Issue 607)            |
| **Focus management**  | Chromium focus/blur via XPC (Issue 606)                              |
| **Control bar**       | `web` TUI draws URL bar, status bar (Issue 504)                      |

### Implementation approach

Each feature is a self-contained experiment. Features that require Chromium-side
changes (new XPC messages, new Content API calls) are harder than features that
can be handled entirely in gui/ Zig code or the `web` TUI.

**Chromium-side changes** are needed for: downloads, file uploads, JS dialogs,
HTTP auth, crash recovery, camera/mic permissions, console capture, DevTools,
dynamic tab titles.

**GUI-side only** changes: target="_blank" (if we load in same tab), page zoom,
URL normalization.

## Experiments

### Experiment 1: Unify test pages and audit existing demos

#### Goal

Replace `html/` and `box-demo/` with a single `test-html/` directory containing
a Bun server that serves all test pages. A main index page links to every demo.
Each existing demo is tested in the current gui/ + Chromium pipeline to identify
which features work and which are broken.

#### Background

The repo currently has test HTML scattered across two top-level directories:

- `html/` — 4 standalone HTML files (dialogs, downloads, mouse, uploads)
- `box-demo/` — Bun server + spinning square demo (FPS, localStorage)

These were created ad-hoc during different experiments. They need a single home
with a proper server so we can systematically test browser features.

`ts4/box-demo/` and `ts5/box-demo/` are identical historical copies and are left
as-is.

#### Steps

##### Step 1: Create `test-html/` with Bun server

Create `test-html/server.ts` — a Bun HTTP server that:

- Serves static files from `test-html/public/`
- Runs on port 9616 (Issue 616)
- Has a root route (`/`) that serves an index page with links to all demos

##### Step 2: Create the index page

Create `test-html/public/index.html` — a main page listing all test demos with
links. Organized by feature category matching the inventory in this issue.

##### Step 3: Move existing test pages

Move the existing test pages into `test-html/public/`:

- `html/test-dialogs.html` → `test-html/public/test-dialogs.html`
- `html/test-download.html` → `test-html/public/test-download.html`
- `html/test-mouse.html` → `test-html/public/test-mouse.html`
- `html/test-upload.html` → `test-html/public/test-upload.html`
- `box-demo/public/index.html` → `test-html/public/test-box-demo.html`

##### Step 4: Add new test pages for untested features

Create minimal test pages for features that don't have test pages yet:

- `test-html/public/test-target-blank.html` — Links with `target="_blank"` and
  `window.open()`
- `test-html/public/test-zoom.html` — Text at various sizes to verify zoom
  behavior
- `test-html/public/test-auth.html` — Link to an HTTP Basic Auth endpoint (can
  use httpbin.org or similar)

##### Step 5: Delete old directories

```bash
git rm -r html/
git rm -r box-demo/
```

##### Step 6: Test each demo

Launch TermSurf, run `web http://localhost:9616`, and systematically test each
demo page. Record pass/fail for each:

| Demo              | Feature tested                      | Expected behavior                           | Result |
| ----------------- | ----------------------------------- | ------------------------------------------- | ------ |
| test-box-demo     | Canvas rendering, FPS, localStorage | Spinning square at 60fps, identity persists |        |
| test-mouse        | Mouse events                        | Click counter increments, events logged     |        |
| test-dialogs      | alert/confirm/prompt                | Native dialogs appear                       |        |
| test-download     | File downloads                      | Save dialog appears                         |        |
| test-upload       | File uploads                        | File picker opens                           |        |
| test-target-blank | target="_blank" links               | Link loads (in same or new view)            |        |
| test-zoom         | Page zoom                           | Cmd+=/-/0 changes text size                 |        |
| test-auth         | HTTP Basic Auth                     | Login dialog appears                        |        |

#### Verification

1. `bun run test-html/server.ts` starts and serves the index page at
   `http://localhost:9616`
2. All demo pages are accessible from the index
3. `html/` and `box-demo/` are deleted from the repo
4. `ts4/box-demo/` and `ts5/box-demo/` are unchanged
5. Each demo has a pass/fail result recorded in the table above

### Experiment 2: Loading state indicator and pink texture removal

#### Goal

Show a blue progress bar at the top of the terminal pane while a web page is
loading. Remove the pink texture that currently shows during page load. The
progress bar is Ghostty's built-in OSC 9;4 indicator — no custom rendering
needed.

#### Background

When a user navigates to a page, the current experience is:

1. Pink texture fills the pane (the default IOSurface before Chromium sends a
   frame)
2. No indication that anything is happening
3. Page suddenly appears when Chromium sends the first frame

The desired experience is:

1. Blue progress bar pulses at the top of the terminal pane
2. Progress bar shows determinate progress as the page loads
3. Progress bar disappears when the page is fully loaded
4. No pink texture — the pane shows nothing (or the previous page) until the
   first Chromium frame arrives

#### Architecture

Three processes participate, connected by XPC:

```
Chromium Profile Server ──XPC──▶ TermSurf GUI ──XPC──▶ web TUI ──stdout──▶ Ghostty
```

**Chromium Profile Server** detects loading state via `WebContentsObserver`
callbacks and sends XPC messages to the GUI.

**TermSurf GUI** receives loading state from Chromium and relays it to the web
TUI via the existing reverse XPC channel.

**web TUI** receives loading state and emits OSC 9;4 escape sequences to stdout.
Ghostty renders the blue progress bar.

#### XPC message protocol

##### Chromium Profile Server → GUI

New message type: `loading_state`

```
{
  "action":   "loading_state",
  "pane_id":  "<uuid>",
  "state":    "loading" | "progress" | "done" | "error",
  "progress": <uint64 0–100>    // only meaningful when state == "progress"
}
```

Sent at these Chromium events:

| Chromium callback        | `state`      | `progress` | When                        |
| ------------------------ | ------------ | ---------- | --------------------------- |
| `DidStartLoading()`      | `"loading"`  | 0          | Navigation begins           |
| `LoadProgressChanged(p)` | `"progress"` | p × 100    | Periodic during load        |
| `DidStopLoading()`       | `"done"`     | 100        | All frames finished loading |
| `DidFailLoad(...)`       | `"error"`    | 0          | Load failed                 |

##### GUI → web TUI

New message type: `loading_state` (relayed from Chromium)

```
{
  "action":   "loading_state",
  "state":    "loading" | "progress" | "done" | "error",
  "progress": <uint64 0–100>
}
```

The GUI strips `pane_id` (the web TUI already knows which pane it is) and
forwards on the existing `web_peer` connection.

##### web TUI → stdout (terminal escape sequences)

| Received state | OSC 9;4 sequence              | Ghostty renders            |
| -------------- | ----------------------------- | -------------------------- |
| `"loading"`    | `\x1b]9;4;3\x1b\\`            | Indeterminate blue pulse   |
| `"progress"`   | `\x1b]9;4;1;{progress}\x1b\\` | Determinate blue bar at N% |
| `"done"`       | `\x1b]9;4;0\x1b\\`            | Bar removed                |
| `"error"`      | `\x1b]9;4;2\x1b\\`            | Red error bar              |

#### Changes

##### Chromium Profile Server (branch: `146.0.7650.0-issue-616`)

**File: `content/chromium_profile_server/browser/shell_video_consumer.h`**

Add `WebContentsObserver` method declarations:

- `void DidStartLoading() override;`
- `void DidStopLoading() override;`
- `void LoadProgressChanged(double progress) override;`
- `void DidFailLoad(content::RenderFrameHost*, const GURL&, int) override;`

Add helper:

- `void SendLoadingState(const char* state, int progress);`

**File: `content/chromium_profile_server/browser/shell_video_consumer.cc`**

Implement the four observer methods. Each calls `SendLoadingState()` which
constructs and sends the XPC dictionary:

```cpp
void ShellVideoConsumer::SendLoadingState(const char* state, int progress) {
  if (!xpc_connection_) return;
  xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
  xpc_dictionary_set_string(msg, "action", "loading_state");
  xpc_dictionary_set_string(msg, "pane_id", pane_id_.c_str());
  xpc_dictionary_set_string(msg, "state", state);
  xpc_dictionary_set_uint64(msg, "progress", progress);
  xpc_connection_send_message(xpc_connection_, msg);
  xpc_release(msg);
}
```

##### TermSurf GUI

**File: `gui/src/apprt/xpc.zig`**

Add `"loading_state"` to the `handleMessage` dispatcher. The handler:

1. Reads `pane_id` from the message
2. Looks up the pane
3. Sends a new `loading_state` message to `pane.web_peer` with `state` and
   `progress` fields (no `pane_id` — the TUI knows its own pane)

New function: `handleLoadingState(msg)` — approximately 15 lines following the
same pattern as `sendModeToWeb`.

##### web TUI

**File: `tui/src/xpc.rs`**

Add `LoadingState` variant to `CompositorMessage`:

```rust
pub enum CompositorMessage {
    ModeChanged { browsing: bool },
    UrlChanged { url: String },
    LoadingState { state: String, progress: u8 },
}
```

Add parsing in the event handler block (alongside `mode_changed` and
`url_changed`):

```rust
} else if action == "loading_state" {
    let state_key = CString::new("state").unwrap();
    let state_ptr = unsafe { xpc_dictionary_get_string(event, state_key.as_ptr()) };
    if !state_ptr.is_null() {
        let state = unsafe { CStr::from_ptr(state_ptr) }
            .to_str().unwrap_or("done").to_string();
        let progress_key = CString::new("progress").unwrap();
        let progress = unsafe { xpc_dictionary_get_uint64(event, progress_key.as_ptr()) } as u8;
        let _ = tx.send(CompositorMessage::LoadingState { state, progress });
    }
}
```

**File: `tui/src/main.rs`**

Add OSC 9;4 emission when `LoadingState` is received:

```rust
CompositorMessage::LoadingState { state, progress } => {
    match state.as_str() {
        "loading" => write!(stdout, "\x1b]9;4;3\x1b\\")?,
        "progress" => write!(stdout, "\x1b]9;4;1;{}\x1b\\", progress)?,
        "done" => write!(stdout, "\x1b]9;4;0\x1b\\")?,
        "error" => write!(stdout, "\x1b]9;4;2\x1b\\")?,
        _ => {}
    }
    stdout.flush()?;
}
```

##### Pink texture removal

**File: `gui/src/renderer/Metal.zig`** (or wherever the pink fallback texture is
created)

Remove or replace the pink fallback color. Options:

- Set the fallback to transparent (clear color)
- Skip rendering the overlay entirely when no IOSurface has been received yet
- Show nothing until the first `display_surface` message arrives

The exact approach depends on how the overlay pipeline handles the "no surface
yet" state. The simplest change is making the initial clear color transparent
instead of pink.

#### Verification

1. Launch TermSurf, run `web http://localhost:9616`
2. **Loading indicator**: While the page loads, a blue progress bar pulses at
   the top of the terminal pane
3. **Progress updates**: For slow-loading pages, the bar shows determinate
   progress (0%–100%)
4. **Completion**: The bar disappears when the page finishes loading
5. **No pink**: No pink texture visible at any point during page load
6. **Error state**: Navigating to an invalid URL shows a red error bar briefly
7. **Subsequent navigations**: Clicking a link on the loaded page triggers the
   progress bar again

**Result:** Pass

The loading indicator works end-to-end: Chromium sends `loading_state` via XPC,
the GUI relays it to the web TUI, and the TUI emits OSC 9;4 which Ghostty
renders as a blue progress bar. The pink fallback texture is removed — the pane
shows nothing until Chromium sends its first frame.

However, there is a critical issue: **the longest wait is Chromium process
startup, not page loading.** When the user runs `web google.com` for the first
time, the Chromium Profile Server process must launch before any page can load.
This cold start takes significantly longer than loading google.com itself. The
loading indicator only appears briefly after Chromium is already running and the
page load begins. During the much longer Chromium startup phase, the user sees
no progress indication at all.

#### Conclusion

The Chromium-to-TUI loading pipeline works. The next experiment must address the
cold-start gap: the web TUI should show the progress bar immediately when
waiting for Chromium to start, not only after Chromium is running and reports
`DidStartLoading`. This can be done entirely in the GUI or TUI — the TUI can
emit OSC 9;4;3 (indeterminate pulse) as soon as it sends `set_overlay`, and
clear it when the first `loading_state` or `display_surface` arrives.

### Experiment 3: Cold-start loading indicator, safety timeout, and slow-load test page

#### Goal

Show the loading progress bar from the moment the user runs `web <url>` — not
just after Chromium is already running. Handle edge cases so the bar never gets
stuck. Add a test page that simulates a slow download for visual verification.

#### Background

Experiment 2 proved the loading pipeline works, but revealed a critical gap: the
longest wait during first use is Chromium process startup, not page loading.
During cold start, the user sees nothing for several seconds. The progress bar
only appears briefly once Chromium is running and fires `DidStartLoading`.

There are also edge cases where the bar could get stuck forever:

- Chromium crashes before sending any `loading_state`
- XPC connection drops
- A page load enters an infinite redirect loop
- The web TUI loses its compositor connection

#### Changes

##### web TUI (`tui/src/main.rs`)

**Immediate indeterminate pulse on overlay send:**

After the first `send_set_overlay()` call, immediately emit OSC 9;4;3
(indeterminate blue pulse). This covers the Chromium cold-start period. The
pulse runs until the first `loading_state` message (from Chromium) or
`display_surface` (first frame) arrives, whichever comes first.

Add a boolean `loading_bar_active` to track whether the progress bar is
currently showing. Set it to `true` after emitting the initial pulse. When a
`LoadingState` with state `"done"` or `"error"` arrives, set it to `false`.

**Safety timeout:**

Add a 30-second timeout. If `loading_bar_active` is `true` for more than 30
seconds without receiving a `"done"` or `"error"` state, emit OSC 9;4;2 (red
error bar) briefly, then OSC 9;4;0 (clear) to prevent the bar from being stuck
forever. Track the start time with `std::time::Instant`.

The timeout check runs on each iteration of the event loop (every 250ms poll
cycle), so it adds no extra threads or complexity.

**Clear on exit:**

Before restoring the terminal, emit OSC 9;4;0 to ensure the progress bar is
cleared if the user quits (Ctrl+C or `q`) while loading.

##### Test page (`test-html/server.ts` and `test-html/public/test-slow-load.html`)

Add a `/slow` route to `server.ts` that accepts a `?seconds=N` query parameter
(default 10). The server sleeps for that duration using `Bun.sleep()`, then
streams a chunked HTML response. Every second, it sends a chunk that updates a
visual progress indicator on the page itself, so the user can see both:

1. Ghostty's blue progress bar at the top of the pane (OSC 9;4)
2. The page's own progress indicator in the viewport

The page design:

- Dark background matching the Tokyo Night theme
- A large circular or bar progress indicator that fills as chunks arrive
- Percentage text that updates with each chunk
- After loading completes: a "Done!" message with the total load time

The `/slow` route uses chunked transfer encoding (streaming `Response` in Bun)
to send partial HTML. Each chunk is a `<script>` tag that updates the progress
element's width/text.

Also add the new test page to `test-html/public/index.html` in a new "Loading"
section.

#### Verification

1. **Cold start**: Kill any running Chromium Profile Server. Launch TermSurf,
   run `web http://localhost:9616`. The blue progress bar should pulse
   immediately (indeterminate), then transition to determinate progress when
   Chromium reports loading, then disappear when the page finishes loading.

2. **Warm start**: With Chromium already running, run
   `web http://localhost:9616/slow?seconds=10`. The bar should pulse briefly
   (indeterminate), then show determinate progress 0%→100% over ~10 seconds. The
   page itself should show matching progress.

3. **Quick load**: Run `web http://localhost:9616`. The bar should pulse briefly
   and disappear quickly — no lingering after the page is loaded.

4. **Error case**: Run `web http://localhost:99999` (unreachable port). The bar
   should eventually show red (error) and then clear.

5. **Safety timeout**: If the user kills the Chromium server mid-load (e.g.,
   `kill -9`), the bar should not stay forever — after 30 seconds it clears with
   an error flash.

6. **Clean exit**: Press `q` or Ctrl+C while the bar is active. The bar should
   disappear — no orphaned progress indicator left in Ghostty's title bar.

7. **Slow page visual**: Navigate to `http://localhost:9616/slow?seconds=10`.
   Both Ghostty's progress bar and the page's own progress indicator should
   advance together over 10 seconds.

**Result:** Partial

The cold-start pulse, safety timeout, clean exit, and test page all work
correctly. Two issues remain:

1. **Slow page progress stalls at ~33%.** On the `/slow?seconds=10` page, the
   Ghostty progress bar advances to roughly one-third and then stops moving
   until the page finishes loading. The in-page progress bar continues advancing
   normally. This happens because Chromium's `LoadProgressChanged` reflects an
   internal loading heuristic, not bytes received. For a single chunked HTTP
   response, Chromium considers the connection and initial headers as
   significant progress (~33%), then reports little additional progress while
   the same response continues streaming. The bar jumps to ~33%, stalls for the
   remaining seconds, then disappears when `DidStopLoading` fires. During
   Chromium cold start the bar keeps moving because Chromium goes through
   multiple internal loading phases (process init, DNS, connection, headers,
   rendering) that each contribute progress.

2. **Back navigation leaves bar stuck at 100%.** Right-clicking a page and
   selecting "Back" causes the progress bar to fill to 100% and stay there
   permanently (until the 30-second safety timeout clears it). This likely
   happens because Chromium restores the page from the back/forward cache
   (bfcache). When bfcache is used, `DidStartLoading` fires and
   `LoadProgressChanged` quickly reaches 100%, but `DidStopLoading` may not fire
   because the page is restored rather than loaded. Without the "done" signal,
   the bar remains active.

#### Conclusion

The cold-start gap from Experiment 2 is solved — the progress bar now pulses
from the moment the user runs `web <url>`. The safety timeout and clean exit
work as designed.

The two remaining issues are both on the Chromium side of the pipeline:

- The **slow page stall** is a fundamental limitation of Chromium's progress
  heuristic. The TUI and XPC pipeline are working correctly — the problem is
  that Chromium simply doesn't report granular progress for single chunked
  responses. A future experiment could switch to indeterminate mode when
  progress hasn't changed for more than 2–3 seconds, giving the user a visual
  cue that loading is still in progress.

- The **back navigation stuck bar** needs investigation in the Chromium Profile
  Server. The fix is likely to observe an additional Chromium callback (e.g.,
  `RenderFrameHost::IsInBackForwardCache`, `DidFinishNavigation`, or
  `NavigationEntryCommitted`) and send "done" when a bfcache restore completes.
  Alternatively, the TUI could treat reaching 100% progress as equivalent to
  "done" after a short delay.

### Experiment 4: Debug back-navigation stuck bar

#### Goal

Add diagnostic logging across all three processes in the loading state pipeline
to determine exactly where the "done" message gets lost during back navigation.

#### Background

Experiment 3 found that right-clicking and selecting "Back" causes the progress
bar to fill to 100% and stay there permanently. Chromium's source code confirms
that `DidStopLoading()` fires on bfcache restores, so the callback should be
invoked. The problem is somewhere in the three-hop pipeline:

```
Chromium Profile Server → GUI (xpc.zig) → web TUI (main.rs)
```

Each hop has potential silent failure modes:

- **Chromium**: `SendLoadingState` returns early if `xpc_connection_` is null
- **GUI**: `handleLoadingState` returns early if pane lookup fails or `web_peer`
  is null — both with no logging
- **TUI**: No logging of received messages — we can't tell if messages arrive

Chromium already has `LOG(INFO)` in `DidStartLoading` and `DidStopLoading`. The
GUI and TUI have no loading state logging at all.

#### Changes

##### GUI (`gui/src/apprt/xpc.zig`)

Add a `log.info` call at the top of `handleLoadingState` so every incoming
loading state message is logged, including the early-return cases:

```zig
fn handleLoadingState(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const state_str = str(xpc_dictionary_get_string(msg, "state") orelse "?");
    const progress = xpc_dictionary_get_uint64(msg, "progress");

    const p = panes.get(pane_id) orelse {
        log.warn("loading_state: unknown pane={s} state={s}", .{ pane_id, state_str });
        return;
    };
    if (p.web_peer == null) {
        log.warn("loading_state: no web_peer pane={s} state={s}", .{ pane_id, state_str });
        return;
    }

    log.info("loading_state pane={s} state={s} progress={} → forwarding to web TUI", .{
        pane_id, state_str, progress,
    });

    // ... relay as before
}
```

This logs three cases: unknown pane (early return), no web_peer (early return),
and successful forward.

##### TUI (`tui/src/main.rs`)

Add `eprintln!` in the `LoadingState` message handler so every received message
is visible on stderr:

```rust
xpc::CompositorMessage::LoadingState { state, progress } => {
    eprintln!("[web] loading_state: state={} progress={}", state, progress);
    // ... existing OSC emission
}
```

##### No Chromium changes

Chromium already logs `DidStartLoading` and `DidStopLoading` via `LOG(INFO)`.
These appear in the Chromium Profile Server's stderr.

#### Verification

1. Launch TermSurf and run `web http://localhost:9616`
2. Wait for the page to load (bar should clear)
3. Click a link on the page to navigate to a subpage
4. Right-click and select "Back"
5. Observe the bar behavior — does it get stuck at 100%?

Check logs from all three processes:

- **Chromium stderr**: Look for `DidStartLoading` and `DidStopLoading` during
  the back navigation. If `DidStopLoading` fires, Chromium is not the problem.
- **GUI (TermSurf) logs**: Look for `loading_state` lines. If the GUI logs
  `state=done` being forwarded, the GUI is not the problem. If the GUI logs
  `unknown pane` or `no web_peer`, that's the drop point.
- **TUI stderr**: Look for `[web] loading_state` lines. If the TUI never
  receives `state=done`, the message was lost between GUI and TUI.

The experiment succeeds when we can identify which hop drops the "done" message
on back navigation.

**Result:** Fail

The TUI logging used `eprintln!` which writes to stderr. In a terminal, stderr
goes to the same PTY as stdout, so the debug output appeared in the alternate
screen and corrupted ratatui's display — the TUI vanished and colors broke.

The GUI logging compiled after fixing a Zig type error (`[]const u8` vs
`[*:0]const u8`) but was reverted along with the TUI changes.

#### Conclusion

`eprintln!` cannot be used for debug logging while ratatui owns the alternate
screen. The next attempt must write to a log file at
`~/dev/termsurf/logs/web.log` instead. The GUI logging approach (Zig
`log.info`/`log.warn`) is correct and can be reapplied as-is.

### Experiment 5: Debug back-navigation stuck bar (file logging)

#### Goal

Same as Experiment 4: add diagnostic logging across all three processes to
determine where the "done" message gets lost during back navigation. This time,
TUI logging writes to a file instead of stderr.

#### Background

Experiment 4 failed because `eprintln!` writes to the same PTY as stdout,
corrupting ratatui's alternate screen. The fix is simple: write to
`/Users/ryan/dev/termsurf/logs/web.log` instead. This is the hard-coded absolute
path to the repo's `logs/` directory, which is gitignored.

#### Changes

##### TUI (`tui/src/main.rs`)

Open `/Users/ryan/dev/termsurf/logs/web.log` in append mode before the event
loop. Store the file handle as `Option<std::fs::File>`. On each `LoadingState`
message, write a timestamped line to the file using `writeln!`. No `eprintln!`
anywhere — nothing touches stderr during the event loop.

```rust
// Before the event loop, after enable_raw_mode:
let mut debug_log = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open("/Users/ryan/dev/termsurf/logs/web.log")
    .ok();
```

```rust
// In the LoadingState handler:
if let Some(ref mut log) = debug_log {
    let _ = writeln!(log, "[web] loading_state: state={} progress={}", state, progress);
}
```

##### GUI (`gui/src/apprt/xpc.zig`)

Same changes as Experiment 4 (which compiled correctly after the type fix).
Rewrite `handleLoadingState` to:

1. Extract `state_raw` (the `[*:0]const u8` C pointer) and `state_str` (a
   `[]const u8` slice via `std.mem.span`) separately
2. Log `warn` on unknown pane or null `web_peer` (with pane ID and state)
3. Log `info` on successful forward (with pane ID, state, and progress)
4. Pass `state_raw` (not `state_str`) to `xpc_dictionary_set_string`

GUI logging uses Zig's `log.info`/`log.warn` which goes to the TermSurf
process's stderr — visible in the terminal where TermSurf was launched, not in
any PTY pane.

##### No Chromium changes

Chromium already logs `DidStartLoading` and `DidStopLoading` via `LOG(INFO)`.

#### Verification

1. Launch TermSurf: `gui/zig-out/TermSurf.app/Contents/MacOS/TermSurf`
2. Run `web http://localhost:9616`
3. Wait for the page to load
4. Click a link to navigate to a subpage
5. Right-click and select "Back"
6. Observe the bar — does it get stuck at 100%?
7. Check `/Users/ryan/dev/termsurf/logs/web.log` for TUI-received messages
8. Check TermSurf's terminal output for GUI `loading_state` log lines
9. Check Chromium logs for `DidStartLoading`/`DidStopLoading`

The TUI must not be visually affected by the logging. The experiment succeeds
when we can identify which hop drops the "done" message on back navigation.

**Result:** Pass

File logging works correctly — the TUI is unaffected and all loading state
messages are captured in `/Users/ryan/dev/termsurf/logs/web.log`. The GUI logs
confirm that every `loading_state` message from Chromium is forwarded to the web
TUI, including `state=done` on back navigation (bfcache restore).

The root cause is a **straggler `progress 100` message** that arrives after
`done`. The web.log shows this pattern on back navigation:

```
[web] loading_state: state=done progress=100       ← bar clears
[web] loading_state: state=progress progress=100   ← STRAGGLER re-activates bar
```

Chromium fires `LoadProgressChanged(1.0)` and `DidStopLoading()` close together,
but XPC message delivery does not guarantee ordering. The `progress 100` message
sometimes arrives after the `done` message. When it does, the TUI sets
`loading_bar_active = true` and emits OSC 9;4;1;100 — but no subsequent `done`
arrives to clear the bar, so it stays stuck at 100% until the 30-second safety
timeout.

#### Conclusion

The three-hop pipeline (Chromium → GUI → TUI) is working correctly. No messages
are dropped. Chromium fires `DidStopLoading` on bfcache restores as expected.

The fix is entirely in the TUI: ignore `progress` messages that arrive after
`done` until the next `loading` message starts a new navigation cycle. This can
be implemented with a simple state machine — track whether we're in a loading
cycle and only process `progress` messages while active. No Chromium or GUI
changes are needed.

### Experiment 6: Fix straggler progress message

#### Goal

Prevent the loading bar from getting stuck at 100% after back navigation (and
any other scenario where a `progress` message arrives after `done`).

#### Background

Experiment 5 identified the root cause: Chromium fires
`LoadProgressChanged(1.0)` and `DidStopLoading()` close together, but XPC
message delivery doesn't guarantee ordering. A straggler `progress 100`
sometimes arrives after `done`, re-activating the loading bar with no subsequent
`done` to clear it.

The current code unconditionally sets `loading_bar_active = true` on every
`progress` message (line 179 of `tui/src/main.rs`). This means any `progress`
arriving after `done` re-lights the bar.

#### Changes

##### Chromium Profile Server (`shell_video_consumer.cc`)

**Branch: `146.0.7650.0-issue-616`**

In `LoadProgressChanged`, skip sending when progress reaches 100%.
`DidStopLoading` already sends `"done"` with progress 100 — the `progress 100`
message is redundant and causes the race condition.

```cpp
void ShellVideoConsumer::LoadProgressChanged(double progress) {
#if BUILDFLAG(IS_MAC)
  int pct = static_cast<int>(progress * 100);
  if (pct >= 100) return;  // DidStopLoading sends "done"
  SendLoadingState("progress", pct);
#endif
}
```

This fixes at the source — no straggler message is ever sent, so no XPC ordering
race is possible.

##### No GUI or TUI changes

The TUI logic is correct as-is. The problem was that Chromium sent a redundant
message that could arrive out of order.

#### Verification

1. Launch TermSurf, run `web http://localhost:9616`
2. Wait for the page to load — bar should clear normally
3. Click a link to navigate to a subpage
4. Right-click and select "Back"
5. **Pass criterion**: The bar fills to 100% and clears — it does NOT stay stuck

Also verify the normal cases still work:

6. Cold start shows indeterminate pulse → determinate progress → clear
7. Navigating to `/slow?seconds=5` shows progress → clear
8. Ctrl+C during loading clears the bar

**Result:** Pass

Back navigation no longer leaves the bar stuck at 100%. Suppressing
`progress
100` at the source eliminates the race condition entirely — no
straggler message is ever sent.

#### Conclusion

One-line fix in Chromium: skip `SendLoadingState` when `pct >= 100` since
`DidStopLoading` already sends `"done"`. Fixing at the source was the right call
— simpler than a TUI state machine workaround, and eliminates the race condition
rather than tolerating it.

### Experiment 7: Multi-resource slow page for progress bar testing

#### Goal

Replace the single-stream `/slow` test page with a multi-resource version that
loads many separate subresources, each with a server-side delay. This exercises
Chromium's `LoadProgressChanged` heuristic properly and verifies that the
progress bar advances smoothly from 0% to 100%.

#### Background

The existing `/slow?seconds=N` page streams a single chunked HTTP response.
Chromium's progress heuristic treats this as one resource — it jumps to ~33%
(connection + headers) and then stalls until the stream finishes. This is not a
bug; it's just a poor match for the heuristic.

Real websites load dozens of separate subresources (images, CSS, JS). Chromium
tracks each completed subresource as progress, producing smooth advancement.
This experiment creates a test page that mimics that pattern.

#### Changes

##### Test server (`test-html/server.ts`)

Add a `/slow-resource` route that accepts `?id=N&delay=S` query parameters. The
server sleeps for `S` seconds (using `Bun.sleep()`), then returns a 1x1 PNG
image. Each request is an independent HTTP response, so Chromium counts each as
a separate subresource.

Replace the existing `/slow` route. The new `/slow?seconds=N&count=C` route
returns a complete HTML page immediately (not streamed). The page contains `C`
`<img>` tags (default 20), each pointing to `/slow-resource?id=N&delay=D` where
`D` is `seconds / count` (distributes the total time evenly across resources).

The page itself shows a progress indicator updated via `<img>` `onload` handlers
— each time an image loads, JavaScript increments a counter and updates the
display. This lets the user see both:

1. Ghostty's blue progress bar (from Chromium's heuristic)
2. The page's own resource-completion counter

##### Index page (`test-html/public/index.html`)

Update the Loading section links to use the new parameters:

- `Slow Load (10s)` → `/slow?seconds=10`
- `Slow Load (3s)` → `/slow?seconds=3`
- Add `Slow Load (10s, 40 resources)` → `/slow?seconds=10&count=40`

#### Verification

1. Launch TermSurf, run `web http://localhost:9616`
2. Click "Slow Load (10s)" — the Ghostty progress bar should advance smoothly
   from 0% to 100% over ~10 seconds, not stall at 33%
3. The in-page counter should show resources loading one by one
4. The bar should clear when all resources finish
5. Repeat with "Slow Load (3s)" — same smooth behavior, faster
6. Repeat with "Slow Load (10s, 40 resources)" — even smoother granularity
