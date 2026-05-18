+++
status = "closed"
opened = "2026-03-14"
closed = "2026-03-14"
+++

# Issue 750: target="\_blank" links don't open

## Goal

Links with `target="_blank"` (and `window.open()` calls) navigate the current
tab instead of silently failing. Multi-tab support is deferred — for now, all
new-window requests open in the same tab.

## Background

### The problem

Clicking a `target="_blank"` link or triggering `window.open()` in a browser
overlay does nothing. The link silently fails because Content Shell's default
behavior creates a new `Shell` window, which has no connection to TermSurf's
overlay system — the new window is invisible and orphaned.

### Prior art

Issue 639 solved this exact problem in an earlier Chromium fork branch
(`146.0.7650.0-issue-639`). The solution used Electron's pattern:

1. Override `IsWebContentsCreationOverridden()` to return `true`, suppressing
   new `WebContents` creation. Post a deferred `PostTask` to navigate the source
   tab to the target URL after the `CreateNewWindow` call chain unwinds.
2. Override `CreateCustomWebContents()` to return `nullptr`.
3. Modify `OpenURLFromTab()` to route `NEW_POPUP`, `NEW_WINDOW`,
   `NEW_BACKGROUND_TAB`, and `NEW_FOREGROUND_TAB` dispositions to the source
   tab.

The patch is preserved at
`chromium/patches/issue-639/0042-Suppress-new-window-navigate-same-tab.patch`.

### Why the fix was lost

Issue 708 refactored the Chromium fork from `content/chromium_profile_server/`
to `content/libtermsurf_chromium/` and created a new branch
(`146.0.7650.0-issue-708`). The Issue 639 commits were not carried forward to
the new branch. The current branch (`146.0.7650.0-issue-708`) has the vanilla
Content Shell `OpenURLFromTab()` behavior — it creates new `Shell` windows for
new-tab/popup requests.

### What needs to happen

Re-apply the Issue 639 fix to the current `shell.h`/`shell.cc` on a new Chromium
branch. The code is nearly identical — the only difference is the file paths
changed from `content/chromium_profile_server/browser/shell.*` to
`content/shell/browser/shell.*`.

## Experiments

### Experiment 1: Re-apply Issue 639 fix to current Chromium branch

#### Description

Re-apply the three `WebContentsDelegate` overrides from Issue 639 to
`content/shell/browser/shell.h` and `shell.cc` on the current Chromium branch.
The original patch targeted `content/chromium_profile_server/browser/shell.*`
but the current branch uses the upstream Content Shell paths.

#### Chromium branch

New branch: `146.0.7650.0-issue-750` Fork from: `146.0.7650.0-issue-708`
(current branch, has the libtermsurf_chromium refactoring and all prior TermSurf
modifications)

```bash
cd chromium/src
git checkout -b 146.0.7650.0-issue-750 146.0.7650.0-issue-708
```

Also add the new branch to `chromium/README.md`.

#### Changes

**`chromium/src/content/shell/browser/shell.h`**

1. After the `AddNewContents` override declaration (~line 125), add two new
   override declarations:

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

**`chromium/src/content/shell/browser/shell.cc`**

2. Add two includes near the top:

   ```cpp
   #include "base/task/sequenced_task_runner.h"
   #include "ui/base/page_transition_types.h"
   ```

3. After `AddNewContents` (~line 334), add the two new method implementations:

   `IsWebContentsCreationOverridden`: Posts a deferred `PostTask` that navigates
   the source `WebContents` to the target URL after `CreateNewWindow` unwinds.
   Returns `true` to suppress creation.

   `CreateCustomWebContents`: Returns `nullptr`.

4. In `OpenURLFromTab` (~line 436), replace the `NEW_POPUP`/`NEW_WINDOW`/
   `NEW_BACKGROUND_TAB`/`NEW_FOREGROUND_TAB` case block that creates a new
   `Shell` window with `target = source` to navigate the current tab instead.

#### Verification

Build Chromium:

```bash
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default libtermsurf_chromium
```

Then build and install TermSurf:

```bash
scripts/build.sh all
scripts/install.sh all
```

Test:

| #   | Test                        | Steps                                                | Expected                                |
| --- | --------------------------- | ---------------------------------------------------- | --------------------------------------- |
| 1   | target="\_blank" link       | Visit a page with a `target="_blank"` link, click it | Current tab navigates to the link's URL |
| 2   | window.open()               | Visit a page that calls `window.open()`, trigger it  | Current tab navigates to the opened URL |
| 3   | Normal links still work     | Click a regular link (no target attribute)           | Navigates normally                      |
| 4   | Back/forward after redirect | After a target="\_blank" redirect, press back        | Returns to previous page                |
| 5   | No orphan windows           | After all tests, check for stray Chromium windows    | No orphaned Shell windows               |

**Result:** Pass

All five tests pass.

#### Conclusion

The Issue 639 fix re-applied cleanly to the current Content Shell paths. The
three `WebContentsDelegate` overrides suppress new window creation and navigate
the current tab instead.

## Conclusion

Re-applied the Issue 639 `target="_blank"` fix to the current Chromium branch
(`146.0.7650.0-issue-750`). Links with `target="_blank"` and `window.open()`
calls now navigate the current tab. No changes needed in Roamium or Wezboard —
the fix is entirely in Content Shell's `WebContentsDelegate`.
