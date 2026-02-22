# Issue 620: Zig Content Shell

## Goal

Build a minimal Chromium embedder using a thin C++ shim and Zig logic that can
load web pages and support multiple browser profiles in a single process. This
replaces the 14,000-line Content Shell fork with about 1,400 lines and
determines whether the browser can run in-process inside the GUI.

## Background

Issue 619 investigated input latency and traced it to three sources: the
FrameSinkVideoCapturer (a recording API, not the display path), asynchronous XPC
message-passing, and a double-vsync penalty from out-of-process streaming.
Research revealed that:

1. **Content Shell uses Chrome's native display path.** CALayerHost, zero-copy
   GPU compositing, compositor-thread input handling. Our FrameSinkVideoCapturer
   bypasses all of this — it is a recording API bolted onto the side of the
   display compositor.

2. **The multi-process architecture is a CEF artifact.** CEF required one
   process per browser profile (`SingletonLock` on `root_cache_path`). The
   Content API has no such limitation — `content::BrowserContext` supports
   multiple instances in one process with full isolation. ts4 proved this
   (Issues 406–413): two profiles at 60fps in a single content_shell process.

3. **The Content API is C++, but Zig can drive it.** A thin C++ shim (about 800
   lines) subclasses the required virtual classes (`ContentMainDelegate`,
   `ContentBrowserClient`, `BrowserMainParts`, `WebContentsDelegate`,
   `WebContentsObserver`) and forwards all calls to C functions. Zig implements
   those C functions — tab lifecycle, input routing, profile management. All
   logic lives in Zig; the C++ shim is mechanical glue.

### What the Zig Content Shell replaces

The current Chromium Profile Server is a fork of Content Shell: 13,000 lines of
unmodified boilerplate + 1,050 lines of TermSurf logic. Of those 1,050 lines,
590 are XPC gateway connection and input routing in
`shell_browser_main_parts.cc` and 460 are the `ShellVideoConsumer` (frame
capture + IOSurface transfer). The rest of the 100+ files are copied verbatim
and never modified.

The Zig Content Shell replaces all of this with two components:

- **C++ shim** (about 800 lines) — Subclasses Content API virtual classes,
  exposes C functions for Zig. Built inside `chromium/src/` with GN/autoninja.
- **Zig embedder** (about 600 lines) — Tab lifecycle, profile management, input
  routing. Built separately, linked against the C++ shim.

### What we strip from Content Shell

- Web test infrastructure (`IsRunWebTestsSwitchPresent()` paths) — roughly 30%
  of Content Shell's code
- Android, iOS, Fuchsia, ChromeOS platform code — macOS only
- Aura/Ozone UI toolkit code — macOS doesn't use these
- The `Shell` class window management — replaced by the C shim
- `ShellPlatformDelegate` platform abstraction — single platform, no abstraction
  needed
- DevTools HTTP server — can be re-added later via the shim if needed

### What we keep (via the C++ shim)

- `ContentMain()` — entry point
- `ContentMainDelegate` — app initialization (5 overrides)
- `ContentBrowserClient` — browser configuration (start with minimal overrides,
  add incrementally)
- `BrowserMainParts` — initialization pipeline
- `BrowserContext` — profile storage and isolation
- `WebContents` — page lifecycle, navigation
- `RenderWidgetHost` — input forwarding
- `NavigationController` — back/forward/reload
- `WebContentsObserver` — navigation events, loading state

### The critical experiment

Can two different browser profiles (`BrowserContext` instances with different
storage paths) coexist in the same Zig process? ts4 proved this works in a
native C++ content_shell. The experiment confirms it works through the C++
shim + Zig bridge.

If two profiles work: in-process is the answer. The GUI binary becomes the
browser process. The entire multi-process architecture (xpc-gateway, profile
server spawning, XPC connections, IOSurface Mach port transfer, frame capture,
120fps oversampling) goes away.

If two profiles fail: out-of-process with the Zig Content Shell as a separate
binary. Still a major improvement — 1,400 lines instead of 14,000, and the
codebase is understandable and modifiable.

## Architecture

### C++ shim (3 files in the Chromium fork)

The shim lives in `chromium/src/content/zig_content_shell/` — just 3 files
(BUILD.gn, one `.h`, one `.cc`). It must be inside `chromium/src/` because GN
can only see files rooted there. Built with autoninja, produces a shared library
(component build). This is the same pattern as the current
`chromium_profile_server/`, but 3 files instead of 100+.

```
content_api_shim.h    — C header (Zig-callable)
content_api_shim.cc   — C++ implementation
├── TsContentMainDelegate : ContentMainDelegate
├── TsContentBrowserClient : ContentBrowserClient
├── TsBrowserMainParts : BrowserMainParts
├── TsWebContentsDelegate : WebContentsDelegate
├── TsWebContentsObserver : WebContentsObserver
├── TsBrowserContext : BrowserContext
│
├── Initialization:
│   ts_content_main(argc, argv)
│
├── Profile management:
│   ts_create_browser_context(path) → context handle
│   ts_destroy_browser_context(handle)
│
├── Tab management:
│   ts_create_web_contents(context, url) → contents handle
│   ts_destroy_web_contents(handle)
│   ts_load_url(handle, url)
│
├── Navigation:
│   ts_go_back(handle)
│   ts_go_forward(handle)
│   ts_reload(handle)
│   ts_can_go_back(handle) → bool
│   ts_can_go_forward(handle) → bool
│
├── Input:
│   ts_forward_mouse_event(handle, type, x, y, button, mods)
│   ts_forward_scroll_event(handle, x, y, dx, dy, phase, mods)
│   ts_forward_key_event(handle, type, keycode, text, mods)
│   ts_set_focus(handle, focused)
│
├── Display:
│   ts_get_ca_context_id(handle) → uint32_t
│   ts_set_view_size(handle, width, height)
│
└── Callbacks (Zig → C function pointers, set at init):
    on_navigation_committed(handle, url)
    on_loading_state_changed(handle, state, progress)
    on_cursor_changed(handle, cursor_type)
    on_title_changed(handle, title)
```

### Zig embedder (`browser/`)

Top-level directory in the main repo, separate from `gui/`. Builds a standalone
binary for the experiment phase. If in-process wins, the Zig logic migrates into
`gui/src/` and the standalone binary goes away.

```
browser/
├── build.zig          — Build system, links against C++ shim
├── src/
│   ├── main.zig       — Entry point, initializes Content API
│   ├── profile.zig    — BrowserContext lifecycle
│   ├── tab.zig        — WebContents lifecycle
│   └── callbacks.zig  — Handles Content API callbacks
```

### Directory layout

```
~/dev/termsurf/
├── browser/                                    ← Zig embedder (main repo)
│   ├── build.zig
│   └── src/*.zig
├── chromium/src/content/zig_content_shell/     ← C++ shim (Chromium fork, 3 files)
│   ├── BUILD.gn
│   ├── content_api_shim.h
│   └── content_api_shim.cc
├── gui/                                        ← TermSurf GUI (Ghostty fork)
└── tui/                                        ← web TUI (Rust/ratatui)
```

### Build

Step 1 — Build the C++ shim (produces shared library in `out/Default/`):

```bash
cd chromium/src
autoninja -C out/Default zig_content_shell
```

Step 2 — Build the Zig embedder (links against the shim):

```bash
cd browser
zig build
```

### Display path

The Zig Content Shell does NOT use `FrameSinkVideoCapturer`. It uses Content
Shell's normal display path:

1. Content API renders into a `CAContext` (GPU process)
2. `AcceleratedWidgetMac` receives `CALayerParams` with `ca_context_id`
3. The C++ shim forwards the `ca_context_id` to Zig via callback
4. For the standalone experiment: create a window with a `CALayerHost`
5. For in-process (future): pass the `ca_context_id` to the GUI's Metal renderer

No frame capture. No IOSurface Mach port transfer. No recording API. The same
display path Chrome uses.

## Chromium branch

`146.0.7650.0-issue-620` — branched from the vanilla `146.0.7650.0` tag. This
experiment only adds new files and depends on unmodified Content Shell classes
(`Shell`, `ShellBrowserContext`, `ShellPlatformDelegate`,
`ShellContentBrowserClient`), so no TermSurf-specific Chromium modifications are
needed.

## Experiments

### Experiment 1: C shim with C main, one profile, one page

Prove that the Content API can be driven through a C function boundary. Write a
C++ shim that wraps `ContentMain()` as a C function, and a `main.c` that calls
it. If a web page loads in a window, the C API architecture works.

For this first experiment, the shim reuses Content Shell's existing classes
internally (`Shell`, `ShellBrowserContext`, `ShellPlatformDelegate`,
`ShellContentBrowserClient`). The caller sees only C functions. Later
experiments replace Content Shell's classes with minimal custom implementations.

#### Files

**`chromium/src/content/zig_content_shell/content_api_shim.h`** — C header:

```c
#ifndef CONTENT_ZIG_CONTENT_SHELL_CONTENT_API_SHIM_H_
#define CONTENT_ZIG_CONTENT_SHELL_CONTENT_API_SHIM_H_

#ifdef __cplusplus
extern "C" {
#endif

// Initialize the Content API, create a browser window, load the URL, and run
// the message loop. Blocks until the window is closed. Returns exit code.
int ts_content_main(int argc, const char** argv, const char* url);

#ifdef __cplusplus
}
#endif

#endif  // CONTENT_ZIG_CONTENT_SHELL_CONTENT_API_SHIM_H_
```

**`chromium/src/content/zig_content_shell/content_api_shim.cc`** — C++
implementation:

The shim defines three classes that override Content Shell's defaults:

1. `TsBrowserMainParts` — Inherits from `ShellBrowserMainParts`. Overrides
   `InitializeMessageLoopContext()` to create a `Shell` window with the URL
   passed to `ts_content_main()` (stored in a global). Skips Content Shell's
   default behavior (which reads the URL from command-line flags).

2. `TsContentBrowserClient` — Inherits from `ShellContentBrowserClient`.
   Overrides `CreateBrowserMainParts()` to return `TsBrowserMainParts` instead
   of `ShellBrowserMainParts`.

3. `TsMainDelegate` — Inherits from `ShellMainDelegate`. Overrides
   `CreateContentBrowserClient()` to return `TsContentBrowserClient`.

The `ts_content_main()` function stores the URL, creates `TsMainDelegate`,
populates `ContentMainParams`, and calls `ContentMain()`.

Key implementation details:

```cpp
static std::string g_initial_url;

class TsBrowserMainParts : public content::ShellBrowserMainParts {
 protected:
  void InitializeMessageLoopContext() override {
    content::Shell::CreateNewWindow(browser_context(),
                                    GURL(g_initial_url),
                                    nullptr, gfx::Size());
  }
};

class TsContentBrowserClient : public content::ShellContentBrowserClient {
 public:
  std::unique_ptr<content::BrowserMainParts> CreateBrowserMainParts(
      bool is_integration_test) override {
    auto parts = std::make_unique<TsBrowserMainParts>();
    set_browser_main_parts(parts.get());
    return parts;
  }
};

class TsMainDelegate : public content::ShellMainDelegate {
 protected:
  content::ContentBrowserClient* CreateContentBrowserClient() override {
    browser_client_ = std::make_unique<TsContentBrowserClient>();
    return browser_client_.get();
  }
 private:
  std::unique_ptr<TsContentBrowserClient> browser_client_;
};

extern "C" int ts_content_main(int argc, const char** argv, const char* url) {
  g_initial_url = url ? url : "about:blank";
  TsMainDelegate delegate;
  content::ContentMainParams params(&delegate);
  params.argc = argc;
  params.argv = argv;
  return content::ContentMain(std::move(params));
}
```

**`chromium/src/content/zig_content_shell/main.c`** — Pure C entry point:

```c
#include "content/zig_content_shell/content_api_shim.h"

int main(int argc, const char** argv) {
  return ts_content_main(argc, argv, "https://google.com");
}
```

This file is pure C — no C++ includes. It proves the C function boundary works.

**`chromium/src/content/zig_content_shell/BUILD.gn`** — Build target:

Follows the `chromium_profile_server` pattern. The executable depends on
`//content/shell:content_shell_lib` (for `Shell`, `ShellBrowserContext`,
`ShellPlatformDelegate`, etc.) and the Content API public targets. On macOS,
uses `mac_app_bundle()` to produce a `.app` bundle (required for `NSApplication`
lifecycle).

Sources: `main.c` and `content_api_shim.cc`.

#### Build

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
gn gen out/Default
autoninja -C out/Default zig_content_shell
```

#### Verification

1. Run the built app:
   ```bash
   open chromium/src/out/Default/Zig\ Content\ Shell.app
   ```
2. A window appears showing google.com
3. The page is interactive — links are clickable, text is selectable, scrolling
   works
4. Closing the window exits the process

If the page loads and is interactive, the C API boundary works. The Content API
is successfully driven from a C `main()` through the shim.

#### Implementation notes

The macOS framework architecture requires the dlopen/dlsym pattern — a plain
`main.c` cannot work. The actual implementation:

- Reuses `shell_main_mac.cc` for the app and helper entry points (identical to
  Content Shell's launcher — it dlopen's the framework and dlsym's
  `ContentMain`)
- The framework contains `content_api_shim.mm` (Objective-C++ for NSBundle
  access) which exports `ContentMain()` as `extern "C"`
- Path override functions use `Ts` prefix (`TsOverrideFrameworkBundlePath`,
  etc.) to avoid symbol conflicts with Content Shell's identically-structured
  functions compiled into `content_shell_app`
- The URL is hardcoded as a constant (`kInitialUrl = "https://google.com"`)
  rather than passed as a parameter, since this experiment only needs to prove
  the C boundary works

The C function boundary is proven by the dlsym call — `shell_main_mac.cc` loads
the framework at runtime and calls `ContentMain` through a C function pointer.

#### Result

**Pass.**

The app launched successfully with the full Chromium multi-process architecture:
main browser process, GPU process, network service, storage service, and two
renderer processes. Google.com loaded in a Content Shell window. The page is
interactive — links clickable, text selectable, scrolling works.

#### Conclusion

The Content API can be driven through a C function boundary. The dlopen/dlsym
pattern (inherited from Content Shell's macOS architecture) is itself the C
boundary — the framework exports `ContentMain` as `extern "C"`, and the launcher
calls it via `dlsym`. The three-class subclassing pattern (`TsMainDelegate` →
`TsContentBrowserClient` → `TsBrowserMainParts`) cleanly overrides the
initialization chain while reusing all of Content Shell's infrastructure.

### Experiment 2: Two profiles, two windows

Prove that two `BrowserContext` instances with different storage paths can
coexist in the same process. This is the critical experiment — if both windows
render and are interactive, in-process browser embedding is viable and the
entire multi-process architecture (xpc-gateway, profile server spawning, XPC
connections, IOSurface Mach port transfer, frame capture, 120fps oversampling)
goes away.

ts4 proved this works in native C++ (Issues 406–413). This experiment confirms
it works through the C shim boundary.

#### Changes

**`chromium/src/content/zig_content_shell/content_api_shim.mm`:**

1. Add include for `content/shell/common/shell_paths.h`
2. Replace `kInitialUrl` with two URL constants:
   - `kProfileAUrl = "https://google.com"`
   - `kProfileBUrl = "https://example.com"`
3. Add profile path constants using `~/.config/termsurf/zig-content-shell/`:
   - Profile A: `~/.config/termsurf/zig-content-shell/profile-a/`
   - Profile B: `~/.config/termsurf/zig-content-shell/profile-b/`
4. Add `browser_context_b_` member to `TsBrowserMainParts`
5. Override `InitializeBrowserContexts()`:
   ```cpp
   void InitializeBrowserContexts() override {
     // Profile A
     base::FilePath home;
     base::PathService::Get(base::DIR_HOME, &home);
     base::FilePath base_path = home.Append(".config")
                                    .Append("termsurf")
                                    .Append("zig-content-shell");

     base::PathService::Override(
         SHELL_DIR_USER_DATA, base_path.Append("profile-a"));
     set_browser_context(new content::ShellBrowserContext(false));

     // Profile B
     base::PathService::Override(
         SHELL_DIR_USER_DATA, base_path.Append("profile-b"));
     browser_context_b_.reset(new content::ShellBrowserContext(false));
   }
   ```
6. Modify `InitializeMessageLoopContext()` to create two windows:
   ```cpp
   void InitializeMessageLoopContext() override {
     content::Shell::CreateNewWindow(browser_context(),
                                     GURL(kProfileAUrl),
                                     nullptr, gfx::Size());
     content::Shell::CreateNewWindow(browser_context_b_.get(),
                                     GURL(kProfileBUrl),
                                     nullptr, gfx::Size());
   }
   ```
7. Override `PostMainMessageLoopRun()` to destroy profile B context before
   parent cleanup:
   ```cpp
   void PostMainMessageLoopRun() override {
     browser_context_b_.reset();
     ShellBrowserMainParts::PostMainMessageLoopRun();
   }
   ```

No changes to `BUILD.gn`, `content_api_shim.h`, or the plist files.

#### Verification

1. Build and run:
   ```bash
   cd ~/dev/termsurf/chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default zig_content_shell
   open out/Default/Zig\ Content\ Shell.app
   ```
2. Two windows appear — one showing google.com, one showing example.com
3. Both windows are interactive (scrolling, clicking, typing work in both)
4. Both render without obvious stuttering or freezing
5. Profile storage directories were created:
   ```bash
   ls ~/.config/termsurf/zig-content-shell/profile-a/
   ls ~/.config/termsurf/zig-content-shell/profile-b/
   ```
6. Closing both windows exits the process

If both windows render and are interactive, multi-profile in-process embedding
works through the C shim. This confirms the architecture direction: the GUI
binary becomes the browser process, eliminating the entire out-of-process
streaming stack.

#### Result

**Pass.**

Two windows appeared — google.com in one, example.com in the other. Both
rendered and were interactive. The process spawned the full multi-process
architecture: main browser, GPU, network, storage, and four renderer processes
(two per profile). Both profile directories were created with isolated storage:

```
~/.config/termsurf/zig-content-shell/profile-a/   (13 entries)
~/.config/termsurf/zig-content-shell/profile-b/   (10 entries)
```

Each has its own Code Cache, Local Storage, GPUCache, DIPS database, etc. No
stuttering or freezing observed.

#### Conclusion

Two `BrowserContext` instances with different storage paths coexist in the same
process through the C shim. This confirms the ts4 finding (Issues 406–413) and
answers the critical architecture question: **in-process embedding is viable.**
The GUI binary can become the browser process, eliminating the entire
out-of-process stack (xpc-gateway, profile server, XPC connections, IOSurface
Mach port transfer, FrameSinkVideoCapturer, 120fps oversampling).
