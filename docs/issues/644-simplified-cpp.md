# Issue 644: Simplified C++ Profile Server

## Goal

Replace the Content Shell fork with a minimal, purpose-built C++ profile server.
The current `chromium_profile_server` carries ~100 Content Shell files we never
modify. Strip it down to only what TermSurf needs: a thin executable that
creates BrowserContexts, manages WebContents, handles XPC, and streams CAContext
IDs back to the GUI. No Shell windows, no DevTools frontend, no Content Shell
boilerplate.

## Background

### The Content Shell problem

The current Chromium Profile Server (`chromium_profile_server`) is a fork of
Content Shell — Chromium's reference embedder. Content Shell is designed to be a
complete minimal browser with its own window, toolbar, DevTools, and test
infrastructure. TermSurf uses none of that. We subclass a few Content Shell
classes, override path resolution, and add XPC handling. But the build target
pulls in ~100 files of Content Shell code: `ShellBrowserMainParts`,
`ShellContentBrowserClient`, `ShellWebContentsViewDelegate`,
`ShellDevToolsFrontend`, `ShellJavaScriptDialog`, and dozens more.

This creates three problems:

1. **Upgrade friction.** Every Chromium version upgrade risks merge conflicts in
   Content Shell files we don't own. The more Content Shell code we depend on,
   the more conflicts we face.

2. **Complexity.** Understanding what our server actually does requires
   separating our ~1,050 lines from Content Shell's thousands. New contributors
   see 100+ files and can't tell which ones matter.

3. **Unnecessary code.** Content Shell creates Shell windows, handles DevTools,
   manages JavaScript dialogs, and implements test-specific behaviors. None of
   this is relevant to a headless profile server that streams CAContext IDs over
   XPC.

### What Issues 642–643 taught us

Issues 642–643 attempted to solve this by rewriting the server in Zig. The
Zig-to-Chromium bridge works (dlopen, C API shim, WebContents creation,
CAContext IDs), but XPC integration never worked end-to-end across 7
experiments. The failure pattern: standalone Chromium works, but the full GUI →
XPC → server → GUI pipeline doesn't.

The lesson isn't that Zig is wrong — it's that the rewrite was too ambitious.
Changing the language AND the build system AND the deployment AND the XPC
implementation all at once made failures hard to diagnose. A simpler approach:
keep C++, keep the working build system, but strip out Content Shell.

### What we actually need

The profile server needs exactly these capabilities:

- **ContentMain entry point** — initialize Chromium
- **BrowserContext** — create isolated browser profiles with persistent storage
- **WebContents** — create headless web pages, navigate, resize
- **Compositor** — persistent compositor for stable CAContext IDs
- **XPC** — connect to the GUI gateway, receive commands, send back events
- **Input forwarding** — route mouse, keyboard, scroll events to WebContents
- **Observation** — URL, title, loading state, cursor changes → XPC messages

Content Shell provides all of this, but buried under layers of Shell-specific
abstractions. A simplified server implements these directly against the Content
API.

## Approach

Create a new directory `chromium/src/content/termsurf_browser/` with a minimal
Content API embedder. Start from scratch — not by forking Content Shell, but by
implementing only the required Content API interfaces. Use the existing
`chromium_profile_server` as a reference for what works, but don't copy its
Content Shell dependencies.

The key Content API classes to implement:

- `ContentMainDelegate` — app initialization, creates the browser client
- `ContentBrowserClient` — creates the BrowserContext, configures the browser
- `BrowserMainParts` — lifecycle hooks (pre-main-message-loop, post-startup)
- `BrowserContext` — profile storage, cookie/cache path configuration
- `WebContentsDelegate` — handles navigation, title changes, new windows
- `WebContentsObserver` — observes loading state, URL changes

Everything else — Shell windows, DevTools frontend, JavaScript dialogs, test
infrastructure — is omitted.

## Experiments

### Experiment 1: Restore the Working C++ Profile Server

Before changing anything, get back to a known-good state. Issues 642–643 left
behind uncommitted Zig code in the main repo and switched the Chromium fork to
branches with the `zig_profile_server` target. The existing C++ profile server
(`chromium_profile_server`) still works — we just need to point at the right
branch and clean up.

#### Clean up the main repo

Delete all Zig profile server code from Issues 642–643:

**Delete the `browser/` directory entirely.** This was created for the Zig
profile server and is no longer needed. Committed files (`browser/build.zig`,
`browser/src/main.zig`) and uncommitted files (`browser/build.zig.zon`,
`browser/macos/Info.plist`, `browser/macos/PkgInfo`) all go.

**Restore `gui/src/apprt/xpc.zig`.** The uncommitted change points the server
path at `Zig Profile Server.app`. Revert it to the committed version, which
points at `Chromium Profile Server.app`:

```
"{s}/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server"
```

#### Create the Chromium branch

The last branch with a working C++ profile server is `146.0.7650.0-issue-639`
(open new-tab links in same tab). The `issue-642` and `issue-643` branches have
the `zig_profile_server` target, not `chromium_profile_server`.

Create `146.0.7650.0-issue-644` from `146.0.7650.0-issue-639`. Add it to
`docs/chromium.md`.

#### Build and verify

```bash
cd chromium/src
git checkout 146.0.7650.0-issue-644
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

cd ../../gui && zig build
open zig-out/TermSurf.app
```

Type `web google.com` in a terminal pane. Expected: web page renders, mouse
clicks work, keyboard input works, URL bar updates, page title syncs. All
features that were working before Issues 642–643 should work again.

#### Pass criteria

The C++ profile server works end-to-end with all previously-working features:
web rendering, mouse input, keyboard input, resize, navigation, URL sync, page
title sync.

#### Result: Pass

The C++ profile server works end-to-end. Web rendering, mouse input, keyboard
input, resize, navigation, URL sync, and page title sync all function correctly.
We are back to a known-good baseline.

### Experiment 2: Research the Existing Profile Server

Before building anything new, understand what we have. The current
`chromium_profile_server` is a fork of Content Shell with TermSurf-specific
modifications layered on top. This experiment maps out what Content Shell
provides, what TermSurf actually uses, and what can be dropped.

#### Questions to answer

1. **What TermSurf files exist?** List every file in
   `content/chromium_profile_server/` that we wrote or modified. For each file,
   summarize what it does in one sentence.

2. **What Content Shell files do we depend on?** Trace the `#include` and
   subclass chains from our files into `content/shell/`. For each Content Shell
   file we touch, document why — what base class or function do we use from it?

3. **What Content Shell files are pulled in transitively?** The BUILD.gn target
   depends on Content Shell sources. Many of those sources pull in more Content
   Shell code. List the full set of Content Shell files that end up in the
   build, grouped by category (browser, renderer, DevTools, test infra, UI,
   etc.).

4. **What Content API interfaces do we actually implement?** List the pure
   Content API classes (from `content/public/`) that our server needs:
   `ContentMainDelegate`, `ContentBrowserClient`, `BrowserMainParts`,
   `BrowserContext`, `WebContentsDelegate`, `WebContentsObserver`, etc. For
   each, note whether we implement it directly or inherit it through a Content
   Shell subclass.

5. **What Content Shell functionality do we rely on?** Some Content Shell code
   may do things we actually need — like setting up the network stack, creating
   the GPU process, or configuring the compositor. Identify any Content Shell
   logic that would need to be replicated in a from-scratch implementation.

6. **Is simplification feasible?** Given the answers above, is it realistic to
   implement a standalone Content API embedder that replaces Content Shell? What
   are the risks — are there Content Shell behaviors we depend on that would be
   hard to replicate?

#### Process

Read the source code in `chromium/src/content/chromium_profile_server/` and
trace its dependencies into `content/shell/` and `content/public/`. Use the
BUILD.gn files to understand what gets compiled. Read the Content Shell source
files we subclass to understand what behavior we inherit.

#### Pass criteria

A written analysis answering all six questions above, with enough detail to
design Experiment 3 (the simplified implementation). The analysis should make it
clear exactly which Content API interfaces to implement and what Content Shell
behavior (if any) needs to be replicated.

#### Analysis

##### 1. What TermSurf files exist?

16 files were modified or created after the initial Content Shell copy. They
break into three categories:

**Created by TermSurf (6 files):**

| File                             | Purpose                                                                 |
| -------------------------------- | ----------------------------------------------------------------------- |
| `shell_tab_observer.h`           | WebContentsObserver that sends nav/loading/title/cursor events over XPC |
| `shell_tab_observer.cc`          | Implementation (~200 lines)                                             |
| `shell_ca_layer_bridge_mac.h`    | Bridge to set CALayerParams callback on RenderWidgetHostViewMac         |
| `shell_ca_layer_bridge_mac.mm`   | Implementation (~17 lines)                                              |
| `shell_compositor_bridge_mac.h`  | AcceleratedWidgetMacNSView impl for persistent compositor CAContext     |
| `shell_compositor_bridge_mac.mm` | Implementation + SetParentUiLayerOnView helper (~35 lines)              |

**Heavily modified by TermSurf (4 files):**

| File                          | Lines added | Purpose of modifications                                                             |
| ----------------------------- | ----------- | ------------------------------------------------------------------------------------ |
| `shell_browser_main_parts.cc` | ~845        | XPC gateway, tab lifecycle, input forwarding, compositor setup                       |
| `shell_browser_main_parts.h`  | ~64         | TabState struct, XPC method declarations                                             |
| `shell.cc`                    | ~70         | Suppress new-window (navigate same tab), disable DevTools                            |
| `shell.h`                     | ~16         | PrimaryPageChanged override, IsWebContentsCreationOverridden/CreateCustomWebContents |

**Lightly modified by TermSurf (4 files):**

| File                                      | Change                                                                                                                                |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `BUILD.gn`                                | Added `shell_ca_layer_bridge_mac.*`, `shell_compositor_bridge_mac.*`, `shell_tab_observer.*`, IOSurface framework, ui/compositor deps |
| `shell_platform_delegate_mac.mm`          | Offscreen window positioning, suppress Shell chrome                                                                                   |
| `shell_web_contents_view_delegate_mac.mm` | Disable context menu                                                                                                                  |
| `common/shell_switches.h`                 | Added `kXpcService`, `kHidden`; removed `kSessionId`                                                                                  |

**Deleted by TermSurf (2 files):**

| File                      | Reason                                                  |
| ------------------------- | ------------------------------------------------------- |
| `shell_video_consumer.cc` | Replaced by CALayerHost; was the FrameSinkVideoCapturer |
| `shell_video_consumer.h`  | Same                                                    |

##### 2. What Content Shell files do we depend on?

Our code subclasses or directly uses these Content Shell classes:

| Content Shell class            | How we use it                                                                                                                                                                                                     |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Shell`                        | **Subclass relationship: IS Content Shell's Shell.** We modified it to suppress new-window popups (navigate same tab), disable DevTools, add PrimaryPageChanged. We call `Shell::CreateNewWindow()` to make tabs. |
| `ShellBrowserMainParts`        | **IS Content Shell's BrowserMainParts subclass.** We added XPC, tab lifecycle, input forwarding, compositor setup. It inherits from `content::BrowserMainParts` (Content API).                                    |
| `ShellBrowserContext`          | **Used unmodified.** Our `BrowserMainParts` creates `ShellBrowserContext(false)`. It implements `content::BrowserContext` with file-backed storage, download manager, permission manager.                         |
| `ShellContentBrowserClient`    | **Used unmodified.** Creates `ShellBrowserMainParts`, provides `WebContentsViewDelegate`, configures network context, DevTools.                                                                                   |
| `ShellMainDelegate`            | **Used unmodified.** The `ContentMainDelegate` — creates all client objects, initializes resource bundles, crash reporting.                                                                                       |
| `ShellPlatformDelegate`        | **Lightly modified (mac).** Creates NSWindow (we positioned it offscreen), manages Shell chrome (we suppressed it).                                                                                               |
| `ShellDevToolsManagerDelegate` | **Used unmodified.** Starts/stops DevTools HTTP handler. We don't need this.                                                                                                                                      |
| `ShellJavaScriptDialogManager` | **Used unmodified.** Handles JS alert/confirm/prompt dialogs. Pulled in via `Shell::GetJavaScriptDialogManager()`.                                                                                                |
| `ShellDevToolsFrontend`        | **Used unmodified.** DevTools frontend window. Referenced by Shell but never opened in our use case.                                                                                                              |
| `ShellDownloadManagerDelegate` | **Used unmodified.** Provides download path resolution.                                                                                                                                                           |
| `ShellPermissionManager`       | **Used unmodified.** Grants all permissions by default.                                                                                                                                                           |
| `ShellContentClient`           | **Used unmodified.** User agent string, content client.                                                                                                                                                           |
| `ShellContentRendererClient`   | **Used unmodified.** Renderer process client.                                                                                                                                                                     |
| `ShellContentGpuClient`        | **Used unmodified.** GPU process client.                                                                                                                                                                          |
| `ShellContentUtilityClient`    | **Used unmodified.** Utility process client.                                                                                                                                                                      |

##### 3. What Content Shell files are pulled in transitively?

The `chromium_profile_server_lib` static library compiles these source files
(macOS-relevant only, excluding Android/iOS/Windows/Fuchsia):

**Browser layer (~30 files):**

- `shell.cc/h` — Shell window management, WebContentsDelegate (**modified**)
- `shell_browser_main_parts.cc/h` + `_mac.mm` — Lifecycle + XPC (**modified**)
- `shell_browser_context.cc/h` — BrowserContext implementation
- `shell_content_browser_client.cc/h` — ContentBrowserClient
- `shell_platform_delegate.cc/h` + `_mac.mm` — Platform window (**modified**)
- `shell_web_contents_view_delegate.h` + `_mac.mm` — View delegate
  (**modified**)
- `shell_devtools_bindings.cc/h` — DevTools bindings (**unnecessary**)
- `shell_devtools_frontend.cc/h` — DevTools frontend window (**unnecessary**)
- `shell_devtools_manager_delegate.cc/h` — DevTools HTTP server
  (**unnecessary**)
- `shell_download_manager_delegate.cc/h` — Download handling
- `shell_javascript_dialog.h` + `_mac.mm` — JS dialog UI (**unnecessary**)
- `shell_javascript_dialog_manager.cc/h` — JS dialog dispatch (**unnecessary**)
- `shell_permission_manager.cc/h` — Permission grants
- `shell_content_index_provider.cc/h` — Content index (**unnecessary**)
- `shell_speech_recognition_manager_delegate.cc/h` — Speech (**unnecessary**)
- `shell_platform_data_aura.cc/h` — Aura (not used on Mac)
- `renderer_host/shell_render_widget_host_view_mac_delegate.h/mm` — View
  delegate
- `shell_application_mac.h/mm` — NSApplication subclass
- `protocol/browser_handler.cc/h` — DevTools protocol (**unnecessary**)
- `protocol/domain_handler.h` — DevTools protocol (**unnecessary**)
- `protocol/shell_devtools_session.cc/h` — DevTools protocol (**unnecessary**)
- `shell_tab_observer.cc/h` — XPC notifications (**ours**)
- `shell_ca_layer_bridge_mac.h/mm` — CALayer callback bridge (**ours**)
- `shell_compositor_bridge_mac.h/mm` — Persistent compositor bridge (**ours**)

**Common layer (~10 files):**

- `shell_content_client.cc/h` — ContentClient (user agent, etc.)
- `shell_origin_trial_policy.cc/h` — Origin trials
- `shell_paths.cc/h` — Path provider
- `shell_switches.cc/h` — Command-line switches (**modified**)
- `main_frame_counter_test_impl.cc/h` — Test infra (**unnecessary**)
- `power_monitor_test_impl.cc/h` — Test infra (**unnecessary**)

**Renderer layer (~4 files):**

- `shell_content_renderer_client.cc/h` — ContentRendererClient
- `shell_render_frame_observer.cc/h` — Frame observer
- `render_frame_test_helper.cc/h` — Test infra (**unnecessary**)

**GPU layer (~2 files):**

- `shell_content_gpu_client.cc/h` — ContentGpuClient

**Utility layer (~2 files):**

- `shell_content_utility_client.cc/h` — ContentUtilityClient

**App layer (~8 files):**

- `shell_main.cc` / `shell_main_mac.cc` — Entry point
- `shell_content_main.cc/h` — Framework entry (mac)
- `shell_main_delegate.cc/h` + `_mac.h/mm` — ContentMainDelegate
- `shell_crash_reporter_client.cc/h` — Crash reporting
- `paths_apple.h/mm` — Apple path overrides

**Build infra:**

- `protocol_config.json` — DevTools protocol codegen (**unnecessary**)
- `shell_resources.grd` — Resources
- Mojom files for test interfaces (**unnecessary**)

**Total: ~56 macOS-relevant source files.** Of those, ~16 are marked unnecessary
(DevTools, test infra, JS dialogs, speech, content index).

##### 4. What Content API interfaces do we actually implement?

| Content API interface        | Implemented by               | Direct or inherited?                 |
| ---------------------------- | ---------------------------- | ------------------------------------ |
| `ContentMainDelegate`        | `ShellMainDelegate`          | Inherited (unmodified Content Shell) |
| `ContentClient`              | `ShellContentClient`         | Inherited (unmodified)               |
| `ContentBrowserClient`       | `ShellContentBrowserClient`  | Inherited (unmodified)               |
| `ContentRendererClient`      | `ShellContentRendererClient` | Inherited (unmodified)               |
| `ContentGpuClient`           | `ShellContentGpuClient`      | Inherited (unmodified)               |
| `ContentUtilityClient`       | `ShellContentUtilityClient`  | Inherited (unmodified)               |
| `BrowserMainParts`           | `ShellBrowserMainParts`      | Inherited (**heavily modified**)     |
| `BrowserContext`             | `ShellBrowserContext`        | Inherited (unmodified)               |
| `WebContentsDelegate`        | `Shell`                      | Inherited (**modified**)             |
| `WebContentsObserver`        | `Shell` + `ShellTabObserver` | Shell inherited, TabObserver direct  |
| `AcceleratedWidgetMacNSView` | `PersistentCompositorBridge` | Direct (**ours**)                    |

All Content API interfaces are implemented **through Content Shell subclasses**,
not directly. The simplified server would implement them directly.

##### 5. What Content Shell functionality do we rely on?

**Critical functionality we actually use:**

1. **`Shell::CreateNewWindow()`** — Creates a `WebContents`, sets up the
   delegate chain, creates an NSWindow (offscreen). This is ~50 lines that call
   `WebContents::Create()`, set the delegate, and call
   `ShellPlatformDelegate::CreatePlatformWindow()`. Straightforward to
   replicate.

2. **`ShellBrowserContext`** — Implements `BrowserContext` with file-backed
   storage. Configures `--user-data-dir` path, creates download manager,
   permission manager, origin trials delegate. This is ~200 lines. It could be
   reimplemented, but it's also clean enough to reuse as-is.

3. **`ShellContentBrowserClient`** — Implements `ContentBrowserClient`. Creates
   the `BrowserMainParts`, configures network context, provides
   `WebContentsViewDelegate`. Much of it is Content Shell boilerplate (test
   support, DevTools delegate creation, feature list setup). The essential parts
   are `CreateBrowserMainParts()`, `ConfigureNetworkContextParams()`, and
   `GetWebContentsViewDelegate()`. Could be simplified to ~100 lines.

4. **`ShellMainDelegate`** — Implements `ContentMainDelegate`. Initializes
   resource bundles, crash reporting, creates all client objects. The essential
   parts are `BasicStartupComplete()`, `PreSandboxStartup()` (resource bundle),
   and the `Create*Client()` methods. Could be simplified to ~80 lines.

5. **`ShellPlatformDelegate` (Mac)** — Creates an offscreen NSWindow for the
   WebContents. We need a window because `RenderWidgetHostViewMac` requires one,
   but we never show it. This is ~30 lines of relevant code.

6. **Resource bundle loading** — `ShellMainDelegate::InitializeResourceBundle()`
   loads `.pak` files. The `.pak` repack target in BUILD.gn bundles Blink
   resources, net resources, UI strings, etc. This is build infrastructure, not
   code — but the simplified server still needs it.

7. **Multi-process architecture** — Content Shell's BUILD.gn and main delegate
   set up helper processes (GPU, renderer, utility) via the
   `mac_app_bundle`/`mac_framework_bundle` pattern with helper apps. This is
   entirely build infrastructure. The simplified server needs the same pattern.

**Functionality we DON'T use but currently carry:**

- DevTools frontend, bindings, manager delegate, protocol handlers (~8 files)
- JavaScript dialog manager and platform dialog (~3 files)
- Speech recognition delegate (~2 files)
- Content index provider (~2 files)
- Test-specific Mojom interfaces and implementations (~6 files)
- Shell toolbar / URL bar / navigation buttons (in Shell and platform delegate)

##### 6. Is simplification feasible?

**Yes, with caveats.**

**What's straightforward:**

- Replace `Shell` with a minimal `WebContentsDelegate` that just creates
  `WebContents` and suppresses popups. Our modifications to `Shell` are small
  (~70 lines added), and most of `Shell`'s 600+ lines are features we don't use
  (toolbar, DevTools, file chooser, color chooser, fullscreen, etc.).

- Replace `ShellBrowserMainParts` with a class that only has our XPC/tab/input
  code. The Content Shell lifecycle methods we override are thin
  (`InitializeBrowserContexts`, `InitializeMessageLoopContext`,
  `PreMainMessageLoopRun`). Our ~845 added lines would become the entire class.

- Drop DevTools, JS dialogs, speech, content index, test infra — ~16 files gone
  with no impact on functionality.

- Implement `ContentMainDelegate`, `ContentBrowserClient`,
  `ContentRendererClient`, `ContentGpuClient`, `ContentUtilityClient` directly
  against the Content API. Content Shell's implementations are mostly
  pass-through with test hooks we don't need.

**What needs care:**

- **`ShellBrowserContext`**: Implements 15+ `BrowserContext` pure virtual
  methods (download manager, permission controller, storage policy, etc.). Could
  reuse it as-is or reimplement. Reusing is simpler.

- **Resource bundle / `.pak` repack**: The BUILD.gn `repack()` target that
  bundles Blink resources is complex but mechanical. We need the same resources.
  Simplest approach: reference the same resource deps.

- **Mac app bundle structure**: The `mac_app_bundle` + `mac_framework_bundle` +
  helper apps pattern is ~200 lines of BUILD.gn. This is required for
  multi-process Chromium on macOS. Can be copied with name changes.

- **`content/browser/` internal headers**: Two of our files
  (`shell_compositor_bridge_mac.mm` and `shell_ca_layer_bridge_mac.mm`) include
  `content/browser/renderer_host/render_widget_host_view_mac.h` — an internal
  header, not part of the public Content API. This works because Content Shell
  has `check_includes = false` in component builds. A simplified server needs
  the same escape hatch.

**Estimated file count for simplified server:**

| Category  | Files   | Notes                                                                                                                                   |
| --------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| App layer | 4-5     | main, delegate, content_main, crash client, paths                                                                                       |
| Browser   | 8-10    | main_parts, browser_context, browser_client, platform delegate, tab_observer, ca_layer_bridge, compositor_bridge, web_contents_delegate |
| Common    | 3-4     | content_client, switches, paths                                                                                                         |
| Renderer  | 2       | renderer_client, frame_observer                                                                                                         |
| GPU       | 1       | gpu_client                                                                                                                              |
| Utility   | 1       | utility_client                                                                                                                          |
| **Total** | **~22** | Down from ~56, minus DevTools/test/dialog/speech                                                                                        |

**Verdict: Feasible and worthwhile.** The simplified server would have \~22
source files instead of \~56, all purpose-written for TermSurf. No Content Shell
subclassing — direct Content API implementations. The XPC, compositor, input,
and observation code (our ~1,050 lines) moves over unchanged. The Content Shell
boilerplate (DevTools, dialogs, test infra, toolbar) is dropped entirely.

The biggest risk is missing some subtle Content Shell behavior that Chromium
depends on at runtime. Mitigation: start minimal, test frequently, add back only
what breaks.

#### Result: Pass

The analysis answers all six questions. Key findings:

- 16 files were modified/created by TermSurf; ~40 files are unmodified Content
  Shell code, of which ~16 are unnecessary (DevTools, test infra, JS dialogs).
- All Content API interfaces are implemented through Content Shell subclasses,
  not directly. The simplified server would implement them directly.
- `ShellBrowserContext` is the most complex piece to reimplement (15+ pure
  virtual methods). Recommend reusing it initially.
- Two files use internal `content/browser/` headers — this is the only tight
  coupling to Chromium internals.
- Estimated simplified server: ~22 files vs. ~56 current. All purpose-built.
