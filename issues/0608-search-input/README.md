+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

# Issue 608: Search Input

## Goal

Search form submissions work on all sites. The overlay shows the new page's
content and continues accepting mouse and keyboard input after form submission.

## Background

Issue 607 discovered that submitting search forms on certain sites freezes the
overlay. The freeze is specific to search inputs — clicking links works fine.
Link clicks navigate to new pages, the overlay renders the new content, and
input continues normally.

### What freezes

- **Google search** — pressing Enter in the search box or clicking the Search
  button freezes the overlay. No new frames, no mouse response, no keyboard
  response.
- **lite.duckduckgo.com** — clicking the Search button freezes the overlay. Same
  symptoms as Google. Note: regular duckduckgo.com works fine.

### What works

- **Clicking links** — navigates to the new page, overlay renders new content,
  input continues. This works on all tested sites.
- **duckduckgo.com** — search submission works normally. Only the lite variant
  freezes.
- **Wikipedia search** — clicking the Search button navigates to results. The
  overlay renders the new page and input continues.

### Key observations

1. The freeze is not caused by navigation in general — link clicks navigate
   without issues.
2. The freeze is not caused by page weight — lite.duckduckgo.com is pure HTML
   with minimal JavaScript.
3. The freeze is not permanent — the overlay eventually "gets unstuck" at a
   random time, suggesting a timeout or internal recovery mechanism.
4. The freeze affects both input (mouse and keyboard) and visual updates (stale
   frame).

### What's different about the frozen cases

Since link navigation works but certain search submissions don't, the issue is
not about `RenderWidgetHostView` swaps or `FrameSinkId` changes during
navigation — link clicks would trigger the same lifecycle events.

Possible differences between the working and frozen cases:

- **Form submission method.** Google search may use JavaScript-driven navigation
  (intercepting the form submit, calling `window.location` or the History API).
  DuckDuckGo lite may use POST (HTML form with `method="POST"`). Wikipedia may
  use a simple GET form. Link clicks are always GET navigations.
- **Redirects.** Google search goes through redirects (302s, URL rewrites).
  DuckDuckGo lite may also redirect. Wikipedia search may not. Redirects during
  navigation could cause intermediate states that confuse the capturer or input
  pipeline.
- **JavaScript event handling.** Google's search button is heavily
  JavaScript-driven. The click handler may do something that interferes with
  normal navigation flow (e.g., preventing the default action and navigating
  programmatically).
- **Content Security Policy or permissions.** Google's pages have strict CSP
  headers. The new page might trigger permission prompts or security checks that
  we don't handle.
- **Renderer process decisions.** Chromium may make different process allocation
  decisions for form submissions vs link clicks, though this seems unlikely for
  same-origin navigations.

### Current navigation handling

`ShellVideoConsumer` is already a `WebContentsObserver`. It overrides two
lifecycle callbacks:

**`RenderViewReady()`** — fires when the initial view is ready. Calls
`Attach()`, which creates the `FrameSinkVideoCapturer`, configures it for 120fps
capture, targets it to the current `FrameSinkId` via `ChangeTarget()`, and
starts capture.

**`DidFinishNavigation()`** — fires when navigation commits. Currently does two
things: re-applies the viewport size (which content_shell may reset after
navigation) and sends a `url_changed` XPC message to the app with the new URL.

### The capturer lifecycle

The `Attach()` method in `ShellVideoConsumer` performs the full capturer setup:

```cpp
capturer_ = manager->CreateVideoCapturer();
capturer_->SetFormat(media::PIXEL_FORMAT_ARGB);
capturer_->SetMinCapturePeriod(base::Milliseconds(8));  // 120fps
capturer_->SetAutoThrottlingEnabled(false);
capturer_->SetResolutionConstraints(physical_size, physical_size, false);
capturer_->ChangeTarget(viz::VideoCaptureTarget(frame_sink_id), 0);
capturer_->Start(this, viz::mojom::BufferFormatPreference::kPreferMappableSharedImage);
```

### Available WebContentsObserver callbacks

| Callback                   | When it fires                           |
| -------------------------- | --------------------------------------- |
| `RenderViewReady()`        | Initial view creation                   |
| `DidFinishNavigation()`    | Navigation commits (already overridden) |
| `RenderViewHostChanged()`  | RenderViewHost is swapped (old, new)    |
| `RenderFrameHostChanged()` | RenderFrameHost is swapped (old, new)   |
| `PrimaryPageChanged()`     | Primary page changes (post-commit)      |

### Investigation needed

The root cause is unclear. Link navigation works, which rules out the simplest
explanation (view swap losing focus/capturer target). The issue is specific to
certain form submissions but not others.

Before proposing a fix, we need to understand what actually happens during the
freeze:

1. **Add logging to `DidFinishNavigation`.** Does the callback fire at all for
   the frozen cases? Does it fire for link clicks? Comparing the two will reveal
   whether the navigation even commits.
2. **Log the `FrameSinkId` before and after navigation.** If it changes for
   frozen cases but not for working ones, re-targeting the capturer may help
   even though it doesn't explain why links work.
3. **Log `GetRenderWidgetHostView()` during the freeze.** Is the view null? Is
   it a different view object than before navigation?
4. **Check the navigation type.** `NavigationHandle` has methods like
   `IsPost()`, `GetRedirectChain()`, `IsSameDocument()`,
   `IsServedFromBackForwardCache()`. Logging these for frozen vs working cases
   will narrow down what's different.
5. **Test form submissions on other sites.** Try a simple HTML page with a GET
   form and a POST form to isolate whether the issue is about form method, site
   behavior, or something else.

### Key files

- `chromium/src/content/chromium_profile_server/browser/shell_video_consumer.h`
  — `ShellVideoConsumer` class, `WebContentsObserver` overrides
- `chromium/src/content/chromium_profile_server/browser/shell_video_consumer.cc`
  — `Attach()`, `DidFinishNavigation()`, `OnFrameCaptured()`
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — `HandleFocusChanged()`, tab management
- `chromium/src/content/public/browser/web_contents_observer.h` — Available
  lifecycle callbacks

## Experiment 1: Diagnostic logging

### Goal

Make the Chromium Profile Server's logs visible and add diagnostic logging to
understand what happens during the freeze. Compare a working case (clicking a
link on Wikipedia) vs a frozen case (submitting a search on Google).

### Chromium branch

Create `146.0.7650.0-issue-608` from `146.0.7650.0-issue-607`. The 608 branch
builds on 607's keyboard forwarding code — we need it to type into search boxes.

### Design

**Phase 1: Route server logs to `~/dev/termsurf/logs/chromium-server.log`.**

The server is spawned with `std.process.Child` in `xpc.zig`. Currently its
stderr is not redirected — all `LOG()` output is lost. Add Chromium's
`--enable-logging=stderr` and `--log-file=<path>` flags as command-line
arguments when spawning the server. This requires no Zig stdio changes — just
additional args in the argv array.

In `spawnServerProcess()`, add two new arguments:

```zig
var logging_buf: [64]u8 = undefined;
const logging_arg = std.fmt.bufPrintZ(
    &logging_buf,
    "--enable-logging=stderr",
    .{},
) catch return;

var logfile_buf: [256]u8 = undefined;
const logfile_arg = std.fmt.bufPrintZ(
    &logfile_buf,
    "--log-file={s}/dev/termsurf/logs/chromium-server.log",
    .{home},
) catch return;
```

Add them to the argv array:

```zig
var child = std.process.Child.init(
    &.{ server_path, xpc_arg, data_arg, hidden_arg, logging_arg, logfile_arg },
    alloc,
);
```

**Phase 2: Add navigation diagnostics to `DidFinishNavigation`.**

In `shell_video_consumer.cc`, expand the `DidFinishNavigation` handler to log
navigation properties that differ between working and frozen cases:

```cpp
LOG(INFO) << "[ShellVideoConsumer] Navigation committed:"
          << " url=" << navigation_handle->GetURL().spec()
          << " is_post=" << navigation_handle->IsPost()
          << " is_same_document=" << navigation_handle->IsSameDocument()
          << " is_error_page=" << navigation_handle->IsErrorPage()
          << " is_download=" << navigation_handle->IsDownload()
          << " is_served_from_bfcache="
          << navigation_handle->IsServedFromBackForwardCache()
          << " redirect_chain_size="
          << navigation_handle->GetRedirectChain().size()
          << " net_error=" << navigation_handle->GetNetErrorCode()
          << " pane=" << pane_id_;
```

Also log the `FrameSinkId` and view pointer so we can see if they change:

```cpp
RenderWidgetHostView* view = web_contents()->GetRenderWidgetHostView();
if (view) {
  auto fsid = view->GetRenderWidgetHost()->GetFrameSinkId();
  LOG(INFO) << "[ShellVideoConsumer] Post-nav view=" << view
            << " FrameSinkId=" << fsid.ToString();
}
```

**Phase 3: Log null view in input handlers.**

In `HandleMouseEvent` and `HandleKeyEvent` in `shell_browser_main_parts.cc`, add
a log when the view is null (currently we silently return):

```cpp
auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
if (!view) {
  LOG(WARNING) << "[ProfileServer] view is null for pane=" << pane_id;
  return;
}
```

### Verification

```bash
cd ghost && zig build
cd ~/dev/termsurf/chromium/src && export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH" && autoninja -C out/Default chromium_profile_server
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
```

Test 1 — working case:

```bash
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
# Click a link on the page (e.g., a Wikipedia article link)
# Observe: page navigates, overlay updates, input continues
```

Test 2 — frozen case:

```bash
cargo run -p web -- https://www.google.com
# Click the search box, type "hello", click the Search button
# Observe: overlay freezes
```

After both tests, check `~/dev/termsurf/logs/chromium-server.log` and compare:

1. Does `DidFinishNavigation` fire for both cases?
2. Do the navigation properties differ (IsPost, redirect chain, etc.)?
3. Do frames stop arriving (fps log stops printing)?
4. Is the view null when input events are forwarded during the freeze?
5. Does the `FrameSinkId` change?

**Result:** Pass

Wikipedia search worked without issues — typing, clicking Search, and navigating
all functioned normally. Lite DuckDuckGo reproduced the freeze: typing worked,
but clicking Search froze the overlay completely for ~30 seconds until the
session was closed.

The logs revealed the root cause. Comparing before and after the POST form
submission on lite.duckduckgo.com:

| Property     | Before navigation | After navigation       |
| ------------ | ----------------- | ---------------------- |
| View pointer | `0xae6404000`     | `0xae6449800`          |
| FrameSinkId  | `(5, 3)`          | `(5, 10)`              |
| `is_post`    | —                 | `1`                    |
| Frames after | 3-6 fps (normal)  | **Zero** — no fps logs |
| View null?   | No                | No                     |

The `RenderWidgetHostView` and `FrameSinkId` both changed during the POST
navigation. The `FrameSinkVideoCapturer` was still targeting the old
`FrameSinkId(5, 3)`, so no frames arrived from the new `FrameSinkId(5, 10)`. The
overlay froze because it was displaying the last frame from the old target.

No null view warnings appeared — the view exists post-navigation, it's just a
different view object with a different frame sink.

**Why POST but not link clicks:** POST form submissions trigger a renderer
process swap in Chromium (POST navigations cannot safely reuse the previous
renderer). Same-origin link clicks reuse the same renderer process, keeping the
same FrameSinkId.

**Note:** Phase 1 required a fix during implementation. The original design
specified `--enable-logging=stderr`, but Chromium's `InitLogging` in
`shell_main_delegate.cc` ignores `--log-file` when `dest == kStderr`. Changed to
`--enable-logging` (without `=stderr`) so file logging is used and `--log-file`
is respected.

#### Conclusion

The freeze is caused by a stale capturer target. The `FrameSinkVideoCapturer` is
created once in `RenderViewReady` targeting the initial `FrameSinkId`, but POST
form submissions swap the `RenderWidgetHostView` and assign a new `FrameSinkId`.
The capturer continues watching the old frame sink and receives no frames.

The fix is to re-target the capturer in `DidFinishNavigation` when the
`FrameSinkId` changes. This should be Experiment 2.

### Experiment 2: Recreate capturer on page change

#### Goal

After a navigation that swaps the `RenderWidgetHostView` (e.g., POST form
submissions), the capturer re-attaches to the new frame sink and frames continue
flowing.

#### Description

Experiment 1 showed that POST form submissions change both the
`RenderWidgetHostView` and the `FrameSinkId`. The capturer stays pointed at the
old frame sink, so no frames arrive and the overlay freezes.

Two approaches were considered:

- **Approach A: `ChangeTarget()` in `DidFinishNavigation`.** Keep the existing
  capturer alive and call `ChangeTarget()` with the new `FrameSinkId`. Minimal
  code, but risks stale capturer configuration (resolution constraints, format)
  if the new view has different properties (e.g., different device scale
  factor).

- **Approach B: Recreate the capturer in `PrimaryPageChanged`.** Destroy the old
  capturer and create a fresh one from the new view. This is how Electron solves
  the same problem in `FrameSubscriber::PrimaryPageChanged()` (see
  `vendor/electron/shell/browser/api/frame_subscriber.cc:77-83`). Electron
  compares the `RenderWidgetHost` pointer — if it changed, it calls
  `DetachFromHost()` then `AttachToHost()`, which destroys the old capturer and
  creates a new one with full configuration.

We follow Approach B because:

1. It is the battle-tested pattern used by Electron across all navigation types.
2. Recreating the capturer guarantees clean state — no stale resolution
   constraints, format, or target from the previous view.
3. We already have an `Attach()` method that performs the full capturer setup.
   Calling it again after a host swap reuses existing code.
4. `PrimaryPageChanged` fires after commit when the new page is fully
   established, which is the right time to attach.

#### Chromium branch

Continue on `146.0.7650.0-issue-608`.

#### Changes

**`shell_video_consumer.h`** — Add a `PrimaryPageChanged` override and a
`RenderWidgetHost*` member to track the current host:

```cpp
// WebContentsObserver:
void PrimaryPageChanged(Page& page) override;

// ...

RenderWidgetHost* current_host_ = nullptr;
```

Also add the `Page` include:

```cpp
#include "content/public/browser/page.h"
```

**`shell_video_consumer.cc`** — Implement `PrimaryPageChanged`:

```cpp
void ShellVideoConsumer::PrimaryPageChanged(Page& page) {
  RenderWidgetHost* new_host =
      page.GetMainDocument().GetMainFrame()->GetRenderWidgetHost();
  if (new_host == current_host_)
    return;

  LOG(INFO) << "[ShellVideoConsumer] PrimaryPageChanged: host changed, "
            << "re-attaching capturer (pane " << pane_id_ << ")";
  Attach(web_contents());
}
```

**`shell_video_consumer.cc`** — In `Attach()`, store the current host and reset
the old capturer before creating a new one:

```cpp
void ShellVideoConsumer::Attach(WebContents* web_contents) {
  RenderWidgetHostView* view = web_contents->GetRenderWidgetHostView();
  if (!view) { /* ... */ }

  RenderWidgetHost* host = view->GetRenderWidgetHost();
  if (!host) { /* ... */ }

  // Destroy old capturer before creating a new one.
  capturer_.reset();
  current_host_ = host;

  // ... rest of existing Attach() logic unchanged ...
}
```

#### Verification

```bash
cd ~/dev/termsurf/chromium/src && export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH" && autoninja -C out/Default chromium_profile_server
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
```

Test 1 — lite.duckduckgo.com (previously frozen):

```
cargo run -p web -- https://lite.duckduckgo.com
# Click search box, type "test search", click Search button
# Expected: results page renders, overlay updates, input continues
```

Test 2 — Google (previously frozen):

```
cargo run -p web -- https://www.google.com
# Click search box, type "hello", click Search button
# Expected: results page renders, overlay updates, input continues
```

Test 3 — Wikipedia (regression check):

```
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
# Click a link, then use the search box
# Expected: both still work
```

Check `~/dev/termsurf/logs/chromium-server.log` for:

1. `PrimaryPageChanged: host changed` log appears for the frozen cases
2. FPS logs continue after the navigation (no gap)
3. Wikipedia link clicks do NOT trigger re-attach (host unchanged)

**Result:** Pass

All three test cases passed. Lite DuckDuckGo and Google search submissions now
navigate to the results page, the overlay renders the new content, and input
continues normally. Wikipedia link clicks and search still work (no regression).

#### Conclusion

Recreating the capturer in `PrimaryPageChanged` when the `RenderWidgetHost`
changes fixes the search input freeze. The pattern matches Electron's
battle-tested approach and ensures clean capturer state after any navigation
that swaps the renderer process.

## Conclusion

Search form submissions work on all tested sites. The root cause was a stale
`FrameSinkVideoCapturer` target — POST form submissions (and other navigations
that trigger renderer process swaps) change the `RenderWidgetHostView` and
`FrameSinkId`, but the capturer was never re-targeted.

The fix follows Electron's `FrameSubscriber::PrimaryPageChanged()` pattern:
detect when the `RenderWidgetHost` changes after a page commit, destroy the old
capturer, and create a fresh one attached to the new host. This is implemented
in `ShellVideoConsumer::PrimaryPageChanged()` which delegates to the existing
`Attach()` method.
