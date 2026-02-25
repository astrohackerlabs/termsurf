# Issue 642: Zig Profile Server

## Goal

Rewrite the Chromium Profile Server in Zig. Keep a thin C++ shim for Content API
subclassing (the only thing that requires C++), but move all application logic —
XPC handling, tab lifecycle, input routing, navigation, state notifications —
into Zig. The result is a new build target called `zig_profile_server` that
replaces `chromium_profile_server`.

## Background

### Why rewrite in Zig

The current Chromium Profile Server is a Content Shell fork: ~100 C++ files, of
which ~1,050 lines are TermSurf-specific logic. The rest is unmodified Content
Shell boilerplate. Three reasons to rewrite:

1. **Language consistency.** The GUI is written in Zig. The profile server is
   the only TermSurf component written in C++. Having both in Zig means one
   language to maintain, one set of idioms, one mental model.

2. **Maintainability.** The Content Shell fork carries 100+ files we never
   modify. Every Chromium upgrade risks conflicts in boilerplate we don't own. A
   thin C++ shim (3 files) with Zig logic is easier to understand, modify, and
   upgrade.

3. **Developer experience.** Zig is more modern and more enjoyable to work in
   than C++. Comptime, explicit allocators, no header files, no preprocessor
   soup. The same reasons we chose Zig for the GUI apply here.

### What Issue 620 proved

Issue 620 built a minimal Chromium embedder (3 files, ~190 lines) that drives
the Content API through a C function boundary. 15 experiments proved:

- The Content API can be driven from a C `main()` via dlopen/dlsym
- Multiple BrowserContexts coexist in one process with isolated storage
- A custom launcher with zero Chromium headers can create profiles, tabs, and
  manage lifecycle through C function pointers
- `WebContents` can be created directly and their NSView attached to custom
  windows

The in-process multi-profile goal was shelved (Blink main thread scheduling
blocks 60fps for JS-heavy pages across two BrowserContexts — Issue 621). But the
C API shim architecture is exactly what we need for the out-of-process Zig
Profile Server.

### What Issue 620 did NOT cover

Issue 620's shim was a proof-of-concept. It did not implement:

- XPC communication (the gateway connection, message dispatch, response sending)
- Input forwarding (mouse, keyboard, scroll events → RenderWidgetHost)
- CALayerHost / persistent compositor (CAContext stability across navigation)
- WebContents observation (URL, title, loading state, cursor changes)
- Dock icon hiding, focus management, auto-exit
- Navigation actions (back, forward, reload, navigate-to-URL)
- New-tab link interception (redirect to same tab)
- Page title sync

All of these currently live in C++ in the Content Shell fork. They need to move
to Zig.

### What the current profile server does

The Chromium Profile Server runs out-of-process and:

1. **Accepts XPC commands** from the GUI: create/destroy tabs, navigate, resize,
   set focus, forward input events
2. **Observes WebContents** for URL, title, loading state, cursor changes and
   sends updates back via XPC
3. **Forwards input** (mouse, keyboard, scroll) from XPC messages to Chromium
   via the Content API's RenderWidgetHost
4. **Renders via persistent compositor** with stable CAContext — no
   `ca_context_id` flicker on navigation (Issue 633)
5. **Manages visibility/focus** to keep the compositor at full framerate
6. **Intercepts new-tab links** and redirects them to the current tab
   (Issue 639)
7. **Hides the Dock icon** via LSUIElement and runtime NSApplication policy

### Current file structure

The profile server lives at `chromium/src/content/chromium_profile_server/` with
~100 files inherited from Content Shell. TermSurf-specific logic (~1,050 lines)
is spread across:

| File                                     | Lines | What it does                                              |
| ---------------------------------------- | ----- | --------------------------------------------------------- |
| `browser/shell_browser_main_parts.cc`    | ~590  | XPC gateway, tab/profile lifecycle, persistent compositor |
| `browser/shell_tab_observer.cc`          | ~200  | URL/title/loading/cursor notifications                    |
| `browser/shell.cc`                       | ~150  | Input forwarding, new-tab interception                    |
| `browser/shell_compositor_bridge_mac.mm` | ~60   | Persistent compositor bridge                              |
| Various                                  | ~50   | Dock hiding, focus, auto-exit                             |

## Chromium Branch

`146.0.7650.0-issue-642` — forked from the vanilla `146.0.7650.0` tag, NOT from
`issue-639` or any other TermSurf branch.

The whole point of the Zig Profile Server is to eliminate the Content Shell
fork. The current profile server (`chromium_profile_server`) carries 42 patches
from `termsurf` through `issue-639` — most of which modify Content Shell
internals. The new branch starts clean: just 3 new files in
`content/zig_profile_server/` (BUILD.gn, header, implementation). No
modifications to existing Chromium source.

This means:

- **Tiny patch set.** Three new files vs 42 inherited patches.
- **Trivial upgrades.** Rebasing 3 new files onto a new Chromium tag is easy.
  Rebasing 42 patches that touch Content Shell internals is fragile.
- **No contamination.** The branch doesn't carry persistent compositor hacks,
  XPC gateway wiring, dock icon patches, or any other Content Shell
  modifications. Those all move to Zig.

For reference during implementation, the old profile server's logic is preserved
in `chromium/patches/issue-639/` and the `146.0.7650.0-issue-639` branch.

## Architecture

### C++ shim (inside `chromium/src/`)

A thin C++ layer (~3 files) that subclasses Content API virtual classes and
exports C functions. This is the same pattern as Issue 620's `content_api_shim`,
extended to cover the full feature set. Must live inside `chromium/src/` because
GN can only see files rooted there.

The shim handles:

- `ContentMainDelegate`, `ContentBrowserClient`, `BrowserMainParts` subclassing
- `WebContentsDelegate`, `WebContentsObserver` subclassing
- `BrowserContext` lifecycle
- Lifecycle callbacks (initialized, shutdown)
- All forwarding to/from Zig via C function pointers

The shim does NOT contain application logic. It is mechanical glue.

### Zig profile server (in the main repo)

All application logic lives in Zig:

- XPC gateway connection and message dispatch
- Tab lifecycle (create, destroy, navigate)
- Input event translation and forwarding (via C shim functions)
- WebContents state observation callbacks (URL, title, loading, cursor)
- Persistent compositor management
- CAContext ID extraction and reporting
- Dock icon hiding
- Focus management
- Auto-exit when all tabs close
- New-tab link interception

### C API surface

Based on Issue 620's proven API, extended for the full feature set:

```c
/* Initialization */
int ts_content_main(int argc, const char** argv);
void ts_set_on_initialized(void (*callback)(void));
void ts_set_on_shutdown(void (*callback)(void));

/* Profile management */
typedef void* ts_browser_context_t;
ts_browser_context_t ts_create_browser_context(const char* path);
void ts_destroy_browser_context(ts_browser_context_t ctx);

/* Tab management */
typedef void* ts_web_contents_t;
ts_web_contents_t ts_create_web_contents(ts_browser_context_t ctx,
                                         const char* url);
void ts_destroy_web_contents(ts_web_contents_t wc);

/* Navigation */
void ts_load_url(ts_web_contents_t wc, const char* url);
void ts_go_back(ts_web_contents_t wc);
void ts_go_forward(ts_web_contents_t wc);
void ts_reload(ts_web_contents_t wc);

/* Input */
void ts_forward_mouse_event(ts_web_contents_t wc, int type,
                            int x, int y, int button, int mods);
void ts_forward_scroll_event(ts_web_contents_t wc,
                             int x, int y, float dx, float dy,
                             int phase, int mods);
void ts_forward_key_event(ts_web_contents_t wc, int type,
                          int keycode, const char* text, int mods);
void ts_set_focus(ts_web_contents_t wc, bool focused);

/* Display */
uint32_t ts_get_ca_context_id(ts_web_contents_t wc);
void ts_set_view_size(ts_web_contents_t wc, int width, int height);

/* Lifecycle */
void ts_quit(void);

/* Callbacks (Zig registers these before calling ts_content_main) */
void ts_set_on_navigation_committed(
    void (*cb)(ts_web_contents_t wc, const char* url));
void ts_set_on_loading_state_changed(
    void (*cb)(ts_web_contents_t wc, bool loading, float progress));
void ts_set_on_cursor_changed(
    void (*cb)(ts_web_contents_t wc, int cursor_type));
void ts_set_on_title_changed(
    void (*cb)(ts_web_contents_t wc, const char* title));
void ts_set_on_ca_context_id_changed(
    void (*cb)(ts_web_contents_t wc, uint32_t ca_context_id));
```

### Build

Two-step build, same as Issue 620:

1. Build the C++ shim inside Chromium (produces shared library):

   ```bash
   cd chromium/src
   autoninja -C out/Default zig_profile_server_shim
   ```

2. Build the Zig profile server (links against the shim):
   ```bash
   cd browser
   zig build
   ```

The Zig binary dlopen's the shim framework, dlsym's all C API symbols, registers
callbacks, and calls `ts_content_main`. Same pattern proven in Issue 620
Experiment 4.

### Directory layout

```
~/dev/termsurf/
├── browser/                                         ← Zig profile server (main repo)
│   ├── build.zig
│   └── src/
│       ├── main.zig                                 ← Entry point, dlopen/dlsym
│       ├── xpc.zig                                  ← XPC gateway and message dispatch
│       ├── profile.zig                              ← BrowserContext lifecycle
│       ├── tab.zig                                  ← WebContents lifecycle
│       ├── input.zig                                ← Mouse/keyboard/scroll forwarding
│       ├── navigation.zig                           ← URL loading, back/forward/reload
│       └── callbacks.zig                            ← Content API event handlers
├── chromium/src/content/zig_profile_server/         ← C++ shim (Chromium fork, 3 files)
│   ├── BUILD.gn
│   ├── content_api_shim.h
│   └── content_api_shim.cc
├── gui/                                             ← TermSurf GUI (Ghostty fork)
└── tui/                                             ← web TUI (Rust/ratatui)
```

## Relationship to Issue 620

Issue 620 proved the architecture. This issue builds the production
implementation. The key differences:

|               | Issue 620                             | Issue 642                             |
| ------------- | ------------------------------------- | ------------------------------------- |
| Goal          | Prove C API works, explore in-process | Production profile server             |
| Process model | In-process experiment                 | Out-of-process (same as current)      |
| XPC           | None                                  | Full gateway + message dispatch       |
| Input         | None                                  | Mouse, keyboard, scroll forwarding    |
| Display       | Basic NSView attachment               | Persistent compositor, CALayerHost    |
| Observation   | None                                  | URL, title, loading, cursor callbacks |
| Shim size     | ~190 lines                            | ~800 lines (estimated)                |
| Zig code      | None (C launcher only)                | Full application logic                |

## Plan

### Stage 1: C++ shim with minimal Zig launcher

Build the C++ shim inside `chromium/src/content/zig_profile_server/` with the
core API (init, profile, tab, quit). Write a minimal Zig launcher that dlopen's
the shim, creates one profile, loads one page, and displays it in a Shell
window. Prove the Issue 620 Experiment 4 result still works.

### Stage 2: Direct WebContents and CAContext

Extend the shim with `ts_create_web_contents`, `ts_get_ca_context_id`,
`ts_set_view_size`. The Zig launcher creates WebContents directly (no Shell
window) and reports the CAContext ID. Prove the display pipeline works.

### Stage 3: XPC gateway in Zig

Implement the XPC gateway connection in Zig. The Zig profile server connects to
the GUI's XPC listener and begins accepting commands. Port the create-tab,
destroy-tab, and resize XPC handlers from C++ to Zig.

### Stage 4: Input forwarding

Port mouse, keyboard, and scroll event forwarding. The Zig code receives XPC
input messages and calls the C shim's `ts_forward_*` functions.

### Stage 5: WebContents observation

Port URL, title, loading state, and cursor change observation. The C++ shim
fires callbacks into Zig, which sends XPC messages back to the GUI.

### Stage 6: Navigation and remaining features

Port navigation actions (back, forward, reload, navigate-to-URL), new-tab link
interception, dock icon hiding, focus management, and auto-exit.

### Stage 7: Replace chromium_profile_server

Once the Zig Profile Server has feature parity, switch the GUI to connect to it
instead of the C++ profile server. Remove the old `chromium_profile_server`
target.

## Experiments

### Experiment 1: Zig drives ContentMain

Prove that a Zig binary can dlopen the Chromium framework, register a callback,
and drive `ContentMain` to display a web page. This is the thinnest possible
end-to-end proof of the Zig-to-Chromium bridge.

Issue 620 Experiment 4 proved this from a C/ObjC launcher (`ts_main.mm`). This
experiment proves it from Zig. Everything else in this issue is incremental once
this works.

#### Chromium side

Create the branch `146.0.7650.0-issue-642` from the vanilla `146.0.7650.0` tag.
Add 3 files in `chromium/src/content/zig_profile_server/`:

**`BUILD.gn`** — Build target. The key difference from the old
`chromium_profile_server`: this target depends on
`//content/shell:content_shell_lib` (linking against Content Shell as a
library), not forking it. No Content Shell files are copied or modified.

**`content_api_shim.h`** — C header with 4 exports:

```c
#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ts_callback_t)(void);

// Register a callback that fires when the browser is ready.
void ts_set_on_initialized(ts_callback_t callback);

// Create a browser profile with the given storage path.
typedef void* ts_browser_context_t;
ts_browser_context_t ts_create_browser_context(const char* path);

// Create a tab (Shell window) in the given profile, loading the URL.
void ts_create_tab(ts_browser_context_t ctx, const char* url);

#ifdef __cplusplus
}
#endif
```

`ContentMain` is not declared here — it is dlsym'd from the framework directly
(same pattern as `shell_main_mac.cc`).

**`content_api_shim.cc`** — C++ implementation. Subclasses `ShellMainDelegate`,
`ShellContentBrowserClient`, and `ShellBrowserMainParts` (same 3-class chain as
Issue 620). `InitializeMessageLoopContext()` fires the `on_initialized`
callback. `ts_create_browser_context` creates a `ShellBrowserContext` with the
given path. `ts_create_tab` calls `Shell::CreateNewWindow`.

The macOS app bundle uses `shell_main_mac.cc` as its entry point (same as Issue
620 Experiments 1–3 and 8–11). The helper bundles also reuse Content Shell's
launcher unchanged.

#### Zig side

Create `browser/` in the main repo:

**`browser/build.zig`** — Build system. Produces a shared library (`.dylib`)
that the app bundle's `shell_main_mac.cc` loads via dlopen.

**`browser/src/main.zig`** — Zig entry point. Exports `ContentMain` as
`extern "C"` — this is the symbol that `shell_main_mac.cc` dlsym's. Inside, it:

1. Uses `std.DynLib` (or direct `@cImport` of `<dlfcn.h>`) to dlopen the
   Chromium framework (the C++ shim's shared library).
2. dlsym's `ts_set_on_initialized`, `ts_create_browser_context`,
   `ts_create_tab`.
3. Registers an `on_initialized` callback.
4. Calls the real `ContentMain` (dlsym'd from the framework).

The `on_initialized` callback:

1. Builds the profile path (`~/.config/termsurf/zig-profile-server/profile-a/`).
2. Calls `ts_create_browser_context(path)`.
3. Calls `ts_create_tab(ctx, "https://google.com")`.

#### Build

```bash
# Step 1: Build the C++ shim (framework + app bundle)
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default zig_profile_server

# Step 2: Build the Zig shared library
cd ~/dev/termsurf/browser
zig build

# Step 3: Copy or symlink the Zig dylib into the framework bundle
# (exact mechanism TBD during implementation)
```

#### Verification

1. Run the app:
   ```bash
   open chromium/src/out/Default/Zig\ Profile\ Server.app
   ```
2. A Content Shell window appears showing google.com
3. The page is interactive (scrolling, clicking, typing work)
4. Profile directory created at
   `~/.config/termsurf/zig-profile-server/profile-a/`
5. Closing the window exits the process

If google.com loads in a Shell window, the Zig → C → C++ bridge works. The
Content API is successfully driven from Zig through the shim.

**Result:** Pass

Google.com loaded in a Shell window, fully interactive. The Zig binary dlopen'd
the framework, resolved all 6 symbols, registered callbacks, and drove
ContentMain successfully. The implementation differs from the design in two
minor ways:

1. The Zig binary replaces the app bundle's main executable directly (like Issue
   620 Experiment 4's `ts_main.mm`) rather than being a shared library loaded by
   `shell_main_mac.cc`. This is simpler — no intermediate dlopen layer.
2. The header exports 6 functions (ContentMain, set_on_initialized,
   set_on_shutdown, create_browser_context, destroy_browser_context, create_tab)
   rather than the 4 listed in the design. The shutdown callback and destroy
   function were added for clean lifecycle management.

#### Conclusion

The Zig-to-Chromium bridge works. A Zig `main()` can drive the entire Chromium
Content API through dlopen/dlsym with zero Chromium headers. The C++ shim is 3
new files on a vanilla Chromium tag — no Content Shell modifications. This
validates the architecture for the full Zig Profile Server rewrite.

### Experiment 2: Direct WebContents + CAContext ID

Prove that Zig can create a WebContents directly (no Shell window), set up a
persistent compositor for stable CAContext IDs, and receive the CAContext ID via
callback.

Experiment 1 used `ts_create_tab` which opens a Content Shell window with
chrome/toolbar. The production Zig Profile Server creates WebContents directly —
the GUI owns the windows, the profile server just reports CAContext IDs. This
experiment proves that pipeline.

#### Chromium side

Two commits on the `146.0.7650.0-issue-642` branch:

**Commit 1: `SetCALayerParamsCallback` on `RenderWidgetHostViewMac`.**

The same patch from Issue 625/633 (patch 0031), applied to the vanilla tag. Adds
a `CALayerParamsCallback` member and `SetCALayerParamsCallback()` method to
`RenderWidgetHostViewMac`. When the GPU process delivers new `CALayerParams`
(containing `ca_context_id`), the callback fires in
`AcceleratedWidgetCALayerParamsUpdated()`.

Files modified:
- `content/browser/renderer_host/render_widget_host_view_mac.h` — add include,
  callback type alias, method declaration, member variable
- `content/browser/renderer_host/render_widget_host_view_mac.mm` — implement
  `SetCALayerParamsCallback`, fire callback in
  `AcceleratedWidgetCALayerParamsUpdated`

**Commit 2: Extend C++ shim with direct WebContents creation.**

Five new exports added to `content_api_shim.h/.mm`:

```c
ts_web_contents_t ts_create_web_contents(ts_browser_context_t ctx,
                                         const char* url,
                                         int pixel_width, int pixel_height);
void ts_destroy_web_contents(ts_web_contents_t wc);
void ts_set_view_size(ts_web_contents_t wc, int pixel_width, int pixel_height);
typedef void (*ts_ca_context_callback_t)(ts_web_contents_t wc,
                                         unsigned int ca_context_id);
void ts_set_on_ca_context_id_changed(ts_ca_context_callback_t callback);
```

Implementation details:

- **Persistent compositor** (created once, on first `ts_create_web_contents`):
  `ui::AcceleratedWidgetMac` + `ui::Compositor` + `ui::Layer` (root, transparent).
  A `PersistentCompositorBridge` class implements
  `AcceleratedWidgetMacNSView`, receives `AcceleratedWidgetCALayerParamsUpdated`,
  extracts `ca_context_id`, and fires the Zig callback.
- **`ts_create_web_contents`**: creates `WebContents::Create(params)`, sets a
  minimal `WebContentsDelegate`, navigates via `LoadURLWithParams`, connects to
  the persistent compositor via `SetParentUiLayer(root_layer)`, registers
  `SetCALayerParamsCallback` on the view, calls `WasShown()`.
- **`ts_set_view_size`**: converts pixel→logical, calls `view->SetSize()`,
  updates persistent compositor bounds.
- All persistent objects are raw pointers (intentionally leaked — process
  lifetime). `PersistentCompositorBridge` is `final` to satisfy Chromium's
  `-Wdelete-non-abstract-non-virtual-dtor`.

New BUILD.gn deps: `//ui/accelerated_widget_mac`, `//ui/compositor`,
`//content/public/browser`.

#### Zig side

Updated `browser/src/main.zig`:

- New function pointer types: `CreateWebContentsFn`, `DestroyWebContentsFn`,
  `SetViewSizeFn`, `CAContextCallbackFn`, `SetOnCAContextChangedFn`
- `onInitialized` calls `ts_create_web_contents(ctx, "https://google.com", 1280, 720)`
  instead of `ts_create_tab`
- New `onCAContextChanged` callback prints `ca_context_id=N` to stderr
- `onShutdown` calls `ts_destroy_web_contents` before `ts_destroy_browser_context`

#### Build and test

```bash
cd chromium/src && autoninja -C out/Default zig_profile_server
cd browser && zig build
cp browser/zig-out/bin/zig_profile_server \
   "chromium/src/out/Default/Zig Profile Server.app/Contents/MacOS/Zig Profile Server"
codesign --force --deep -s - "chromium/src/out/Default/Zig Profile Server.app"
```

#### Issues encountered

1. **Exit-time destructors.** Chromium's `-Wexit-time-destructors` rejects
   `static std::unique_ptr<>` globals. Fixed by using raw pointers
   (intentionally leaked — these are process-lifetime objects).
2. **Non-virtual destructor on non-final class.** `PersistentCompositorBridge`
   inherits from `AcceleratedWidgetMacNSView` which has a non-virtual
   destructor. `std::unique_ptr::~unique_ptr` triggers
   `-Wdelete-non-abstract-non-virtual-dtor`. Fixed by marking the class `final`
   and removing `override` from the destructor.
3. **Storage service path conflict.** Using a different profile path
   (`profile-a`) than the default context (`default`) caused a FATAL crash in
   the storage service's filesystem proxy — it couldn't make `profile-a/` paths
   relative to `default/`. Fixed by using the same path for both contexts. This
   is fine for the experiment; production will handle multi-profile properly.
4. **Code signature invalidation.** Copying the Zig binary into the app bundle
   invalidates the code signature from `autoninja`. Fixed by re-codesigning:
   `codesign --force --deep -s -`.

#### Result: Pass

```
[ZigProfileServer] Created persistent compositor
[ZigProfileServer] Set parent_ui_layer_ on view
[ZigProfileServer] Created WebContents, navigating to https://google.com
ca_context_id=1704182908
```

All verification criteria met:

1. **No Shell window** — no Content Shell chrome/toolbar appeared
2. **`ca_context_id=1704182908`** — nonzero, printed to stderr by Zig callback
3. **No crash** — app stayed running, killed cleanly after 10 seconds
4. **Persistent compositor working** — GPU process rendering frames offscreen

#### Conclusion

Zig can create WebContents directly without Shell windows and receive stable
CAContext IDs from the persistent compositor. The GUI can use this ID with
`CALayerHost` to display the content — that integration happens in Stage 3
(XPC). The C++ shim grew from 3 exports (Experiment 1) to 8, still under 400
lines. The Zig launcher remains under 150 lines with zero Chromium headers.
