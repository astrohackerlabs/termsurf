+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

# Issue 639: Open New-Tab Links in Same Tab

## Goal

Intercept links that request a new tab or window (`target="_blank"`,
`window.open()`, middle-click, Cmd+click) and open them in the current tab
instead. This makes these links functional now while deferring true multi-tab
support.

## Background

The Chromium Profile Server's `Shell` class inherits its `WebContentsDelegate`
implementation from `content_shell`. When a link requests a new tab or window,
two delegate methods handle it:

- **`OpenURLFromTab`** — Called for navigations with a non-`CURRENT_TAB`
  disposition (e.g. `target="_blank"` links, Cmd+click). Currently creates a new
  `Shell` window via `Shell::CreateNewWindow`.
- **`AddNewContents`** — Called when Chromium has already created a new
  `WebContents` (e.g. `window.open()` popups). Currently creates a new `Shell`
  to host it via `CreateShell`.

Both methods create standalone windows that TermSurf doesn't manage — they float
outside the terminal, have no XPC connection, no pane ID, and no way to stream
back to the TUI. The links "work" in the sense that Chromium opens them, but the
user never sees the result.

The fix is simple: instead of creating a new window, navigate the current tab to
the target URL.

## Current state

- **`Shell::OpenURLFromTab`** (`shell.cc:427`): For `NEW_FOREGROUND_TAB`,
  `NEW_BACKGROUND_TAB`, `NEW_POPUP`, and `NEW_WINDOW` dispositions, calls
  `Shell::CreateNewWindow` and navigates the new shell.
- **`Shell::AddNewContents`** (`shell.cc:311`): Creates a new `Shell` for the
  incoming `WebContents` via `CreateShell`.

## Experiment 1: Redirect to current tab

### Hypothesis

Modifying `OpenURLFromTab` to navigate the source tab (instead of creating a new
window) for all new-tab/new-window dispositions, and modifying `AddNewContents`
to navigate the source tab to the target URL (discarding the pre-created
`WebContents`), will make all "open in new tab" links work in the current tab.

### Changes

#### 1. `Shell::OpenURLFromTab` (`shell.cc`)

Change the `NEW_POPUP` / `NEW_WINDOW` / `NEW_BACKGROUND_TAB` /
`NEW_FOREGROUND_TAB` cases to navigate the source tab instead of creating a new
window:

```cpp
case WindowOpenDisposition::NEW_POPUP:
case WindowOpenDisposition::NEW_WINDOW:
case WindowOpenDisposition::NEW_BACKGROUND_TAB:
case WindowOpenDisposition::NEW_FOREGROUND_TAB:
  // Issue 639: Open in current tab instead of creating a new window.
  // True multi-tab support deferred.
  target = source;
  break;
```

#### 2. `Shell::AddNewContents` (`shell.cc`)

Navigate the source to the target URL and discard the pre-created `WebContents`.
The `new_contents` unique_ptr will be destroyed when it goes out of scope:

```cpp
WebContents* Shell::AddNewContents(
    WebContents* source,
    std::unique_ptr<WebContents> new_contents,
    const GURL& target_url,
    WindowOpenDisposition disposition,
    const blink::mojom::WindowFeatures& window_features,
    bool user_gesture,
    bool* was_blocked) {
  // Issue 639: Instead of creating a new window, navigate the source tab
  // to the target URL. The pre-created WebContents is discarded.
  if (source && target_url.is_valid()) {
    NavigationController::LoadURLParams params(target_url);
    params.transition_type = ui::PAGE_TRANSITION_LINK;
    source->GetController().LoadURLWithParams(params);
    return source;
  }
  return nullptr;
}
```

Required includes (check if already present):

```cpp
#include "content/public/browser/navigation_controller.h"
#include "ui/base/page_transition_types.h"
```

### Verification

1. Build Chromium (`autoninja -C out/Default chromium_profile_server`)
2. Launch TermSurf, `web google.com`
3. Search for something, click a result — opens in same tab
4. Find a `target="_blank"` link (e.g. footer links on many sites) — opens in
   same tab instead of a new window
5. Test `window.open()` via DevTools or a test page — opens in same tab
6. No stray Chromium windows should appear

### Success criteria

- `target="_blank"` links navigate the current tab
- `window.open()` navigates the current tab
- No new Chromium windows are created
- Back/forward navigation still works after redirected navigations
- Page title and URL bar update correctly after redirected navigations

### Result: Failure

The TUI became completely unresponsive after clicking a `target="_blank"` link.
The old page disappeared, but the new page never appeared, and all keybindings
stopped working, and the app had to be force-closed. The naive approach of
redirecting `OpenURLFromTab` and discarding the pre-created `WebContents` in
`AddNewContents` likely caused Chromium internal state corruption or a deadlock.
More research into how Chromium and Electron handle this is needed before the
next experiment.

## Experiment 2: Research new-tab interception patterns

### Hypothesis

Understanding how Chromium's new-tab lifecycle works internally, how Electron
intercepts it, and how our own app's CALayerHost pipeline reacts to WebContents
changes will reveal the correct interception point.

### Research questions

#### 1. Chromium internals

- What is the full call chain when a `target="_blank"` link is clicked? Which
  methods fire in what order (`OpenURLFromTab`, `AddNewContents`,
  `CreateNewWindow`, `WebContentsCreated`, etc.)?
- What happens to the original `WebContents` when a new one is created? Does
  Chromium expect the caller to adopt the new `WebContents`, and what breaks if
  it's discarded?
- Is there a delegate method that fires _before_ the new `WebContents` is
  created (e.g. `IsWebContentsCreationOverridden`) that could suppress creation
  entirely?
- What is `ShouldAllowRendererInitiatedCrossProcessNavigation`? Is it relevant?

#### 2. Electron

- How does Electron handle `window.open()` and `target="_blank"`? Look at
  Electron's `WebContentsDelegate` overrides in `vendor/electron/`.
- Does Electron suppress new-window creation, redirect it, or intercept before
  creation?
- Does Electron use `IsWebContentsCreationOverridden`, `SetAutoResizeMode`,
  `did-create-window`, or a different mechanism?
- What events does Electron emit (`new-window`, `will-navigate`,
  `did-create-window`) and where are they triggered from?

#### 3. Our app (TermSurf)

- When `Shell::CreateNewWindow` is called, what happens to the CALayerHost
  pipeline? Does the new Shell get a new `CAContext` / `CAContextID`?
- When the original Shell's `WebContents` navigates, what triggers the
  CALayerHost update? Is the issue that navigation to a new `WebContents`
  detaches the `RenderWidgetHostView` from the original `CAContext`?
- In the Experiment 1 failure, did the old page disappearing suggest the
  original `WebContents` was destroyed or detached? Or did the navigation start
  but the compositor never received the new frame?
- Look at `ShellTabObserver::RenderViewHostChanged` — does it fire during this
  scenario? Is it possible the observer lost its connection?

### Deliverable

A written summary of findings for each section above, with specific file paths,
method names, and line numbers. The summary should conclude with a recommended
approach for Experiment 3.

### Success criteria

- All three research areas answered with code references
- Root cause of Experiment 1 failure identified or narrowed down
- Clear recommendation for the next implementation experiment

### Findings

#### 1. Chromium internals

**Call chain for `target="_blank"`:**

1. Renderer calls `RenderFrameHostImpl::CreateNewWindow()`
   (`render_frame_host_impl.cc:9930`)
2. This calls `WebContentsImpl::CreateNewWindow()` (`web_contents_impl.cc:5245`)
3. A new `WebContents` is created and stored in `pending_contents_`
4. Renderer later calls `ShowCreatedWindow()` (`web_contents_impl.cc:5605`)
5. Which calls `delegate_->AddNewContents()` to present the window

**`OpenURLFromTab` vs `AddNewContents`** — these are complementary, not
alternatives:

- `OpenURLFromTab`: Called for navigation requests within existing pages (e.g.
  context menu "Open in New Tab"). The delegate chooses where to navigate.
- `AddNewContents`: Called when Chromium has _already created_ a new
  `WebContents` (e.g. `window.open()`, `target="_blank"`). The delegate must
  adopt it or let it be destroyed.

For `target="_blank"` links, the flow goes through `CreateNewWindow` →
`AddNewContents`, **not** through `OpenURLFromTab`.

**Pre-creation interception**: `IsWebContentsCreationOverridden()` fires
_before_ the new `WebContents` is created (`web_contents_impl.cc:5282`). If it
returns `true`, Chromium calls `CreateCustomWebContents()`. If that returns
`nullptr`, no `WebContents` is created at all — `window.open()` returns `null`
in the renderer.

There is also `CanCreateWindow()` (`render_frame_host_impl.cc:9978`) which can
block creation even earlier.

**Discarding WebContents**: Chromium explicitly handles the case where
`AddNewContents` discards the `unique_ptr` (comment at
`web_contents_impl.cc:5471`). However, navigating the _source_ WebContents from
_within_ `AddNewContents` may cause re-entrant navigation while the
`CreateNewWindow` call chain is still unwinding.

#### 2. Electron

Electron uses `IsWebContentsCreationOverridden()` as its primary interception
point (`electron_api_web_contents.cc:1223`).

**The flow:**

1. Chromium calls `IsWebContentsCreationOverridden()`
2. Electron emits `-will-add-new-contents` C++ event
3. JavaScript `setWindowOpenHandler()` callback decides: `allow` or `deny`
4. If denied: returns `true` → `CreateCustomWebContents()` returns `nullptr` →
   no WebContents created, `window.open()` returns `null`
5. If allowed: returns `false` → Chromium creates WebContents normally →
   `AddNewContents()` is called → Electron creates a `BrowserWindow` to adopt it

For `OpenURLFromTab` with non-`CURRENT_TAB` disposition, Electron emits
`-new-window` and returns `nullptr` (`electron_api_web_contents.cc:1327`).

**Key insight**: Electron prevents creation _before_ it happens rather than
cleaning up afterward. No WebContents is created and then discarded.

#### 3. Our app (TermSurf)

**`Shell::CreateNewWindow` creates an unmanaged Shell**: The new Shell gets its
own `WebContents`, `RenderWidgetHostView`, `BrowserCompositorMac`, and
`CAContext`. But it does NOT get a `ShellTabObserver` (only `CreateTab()`
creates those) and does NOT register with the XPC connection. The new Shell is
invisible to TermSurf.

**`RenderViewHostChanged`**: When a cross-site navigation happens on an existing
`WebContents`, a new `RenderViewHost` is created with a new
`RenderWidgetHostView` and new `CAContext`. `ShellTabObserver` re-registers
callbacks on the new view (`shell_tab_observer.cc:65`).

**Persistent compositor**: Each tab gets its own `ui::Compositor` +
`PersistentCompositorBridge` that sends `ca_context_id` via XPC
(`shell_browser_main_parts.cc:373`).

### Root cause of Experiment 1 failure

The Experiment 1 changes modified both `OpenURLFromTab` and `AddNewContents`.
For `target="_blank"` links, the flow goes through `CreateNewWindow` →
`AddNewContents` — not through `OpenURLFromTab`. Our `AddNewContents` discarded
the pre-created `WebContents` and called `LoadURLWithParams` on the source
WebContents from _within_ the `AddNewContents` call. This likely caused a
re-entrant navigation while the `CreateNewWindow` call chain was still
unwinding, corrupting Chromium's internal state and leaving the source
WebContents in a broken state where the compositor stopped producing frames.

### Recommendation for Experiment 3

Use Electron's approach: override `IsWebContentsCreationOverridden()` to return
`true`, and `CreateCustomWebContents()` to return `nullptr`. This suppresses
WebContents creation entirely — no new window, no cleanup needed.

Then, to navigate the current tab, post a task to navigate the source
WebContents to the target URL _after_ the `CreateNewWindow` call chain has fully
unwound. This avoids re-entrant navigation.

For `OpenURLFromTab` with non-`CURRENT_TAB` dispositions, navigate the source
tab directly (as in Experiment 1) — this path doesn't go through
`CreateNewWindow` so re-entrancy isn't a concern.

### Result: Success

All three research areas answered with code references. Root cause of Experiment
1 failure identified (re-entrant navigation from within `AddNewContents`). Clear
recommendation: use `IsWebContentsCreationOverridden` + deferred navigation.

## Experiment 3: Suppress creation + deferred navigation

### Hypothesis

Using Electron's pattern — `IsWebContentsCreationOverridden` returns `true` to
intercept, `CreateCustomWebContents` returns `nullptr` to suppress — prevents
the new `WebContents` from being created. A deferred `PostTask` navigates the
source tab to the target URL after the `CreateNewWindow` call chain unwinds,
avoiding re-entrant navigation.

### Changes

All changes in `shell.h` and `shell.cc` (Chromium-only, no GUI/TUI changes).

#### 1. `shell.h`: Add two new overrides

Add after the `AddNewContents` declaration:

```cpp
bool IsWebContentsCreationOverridden(
    RenderFrameHost* opener,
    SiteInstance* source_site_instance,
    mojom::WindowContainerType window_container_type,
    const GURL& opener_url,
    const std::string& frame_name,
    const GURL& target_url) override;
WebContents* CreateCustomWebContents(
    RenderFrameHost* opener,
    SiteInstance* source_site_instance,
    bool is_new_browsing_instance,
    const GURL& opener_url,
    const std::string& frame_name,
    const GURL& target_url,
    const StoragePartitionConfig& partition_config,
    SessionStorageNamespace* session_storage_namespace) override;
```

#### 2. `shell.cc`: Implement `IsWebContentsCreationOverridden`

Suppress creation and post a deferred navigation on the opener's tab:

```cpp
bool Shell::IsWebContentsCreationOverridden(
    RenderFrameHost* opener,
    SiteInstance* source_site_instance,
    mojom::WindowContainerType window_container_type,
    const GURL& opener_url,
    const std::string& frame_name,
    const GURL& target_url) {
  // Issue 639: Intercept new-window requests and navigate the opener tab
  // to the target URL instead. Post a task so the navigation runs after
  // the CreateNewWindow call chain fully unwinds.
  if (opener && target_url.is_valid()) {
    WebContents* source = WebContents::FromRenderFrameHost(opener);
    if (source) {
      base::SequencedTaskRunner::GetCurrentDefault()->PostTask(
          FROM_HERE,
          base::BindOnce(
              [](base::WeakPtr<WebContents> wc, GURL url) {
                if (!wc)
                  return;
                NavigationController::LoadURLParams params(url);
                params.transition_type = ui::PAGE_TRANSITION_LINK;
                wc->GetController().LoadURLWithParams(params);
              },
              source->GetWeakPtr(), target_url));
    }
  }
  return true;  // Always suppress — we handle it ourselves.
}
```

#### 3. `shell.cc`: Implement `CreateCustomWebContents`

Return `nullptr` to tell Chromium not to create a `WebContents`:

```cpp
WebContents* Shell::CreateCustomWebContents(
    RenderFrameHost* opener,
    SiteInstance* source_site_instance,
    bool is_new_browsing_instance,
    const GURL& opener_url,
    const std::string& frame_name,
    const GURL& target_url,
    const StoragePartitionConfig& partition_config,
    SessionStorageNamespace* session_storage_namespace) {
  // Issue 639: No custom WebContents — creation is fully suppressed.
  return nullptr;
}
```

#### 4. `shell.cc`: Modify `OpenURLFromTab`

Same as Experiment 1 — navigate the source tab for new-tab dispositions:

```cpp
case WindowOpenDisposition::NEW_POPUP:
case WindowOpenDisposition::NEW_WINDOW:
case WindowOpenDisposition::NEW_BACKGROUND_TAB:
case WindowOpenDisposition::NEW_FOREGROUND_TAB:
  // Issue 639: Open in current tab instead of creating a new window.
  target = source;
  break;
```

This path doesn't go through `CreateNewWindow` so re-entrancy is not a concern.

#### 5. `shell.cc`: Leave `AddNewContents` unchanged

`AddNewContents` should never be called for new-window requests now, since
`IsWebContentsCreationOverridden` suppresses creation before it happens. Keep
the original implementation as a safety net for any edge cases (e.g.
picture-in-picture).

#### 6. Includes

Add if not already present:

```cpp
#include "base/task/sequenced_task_runner.h"
#include "ui/base/page_transition_types.h"
```

### Verification

1. Build Chromium (`autoninja -C out/Default chromium_profile_server`)
2. Launch TermSurf, `web localhost:9616/test-target-blank.html`
3. Click "Open example.com in new tab" (`target="_blank"`) — opens in same tab
4. Click back — returns to test page
5. Click `window.open()` button — opens in same tab
6. Click "Open with rel=noopener" — opens in same tab
7. No stray Chromium windows should appear
8. URL bar and page title update correctly after each navigation

### Success criteria

- `target="_blank"` links navigate the current tab
- `window.open()` navigates the current tab
- No new Chromium windows are created
- No TUI freezes or unresponsiveness
- Back/forward navigation works after redirected navigations
- Page title and URL bar update correctly

### Result: Success

All success criteria pass. `target="_blank"` links, `window.open()`, and
`rel="noopener"` links all navigate in the current tab. No stray windows, no
freezes. Back navigation returns to the previous page. URL bar and page title
update correctly.

## Conclusion

Issue 639 is closed. New-tab and new-window requests (`target="_blank"`,
`window.open()`, `rel="noopener"`) are intercepted and opened in the current tab
instead. The implementation uses Electron's pattern:
`IsWebContentsCreationOverridden` suppresses `WebContents` creation, and a
deferred `PostTask` navigates the source tab after the call chain unwinds. True
multi-tab support is deferred to a future issue.
