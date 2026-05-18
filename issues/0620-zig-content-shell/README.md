+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

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

### Experiment 3: C API for profile and tab lifecycle

Expose C functions for creating and destroying browser profiles and tabs. Prove
the API works by having `InitializeMessageLoopContext()` call the exported C
functions instead of directly using Content Shell C++ classes. Same observable
result as Experiment 2 (two windows, two profiles), but the creation path goes
through the public C API.

This is the foundation for the Zig embedder. Once these functions exist, Zig
calls them to manage browser profiles and tabs without any C++ knowledge.

#### Changes

**`content_api_shim.h`** — Add exported C function declarations:

```c
// Opaque handle type for a browser profile.
typedef void* ts_browser_context_t;

// Create a browser profile with the given absolute storage path.
// Must be called on the browser thread (i.e., from the initialized callback
// or after ContentMain has started the message loop).
ts_browser_context_t ts_create_browser_context(const char* path);

// Destroy a browser profile.
void ts_destroy_browser_context(ts_browser_context_t ctx);

// Create a tab in the given profile, loading the URL.
// Opens a Content Shell window (temporary — later experiments replace Shell
// with a headless WebContents and CAContextID extraction).
void ts_create_tab(ts_browser_context_t ctx, const char* url);
```

**`content_api_shim.mm`** — Implement the C functions and simplify
TsBrowserMainParts:

1. Global storage: `std::vector<std::unique_ptr<ShellBrowserContext>>` owns all
   contexts created through the C API. Handles are raw pointers into this
   vector.

2. `ts_create_browser_context(path)` — Override `SHELL_DIR_USER_DATA` with the
   given path, create a `ShellBrowserContext(false)`, store in global vector,
   return raw pointer as opaque handle.

3. `ts_destroy_browser_context(handle)` — Find and erase from global vector.

4. `ts_create_tab(ctx, url)` — Cast handle to `ShellBrowserContext*`, call
   `Shell::CreateNewWindow(ctx, GURL(url), nullptr, gfx::Size())`.

5. Override `PreMainMessageLoopRun()` instead of just the sub-methods. The
   parent's implementation calls
   `ShellDevToolsManagerDelegate::StartHttpHandler` with
   `browser_context_.get()` which would be null (we don't use
   `set_browser_context`). Also, `PlatformResourceProvider` is a static function
   in the parent's `.cc` file — inaccessible from subclasses. Our override:

   ```cpp
   int PreMainMessageLoopRun() override {
     Shell::Initialize(CreateShellPlatformDelegate());
     InitializeMessageLoopContext();
     return 0;
   }
   ```

   Skips `InitializeBrowserContexts()` (empty), resource provider (net error
   pages — not needed), and DevTools handler (not needed for experiment).

6. `InitializeMessageLoopContext()` uses the C API:

   ```cpp
   void InitializeMessageLoopContext() override {
     base::FilePath home;
     base::PathService::Get(base::DIR_HOME, &home);
     std::string base_path =
         home.Append(".config/termsurf/zig-content-shell").value();

     ctx_a_ = ts_create_browser_context(
         (base_path + "/profile-a").c_str());
     ctx_b_ = ts_create_browser_context(
         (base_path + "/profile-b").c_str());
     ts_create_tab(ctx_a_, "https://google.com");
     ts_create_tab(ctx_b_, "https://example.com");
   }
   ```

7. Override `PostMainMessageLoopRun()` — destroy contexts via the C API (parent
   would try to stop DevTools and destroy its own null context):

   ```cpp
   void PostMainMessageLoopRun() override {
     ts_destroy_browser_context(ctx_b_);
     ts_destroy_browser_context(ctx_a_);
     ctx_a_ = nullptr;
     ctx_b_ = nullptr;
   }
   ```

No changes to `BUILD.gn` or the plist files.

#### Verification

Same as Experiment 2:

1. Two windows appear (google.com + example.com)
2. Both interactive (scrolling, clicking, typing)
3. Profile directories created
   (`~/.config/termsurf/zig-content-shell/profile-{a,b}/`)
4. Closing both windows exits the process

The key difference is architectural: all profile and tab creation goes through
the exported C functions declared in `content_api_shim.h`. A future experiment
calls these same functions from Zig.

#### Implementation notes

The initial attempt overrode `PreMainMessageLoopRun()` entirely (skipping the
parent's implementation). This crashed at
`StoragePartitionImpl::GetStorageService()` because the parent's
`PreMainMessageLoopRun()` sets up infrastructure that `WebContents` creation
depends on (DevTools, resource provider, etc.).

The fix: don't override `PreMainMessageLoopRun()`. Override
`InitializeBrowserContexts()` and `InitializeMessageLoopContext()` instead — the
parent handles all infrastructure setup.

Ownership model: `ts_create_browser_context` returns a caller-owned handle (raw
`new`). Profile A is transferred to the parent via `set_browser_context()` (the
parent's `unique_ptr` owns it and destroys it in `PostMainMessageLoopRun`).
Profile B stays caller-owned and is destroyed via `ts_destroy_browser_context`
before the parent cleans up.

The header uses `#ifdef __cplusplus` / `extern "C"` guards so it's valid as both
a C and C++ header — Zig can include it directly.

#### Result

**Pass.**

Two windows appeared (google.com + example.com), both interactive, both with
isolated profile storage. 8 processes running (main + GPU + network + storage +
4 renderers). Profile directories freshly created:

```
~/.config/termsurf/zig-content-shell/profile-a/   (13 entries)
~/.config/termsurf/zig-content-shell/profile-b/   (10 entries)
```

#### Conclusion

The C API works: `ts_create_browser_context(path)` creates profiles,
`ts_create_tab(ctx, url)` opens windows, `ts_destroy_browser_context(ctx)`
cleans up. The functions are exported with `extern "C"` and default visibility —
callable from Zig via `@cImport`. The header is a valid C header (no C++ types
in the public API, only opaque `void*` handles).

### Experiment 4: Callback-driven initialization from a custom launcher

Add lifecycle callbacks to the C API (`ts_set_on_initialized`,
`ts_set_on_shutdown`). Write a custom launcher (`ts_main.mm`) that replaces
`shell_main_mac.cc` — it dlopen's the framework, dlsym's all C API symbols,
registers callbacks, and calls ContentMain. The callbacks create and destroy
profiles and tabs using only dlsym'd C function pointers.

This proves the full external-control pattern: the launcher has zero Chromium
headers, zero C++ knowledge — only C function pointers obtained via dlsym. Zig
does exactly the same thing.

#### Changes

**`content_api_shim.h`** — Add callback API:

```c
typedef void (*ts_callback_t)(void);

// Register a callback that fires once when the browser is ready.
// Must be called before ContentMain. The callback should create
// profiles and tabs via the C API.
void ts_set_on_initialized(ts_callback_t callback);

// Register a callback that fires during shutdown, before contexts
// are destroyed. The callback should call ts_destroy_browser_context
// on any contexts it created.
void ts_set_on_shutdown(ts_callback_t callback);
```

**`content_api_shim.mm`** — Wire callbacks into the initialization chain:

1. Global callback pointers (`g_on_initialized`, `g_on_shutdown`).
2. `ts_set_on_initialized` / `ts_set_on_shutdown` — store the pointers.
3. `TsBrowserMainParts::InitializeBrowserContexts()` — create a default context
   for parent infrastructure (DevTools needs a non-null `browser_context_`). The
   real profiles are created by the callback.
4. `TsBrowserMainParts::InitializeMessageLoopContext()` — fire
   `g_on_initialized`.
5. `TsBrowserMainParts::PostMainMessageLoopRun()` — fire `g_on_shutdown`, then
   call parent (which destroys the default context).
6. Remove hardcoded profile paths and URLs from the shim — all application logic
   moves to the launcher.

**`ts_main.mm`** — Custom launcher (new file, ~60 lines):

Pure C with dlopen/dlsym. No Chromium headers, no C++ includes.

```c
// Function pointer types
typedef int (*ContentMainFn)(int, const char**);
typedef void (*SetCallbackFn)(void (*)(void));
typedef void* (*CreateContextFn)(const char*);
typedef void (*DestroyContextFn)(void*);
typedef void (*CreateTabFn)(void*, const char*);

// dlsym'd function pointers
static CreateContextFn  g_create_ctx;
static DestroyContextFn g_destroy_ctx;
static CreateTabFn      g_create_tab;

// Context handles
static void* ctx_a;
static void* ctx_b;

static void on_initialized(void) {
  ctx_a = g_create_ctx("/Users/.../profile-a");
  ctx_b = g_create_ctx("/Users/.../profile-b");
  g_create_tab(ctx_a, "https://google.com");
  g_create_tab(ctx_b, "https://example.com");
}

static void on_shutdown(void) {
  g_destroy_ctx(ctx_b);
  g_destroy_ctx(ctx_a);
}

int main(int argc, const char** argv) {
  // ... find framework path, dlopen ...
  // ... dlsym all symbols ...
  set_on_initialized(on_initialized);
  set_on_shutdown(on_shutdown);
  int rv = content_main(argc, argv);
  exit(rv);
}
```

The framework path logic mirrors `shell_main_mac.cc` but is self-contained (no
`SHELL_PRODUCT_NAME` define — the framework name is hardcoded).

**`BUILD.gn`** — Change the main app bundle source:

```gn
mac_app_bundle("zig_content_shell") {
  sources = [ "ts_main.mm" ]   # was shell_main_mac.cc
  # Remove SHELL_PRODUCT_NAME define (not needed)
  # Remove content_shell_app dep (not needed)
  ...
}
```

Helper apps keep using `shell_main_mac.cc` unchanged.

#### Verification

1. Two windows appear (google.com + example.com)
2. Both interactive
3. Profile directories created
4. Closing both windows exits cleanly (no crash, no leak)
5. The launcher (`ts_main.mm`) has zero `#include` of Chromium headers — only
   `<dlfcn.h>`, `<stdio.h>`, and platform headers for path resolution

The key proof: application logic (which profiles, which URLs) lives entirely in
the launcher. The framework is a generic Content API service layer. A Zig
launcher would use the identical dlsym pattern.

#### Result

**Pass.**

Two windows appeared (google.com + example.com), both interactive. The custom
launcher (`ts_main.mm`) uses only system C headers — zero Chromium `#include`s.
All application logic (profile paths, URLs) lives in the launcher's callbacks.
The framework is a generic service layer: dlopen it, dlsym six symbols, register
two callbacks, call ContentMain.

Profile directories:

```
~/.config/termsurf/zig-content-shell/profile-a/   (13 entries)
~/.config/termsurf/zig-content-shell/profile-b/   (10 entries)
~/.config/termsurf/zig-content-shell/default/      (DevTools only)
```

8 processes running: main + GPU + network + storage + 4 renderers. No crashes on
window close.

#### Conclusion

The full external-control pattern works. A launcher with zero Chromium knowledge
can drive the Content API entirely through dlsym'd C function pointers:
`ContentMain`, `ts_set_on_initialized`, `ts_set_on_shutdown`,
`ts_create_browser_context`, `ts_destroy_browser_context`, `ts_create_tab`. The
`on_initialized` callback fires after the message loop is ready; the
`on_shutdown` callback fires before teardown. Zig does exactly this — `@cImport`
the header (or just declare the externs) and call the same six functions.

### Experiment 5: Direct WebContents with launcher-created windows

Stop using `Shell::CreateNewWindow`. Create `WebContents` directly via the
Content API and render it in an NSWindow created by the launcher. The framework
becomes a headless WebContents factory — the launcher owns all window
management.

This is the key architectural shift. Shell provides an NSWindow + toolbar +
navigation buttons. We don't want any of that. We want the raw `WebContents`
NSView, which we attach to our own window (and in the future, to a Zig-managed
Metal surface).

#### Changes

**`content_api_shim.h`** — Add new types and functions:

```c
// Opaque handle for a WebContents (page).
typedef void* ts_web_contents_t;

// Create a WebContents in the given profile, loading the URL.
// Does NOT create a window — caller must attach the native view
// to their own window via ts_get_native_view.
ts_web_contents_t ts_create_web_contents(ts_browser_context_t ctx,
                                         const char* url);

// Get the native view (NSView* on macOS) for a WebContents.
// The returned pointer is a valid NSView* that can be added as a
// subview to any NSWindow's contentView.
void* ts_get_native_view(ts_web_contents_t contents);

// Destroy a WebContents.
void ts_destroy_web_contents(ts_web_contents_t contents);

// Quit the message loop. Call this when all windows are closed.
void ts_quit(void);
```

Keep `ts_create_tab` for backward compatibility (it still uses Shell
internally).

**`content_api_shim.mm`** — Implement the new functions:

1. Capture the quit closure. Override `WillRunMainMessageLoop` in
   `TsBrowserMainParts` to store the run loop's quit closure in a global:

   ```cpp
   static base::OnceClosure g_quit_closure;

   void WillRunMainMessageLoop(
       std::unique_ptr<base::RunLoop>& run_loop) override {
     g_quit_closure = run_loop->QuitClosure();
     ShellBrowserMainParts::WillRunMainMessageLoop(run_loop);
   }
   ```

2. `ts_create_web_contents(ctx, url)` — Create a `WebContents` directly:

   ```cpp
   auto* browser_context = static_cast<ShellBrowserContext*>(ctx);
   WebContents::CreateParams params(browser_context);
   auto* wc = WebContents::Create(params).release();
   // Load URL via NavigationController
   NavigationController::LoadURLParams load_params(GURL(url));
   load_params.transition_type = ui::PAGE_TRANSITION_TYPED;
   wc->GetController().LoadURLWithParams(load_params);
   return static_cast<ts_web_contents_t>(wc);
   ```

3. `ts_get_native_view(contents)` — Extract the NSView:

   ```cpp
   auto* wc = static_cast<WebContents*>(contents);
   return (__bridge void*)wc->GetNativeView().GetNativeNSView();
   ```

4. `ts_destroy_web_contents(contents)` — Delete:

   ```cpp
   delete static_cast<WebContents*>(contents);
   ```

5. `ts_quit()` — Run the stored quit closure:

   ```cpp
   if (g_quit_closure)
     std::move(g_quit_closure).Run();
   ```

New includes needed: `content/public/browser/navigation_controller.h`,
`ui/base/page_transition_types.h`.

**`ts_main.mm`** — Create NSWindows and attach WebContents views:

1. Add `#import <Cocoa/Cocoa.h>` (system header, not Chromium).

2. Define an NSWindowDelegate that tracks open window count:

   ```objc
   static int g_open_windows = 0;
   static void (*g_quit_fn)(void) = NULL;

   @interface TsWindowDelegate : NSObject <NSWindowDelegate>
   @end

   @implementation TsWindowDelegate
   - (void)windowWillClose:(NSNotification*)notification {
     if (--g_open_windows == 0 && g_quit_fn)
       g_quit_fn();
   }
   @end
   ```

3. Add dlsym for the new symbols: `ts_create_web_contents`,
   `ts_get_native_view`, `ts_destroy_web_contents`, `ts_quit`.

4. Replace the `on_initialized` callback: instead of calling `ts_create_tab`
   (which creates Shell windows), call `ts_create_web_contents` to get handles,
   `ts_get_native_view` to get NSViews, create NSWindows manually, and attach:

   ```objc
   static void on_initialized(void) {
     // ... build profile paths ...
     ctx_a = g_create_ctx(path_a);
     ctx_b = g_create_ctx(path_b);

     wc_a = g_create_web_contents(ctx_a, "https://google.com");
     wc_b = g_create_web_contents(ctx_b, "https://example.com");

     create_window(g_get_native_view(wc_a), "Profile A");
     create_window(g_get_native_view(wc_b), "Profile B");
   }
   ```

   `create_window` creates an NSWindow, sets the TsWindowDelegate, adds the
   NSView as a subview, and makes the window key.

5. Update `on_shutdown` to destroy WebContents before BrowserContexts:

   ```objc
   static void on_shutdown(void) {
     g_destroy_web_contents(wc_b);
     g_destroy_web_contents(wc_a);
     g_destroy_ctx(ctx_b);
     g_destroy_ctx(ctx_a);
   }
   ```

**`BUILD.gn`** — Add Cocoa framework link for the launcher:

```gn
mac_app_bundle("zig_content_shell") {
  ...
  frameworks = [ "Cocoa.framework" ]
}
```

#### Verification

1. Two windows appear — one showing google.com, one showing example.com
2. Window titles say "Profile A" and "Profile B" (not "Content Shell")
3. No Content Shell toolbar, URL bar, or navigation buttons — just the web page
4. Both windows are interactive (scrolling, clicking, typing)
5. Closing one window leaves the other running
6. Closing the last window exits the process cleanly
7. `ts_main.mm` still has zero Chromium `#include`s — only system headers
   (`<dlfcn.h>`, `<Cocoa/Cocoa.h>`, etc.)

The key proof: window management lives entirely in the launcher. The framework
creates WebContents and hands back an NSView. The launcher decides where and how
to display it. A Zig launcher would do the same thing — create an NSWindow via
`@cImport("Cocoa/Cocoa.h")`, get an NSView from the framework, add it as a
subview.

#### Result

**Partial.** Windows appear, pages load, both profiles have isolated storage,
input works (typing, clicking, scrolling). But rendering runs at ~2fps — classic
Chromium background throttling.

The 2fps was introduced in this experiment. Experiments 1–4 all used
`Shell::CreateNewWindow`, which handles visibility and focus management
internally, and rendered at full speed. The direct `WebContents::Create` path
bypasses Shell entirely, and Chromium throttles the compositor because it
believes the content is in a background state.

Attempted fix: calling `wc->WasShown()` immediately after `WebContents::Create`
and `LoadURLWithParams`. This did not resolve the throttling.

#### Conclusion

Creating `WebContents` directly works — pages load, input is handled, profiles
are isolated. But Shell provides visibility/focus management that we haven't
replicated. Without it, Chromium's compositor throttles to ~2fps.

The throttling is not a mutex or lock contention issue. It is Chromium's
intentional power-saving behavior for content it considers "not visible" or
"background." `WasShown()` alone is not sufficient — Shell does more than just
signal visibility.

**Ideas for next steps:**

1. **Study what Shell does for visibility.** Shell implements
   `WebContentsDelegate` which has methods like `ActivateContents`,
   `IsNeverComposited`, `ShouldCreateWebContents`. One of these may control
   compositor throttling. Compare what Shell's delegate does vs having no
   delegate.

2. **Set a minimal WebContentsDelegate.** Create a `TsWebContentsDelegate` in
   the shim that inherits from `WebContentsDelegate` and implements the bare
   minimum. Attach it to each WebContents. This may be all that's needed to
   convince Chromium the content is foreground.

3. **Check RenderWidgetHostView visibility.** The `RenderWidgetHostView` (the
   actual rendering surface) has its own visibility state independent of
   `WebContents`. Shell may be doing something to the RWHV that we're not.

4. **Compare with Shell::CreateNewWindow path.** Instrument both paths — the
   Shell path (which works at 60fps) and our direct path — to see exactly which
   visibility/focus calls differ. The delta is the fix.

5. **Check `WebContentsViewMac` visibility notifications.** The Cocoa view
   bridge should detect when the NSView moves into a visible window and
   propagate visibility. Investigate whether this propagation is failing or
   happening too late.

### Experiment 6: Single WebContents isolation test

Isolate the 2fps throttling from Experiment 5. Open a single WebContents
(google.com) with a single profile instead of two. If the single WebContents
also renders at 2fps, the problem is fundamental to the direct
`WebContents::Create` path. If it renders at full speed, the problem is related
to multiple WebContents or multiple profiles competing for visibility.

#### Changes

**`ts_main.mm`** only — reduce to one profile, one WebContents, one window:

1. Remove `ctx_b`, `wc_b`, `path_b` — only create profile A.
2. `on_initialized`: create one context, one WebContents (google.com), one
   window titled "Profile A".
3. `on_shutdown`: destroy one WebContents, one context.

No changes to the framework (`content_api_shim.h`, `content_api_shim.mm`,
`BUILD.gn`).

#### Verification

1. One window appears showing google.com
2. Check rendering speed — smooth (60fps) or throttled (~2fps)?
3. Page is interactive (scrolling, clicking, typing)

If 2fps: the throttling is inherent to direct `WebContents::Create` without
Shell. The fix must address visibility/delegate management in the shim.

If 60fps: the throttling is caused by multiple WebContents or profiles competing
for foreground state. The fix is different — focus/activation management between
multiple contents.

#### Result

**Pass.** Single WebContents also renders at ~2fps. This confirms the throttling
is inherent to the direct `WebContents::Create` path — not caused by multiple
WebContents or profiles competing for visibility.

#### Conclusion

The 2fps throttling is fundamental to bypassing Shell. Even a single WebContents
in a single profile throttles. The problem is not contention between multiple
views — it is something Shell does during creation or lifecycle management that
our direct path omits. The next experiment should focus on what Shell provides
that we don't: likely `WebContentsDelegate`, `RenderWidgetHostView` visibility,
or the `ShellPlatformDelegate` setup.

### Experiment 7: Replay Experiment 4 to verify baseline FPS

Before investigating the 2fps cause, confirm that Experiment 4's
`Shell::CreateNewWindow` path actually renders at full speed. Experiments 1–4
were marked as passing based on page loading and interactivity, but FPS was
never explicitly tested. If Experiment 4 also runs at 2fps, the problem predates
Experiment 5 and the diagnosis changes entirely.

#### Changes

**`ts_main.mm`** only — revert to using `ts_create_tab` (which calls
`Shell::CreateNewWindow`) instead of `ts_create_web_contents`:

1. Restore `ctx_b`, `wc_b` removal — use `ts_create_tab` for both profiles.
2. Remove dlsym of `ts_create_web_contents`, `ts_get_native_view`,
   `ts_destroy_web_contents`, `ts_quit` — not needed.
3. Remove NSWindow creation, `TsWindowDelegate`, `create_window` helper,
   `<Cocoa/Cocoa.h>` import — Shell creates its own windows.
4. `on_initialized`: create two contexts via `ts_create_browser_context`, then
   call `ts_create_tab` for each (google.com + example.com).
5. `on_shutdown`: destroy both contexts.

This is effectively the Experiment 4 launcher with the Experiment 5 framework
(which has the extra exports but doesn't use them). The Shell path is exercised.

No changes to the framework (`content_api_shim.h`, `content_api_shim.mm`,
`BUILD.gn`).

#### Verification

1. Two Content Shell windows appear (with toolbar, URL bar, navigation buttons)
2. Check rendering speed — smooth or ~2fps?
3. Both pages interactive

If smooth: Experiment 4's Shell path is fine, confirming that the 2fps is caused
by the direct `WebContents::Create` path in Experiment 5.

If 2fps: the problem predates Experiment 5 and may be in the framework changes
(callback wiring, default context, `WillRunMainMessageLoop` override, etc.).

#### Result: Pass

Both Shell windows appeared with toolbars and navigation, pages loaded and were
interactive — but rendering was ~2fps, identical to Experiments 5 and 6.

This is the critical finding: the 2fps throttle is **not** caused by the direct
`WebContents::Create` path introduced in Experiment 5. The
Shell::CreateNewWindow path exhibits the same problem. Since Experiments 1–3
used `content_shell`'s original `shell_main_mac.cc` launcher (with no callback
infrastructure), the regression was introduced in Experiment 4 when we replaced
the launcher with `ts_main.mm` and added the callback/lifecycle machinery in the
framework.

#### Conclusion

The 2fps throttle predates Experiment 5. It was introduced somewhere in the
Experiment 4 framework changes: the `TsBrowserMainParts` subclass (custom
`InitializeBrowserContexts`, `InitializeMessageLoopContext`,
`WillRunMainMessageLoop`, `PostMainMessageLoopRun`), the default browser context
override, or the callback wiring itself. The next experiment should bisect these
changes by reverting to the original `ShellBrowserMainParts` behavior as closely
as possible while still supporting multiple profiles.

### Experiment 8: Replay Experiment 3 to bisect the regression

Experiment 7 proved the 2fps throttle exists even with Shell::CreateNewWindow.
Experiment 3 was the last experiment that used the stock `shell_main_mac.cc`
launcher and had no callback infrastructure. If Experiment 3's configuration
renders at full speed, the regression is in the Experiment 4 changes (callback
API, `WillRunMainMessageLoop` override, `GetQuitClosure()`, default context
creation, `ts_main.mm` launcher). If it also shows 2fps, the regression is in
the Experiment 3 framework itself (custom `TsBrowserMainParts` subclass).

This rewinds both the framework and the launcher to Experiment 3's exact state.

#### Changes

**`content_api_shim.h`** — strip back to Experiment 3's API surface:

1. Remove `ts_callback_t`, `ts_set_on_initialized`, `ts_set_on_shutdown`.
2. Remove `ts_web_contents_t`, `ts_create_web_contents`, `ts_get_native_view`,
   `ts_destroy_web_contents`, `ts_quit`.
3. Keep only: `ContentMain`, `ts_browser_context_t`,
   `ts_create_browser_context`, `ts_destroy_browser_context`, `ts_create_tab`.

**`content_api_shim.mm`** — revert to Experiment 3's implementation:

1. Remove `g_on_initialized`, `g_on_shutdown` globals.
2. Remove `GetQuitClosure()` and `#include "base/no_destructor.h"`.
3. Remove `#include "base/run_loop.h"`.
4. Remove `#include "content/public/browser/navigation_controller.h"`,
   `#include "content/public/browser/web_contents.h"`,
   `#include "ui/base/page_transition_types.h"`.
5. `TsBrowserMainParts`: remove `WillRunMainMessageLoop` override entirely.
6. `InitializeBrowserContexts()`: hardcode profile creation (profile-a via
   `set_browser_context`, profile-b stored as member) — no default context, no
   callback.
7. `InitializeMessageLoopContext()`: hardcode `ts_create_tab` calls for both
   profiles.
8. `PostMainMessageLoopRun()`: destroy profile-b, let parent destroy profile-a.
9. Add `ctx_a_`/`ctx_b_` member variables to `TsBrowserMainParts`.
10. Remove `ts_set_on_initialized`, `ts_set_on_shutdown`,
    `ts_create_web_contents`, `ts_get_native_view`, `ts_destroy_web_contents`,
    `ts_quit` implementations.

**`BUILD.gn`** — revert the app bundle to use `shell_main_mac.cc`:

1. Change `sources` from `[ "ts_main.mm" ]` to
   `[ "//content/shell/app/shell_main_mac.cc" ]`.
2. Add `defines = [ "SHELL_PRODUCT_NAME=\"$zig_content_shell_product_name\"" ]`.
3. Add deps: `"//base/allocator:early_zone_registration_apple"`,
   `"//sandbox/mac:seatbelt"`.
4. Add `data_deps = [ "//content/shell:content_shell_app" ]`.
5. Remove `frameworks = [ "Cocoa.framework" ]`.

**`ts_main.mm`** — not used (still exists in the tree but is not compiled).

#### Verification

1. Two Content Shell windows appear (google.com + example.com)
2. Check rendering speed — smooth or ~2fps?
3. Both pages interactive

If smooth: the regression is confirmed in the Experiment 4 changes. The next
experiment bisects within Experiment 4 (callback API, `WillRunMainMessageLoop`,
default context, `ts_main.mm` launcher).

If 2fps: the regression is in the Experiment 3 framework itself (custom
`TsBrowserMainParts` subclass with hardcoded profiles). The next experiment
strips even further back to Experiment 1 or 2.

#### Result: Pass

Rendering was ~2fps — identical to Experiments 5, 6, and 7. The stock
`shell_main_mac.cc` launcher made no difference. The 2fps throttle is present in
the Experiment 3 framework itself.

This means the regression was **never** in the Experiment 4 changes (callback
API, `WillRunMainMessageLoop`, quit closure, `ts_main.mm` launcher). It has been
present since at least Experiment 3 — the first experiment that introduced
`TsBrowserMainParts` with custom `InitializeBrowserContexts()` and multiple
profiles. Experiments 1–3 were all marked as passing based on page loading and
interactivity; FPS was never explicitly tested.

#### Conclusion

The 2fps throttle has been present since the very first experiment that used a
custom `TsBrowserMainParts` subclass. The key suspects are now:

1. **The `TsBrowserMainParts` subclass itself** — overriding
   `InitializeBrowserContexts()` and `PostMainMessageLoopRun()` may break
   assumptions in the parent `ShellBrowserMainParts`.
2. **The `TsMainDelegate` / `TsContentBrowserClient` chain** — subclassing
   `ShellMainDelegate` and `ShellContentBrowserClient` to inject our
   `TsBrowserMainParts` may interfere with initialization order.
3. **Something unrelated to our code** — the vanilla `content_shell` target
   itself may render at 2fps on this system. This has never been tested.

The next experiment should test the unmodified `content_shell` to establish a
true baseline. If `content_shell` also renders at 2fps, the problem is systemic
(build flags, macOS settings, GPU configuration) — not our code.

### Experiment 9: Replay Experiment 2 to continue bisecting

Experiment 8 showed 2fps with Experiment 3's code. Experiment 2 is nearly
identical in structure — same subclass chain (`TsMainDelegate` →
`TsContentBrowserClient` → `TsBrowserMainParts`), same overrides
(`InitializeBrowserContexts`, `InitializeMessageLoopContext`,
`PostMainMessageLoopRun`) — but without the C API wrapper functions
(`ts_create_browser_context`, `ts_destroy_browser_context`, `ts_create_tab`).
Profiles are created directly with `PathService::Override` +
`new
ShellBrowserContext`.

The purpose is to continue the bisection. If Experiment 2 also shows 2fps, the
problem is in the subclass chain or the build itself. If it's smooth, the C API
wrappers somehow introduce the throttle (unlikely but must be eliminated).

#### Changes

**`content_api_shim.h`** — strip to Experiment 2's minimal header:

1. Remove all `ts_*` declarations (`ts_browser_context_t`,
   `ts_create_browser_context`, `ts_destroy_browser_context`, `ts_create_tab`).
2. Remove `#ifdef __cplusplus` guards (Experiment 2 didn't have them — the
   header was C++-only with a bare `extern "C"`).
3. Keep only `ContentMain`.

**`content_api_shim.mm`** — revert to Experiment 2's implementation:

1. Remove all `ts_*` C API function implementations.
2. `TsBrowserMainParts::InitializeBrowserContexts()`: create profiles directly
   with `PathService::Override` + `new ShellBrowserContext` +
   `set_browser_context`. Profile B stored as
   `std::unique_ptr<ShellBrowserContext> browser_context_b_`.
3. `TsBrowserMainParts::InitializeMessageLoopContext()`: call
   `Shell::CreateNewWindow` directly (not through `ts_create_tab`).
4. `TsBrowserMainParts::PostMainMessageLoopRun()`: `browser_context_b_.reset()`,
   then call parent.
5. Add `kProfileAUrl` / `kProfileBUrl` constants.

No changes to `BUILD.gn` (already using `shell_main_mac.cc` from Experiment 8).

#### Verification

1. Two Content Shell windows appear (google.com + example.com)
2. Check rendering speed — smooth or ~2fps?
3. Both pages interactive

If 2fps: the C API wrappers are irrelevant. The problem is in the subclass chain
or the build environment. The next experiment tests vanilla `content_shell`.

If smooth: the C API wrappers somehow cause the throttle (investigate
`PathService::Override` interaction with `ts_create_browser_context`).

#### Result: Pass

Rendering was ~2fps — identical to all previous experiments. The C API wrappers
are confirmed irrelevant. Experiment 2's code is the simplest possible form of
our custom framework: just the subclass chain + two profiles + direct C++ calls.
Still 2fps.

The bisection within our code is exhausted. Every experiment from 2 through 8
shows the same 2fps behavior. The only thing we haven't tested is the vanilla
`content_shell` target itself.

#### Conclusion

The 2fps throttle has been present since Experiment 1 (the very first Zig
Content Shell build). It is not caused by:

- The C API wrappers (Experiment 8 vs 9)
- The callback/lifecycle machinery (Experiment 7 vs 8)
- Direct WebContents::Create vs Shell::CreateNewWindow (Experiments 5–7)
- Multiple profiles vs single profile (Experiment 6)
- The custom launcher `ts_main.mm` vs stock `shell_main_mac.cc` (Experiment 8)

The only remaining hypothesis is that the vanilla `content_shell` target itself
renders at 2fps with these build flags (`is_debug=false`, `symbol_level=0`,
`is_component_build=true`). The next experiment must build and run the
unmodified `content_shell` to establish a true baseline.

### Experiment 10: Replay Experiment 1 — single profile, minimal overrides

Experiment 9 replayed Experiment 2 (two profiles, three overrides). Experiment 1
is the absolute minimum: single profile, single window, only
`InitializeMessageLoopContext` overridden. The parent's default
`InitializeBrowserContexts` and `PostMainMessageLoopRun` run unmodified. This
eliminates the multi-profile path and the `PostMainMessageLoopRun` override as
suspects.

If this also shows 2fps, only the subclass chain itself remains — or the vanilla
`content_shell` target.

#### Changes

**`content_api_shim.h`** — no change (already minimal from Experiment 9).

**`content_api_shim.mm`** — revert to Experiment 1's implementation:

1. Remove `#include "content/shell/browser/shell_browser_context.h"`,
   `#include "content/shell/common/shell_paths.h"`.
2. Remove `kProfileAUrl` / `kProfileBUrl` constants. Add
   `kInitialUrl = "https://google.com"`.
3. `TsBrowserMainParts`: remove `InitializeBrowserContexts()` override entirely
   (parent creates the default profile). Remove `PostMainMessageLoopRun()`
   override entirely (parent handles cleanup).
4. `InitializeMessageLoopContext()`: single `Shell::CreateNewWindow` call with
   `browser_context()` and `kInitialUrl`.
5. Remove `browser_context_b_` member.

No changes to `BUILD.gn` (already using `shell_main_mac.cc` from Experiment 8).

#### Verification

1. One Content Shell window appears (google.com)
2. Check rendering speed — smooth or ~2fps?
3. Page interactive

If 2fps: the subclass chain (`TsMainDelegate` → `TsContentBrowserClient` →
`TsBrowserMainParts`) with even a single trivial override causes the throttle,
or the vanilla `content_shell` itself has the same problem. The next experiment
tests vanilla `content_shell`.

If smooth: the `InitializeBrowserContexts` / `PostMainMessageLoopRun` overrides
in Experiment 2 are the cause.

#### Result: Pass

Rendering was **flawless** — smooth, full-speed, no stuttering. This is the
first experiment since the FPS investigation began (Experiment 5) that renders
at full speed.

#### Conclusion

The regression is between Experiment 1 and Experiment 2. Experiment 1 (single
profile, only `InitializeMessageLoopContext` overridden, parent handles
everything else) renders perfectly. Experiment 2 (two profiles,
`InitializeBrowserContexts` + `PostMainMessageLoopRun` overridden) renders at
2fps.

The diff between Experiment 1 and Experiment 2 is:

1. **`InitializeBrowserContexts()` override** — creates two profiles via
   `PathService::Override` + `new ShellBrowserContext`, calls
   `set_browser_context()` for profile A.
2. **`PostMainMessageLoopRun()` override** — destroys profile B, calls parent.
3. **`InitializeMessageLoopContext()`** — opens two windows instead of one.
4. Two extra includes (`shell_browser_context.h`, `shell_paths.h`).

The next experiment should bisect within this diff: try Experiment 1's code but
with the `InitializeBrowserContexts` override (single profile created
explicitly) to determine whether it's the override itself or the second profile
that causes the throttle.

### Experiment 11: Two windows, same profile

Issue 413 Experiments 5–6 showed that two navigating WebContents from the same
BrowserContext run at 60fps in the `content_shell` clone. This experiment
reproduces that finding in the Zig Content Shell framework: take Experiment 10
(single profile, 60fps) and add a second `Shell::CreateNewWindow` call on the
same profile.

If smooth: confirms the multi-profile contention from Issue 413.4 is the only
2fps cause — same-profile multi-window is fine.

If 2fps: something about our framework (the subclass chain, path overrides)
breaks even same-profile multi-window, which would be a different bug.

#### Changes

**`content_api_shim.mm`** — one-line change to `InitializeMessageLoopContext`:

1. Add a second `Shell::CreateNewWindow` call using `browser_context()` (the
   same parent-owned profile) with a different URL (example.com).

No other changes. Same header, same BUILD.gn, same single BrowserContext.

#### Verification

1. Two Content Shell windows appear (google.com + example.com)
2. Check rendering speed — smooth or ~2fps?
3. Both pages interactive

If smooth: same-profile multi-window works. The next experiment adds a second
BrowserContext (without navigating) to reproduce 413.3, then adds navigation to
reproduce 413.4.

If 2fps: the subclass chain or path overrides break multi-window even within one
profile — investigate.

#### Result: Pass

Both windows rendered at **60fps** — flawless, smooth, fully interactive. Two
Shell windows sharing the same BrowserContext have no contention.

#### Conclusion

Same-profile multi-window works perfectly at 60fps, exactly matching Issue 413
Experiments 5–6. This confirms that the 2fps throttle seen in Experiments 2–9 is
exclusively caused by multi-profile contention — two different BrowserContexts
with navigating WebContents in one process.

The findings so far:

| Configuration         | FPS   | Experiment |
| --------------------- | ----- | ---------- |
| 1 profile, 1 window   | 60fps | 10         |
| 1 profile, 2 windows  | 60fps | 11         |
| 2 profiles, 2 windows | 2fps  | 2–9        |

This reproduces the Issue 413 finding in the Zig Content Shell framework. The
multi-profile contention is the sole cause of the 2fps throttle in this issue.

### Experiment 12: Instrument CVDisplayLink and compositor to find the 2fps cause

The multi-profile 2fps throttle has been reproduced (Experiments 2–9) and the
boundary identified (Experiment 10 vs 11). Code review of the Chromium source
identified a primary suspect: the `CVDisplayLinkMac` vsync lifecycle in
`ui/display/mac/cv_display_link_mac.mm`.

On macOS, vsync is driven by `CVDisplayLink`. Chromium caches one
`CVDisplayLinkMac` per (display_id, thread_id) pair in a global
`DisplayLinkGlobals` singleton. Multiple `ExternalBeginFrameSourceMac` instances
(one per compositor/Display) share the same `CVDisplayLinkMac` by registering
callbacks. When all callbacks are unregistered, `StopDisplayLinkIfNeeded()`
stops the entire `CVDisplayLink` after 12 empty vsyncs.

The hypothesis: when two BrowserContexts create separate compositor chains,
their `ExternalBeginFrameSourceMac` instances toggle `OnNeedsBeginFrames` on/off
independently. If one unregisters while the other needs frames, the shared
`CVDisplayLinkMac`'s callback set shrinks, potentially triggering a stop/restart
cycle that degrades both to ~2fps.

However, the `DelayBasedTimeSource` fallback timer uses `preferred_interval_`
(set to ~16.6ms for 60Hz), so a clean fallback to the timer would still produce
~60fps. The 2fps suggests something more fundamental: either the display link is
repeatedly starting/stopping (thrashing), the begin frame source is stuck in a
bad state, or the contention is elsewhere entirely (e.g., GPU process
scheduling, `HostFrameSinkManager`).

This experiment adds logging to trace the exact flow and find where frames are
lost.

#### Changes

**`ui/display/mac/cv_display_link_mac.mm`** — add logging to lifecycle events:

1. `EnsureDisplayLinkRunning()`: log when CVDisplayLink starts
   (`LOG(ERROR) << "CVDisplayLink START ..."`)
2. `StopDisplayLinkIfNeeded()`: log when CVDisplayLink stops, and log each vsync
   with empty callbacks (`LOG(ERROR) << "CVDisplayLink STOP ..."` and
   `LOG(ERROR) << "CVDisplayLink empty callbacks ..."`)
3. `RegisterCallback()`: log callback registration with callback count
4. `UnregisterCallback()` (in destructor of `VSyncCallbackMac`): log callback
   removal with remaining count
5. `RunCallbacks()`: log callback count on each vsync tick (use `VLOG(1)` to
   avoid flooding — can enable with `--v=1`)

**`components/viz/service/frame_sinks/external_begin_frame_source_mac.cc`** —
add logging to begin frame lifecycle:

1. `OnNeedsBeginFrames()`: log when begin frames are requested/stopped, with the
   `ExternalBeginFrameSourceMac` pointer as identifier
2. `StartBeginFrame()`: log whether using display link or timer fallback
3. `StopBeginFrame()`: log which path (display link unregister or timer stop)
4. `OnDisplayLinkCallback()`: log at `VLOG(1)` level each callback with frame
   time

Use `LOG(ERROR)` for infrequent lifecycle events (start/stop/register) so they
appear in stderr without flags. Use `VLOG(1)` for per-frame events to avoid
flooding.

**`content_api_shim.mm`** — revert to Experiment 2 (two profiles, two windows)
to trigger the 2fps condition. Use the Experiment 9 code (direct C++, no C API
wrappers).

No changes to `content_api_shim.h` or `BUILD.gn`.

#### Verification

1. Build and launch — two Shell windows appear (2fps expected)
2. Capture stderr output (launch from terminal or redirect)
3. Look for:
   - How many `CVDisplayLinkMac` instances are created (one or two?)
   - How many `ExternalBeginFrameSourceMac` callbacks register
   - Whether `StopDisplayLinkIfNeeded` fires and stops the display link
   - Whether `OnNeedsBeginFrames(false)` is called repeatedly (thrashing)
   - The pattern of register/unregister cycles
4. Compare with single-profile launch (Experiment 11) to see the difference

The logs will reveal whether the CVDisplayLink theory is correct or the
contention is elsewhere.

**Result:** Pass

Launched with `--enable-logging=stderr --v=1`, captured 819 `[TS-DIAG]` log
lines in 12 seconds. The CVDisplayLink-level logs did not appear (macOS may be
using `CADisplayLinkMac` instead of `CVDisplayLinkMac` on this OS version), but
the `ExternalBeginFrameSourceMac` logs revealed the exact mechanism.

**The compositor is thrashing.** Three `ExternalBeginFrameSourceMac` instances
were observed:

- `0x75cde7700` — initial compositor for profile A
- `0x75ce48780` — initial compositor for profile B
- `0x75cef1400` — replacement compositor (appears at +500ms)

**Startup phase (452.662–452.788, ~125ms):** Both initial instances start
healthy, receiving `OnDisplayLinkCallback` every ~16.6ms (60fps). Within 75ms,
both begin thrashing — `OnNeedsBeginFrames` toggles true→false→true rapidly.

**Steady-state thrashing pattern (457s onward):** Instance `0x75cef1400` shows a
clear cycle:

| Time    | Event                                 |
| ------- | ------------------------------------- |
| 457.121 | `OnNeedsBeginFrames(true)` → register |
| 457.154 | `OnNeedsBeginFrames(false)` → unreg   |
|         | _(~250ms gap)_                        |
| 457.438 | `OnNeedsBeginFrames(true)` → register |
| 457.471 | `OnNeedsBeginFrames(false)` → unreg   |
|         | _(~300ms gap)_                        |
| 457.771 | `OnNeedsBeginFrames(true)` → register |
| 457.804 | `OnNeedsBeginFrames(false)` → unreg   |
|         | _(~300ms gap)_                        |
| 458.105 | `OnNeedsBeginFrames(true)` → register |
| 458.138 | `OnNeedsBeginFrames(false)` → unreg   |

Each cycle: register for vsync, get 1–2 callbacks (~33ms), immediately
unregister, wait ~300ms, repeat. That's ~3 updates per second — matching the
observed 2fps.

The Chromium source has a TODO at exactly this location:

```cpp
// TODO: Try to prevent constant switching between callback register and
// unregister.
```

**Key finding:** The bottleneck is NOT in the display link layer. The display
link continues running fine. The problem is upstream — something calls
`OnNeedsBeginFrames(false)` after just 1–2 frames, then waits ~300ms before
calling `OnNeedsBeginFrames(true)` again. The caller is likely in
`DisplayScheduler`, `FrameSinkManager`, or the surface aggregation layer.

#### Conclusion

The 2fps throttle is caused by rapid `OnNeedsBeginFrames` thrashing in the
`ExternalBeginFrameSourceMac`. The compositor registers for vsync, draws 1–2
frames, gets told to stop, waits ~300ms, then starts again. The CVDisplayLink
hypothesis was partially correct — the display link itself is fine, but the
begin frame source that sits on top of it is being toggled on/off by something
upstream.

#### Call chain analysis

Tracing the call chain from the logs to the decision-maker:

```
DisplayScheduler::AttemptDrawAndSwap()           [display_scheduler.cc:589]
  → ShouldDraw() returns false
  → StopObservingBeginFrames()
    → BeginFrameSource::RemoveObserver()          [begin_frame_source.cc:538]
      → client_->OnNeedsBeginFrames(false)
        → ExternalBeginFrameSourceMac::OnNeedsBeginFrames(false)
          → StopBeginFrame()
            → vsync_callback_mac_.reset()          ← unregisters from DisplayLink
```

`ShouldDraw()` checks:

```cpp
return needs_draw_ && !output_surface_lost_ && visible_ &&
       !damage_tracker_->root_frame_missing();
```

After a successful `DrawAndSwap()`, `needs_draw_` is cleared. If no new damage
arrives before the next deadline, the compositor stops observing. The ~300ms gap
between cycles is the time waiting for the renderer to produce new damage.

Each profile gets its own `Display` + `DisplayScheduler` +
`ExternalBeginFrameSourceMac`, all sharing the same `DisplayLinkMac` (keyed by
`display_id=2`). The independent thrashing of each compositor degrades the whole
pipeline.

#### Deep dive: the observer–renderer deadlock

Source code analysis revealed a **deadlock between frame observation and frame
production**. The full cycle:

1. **DisplayScheduler draws** → `DrawAndSwap()` succeeds → `needs_draw_ = false`
2. **Next deadline** → `ShouldDraw()` returns false (no new damage yet) →
   `StopObservingBeginFrames()` → removes itself from `BeginFrameSource`
3. **Renderer starves** — `CompositorFrameSinkSupport::OnBeginFrame()` only
   forwards BeginFrames to the renderer when the support is actively observing.
   When the DisplayScheduler stops observing, the renderer also stops receiving
   BeginFrames.
4. **No new frames** — renderer can't produce CompositorFrames without
   BeginFrames → no `SubmitCompositorFrame()` → no
   `SurfaceManager::SurfaceModified()` → no `OnDisplayDamaged()`
5. **Stays stopped** — `needs_draw_` stays false, `root_frame_missing()` may be
   true → `ShouldDraw()` stays false → DisplayScheduler remains idle
6. **~300ms later** — an external event (watchdog timer, input, or forced
   refresh) triggers new damage → `OnDisplayDamaged()` sets `needs_draw_ = true`
   → `MaybeStartObservingBeginFrames()` restarts the cycle
7. **1–2 frames drawn**, then back to step 1

**Why multi-profile triggers this but single-profile doesn't:**

With a single DisplayScheduler, the renderer produces frames fast enough that
new damage always arrives before the next deadline. The scheduler never stops
observing.

With two DisplaySchedulers on two profiles, they **both** stop observing
simultaneously after drawing a frame. This starves **both** renderers at once.
Neither can produce damage to restart the other. The entire pipeline stalls
until an external event (the ~300ms watchdog) forces a restart.

**`root_frame_missing()` path:**

`DisplayDamageTracker::UpdateRootFrameMissing()` checks
`surface->HasActiveFrame()`. If no CompositorFrame has been activated on the
root surface (because the renderer was starved of BeginFrames), this returns
true, which makes `ShouldDraw()` return false, reinforcing the deadlock.

**The fix space:**

The Chromium TODO at the `OnNeedsBeginFrames` call site acknowledges this:

```cpp
// TODO: Try to prevent constant switching between callback register and
// unregister.
```

Potential approaches:

1. **Never stop observing** if renderers have pending surfaces — keep sending
   BeginFrames even when the compositor doesn't need to draw
2. **Decouple renderer BeginFrames from compositor observation** — always
   deliver BeginFrames to the renderer, only gate the draw/composite step
3. **Coordinate multi-display schedulers** — prevent all schedulers from
   stopping simultaneously when they share the same DisplayLinkMac
4. **Add a keepalive** — when StopObservingBeginFrames fires, send one more
   BeginFrame to give the renderer a chance to submit

Key files:

| Component            | File                             | Lines        |
| -------------------- | -------------------------------- | ------------ |
| root_frame_missing   | display_damage_tracker.cc        | 250–253      |
| ShouldDraw gate      | display_scheduler.cc             | 448–453      |
| Observer stop        | display_scheduler.cc             | 437–446, 603 |
| Renderer BeginFrame  | compositor_frame_sink_support.cc | 1138–1233    |
| Renderer observation | compositor_frame_sink_support.cc | 1246–1270    |

### Experiment 13: Prevent DisplayScheduler from stopping observation

#### Background: the full picture

Experiments 10–12 established the multi-profile 2fps boundary and captured
diagnostic logs. The analysis below traces the complete root cause.

**The observer architecture.** Each profile's display creates a
`RootCompositorFrameSinkImpl` (root_compositor_frame_sink_impl.cc) which owns:

- One `ExternalBeginFrameSourceMac` — the vsync source (macOS-specific)
- One `Display` with a `DisplayScheduler` — manages draw timing
- Child `CompositorFrameSinkSupport` instances — one per renderer

Both `DisplayScheduler` and `CompositorFrameSinkSupport` are **observers** of
the same `ExternalBeginFrameSourceMac`. The `ExternalBeginFrameSource` base
class (begin_frame_source.cc:510–540) tracks all observers. When the first
observer is added, it calls `client_->OnNeedsBeginFrames(true)` which registers
the vsync callback. When the last observer is removed, it calls
`client_->OnNeedsBeginFrames(false)` which unregisters it.

**The deadlock mechanism.** After drawing a frame:

1. `DisplayScheduler::AttemptDrawAndSwap()` (display_scheduler.cc:589) calls
   `ShouldDraw()`. This checks four conditions:
   ```cpp
   return needs_draw_ && !output_surface_lost_ && visible_ &&
          !damage_tracker_->root_frame_missing();
   ```
2. After a successful draw, `needs_draw_` is cleared (line 278). If no new
   damage has arrived, `ShouldDraw()` returns false.
3. `AttemptDrawAndSwap()` calls `StopObservingBeginFrames()` (line 603), which
   calls `RemoveObserver()` on the `ExternalBeginFrameSource`.
4. The renderer's `CompositorFrameSinkSupport` manages its own observation
   independently via `UpdateNeedsBeginFramesInternal()` (line 1246). If the
   renderer has no pending work, it also removes itself as an observer.
5. When **both** the DisplayScheduler and all CompositorFrameSinkSupports have
   removed themselves, `observers_.empty()` becomes true →
   `OnNeedsBeginFrames(false)` → the vsync callback is unregistered from the
   DisplayLink.
6. Without vsync callbacks, no BeginFrames are generated. Without BeginFrames,
   the renderer cannot produce new CompositorFrames. Without new frames, no
   damage arrives. Without damage, `needs_draw_` stays false. **The pipeline is
   stalled.**
7. ~300ms later, an external event (watchdog, input, or timer) triggers new
   damage → `OnDisplayDamaged()` → `needs_draw_ = true` →
   `MaybeStartObservingBeginFrames()` → the cycle restarts for 1–2 frames.

**Why single-profile works.** With one profile, the renderer produces frames
fast enough that new damage always arrives before the deadline. The
DisplayScheduler never calls `StopObservingBeginFrames()` because `needs_draw_`
is always true when checked.

**Why multi-profile deadlocks.** With two profiles, each has an independent
DisplayScheduler. After drawing, both stop observing simultaneously. Both
renderers lose BeginFrames. Neither can produce new frames to restart the other.
The entire pipeline stalls until an external event forces a restart.

**The `root_frame_missing()` path.**
`DisplayDamageTracker::UpdateRootFrameMissing()` (display_damage_tracker.cc:250)
checks whether the root surface has an active CompositorFrame:

```cpp
Surface* surface = surface_manager_->GetSurfaceForId(root_surface_id_);
SetRootFrameMissing(!surface || !surface->HasActiveFrame());
```

If the renderer was starved of BeginFrames and hasn't submitted a frame, the
root surface may lack an active frame, making `root_frame_missing()` return
true. This makes `ShouldDraw()` return false even if `needs_draw_` is true,
reinforcing the deadlock. However, this is a secondary effect — the primary
cause is the `needs_draw_` / `StopObservingBeginFrames` cycle described above.

**Chromium's awareness.** The source code has a TODO at the exact location where
`OnNeedsBeginFrames` is toggled (external_begin_frame_source_mac.cc:231):

```cpp
// TODO: Try to prevent constant switching between callback register and
// unregister.
```

#### Candidate fixes

Five approaches were identified, ranked by simplicity and directness:

**Fix A: Remove `StopObservingBeginFrames()` from `AttemptDrawAndSwap()`**
(display_scheduler.cc:603). One-line change. The DisplayScheduler never stops
observing, so the BeginFrameSource always has at least one observer, the vsync
callback stays registered, and the renderer keeps receiving BeginFrames. The
`ShouldDraw()` gate still prevents unnecessary draws — the scheduler just keeps
receiving BeginFrames without acting on them until new damage arrives. Cost:
minor CPU from processing idle BeginFrames (negligible for our use case).

**Fix B: Debounce in `ExternalBeginFrameSourceMac::OnNeedsBeginFrames(false)`.**
Don't immediately unregister — keep the vsync callback for N more vsyncs. If
`OnNeedsBeginFrames(true)` comes back before timeout, cancel the unregister.
Matches the existing pattern in `CVDisplayLinkMac::StopDisplayLinkIfNeeded()`
which waits 12 empty vsyncs. More power-friendly than Fix A but more code and
doesn't address the root cause (DisplayScheduler still thrashes).

**Fix C: Remove `root_frame_missing()` from `ShouldDraw()`.** If the root frame
is missing, draw an empty frame instead of stopping. Prevents the secondary
deadlock path but doesn't address the primary `needs_draw_` path.

**Fix D: Decouple renderer BeginFrames from compositor observation.** Always
forward BeginFrames to child frame sinks regardless of whether the
DisplayScheduler is observing. Architecturally clean but a significant refactor
touching `FrameSinkManagerImpl`, `CompositorFrameSinkSupport`, and the
BeginFrame forwarding logic.

**Fix E: Coordinate multi-display schedulers.** Don't stop observing if other
DisplaySchedulers sharing the same DisplayLinkMac still need frames. Requires
cross-scheduler communication that doesn't currently exist.

#### This experiment: Fix A

Fix A is the simplest and most direct. It targets the exact line identified as
the trigger (`StopObservingBeginFrames()` at display_scheduler.cc:603) and
breaks the deadlock by ensuring the BeginFrameSource always has at least one
observer.

#### Changes

**`components/viz/service/display/display_scheduler.cc`** — in
`AttemptDrawAndSwap()`, remove the call to `StopObservingBeginFrames()` when
`ShouldDraw()` returns false. Keep the reset of resize expectations.

Before:

```cpp
} else {
    damage_tracker_->reset_expecting_root_surface_damage_because_of_resize();
    StopObservingBeginFrames();
}
```

After:

```cpp
} else {
    damage_tracker_->reset_expecting_root_surface_damage_because_of_resize();
    // TermSurf: Don't stop observing. With multiple BrowserContexts,
    // stopping causes a deadlock: the renderer loses BeginFrames, can't
    // produce new frames, no damage arrives, and the compositor stays
    // stopped. Keeping observation alive costs minor CPU but prevents
    // the 2fps thrashing seen in Experiments 5–9 and 12.
}
```

**`content_api_shim.mm`** — keep the Experiment 12 two-profile code (same as
Experiment 9). No changes.

**Diagnostic logging** — keep the `[TS-DIAG]` instrumentation from Experiment 12
in `external_begin_frame_source_mac.cc` and `cv_display_link_mac.mm`. The logs
will confirm whether `OnNeedsBeginFrames(false)` stops firing.

#### Verification

1. Build and launch — two Shell windows appear (one per profile)
2. Check visual FPS: expect 60fps (previously 2fps)
3. Capture stderr with `--enable-logging=stderr --v=1`
4. Confirm that `OnNeedsBeginFrames(false)` no longer fires repeatedly in the
   steady state (the thrashing pattern from Experiment 12 should disappear)
5. Confirm that `OnNeedsBeginFrames(true)` fires once at startup and stays
   active

#### Subsequent experiments if this fails

If Fix A produces 60fps: **done** — the deadlock was the sole cause. Future
refinement (Fix B debounce) can be a separate issue for power optimization.

If Fix A still shows 2fps: the deadlock is deeper than `DisplayScheduler`. The
renderer's `CompositorFrameSinkSupport` is independently starving. Try **Fix D**
(decouple renderer BeginFrames) or add logging to
`CompositorFrameSinkSupport::UpdateNeedsBeginFramesInternal()` to trace why the
renderer stops requesting frames.

If Fix A shows improvement but not 60fps (e.g., 30fps): both paths contribute.
Combine Fix A with Fix C (remove `root_frame_missing` from `ShouldDraw`) to
eliminate both deadlock triggers.

**Result:** Fail

The fix successfully eliminated the `OnNeedsBeginFrames` thrashing in the logs —
the vsync callback stayed registered and delivered steady 16ms callbacks. But
**visual performance was still 2fps**. Typing into Google's search input was
obviously sluggish, matching Experiment 12's behavior exactly.

What the logs showed:

| Instance    | Profile     | Vsync callbacks | Duration | Behavior                           |
| ----------- | ----------- | --------------- | -------- | ---------------------------------- |
| 0x7a91ad680 | google.com  | 26              | 430ms    | Ran briefly, then stopped          |
| 0x7a91afc00 | example.com | 1053            | 17.5s    | Continuous 16ms callbacks (60.2/s) |

The logs prove that the `ExternalBeginFrameSourceMac` was no longer thrashing —
it received steady vsync callbacks. But the renderer was still not producing
frames at 60fps. The `ShouldDraw()` gate continued returning false, meaning the
scheduler received BeginFrames but skipped drawing on most of them.

This means **the deadlock is deeper than `DisplayScheduler` stopping
observation**. Even with the vsync pipeline kept alive, the renderer does not
produce frames. The `CompositorFrameSinkSupport` or some other component is
independently starving the renderer of BeginFrames, or the renderer itself is
throttled by a different mechanism.

#### Conclusion

Fix A is necessary but not sufficient. It keeps the vsync source alive, but
something further downstream still prevents frames from being drawn. The next
investigation should trace why `ShouldDraw()` returns false — specifically
whether `needs_draw_` is never set (no damage arriving) or
`root_frame_missing()` is true (renderer not submitting frames). Adding
instrumentation to `ShouldDraw()` and
`CompositorFrameSinkSupport::OnBeginFrame()` would reveal which path is
blocking.

### Experiment 14: Instrument the draw and frame delivery pipeline

#### Background: what Experiment 13 revealed

Experiment 13 proved that keeping the `ExternalBeginFrameSourceMac` alive (Fix
A) is necessary but not sufficient. The vsync source delivered steady 16ms
callbacks, yet visual rendering remained at 2fps. This means the bottleneck is
downstream — somewhere between the vsync callback arriving and pixels actually
being drawn.

There are three independent throttle/starvation mechanisms in the pipeline:

**1. DisplayScheduler::ShouldDraw() gate** (display_scheduler.cc:448–452)

```cpp
return needs_draw_ && !output_surface_lost_ && visible_ &&
       !damage_tracker_->root_frame_missing();
```

Four conditions, any of which being wrong prevents drawing. The most likely
culprits:

- `needs_draw_` — only set by `OnDisplayDamaged()`. If the renderer never
  submits new CompositorFrames, no damage is generated, and `needs_draw_` stays
  false.
- `root_frame_missing()` — if the renderer was starved of BeginFrames and hasn't
  activated a surface, this returns true, blocking the draw.

**2. CompositorFrameSinkSupport::ShouldSendBeginFrame() throttle**
(compositor_frame_sink_support.cc:1452–1557)

Even when the `ExternalBeginFrameSourceMac` delivers a callback, each
`CompositorFrameSinkSupport` independently decides whether to forward the
BeginFrame to its renderer. Multiple throttle gates exist:

| Gate                         | Threshold                 | Effect                               |
| ---------------------------- | ------------------------- | ------------------------------------ |
| `ShouldStopBeginFrame()`     | outstanding >= 100        | Completely stops sending BeginFrames |
| `ShouldThrottleBeginFrame()` | outstanding >= 10         | Throttles BeginFrame delivery        |
| `kUndrawnFrameLimit`         | undrawn frames > 3        | Throttles if too many undrawn frames |
| `!client_needs_begin_frame_` | client stopped requesting | Stops sending                        |
| `PendingAck`                 | pending >= limit          | Stops until ack received             |

The `outstanding_begin_frames_` counter in `BeginFrameTracker`
(begin_frame_tracker.cc) tracks how many BeginFrames were sent without
acknowledgment. With two profiles competing for GPU time, renderers may be slow
to acknowledge, causing the counter to build up past the throttle threshold (10)
or stop threshold (100).

**3. GPU busy backpressure** (begin_frame_source.cc:152–167)

When `pending_swaps_ >= MaxPendingSwaps()`, the `DisplayScheduler` calls
`SetIsGpuBusy(true)` on the `BeginFrameSource`. This can throttle future
BeginFrame delivery at the source level. With two profiles sharing the GPU, swap
buffers may back up.

**Which mechanism is blocking?** We don't know — that's what this experiment
will determine.

#### Changes

Add `[TS-DIAG]` instrumentation to three decision points. Use LOG(ERROR) for
state transitions and infrequent events; VLOG(1) for per-frame data.

**1. `display_scheduler.cc` — `ShouldDraw()` condition breakdown**

Add VLOG(1) inside `AttemptDrawAndSwap()` when `ShouldDraw()` returns false,
logging all four conditions:

```cpp
} else {
    VLOG(1) << "[TS-DIAG] ShouldDraw=false"
            << " needs_draw=" << needs_draw_
            << " output_surface_lost=" << output_surface_lost_
            << " visible=" << visible_
            << " root_frame_missing=" << damage_tracker_->root_frame_missing()
            << " pending_swaps=" << pending_swaps_;
    damage_tracker_->reset_expecting_root_surface_damage_because_of_resize();
    // StopObservingBeginFrames();  // (Experiment 13)
}
```

Also add LOG(ERROR) to `DrawAndSwap()` to count successful draws:

```cpp
bool DisplayScheduler::DrawAndSwap() {
  LOG(ERROR) << "[TS-DIAG] DrawAndSwap display=" << this;
  // ... existing code ...
```

**2. `compositor_frame_sink_support.cc` — `RecordShouldSendBeginFrame()` with
logging**

Replace the file-level `RecordShouldSendBeginFrame()` function to also LOG when
returning false:

```cpp
bool RecordShouldSendBeginFrame(const std::string& reason, bool should_send) {
  TRACE_EVENT2("viz", "SendBeginFrameDecision", "reason", reason, "should_send",
               should_send);
  if (!should_send) {
    LOG(ERROR) << "[TS-DIAG] ShouldSendBeginFrame=false reason=" << reason;
  }
  return should_send;
}
```

**3. `begin_frame_tracker.cc` — outstanding frame count**

Add LOG(ERROR) when throttle or stop thresholds are hit:

```cpp
bool BeginFrameTracker::ShouldThrottleBeginFrame() const {
  bool result = outstanding_begin_frames_ >= kLimitThrottle &&
                outstanding_begin_frames_ < kLimitStop;
  if (result) {
    LOG(ERROR) << "[TS-DIAG] BeginFrameTracker THROTTLE outstanding="
               << outstanding_begin_frames_;
  }
  return result;
}

bool BeginFrameTracker::ShouldStopBeginFrame() const {
  bool result = outstanding_begin_frames_ >= kLimitStop;
  if (result) {
    LOG(ERROR) << "[TS-DIAG] BeginFrameTracker STOP outstanding="
               << outstanding_begin_frames_;
  }
  return result;
}
```

**4. `content_api_shim.mm` — update URLs**

Change Profile B from `https://example.com` to `https://lite.duckduckgo.com`.
Both profiles now load interactive pages with search inputs, making it easier to
generate sustained damage for testing.

```cpp
const char* kProfileAUrl = "https://google.com";
const char* kProfileBUrl = "https://lite.duckduckgo.com";
```

**Keep Experiment 13's fix** (`StopObservingBeginFrames` commented out) and
Experiment 12's `ExternalBeginFrameSourceMac` instrumentation. This builds on
both.

#### Verification

1. Build and launch — two Shell windows appear (one per profile)
2. Capture stderr with `--enable-logging=stderr --v=1`
3. Interact with the google.com window (type in search) to generate damage
4. Analyze logs:
   - If `ShouldDraw=false` lines dominate with `needs_draw=0`: damage isn't
     arriving (renderer starved)
   - If `ShouldDraw=false` with `root_frame_missing=1`: renderer hasn't
     activated a surface
   - If `ShouldSendBeginFrame=false` with `ThrottleUnresponsiveClient` or
     `StopUnresponsiveClient`: the `BeginFrameTracker` is killing frame delivery
   - If `ShouldSendBeginFrame=false` with `ThrottleUndrawnFrames`: frames are
     produced but never drawn
   - If `DrawAndSwap` fires infrequently: draws happen but are rare
   - If `BeginFrameTracker THROTTLE/STOP` appears: outstanding frames are
     building up

The answer will point directly at which throttle mechanism to disable next.

**Result:** Pass (diagnostic)

The instrumentation identified the bottleneck as `needs_draw=0` and eliminated
several candidate throttle mechanisms.

**Visual observation:** Both windows were individually focused and interacted
with. google.com (Profile A, opened first) rendered at 2fps when typing into the
search bar. lite.duckduckgo.com (Profile B, opened second) rendered at 60fps
when typing. The slowdown is order-dependent: **the first-opened profile is the
one that gets throttled**.

**Summary counts (~90-second capture):**

| Metric                       | Count | Meaning                                |
| ---------------------------- | ----- | -------------------------------------- |
| `ShouldDraw=false`           | 1585  | Dominant — draw gate blocks most vsync |
| `DrawAndSwap`                | 292   | Draws happen, but infrequently         |
| `ShouldSendBeginFrame=false` | 9     | Almost never blocked                   |
| `BeginFrameTracker THROTTLE` | 0     | Not the problem                        |
| `BeginFrameTracker STOP`     | 0     | Not the problem                        |

**The `ShouldDraw=false` breakdown — every single instance shows:**

```
needs_draw=0 output_surface_lost=0 visible=1 root_frame_missing=0
```

All 1585 failures are `needs_draw=0`. The other three conditions are healthy.
The ONLY problem is that `needs_draw_` is not being set — no damage is arriving
from the first profile's renderer.

**`ShouldSendBeginFrame` is NOT the bottleneck.** Only 9 rejections total, all
with reason `StopNotRequested` (early initialization). The `BeginFrameTracker`
throttle/stop mechanisms never fired. BeginFrames ARE being delivered to the
renderer.

**Draw timing for the slow display (~3fps steady state).** The google.com
display showed an initial 60fps burst during page load, then degraded to ~333ms
intervals (3fps) in steady state:

```
102208.850 → 102209.183 → 102209.516 → 102209.833 → 102210.166
          333ms        333ms        317ms        333ms
```

**The key insight: this is order-dependent, likely lock contention.** The first
profile (Profile A, google.com) is the one that gets throttled to 2fps. The
second profile (Profile B, duckduckgo) runs at 60fps. This is consistent with a
lock contention or resource ordering issue where the second BrowserContext's
creation disrupts the first's rendering pipeline — possibly a shared lock,
shared GPU channel, or shared compositor resource that the second profile
acquires and doesn't release fairly.

#### Conclusion

Experiment 14 eliminated the viz compositor pipeline as the bottleneck:

- BeginFrameTracker throttle/stop: **not the cause** (never fires)
- ShouldSendBeginFrame: **not the cause** (9 rejections in ~90s, all init-time)
- ShouldDraw conditions other than `needs_draw_`: **not the cause** (all
  healthy)

The bottleneck is `needs_draw_=0` — the first profile's renderer is not
submitting CompositorFrames at 60fps. This is order-dependent: the first-opened
profile is throttled while the second runs fine. This points to lock contention
or resource ordering in the shared infrastructure (GPU channel, compositor
thread, or renderer process scheduling).

The next experiment should reverse the URL order (duckduckgo first, google
second) to confirm the order-dependence hypothesis. If duckduckgo becomes the
slow one, the problem is definitively about creation order, not page complexity.

### Experiment 15: Reverse URL order to confirm order-dependence

Identical to Experiment 14 (same instrumentation, same Experiment 13 fix) but
with the URL assignment reversed: Profile A loads lite.duckduckgo.com (opened
first), Profile B loads google.com (opened second).

#### Changes

**`content_api_shim.mm`** — swap URLs:

```cpp
const char* kProfileAUrl = "https://lite.duckduckgo.com";
const char* kProfileBUrl = "https://google.com";
```

No other changes. All Experiment 14 instrumentation remains in place.

#### Verification

1. Build and launch — two Shell windows appear
2. Focus the duckduckgo window (Profile A, opened first), type into it
3. Focus the google.com window (Profile B, opened second), type into it
4. Compare visual responsiveness:
   - If duckduckgo (first) is now 2fps and google (second) is 60fps: **order-
     dependent** — the first profile is always throttled regardless of page
     content
   - If google (second) is 2fps and duckduckgo (first) is 60fps: **page-
     dependent** — google.com specifically triggers the throttle
   - If both are 2fps: something else changed

**Result:** Page-dependent

Tested 5 consecutive runs with the reversed URL order (DDG first, google
second). Every run produced the same result: **lite.duckduckgo.com was fast
(60fps), google.com was slow (2fps)**. The order hypothesis is disproven — it is
not about which profile is created first.

| Run | DDG (Profile A, first) | Google (Profile B, second) |
| --- | ---------------------- | -------------------------- |
| 1   | 60fps                  | 2fps                       |
| 2   | 60fps                  | 2fps                       |
| 3   | 60fps                  | 2fps                       |
| 4   | 60fps                  | 2fps                       |
| 5   | 60fps                  | 2fps                       |

Combined with Experiment 14 (google first, DDG second — same result: google
2fps, DDG 60fps), the conclusion is clear: **google.com specifically triggers
the 2fps throttle when a second BrowserContext exists**. DDG is unaffected
regardless of order.

#### Conclusion

The 2fps problem is page-dependent, not order-dependent. google.com is throttled
to 2fps in the presence of a second BrowserContext while lite.duckduckgo.com
runs at 60fps. This is consistent across all 5 runs in both orderings. The
difference is likely page complexity — google.com's heavier JS, larger DOM, or
specific rendering behavior interacts badly with whatever multi-BrowserContext
contention exists. lite.duckduckgo.com is lightweight enough to avoid the
bottleneck.

The next experiment should test with two heavyweight pages (e.g., google.com on
both profiles, or google.com + another complex site) to determine whether the
problem is google.com-specific or whether any heavy page triggers it.

## Conclusion

Issue 620 achieved its primary goal: a minimal Chromium embedder (3 files, ~190
lines) that loads web pages through the Content API via a C function boundary.
The Zig Content Shell replaces Content Shell's 14,000-line fork with a thin C++
shim that can be driven from Zig.

The secondary goal — multiple browser profiles at 60fps in a single process —
remains unsolved. 15 experiments systematically narrowed the problem:

### What works

| Configuration        | FPS   | Experiments |
| -------------------- | ----- | ----------- |
| 1 profile, 1 window  | 60fps | 1, 10       |
| 1 profile, 2 windows | 60fps | 11          |

### What doesn't work

| Configuration                 | FPS   | Experiments |
| ----------------------------- | ----- | ----------- |
| 2 profiles, any window count  | 2fps  | 2–9, 12     |
| 2 profiles + DDG + google.com | mixed | 14–15       |

Experiments 14–15 revealed that the 2fps is page-dependent: lite.duckduckgo.com
renders at 60fps while google.com renders at 2fps, regardless of which profile
is created first. This was consistent across 6 runs in both URL orderings.

### What was eliminated

The bisection across Experiments 5–11 and the instrumentation in 12–14
eliminated these as causes:

- **C API wrappers** — Experiment 8 vs 9: identical 2fps with and without them
- **Callback/lifecycle machinery** — Experiment 7 vs 8: identical 2fps
- **Direct WebContents vs Shell::CreateNewWindow** — Experiments 5–7: identical
- **Custom launcher vs stock shell_main_mac.cc** — Experiment 8: identical
- **Creation order** — Experiment 15: 5 runs, always google slow, DDG fast
- **ExternalBeginFrameSourceMac thrashing** — Experiment 12 found it, Experiment
  13 fixed it, but 2fps persisted
- **BeginFrameTracker throttle/stop** — Experiment 14: never fired (0 events)
- **ShouldSendBeginFrame gate** — Experiment 14: only 9 rejections in 90s, all
  during init
- **DisplayScheduler ShouldDraw conditions** (output_surface_lost, visible,
  root_frame_missing) — Experiment 14: all healthy, only `needs_draw=0` fails

### What was found

**The viz compositor pipeline works correctly.** BeginFrames arrive at 60fps,
pass through all gates, and the DisplayScheduler is ready to draw. The problem
is that the renderer for complex pages (google.com) only submits
CompositorFrames at ~3fps when a second BrowserContext exists. The renderer
receives BeginFrames but doesn't produce output.

**The ExternalBeginFrameSourceMac thrashing** (Experiment 12) was a real bug —
the DisplayScheduler's `StopObservingBeginFrames()` call at
display_scheduler.cc:603 caused a deadlock where the renderer lost BeginFrames
and couldn't produce frames. Commenting it out (Experiment 13) fixed the
thrashing but didn't fix the 2fps, proving the thrashing was a symptom, not the
root cause.

**The bottleneck is in the renderer process.** The renderer receives BeginFrames
via `CompositorFrameSinkSupport::OnBeginFrame()` but only produces
CompositorFrames at ~3fps for complex pages. Lightweight pages
(lite.duckduckgo.com) are unaffected. This points to renderer-side contention
when multiple BrowserContexts exist — possibly in the GPU channel, shared
compositor thread, Blink's main thread scheduling, or JavaScript execution
competing for the same process resources.

### Chromium internals research

The following architecture was mapped during the investigation:

**Vsync and frame delivery chain:**

```
CADisplayLinkMac / CVDisplayLinkMac (macOS vsync driver)
  → ExternalBeginFrameSourceMac (per-compositor begin frame source)
    → ExternalBeginFrameSource::AddObserver/RemoveObserver
      → DisplayScheduler::OnBeginFrame (schedules deadline)
        → DisplayScheduler::AttemptDrawAndSwap
          → ShouldDraw() gate (needs_draw_, visible, output_surface_lost,
                               root_frame_missing)
          → DrawAndSwap() → actual pixel output
      → CompositorFrameSinkSupport::OnBeginFrame (forwards to renderer)
        → ShouldSendBeginFrame() gate (multiple throttle checks)
        → client_->OnBeginFrame() (Mojo IPC to renderer process)
```

**Throttle mechanisms investigated:**

| Mechanism                                   | Location                         | Threshold                 | Status                         |
| ------------------------------------------- | -------------------------------- | ------------------------- | ------------------------------ |
| StopObservingBeginFrames                    | display_scheduler.cc:603         | ShouldDraw()=false        | Fixed (Exp 13), not root cause |
| BeginFrameTracker::ShouldThrottleBeginFrame | begin_frame_tracker.cc           | outstanding >= 10         | Never triggered                |
| BeginFrameTracker::ShouldStopBeginFrame     | begin_frame_tracker.cc           | outstanding >= 100        | Never triggered                |
| kUndrawnFrameLimit                          | compositor_frame_sink_support.cc | undrawn > 3               | Never triggered                |
| SetIsGpuBusy                                | begin_frame_source.cc:152        | pending_swaps >= max      | Not investigated               |
| client_needs_begin_frame_                   | compositor_frame_sink_support.cc | client stopped requesting | 9 events (init only)           |

**Key source files:**

| File                               | Role                                                         |
| ---------------------------------- | ------------------------------------------------------------ |
| cv_display_link_mac.mm             | macOS CVDisplayLink vsync driver                             |
| ca_display_link_mac.mm             | macOS CADisplayLink vsync driver (used on newer macOS)       |
| external_begin_frame_source_mac.cc | Per-compositor vsync source, registers/unregisters callbacks |
| begin_frame_source.cc              | Observer management, OnNeedsBeginFrames dispatch             |
| display_scheduler.cc               | Draw timing, ShouldDraw gate, AttemptDrawAndSwap             |
| display_damage_tracker.cc          | Tracks root_frame_missing, surface damage                    |
| compositor_frame_sink_support.cc   | Forwards BeginFrames to renderer, throttle gates             |
| begin_frame_tracker.cc             | Tracks outstanding BeginFrames for throttle/stop             |
| root_compositor_frame_sink_impl.cc | Creates ExternalBeginFrameSourceMac per profile              |

### Next steps

The investigation continues in Issue 621. The remaining unknowns:

1. **Why does the renderer produce frames at 3fps for complex pages?** The viz
   pipeline delivers BeginFrames at 60fps. The renderer receives them but only
   submits CompositorFrames at ~3fps for google.com while lite.duckduckgo.com is
   fine. Is this Blink main thread contention, GPU channel contention, or
   something else?

2. **Is the problem google.com-specific or complexity-dependent?** Testing with
   two heavyweight pages (both google.com, or google.com + another complex site)
   would answer this.

3. **Can the renderer-side contention be fixed?** The fix may be in Blink's
   frame scheduling, the GPU channel sharing between BrowserContexts, or process
   priority management.
