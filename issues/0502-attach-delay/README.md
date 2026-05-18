+++
status = "closed"
opened = "2026-02-15"
closed = "2026-02-15"
+++

# Issue 502: Eliminate Hardcoded 2-Second Capturer Attach Delay

## Problem

The Chromium Profile Server uses a hardcoded 2-second `PostDelayedTask` to
attach the `FrameSinkVideoCapturer` to the `WebContents`:

```cpp
// shell_browser_main_parts.cc:152-181
void ShellBrowserMainParts::InitializeMessageLoopContext() {
  Shell* shell = Shell::CreateNewWindow(browser_context_.get(), GetStartupURL(),
                                        nullptr, gfx::Size());

  video_consumer_ = std::make_unique<ShellVideoConsumer>();
  // ... XPC setup ...

  base::SingleThreadTaskRunner::GetCurrentDefault()->PostDelayedTask(
      FROM_HERE,
      base::BindOnce(
          [](ShellVideoConsumer* consumer, WebContents* web_contents) {
            consumer->Attach(web_contents);
          },
          video_consumer_.get(), shell->web_contents()),
      base::Seconds(2));
}
```

The delay exists because `Shell::CreateNewWindow()` starts a navigation, but the
`RenderWidgetHostView` (RWHV) — which owns the compositor surface — doesn't
exist yet. The capturer needs the RWHV's `FrameSinkId` to know which surface to
capture. The 2-second delay is a crude workaround: just wait long enough that
the RWHV is guaranteed to exist.

### Why this is bad

1. **Wastes ~2 seconds of frames.** The page is already rendering during those 2
   seconds, but nothing is captured or sent via XPC. The receiver sees nothing
   for 2 seconds after launch.
2. **Fragile.** On a slow machine or under heavy load, 2 seconds might not be
   enough. On a fast machine, it wastes 1.99 seconds.
3. **Unnecessarily slow.** The RWHV is typically ready within milliseconds, not
   seconds.

## Electron's Approach

Electron solves this problem in `vendor/electron/shell/browser/osr/`. It
provides a custom `RenderWidgetHostView` subclass
(`OffScreenRenderWidgetHostView`) and a custom `WebContentsView` subclass
(`OffScreenWebContentsView`). This gives Electron total control over the RWHV
lifecycle:

1. **`OffScreenWebContentsView::CreateViewForWidget()`** — When Chromium asks
   for a view, Electron returns its own `OffScreenRenderWidgetHostView`. This is
   the factory hook.

2. **RWHV constructor creates the capturer immediately**
   (`osr_render_widget_host_view.cc:225-231`):
   ```cpp
   video_consumer_ = std::make_unique<OffScreenVideoConsumer>(
       this, base::BindRepeating(&OffScreenRenderWidgetHostView::OnPaint,
                                 weak_ptr_factory_.GetWeakPtr()));
   video_consumer_->SetActive(is_painting());
   ```

3. **`OffScreenVideoConsumer` calls `view_->CreateVideoCapturer()` in its
   constructor** (`osr_video_consumer.cc:42`), which sets up the capture
   pipeline via `HostFrameSinkManager`. This is safe because it just establishes
   plumbing — no frames flow yet.

4. **`ShowWithVisibility()` is the readiness signal**
   (`osr_render_widget_host_view.cc:319-339`). Called by `WebContentsImpl` when
   the tab becomes visible, it attaches the `DelegatedFrameHost` to the
   compositor and tells the renderer to start producing frames. Everything is
   already wired up, so frames flow immediately.

No delay. No timer. No race condition. The capturer is born alongside the view.

### Why we don't need the full Electron approach

Electron's custom RWHV is ~1500 lines because Electron replaces the entire
compositor pipeline — it does its own off-screen rendering. We don't do that. We
use Chromium's default RWHV and default rendering pipeline, then capture from
the compositor externally via `FrameSinkVideoCapturer`.

We just need to know **when** the default RWHV is ready so we can call
`Attach()`. We don't need to own the RWHV — we just need to observe its
creation.

## Proposed Solution

Use a `WebContentsObserver` to detect when the RWHV is ready, then attach
immediately. Chromium's `content::WebContentsObserver` provides the
`RenderViewReady()` callback, which fires when the `blink::WebView` is ready. By
this point, the RWHV and its `FrameSinkId` exist.

The change is small:

1. **Make `ShellVideoConsumer` inherit from `WebContentsObserver`** (or create a
   small helper class that does).

2. **Override `RenderViewReady()`** to call `Attach()`.

3. **Delete the `PostDelayedTask` block** from
   `ShellBrowserMainParts::InitializeMessageLoopContext()`.

4. **Start observing the `WebContents`** right after `Shell::CreateNewWindow()`
   returns, instead of scheduling a delayed task.

This replaces the 2-second timer with an event-driven hook. The capturer
attaches as soon as the RWHV exists — no wasted frames, no arbitrary timeout, no
race condition.

### Estimated scope

~20-30 lines changed across 2-3 files (`shell_video_consumer.h`,
`shell_video_consumer.cc`, `shell_browser_main_parts.cc`).

## Experiments

### Experiment 1: WebContentsObserver with RenderViewReady

#### Goal

Replace the 2-second `PostDelayedTask` with an event-driven
`WebContentsObserver` that calls `Attach()` when `RenderViewReady()` fires.

#### Branch

`146.0.7650.0-issue-502` (off `146.0.7650.0-issue-501`)

#### Changes

##### `shell_video_consumer.h`

Add `WebContentsObserver` as a second base class alongside
`viz::mojom::FrameSinkVideoConsumer`:

```cpp
#include "content/public/browser/web_contents_observer.h"

class ShellVideoConsumer : public viz::mojom::FrameSinkVideoConsumer,
                           public WebContentsObserver {
 public:
  ShellVideoConsumer();
  ~ShellVideoConsumer() override;

  // Begin observing a WebContents. When RenderViewReady() fires,
  // Attach() is called automatically.
  void ObserveContents(WebContents* web_contents);

  // WebContentsObserver:
  void RenderViewReady() override;

  // ... rest unchanged ...
```

The existing `Attach(WebContents*)` method stays as-is — it still does the
actual capturer setup. `ObserveContents()` is the new entry point that replaces
the delayed `Attach()` call.

##### `shell_video_consumer.cc`

Add the two new methods:

```cpp
void ShellVideoConsumer::ObserveContents(WebContents* web_contents) {
  Observe(web_contents);  // WebContentsObserver::Observe()
}

void ShellVideoConsumer::RenderViewReady() {
  Attach(web_contents());  // WebContentsObserver::web_contents()
}
```

`Observe()` is the protected `WebContentsObserver` method that starts
observation. `web_contents()` is the accessor that returns the observed
`WebContents*`.

##### `shell_browser_main_parts.cc`

Replace the `PostDelayedTask` block with a single `ObserveContents()` call:

```cpp
void ShellBrowserMainParts::InitializeMessageLoopContext() {
  Shell* shell = Shell::CreateNewWindow(browser_context_.get(), GetStartupURL(),
                                        nullptr, gfx::Size());

  video_consumer_ = std::make_unique<ShellVideoConsumer>();

#if BUILDFLAG(IS_MAC)
  base::CommandLine* cmd = base::CommandLine::ForCurrentProcess();
  if (cmd->HasSwitch(switches::kXpcService)) {
    video_consumer_->ConnectToService(
        cmd->GetSwitchValueASCII(switches::kXpcService));
  }
  if (cmd->HasSwitch(switches::kSessionId)) {
    video_consumer_->SetSessionId(
        cmd->GetSwitchValueASCII(switches::kSessionId));
  }
#endif

  // Observe the WebContents and attach the capturer when the
  // RenderWidgetHostView is ready (via RenderViewReady callback).
  video_consumer_->ObserveContents(shell->web_contents());
}
```

The `PostDelayedTask` / `base::Seconds(2)` block is deleted entirely.

#### Pass Criteria

1. Builds with zero errors.
2. Both panes render at ~60fps (same as before).
3. First frame arrives in under 1 second (not 2+ seconds).
4. No Dock icon (LSUIElement still in effect from Issue 501).

#### Test Command

```bash
cd chromium/src
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>/tmp/cps-502-test.log
```

#### Result: Pass

Build: 21 targets, zero errors.

Timing comparison:

| Metric           | Before (Issue 501) | After (Issue 502) |
| ---------------- | ------------------ | ----------------- |
| Launch timestamp | `060800.723`       | `073448.144`      |
| Attach timestamp | `060802.798`       | `073448.826`      |
| **Delay**        | **2.07s**          | **0.68s**         |

The `RenderViewReady()` callback fired 0.68 seconds after launch — as soon as
the RWHV was actually ready. The capturer attached immediately and began
delivering frames at 60fps:

```
[ShellVideoConsumer] Attached to FrameSinkId FrameSinkId(5, 3), starting capture
[ShellVideoConsumer] 47 frames in 1.01565s (46.2756 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 60 frames in 1.0003s (59.9821 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.0166s (60.0039 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01656s (60.006 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01671s (59.9973 fps) | IOSurface 1600x1200
```

No Dock icon. No timer. No race condition.

#### Conclusion

The `WebContentsObserver` approach works exactly as designed. Three small
changes across three files replaced a fragile 2-second `PostDelayedTask` with an
event-driven `RenderViewReady()` callback. The capturer now attaches ~1.4
seconds earlier — as soon as the RWHV exists rather than after an arbitrary
timeout. The 0.68-second remaining delay is genuine startup time (process init,
renderer spawn, navigation start), not wasted waiting.

## Conclusion

Issue 502 replaced a hardcoded 2-second `PostDelayedTask` with a
`WebContentsObserver` that attaches the `FrameSinkVideoCapturer` the instant the
`RenderWidgetHostView` is ready. One experiment, first-try pass.

The fix was 10 net lines across 3 files: add `WebContentsObserver` as a base
class on `ShellVideoConsumer`, override `RenderViewReady()` to call `Attach()`,
and replace the timer in `shell_browser_main_parts.cc` with `ObserveContents()`.
Capturer attachment dropped from 2.07 seconds to 0.68 seconds — a 67% reduction,
with the remaining time being genuine Chromium startup (process init, renderer
spawn, navigation).

The key insight came from Electron. Electron's `OffScreenRenderWidgetHostView`
avoids the timing problem entirely by owning the RWHV and creating the capturer
in its constructor. We don't need that level of control — we use Chromium's
default rendering pipeline — but studying Electron's approach revealed that the
right answer is always event-driven observation, never arbitrary delays.
Chromium's `WebContentsObserver` provides exactly the hook we needed.
