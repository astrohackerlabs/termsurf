# Issue 413: One Profile to Two Profiles

## Goal

Convert the One Profile app (a Content Shell clone running at 60fps) into a Two
Profiles app that renders two isolated browser profiles side by side in a single
window at 60fps. Each change is a separate experiment. When something breaks, we
fix it before moving on.

## Background

### How we got here

Issues 407–412 explored multiple approaches to rendering two Chromium
`BrowserContext` instances in one window:

- **Issue 407** proved in-process Chromium works: multiple profiles coexist with
  full isolation, and a single WebContents renders at 60fps via Content Shell.
  But placing two WebContents in one window dropped both to 2fps.
- **Issue 408** studied Electron's patches. Three throttling bypass patches
  target `Hide()`, `WasOccluded()`, and `WasHidden()`.
- **Issue 409** tried applying all 147 Electron patches. They can't build
  without Electron's full build infrastructure (Node.js, custom DEPS, etc.).
- **Issue 410** applied the three throttling patches in isolation. The bypass
  calls had no effect — both panes still rendered at 2fps.
- **Issue 410.2** added logging and discovered that `Hide()`, `WasOccluded()`,
  and `WasHidden()` are **never called** on either view. The entire throttling
  hypothesis was targeting the wrong code path.
- **Issue 411** attempted a deferred view attachment fix (wait for
  `RenderFrameCreated` before adding the view to the window). WebContents B
  never appeared, and Shell A was still 2fps — even though Shell A uses the
  exact same `Shell::CreateNewWindow` code path that Content Shell uses for
  60fps.
- **Issue 412** took a step back and cloned Content Shell as "One Profile."
  Confirmed it runs at 60fps. Established a known-good baseline.

### What we know

1. Content Shell runs at 60fps with a single profile.
2. One Profile (a Content Shell clone) runs at 60fps.
3. Two Profiles runs at 2fps — both panes, including Shell A which goes through
   the standard Content Shell lifecycle.
4. The throttling patches (Issues 408–410) are irrelevant — `Hide()`,
   `WasOccluded()`, and `WasHidden()` are never called.
5. The deferred view attachment (Issue 411) is irrelevant — Shell A is 2fps
   despite going through the standard lifecycle.
6. Something about the Two Profiles app's setup degrades Shell A's rendering.
   The candidates are: `TwoProfilesMainParts` class, `SHELL_DIR_USER_DATA`
   override, second `ShellBrowserContext`, second `WebContents`, or view
   hierarchy manipulation.

### The key architectural problem

In Content Shell and One Profile, **Chromium owns the window**. The `Shell`
class creates the NSWindow, manages the toolbar, and places the WebContents view
as the sole occupant of the content area. This works perfectly for one profile.

To render two profiles side by side, we need to **own the window ourselves** so
we can place two WebContents views into it. This is the single biggest
architectural change. Chromium's Shell class assumes one WebContents per window,
and its visibility tracking, compositor lifecycle, and platform delegate all
reflect this assumption.

## Branch

Create a new branch `146.0.7650.0-issue-413` in the `termsurf-chromium`
submodule, starting from the `146.0.7650.0-issue-412` branch (which has the One
Profile app at 60fps). Each experiment is a commit on this branch.

## Approach

Start from One Profile (60fps) and make one change at a time toward Two
Profiles. After each change, build and test. If fps drops, stop and fix before
proceeding. The changes, in order:

### Step 1: Override SHELL_DIR_USER_DATA

Add the `SHELL_DIR_USER_DATA` path override to point the profile at
`~/.config/termsurf/poc/profile-a`. This changes where Chromium stores profile
data but should not affect rendering.

**Expected: 60fps.**

### Step 2: Add second BrowserContext

Create a second `ShellBrowserContext` with a `SHELL_DIR_USER_DATA` override
pointing to `~/.config/termsurf/poc/profile-b`. Hold it but don't use it.

**Expected: 60fps.** If this drops to 2fps, creating a second BrowserContext
alone (possibly through the storage service crash that Issue 411 observed)
degrades Shell A.

### Step 3: Own the window

This is the critical step. Stop letting Chromium's `Shell` class own the window.
Instead, create the NSWindow ourselves in `InitializeMessageLoopContext` and
place Shell A's WebContents view into it. The `Shell` still creates its own
window (we can't easily prevent that), but we reparent the WebContents view into
our window.

This tests whether reparenting a single WebContents view out of its Shell-owned
window and into our own window breaks the compositor lifecycle. If it does, we
need to fix it before we can add a second profile.

**Expected: 60fps.** If this drops to 2fps, the view reparenting itself is the
problem and we need to fix the compositor lifecycle for reparented views.

### Step 4: Add second WebContents (no view attachment)

Create a second `WebContents` with `browser_context_b_` and navigate it to the
test page. Don't add its view to any window.

**Expected: 60fps.** If this drops to 2fps, the mere existence of a navigating
second WebContents degrades Shell A's rendering.

### Step 5: Attach second view side by side

Add WebContents B's view to our window, side by side with WebContents A.

**Expected: Both at 60fps.** If Shell A drops, the view hierarchy manipulation
is the cause. If Shell A stays at 60fps but Shell B is at 2fps, the visibility
race condition from Issue 411 applies to Shell B specifically and we need to fix
it (e.g., by deferring attachment until `RenderFrameCreated`).

## Process

For each step:

1. Modify `content/one_profile/` to match the step's description.
2. Build with `autoninja -C out/Default one_profile`.
3. Run the app and observe fps.
4. Record the result.
5. If fps dropped, investigate and fix before proceeding.
6. Commit each step (and each fix) separately.

## Experiments

### Experiment 1: Override SHELL_DIR_USER_DATA (Step 1)

#### Hypothesis

The `SHELL_DIR_USER_DATA` path override changes where Chromium stores profile
data but should not affect the rendering pipeline. One Profile currently uses
the default macOS path (`~/Library/Application Support/Chromium One Profile`).
Overriding it to `~/.config/termsurf/poc/profile-a` should have no effect on
framerate.

If this drops to 2fps, the path override itself interferes with some subsystem
(e.g., the storage service, the network service, or the path provider's
interaction with utility processes). This would be a significant finding — it
would mean the Two Profiles 2fps problem starts with the very first change.

#### Design

Override `InitializeBrowserContexts()` in `shell_browser_main_parts.cc` to call
`base::PathService::Override(SHELL_DIR_USER_DATA, ...)` before constructing the
`ShellBrowserContext`. The context's constructor calls `InitWhileIOAllowed()`,
which calls `base::PathService::Get(SHELL_DIR_USER_DATA, &path_)` — so the
override must happen before construction.

The profile path is `~/.config/termsurf/poc/profile-a`. Resolve the home
directory via `base::GetHomeDir()`.

The change to `InitializeBrowserContexts()`:

```cpp
void ShellBrowserMainParts::InitializeBrowserContexts() {
  base::FilePath profile_path =
      base::GetHomeDir()
          .Append(".config")
          .Append("termsurf")
          .Append("poc")
          .Append("profile-a");
  base::PathService::Override(SHELL_DIR_USER_DATA, profile_path);

  set_browser_context(new ShellBrowserContext(false));
  set_off_the_record_browser_context(new ShellBrowserContext(true));
  browser_context()->GetOriginTrialsControllerDelegate();
  off_the_record_browser_context()->GetOriginTrialsControllerDelegate();
}
```

Two includes are needed at the top of the file:

```cpp
#include "base/path_service.h"
#include "content/one_profile/common/shell_paths.h"
```

(`base/files/file_path.h` is already present.)

Everything else stays the same. `InitializeMessageLoopContext()` still calls
`Shell::CreateNewWindow(browser_context(), GetStartupURL(), nullptr,
gfx::Size())`
— a single Shell, single profile, standard lifecycle.

#### Branch setup

```bash
cd ts4/termsurf-chromium/src
git checkout -b 146.0.7650.0-issue-413 146.0.7650.0-issue-412
```

This starts from Issue 412's confirmed 60fps baseline.

#### Files to modify

- `content/one_profile/browser/shell_browser_main_parts.cc` — add
  `PathService::Override` call in `InitializeBrowserContexts()`, add two
  includes

#### Build and run

```bash
autoninja -C out/Default one_profile
cd /Users/ryan/dev/termsurf/ts4/box-demo && bun run server.ts &
./out/Default/One\ Profile.app/Contents/MacOS/One\ Profile http://localhost:9407
```

#### What this tests

- Whether overriding `SHELL_DIR_USER_DATA` to a custom path affects rendering
  framerate
- Whether the storage service, network service, and other utility processes
  function correctly with a non-default profile path
- Whether localStorage persists correctly at the new path
  (`~/.config/termsurf/poc/profile-a`)

#### What determines success or failure

- **60fps + localStorage persists at the new path:** The override is harmless.
  Proceed to Experiment 2 (Step 2: add second BrowserContext).
- **2fps:** The path override is the root cause (or a contributing factor).
  Investigate which subsystem breaks — likely the storage service failing to
  initialize at the custom path, falling back to degraded mode. Compare with
  `--user-data-dir=~/.config/termsurf/poc/profile-a` on vanilla Content Shell to
  see if the same override works there.
- **Crash or failure to launch:** The custom path doesn't exist or can't be
  created. Ensure `~/.config/termsurf/poc/profile-a` exists before running, or
  let Chromium create it (it should — `ShellBrowserContext` doesn't require the
  directory to pre-exist).

#### Expected result

60fps. This is a data-path-only change with no effect on the rendering pipeline,
compositor lifecycle, or view hierarchy. The storage service should handle the
custom path identically to the default path.

#### Result: PASSED

60fps. The spinning blue square renders smoothly at full framerate with the
profile data stored at `~/.config/termsurf/poc/profile-a`.

#### Build note

`autoninja` is at `/Users/ryan/depot_tools/autoninja` and is not in the default
shell PATH. When building from a script or automation, prefix with
`PATH="/Users/ryan/depot_tools:$PATH"`. The incremental build compiled only 8
steps (~12 seconds) since the only change was `shell_browser_main_parts.cc`.

#### Conclusion

The `SHELL_DIR_USER_DATA` path override has no effect on rendering. Redirecting
profile storage from the default macOS location to a custom path under
`~/.config/termsurf/poc/` does not degrade framerate, break the storage service,
or interfere with any utility processes. The storage service, network service,
and compositor all function identically with the custom path.

This eliminates the path override as a cause of the 2fps degradation seen in the
Two Profiles app. The next experiment (Step 2) adds a second
`ShellBrowserContext` — the first change that introduces a second profile into
the process.

### Experiment 2: Add second BrowserContext (Step 2)

#### Hypothesis

Creating a second `ShellBrowserContext` with a different storage path should not
affect Shell A's rendering. The `BrowserContext` is a data container — it holds
cookies, localStorage, and cache configuration. It does not interact with the
compositor, the view hierarchy, or the rendering pipeline.

If this drops to 2fps, the second `BrowserContext` itself degrades Shell A. The
most likely mechanism is the storage service: `ShellBrowserContext`'s
constructor calls `CreateBrowserContextServices(this)`, which may trigger the
storage service to initialize for the new context. Issue 411 observed a storage
service crash when two profiles coexisted — the service couldn't make
profile-b's paths relative to profile-a's root. If the crash or error handling
blocks or degrades the storage service for profile-a too, that could explain the
systemic 2fps.

#### Design

Add a `browser_context_b_` member to `ShellBrowserMainParts`. In
`InitializeBrowserContexts()`, after creating the profile-a context, override
`SHELL_DIR_USER_DATA` to profile-b and create a second `ShellBrowserContext`.
Don't use it for anything — no `WebContents`, no navigation. Clean it up in
`PostMainMessageLoopRun()`.

The change to the header (`shell_browser_main_parts.h`), add one member:

```cpp
private:
 std::unique_ptr<ShellBrowserContext> browser_context_;
 std::unique_ptr<ShellBrowserContext> off_the_record_browser_context_;
 std::unique_ptr<ShellBrowserContext> browser_context_b_;
```

The change to `InitializeBrowserContexts()`:

```cpp
void ShellBrowserMainParts::InitializeBrowserContexts() {
  base::FilePath profile_a_path =
      base::GetHomeDir()
          .Append(".config")
          .Append("termsurf")
          .Append("poc")
          .Append("profile-a");
  base::PathService::Override(SHELL_DIR_USER_DATA, profile_a_path);

  set_browser_context(new ShellBrowserContext(false));
  set_off_the_record_browser_context(new ShellBrowserContext(true));
  browser_context()->GetOriginTrialsControllerDelegate();
  off_the_record_browser_context()->GetOriginTrialsControllerDelegate();

  base::FilePath profile_b_path =
      base::GetHomeDir()
          .Append(".config")
          .Append("termsurf")
          .Append("poc")
          .Append("profile-b");
  base::PathService::Override(SHELL_DIR_USER_DATA, profile_b_path);

  browser_context_b_ = std::make_unique<ShellBrowserContext>(false);
}
```

The change to `PostMainMessageLoopRun()`, add cleanup before the existing
context resets:

```cpp
browser_context_b_.reset();
browser_context_.reset();
off_the_record_browser_context_.reset();
```

Note: after both contexts are created, the global `SHELL_DIR_USER_DATA` is left
pointing at profile-b. This is the same state the old Two Profiles app was in.
If this causes the storage service to resolve paths against profile-b instead of
profile-a for Shell A's context, that's a finding — but it should not affect
rendering since the storage path is read once during `InitWhileIOAllowed()` and
cached in `ShellBrowserContext::path_`.

#### Files to modify

- `content/one_profile/browser/shell_browser_main_parts.h` — add
  `browser_context_b_` member
- `content/one_profile/browser/shell_browser_main_parts.cc` — expand
  `InitializeBrowserContexts()` with profile-b context creation, add cleanup in
  `PostMainMessageLoopRun()`

#### Build and run

```bash
autoninja -C out/Default one_profile
cd /Users/ryan/dev/termsurf/ts4/box-demo && bun run server.ts &
./out/Default/One\ Profile.app/Contents/MacOS/One\ Profile http://localhost:9407
```

#### What this tests

- Whether creating a second `ShellBrowserContext` (unused) degrades Shell A's
  framerate
- Whether the storage service can handle two `BrowserContext` instances with
  different storage paths without crashing or degrading
- Whether `CreateBrowserContextServices()` for the second context has any side
  effects on the first

#### What determines success or failure

- **60fps:** The second `BrowserContext` is harmless. Proceed to Experiment 3
  (Step 3: own the window).
- **2fps:** The second `BrowserContext` is the culprit. Investigate the storage
  service — check for crashes in the console output (`[ERROR:storage_...]`),
  check whether
  `browser_context_b_ = std::make_unique<ShellBrowserContext>(false, /*delay_services_creation=*/true)`
  (delaying service creation) restores 60fps. If it does,
  `CreateBrowserContextServices` is the trigger.
- **Crash:** Likely the storage service crash from Issue 411. Check whether the
  second `PathService::Override` leaves the storage service in a broken state
  for profile-a.

#### Expected result

60fps. The second `BrowserContext` is a data object with no rendering side
effects. But this is the first experiment that introduces a second profile into
the process, which is exactly what distinguishes the Two Profiles app from
Content Shell — so a failure here would be a major finding.

#### Result: PASSED

60fps. Shell A renders the spinning blue square at full framerate with a second
`ShellBrowserContext` (profile-b) created and held in memory.

#### Conclusion

A second `BrowserContext` with a different storage path has no effect on Shell
A's rendering. The storage service handles two `BrowserContext` instances
without crashing or degrading. `CreateBrowserContextServices()` for the second
context has no observable side effects on the first. The global
`SHELL_DIR_USER_DATA` being left pointing at profile-b after initialization does
not matter — each context cached its path during construction.

This eliminates the second `BrowserContext` as the cause of the 2fps
degradation. Two suspects remain: the window ownership change (Step 3) and the
second `WebContents` (Steps 4–5). The next experiment is the critical one — Step
3 takes window ownership away from Chromium's `Shell` class and into our own
NSWindow, which is the fundamental architectural change needed for side-by-side
rendering.

### Experiment 3: Own the window (Step 3)

#### Hypothesis

In Content Shell and One Profile, Chromium's `Shell` class creates the NSWindow,
manages the toolbar, and places the WebContents view as the sole occupant. To
render two profiles side by side, we need to own the window ourselves.

Reparenting a single WebContents view from Shell's NSWindow into a custom
NSWindow should not break rendering. When the NSView moves between windows,
macOS fires `viewDidMoveToWindow`, which triggers Chromium's visibility chain
(`UpdateWebContentsVisibility` → `WasShown` → `ShowWithVisibility`). If the
renderer and `RenderWidgetHostView` already exist at the time of the reparent
(which they do — Shell creates the renderer before we reparent), the
`BrowserCompositorMac` should transition to `HasOwnCompositor` in the new window
and continue producing frames.

If this drops to 2fps, the reparenting itself is the problem — the compositor
lifecycle doesn't survive moving between windows. This would explain why the
original Two Profiles app was at 2fps: it wasn't the second profile or the
second WebContents, it was the act of placing a WebContents view into a window
that Shell didn't create.

#### Design

Let `Shell::CreateNewWindow()` do its normal thing (create window, add
WebContents view, start navigation). Then immediately after, create our own
NSWindow, reparent the WebContents' NSView into it, and hide Shell's original
window.

The reparenting logic requires Objective-C (NSWindow, NSView). Since
`shell_browser_main_parts.cc` is compiled as C++, the macOS-specific code goes
in `shell_browser_main_parts_mac.mm` as a free function, forward-declared in the
.cc file.

**In `shell_browser_main_parts.cc`**, change `InitializeMessageLoopContext()`:

```cpp
#if BUILDFLAG(IS_MAC)
namespace content {
void ReparentToCustomWindow(Shell* shell);
}
#endif

void ShellBrowserMainParts::InitializeMessageLoopContext() {
  Shell* shell = Shell::CreateNewWindow(browser_context_.get(), GetStartupURL(),
                                        nullptr, gfx::Size());
#if BUILDFLAG(IS_MAC)
  ReparentToCustomWindow(shell);
#endif
}
```

**In `shell_browser_main_parts_mac.mm`**, add the reparenting function:

```cpp
#include "content/one_profile/browser/shell.h"
#include "content/public/browser/web_contents.h"

namespace content {

static NSWindow* g_custom_window = nil;

void ReparentToCustomWindow(Shell* shell) {
  // Create our own NSWindow
  NSRect frame = NSMakeRect(200, 200, 800, 600);
  NSUInteger style = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                     NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable;
  g_custom_window = [[NSWindow alloc] initWithContentRect:frame
                                                styleMask:style
                                                  backing:NSBackingStoreBuffered
                                                    defer:NO];
  g_custom_window.title = @"One Profile (Custom Window)";
  g_custom_window.releasedWhenClosed = NO;

  // Get the WebContents' native NSView
  NSView* web_view =
      shell->web_contents()->GetNativeView().GetNativeNSView();

  // Remove from Shell's window
  [web_view removeFromSuperview];

  // Add to our window
  [g_custom_window.contentView addSubview:web_view];
  web_view.frame = g_custom_window.contentView.bounds;
  web_view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;

  // Hide Shell's original window (orderOut doesn't trigger windowShouldClose)
  NSWindow* shell_window = shell->window().GetNativeNSWindow();
  [shell_window orderOut:nil];

  // Show our window
  [g_custom_window makeKeyAndOrderFront:nil];
}

}  // namespace content
```

Key details:

- `g_custom_window` is a file-static strong reference to prevent deallocation.
- `orderOut:nil` hides Shell's window without triggering the
  `OneProfileWindowDelegate`'s `windowShouldClose:` (which would call
  `Shell::ClearAndDelete()` and destroy the WebContents). The Shell stays alive.
- The WebContents view is removed from Shell's `contentView` and added to ours.
  This triggers `viewDidMoveToWindow` on the NSView, which Chromium uses to
  drive compositor visibility.
- `Shell::CreateNewWindow` returns `Shell*`. The current code discards it; we
  capture it.

#### Files to modify

- `content/one_profile/browser/shell_browser_main_parts.cc` — capture `Shell*`
  return value, call `ReparentToCustomWindow(shell)` behind
  `#if BUILDFLAG(IS_MAC)`, add forward declaration
- `content/one_profile/browser/shell_browser_main_parts_mac.mm` — implement
  `ReparentToCustomWindow`, add includes for Shell and WebContents

No header changes needed (forward declaration is local to the .cc file). No
BUILD.gn changes needed (both files are already in the build).

#### Build and run

```bash
autoninja -C out/Default one_profile
cd /Users/ryan/dev/termsurf/ts4/box-demo && bun run server.ts &
./out/Default/One\ Profile.app/Contents/MacOS/One\ Profile http://localhost:9407
```

#### What this tests

- Whether reparenting a WebContents NSView from Shell's NSWindow to a custom
  NSWindow breaks the compositor lifecycle
- Whether `viewDidMoveToWindow` fires correctly and the `BrowserCompositorMac`
  transitions to `HasOwnCompositor` in the new window
- Whether the renderer continues producing frames at 60fps in a non-Shell window

#### What determines success or failure

- **60fps in the custom window:** Reparenting works. The compositor survives the
  window change. Proceed to Experiment 4 (Step 4: add second WebContents).
- **2fps in the custom window:** The reparent breaks the compositor lifecycle.
  The `BrowserCompositorMac` either stays in `HasNoCompositor` or fails to
  re-attach to the new window's display link. This would explain the Two
  Profiles 2fps and point to a fix: ensure the compositor is properly restarted
  after reparenting (e.g., by calling `WasShown` or `ShowWithVisibility` after
  the view is in the new window).
- **Blank window / no rendering:** The web view moved but the compositor didn't
  follow. Check if the view is visible (`isHiddenOrHasHiddenAncestor`), if the
  window is on screen, and if the compositor state is `HasNoCompositor`.
- **Crash:** Likely a use-after-free if Shell's cleanup runs unexpectedly, or a
  compositor assertion. Check the crash log for the specific failure.

#### Expected result

60fps. The reparenting should trigger `viewDidMoveToWindow`, which re-runs the
visibility chain with the renderer already present. The `BrowserCompositorMac`
should transition correctly. But this is the most likely experiment to fail —
it's the first change that breaks the assumption that Shell owns the window.

#### Result: PASSED

60fps. The spinning blue square renders at full framerate in the custom window
titled "One Profile (Custom Window)". Shell's original window is hidden and the
reparented WebContents view renders correctly.

#### Conclusion

Reparenting a WebContents NSView from Shell's NSWindow into a custom NSWindow
does not break the compositor lifecycle. The `BrowserCompositorMac` survives the
window change — `viewDidMoveToWindow` fires, the visibility chain runs with the
renderer already present, and the compositor transitions to `HasOwnCompositor`
in the new window. Frames flow at 60fps.

This is a major finding. It means TermSurf can own its own window and place
Chromium WebContents views into it without any rendering penalty. The
`Shell::CreateNewWindow` lifecycle creates the renderer and compositor
correctly, and the subsequent reparent is transparent to the rendering pipeline.

Three of five steps now pass at 60fps:

1. Path override — harmless
2. Second BrowserContext — harmless
3. Custom window with reparented view — harmless

The remaining suspects are the second WebContents itself (Step 4) and attaching
both views side by side (Step 5). If Step 4 passes, the root cause is isolated
to the side-by-side view attachment — which is where the original Two Profiles
app's visibility race condition (Issue 411) would apply, but only to the second
view.

### Experiment 4: Add second WebContents, no view attachment (Step 4)

#### Hypothesis

Creating a second `WebContents` with `browser_context_b_` and navigating it to
the test page should not affect Shell A's rendering. The second WebContents has
no view attached to any window — it exists in memory with a live renderer
process, but its `WebContentsViewCocoa` is never added to the view hierarchy.

If this drops to 2fps, the mere act of creating and navigating a second
WebContents degrades Shell A. This would point to renderer process contention,
GPU process resource sharing, or compositor interference between two active
WebContents in the same process. This would be a Chromium-level issue rather
than a view hierarchy issue.

#### Design

Create a bare `WebContents` (not wrapped in a `Shell`) using
`browser_context_b_` and navigate it to the test page URL. Store it as a member,
clean it up on shutdown. The second WebContents' view is never added to any
window.

The change to the header (`shell_browser_main_parts.h`), add a forward
declaration and member:

```cpp
namespace content {
class WebContents;
// ...
class ShellBrowserMainParts : public BrowserMainParts {
  // ...
 private:
  // ...
  std::unique_ptr<ShellBrowserContext> browser_context_b_;
  std::unique_ptr<WebContents> web_contents_b_;
```

The change to `InitializeMessageLoopContext()`:

```cpp
void ShellBrowserMainParts::InitializeMessageLoopContext() {
  Shell* shell = Shell::CreateNewWindow(browser_context_.get(), GetStartupURL(),
                                        nullptr, gfx::Size());
#if BUILDFLAG(IS_MAC)
  ReparentToCustomWindow(shell);
#endif

  // Create second WebContents with profile B, navigate but don't attach view.
  WebContents::CreateParams params(browser_context_b_.get());
  web_contents_b_ = WebContents::Create(params);
  NavigationController::LoadURLParams load_params(GetStartupURL());
  load_params.transition_type = ui::PageTransitionFromInt(
      ui::PAGE_TRANSITION_TYPED | ui::PAGE_TRANSITION_FROM_ADDRESS_BAR);
  web_contents_b_->GetController().LoadURLWithParams(load_params);
}
```

The change to `PostMainMessageLoopRun()`, add cleanup before context resets:

```cpp
web_contents_b_.reset();
browser_context_b_.reset();
browser_context_.reset();
off_the_record_browser_context_.reset();
```

Additional includes needed in `shell_browser_main_parts.cc`:

```cpp
#include "content/public/browser/navigation_controller.h"
#include "content/public/browser/web_contents.h"
#include "ui/base/page_transition_types.h"
```

Note: `GetStartupURL()` is a file-local function already defined in the .cc
file. Both WebContents load the same URL (the box-demo test page).

#### Files to modify

- `content/one_profile/browser/shell_browser_main_parts.h` — forward-declare
  `WebContents`, add `web_contents_b_` member
- `content/one_profile/browser/shell_browser_main_parts.cc` — create and
  navigate second WebContents in `InitializeMessageLoopContext()`, add cleanup
  in `PostMainMessageLoopRun()`, add includes

#### Build and run

```bash
autoninja -C out/Default one_profile
cd /Users/ryan/dev/termsurf/ts4/box-demo && bun run server.ts &
./out/Default/One\ Profile.app/Contents/MacOS/One\ Profile http://localhost:9407
```

#### What this tests

- Whether a second navigating WebContents (with a live renderer) degrades Shell
  A's framerate even when the second view is not attached to any window
- Whether renderer process contention or GPU process resource sharing between
  two active WebContents causes degradation
- Whether the storage service handles two active WebContents from two different
  BrowserContexts without crashing

#### What determines success or failure

- **60fps:** The second WebContents is harmless when not attached. Proceed to
  Experiment 5 (Step 5: attach both views side by side). This would isolate the
  root cause to the view attachment step itself.
- **2fps:** The second WebContents degrades Shell A even without a view. This
  points to process-level interference — renderer contention, GPU resource
  sharing, or compositor crosstalk. Investigate by checking if the second
  WebContents' renderer is consuming excessive resources (Activity Monitor), or
  try creating the WebContents without navigating it to see if navigation is the
  trigger.
- **Crash:** Likely the storage service crash from Issue 411 (profile-b paths
  can't be made relative to profile-a's root). Check console output for storage
  service errors.

#### Expected result

60fps. A WebContents without a view shouldn't participate in the rendering
pipeline — no compositor, no BeginFrame signals, no display link subscription.
The renderer process exists but has nothing to paint to.

#### Result: FAILED

2.3fps. Shell A dropped from 60fps to ~4fps initially, then settled at 2.3fps.
The custom window appeared but the spinning blue square was nearly frozen.

#### Conclusion

**The root cause is found.** Creating and navigating a second `WebContents` —
even with its view never attached to any window — degrades Shell A from 60fps
to 2fps. This is not a view hierarchy issue, not a compositor lifecycle issue,
not a visibility race condition. It is a process-level interference caused by
the mere existence of a second navigating WebContents.

This explains every previous observation:

- **Issue 407:** Two Profiles at 2fps — both panes, including Shell A which used
  the standard `Shell::CreateNewWindow` lifecycle. The second WebContents was the
  cause, not the view attachment.
- **Issues 408–410:** Throttling patches had no effect because `Hide()`,
  `WasOccluded()`, and `WasHidden()` were never called. The throttling was never
  the problem — the second WebContents was.
- **Issue 411:** Deferred view attachment didn't help because the degradation
  happens before any view is attached. Shell A was at 2fps from the moment the
  second WebContents was created and navigated.
- **Issue 412/413 Experiments 1–3:** Path override, second BrowserContext, and
  window reparenting all passed at 60fps because none of them created a second
  navigating WebContents.

The degradation is not caused by the second `BrowserContext` (Experiment 2
passed), not by the `WebContents::Create` call alone (untested yet), but by
something in the navigation or renderer process lifecycle. The second
WebContents spawns a renderer process that competes for GPU process resources,
compositor scheduling, or display link callbacks — and this contention drags
Shell A down to 2fps.

### What's next

The root cause is isolated to Step 4: creating and navigating a second
WebContents. The next experiments should decompose this further:

1. **Create WebContents without navigating.** Does `WebContents::Create(params)`
   alone (no `LoadURLWithParams`) cause the drop? If not, the navigation or
   renderer process creation is the trigger.
2. **Navigate to about:blank instead of the test page.** Does a trivial page
   still cause the drop? If not, the test page's `requestAnimationFrame` loop
   may be competing for GPU time.
3. **Check GPU process utilization.** Is the GPU process at 100%? Are both
   renderers sharing a single GPU thread? Is the compositor scheduler giving
   equal time to both and starving Shell A?
4. **Check if this is a content_shell-specific limitation.** Run Chrome or
   Electron with two tabs — they handle multiple WebContents at 60fps. What do
   they do differently? Likely a proper compositor scheduler configuration or
   GPU process thread pool that content_shell lacks.
