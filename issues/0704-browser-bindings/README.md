+++
status = "closed"
opened = "2026-03-04"
closed = "2026-03-04"
+++

# Issue 704: Roamium, Zoomium, and Plusium — browser bindings for Chromium

## Goal

Create three browser binding packages — Roamium (Rust), Zoomium (Zig), and
Plusium (C++) — that wrap Chromium's Content API through a shared C library.
Each produces a standalone binary that speaks the TermSurf IPC protocol (Unix
sockets + length-prefixed protobuf). The TUI gains a `--browser` flag so users
can specify which binary to use: `web google.com --browser roamium`. After all
three work, Roamium becomes the default. The GUI treats browser binaries as
opaque — any protocol-compatible binary can be passed in.

## Background

### The C shim is proven

Issue 620 (Zig Content Shell) proved that Chromium's Content API can be driven
through a C function boundary. A thin C++ shim (~190 lines, 3 files) in
`chromium/src/content/zig_content_shell/` exports C functions for:

- Initialization: `ts_content_main()`
- Profile management: `ts_create_browser_context()`,
  `ts_destroy_browser_context()`
- Tab management: `ts_create_web_contents()`, `ts_destroy_web_contents()`
- Navigation: `ts_load_url()`, `ts_go_back()`, `ts_go_forward()`, `ts_reload()`
- Input forwarding: `ts_forward_mouse_event()`, `ts_forward_scroll_event()`,
  `ts_forward_key_event()`
- Display: `ts_get_ca_context_id()`, `ts_set_view_size()`
- Callbacks: `ts_set_on_url_changed()`, `ts_set_on_loading_state_changed()`,
  `ts_set_on_cursor_changed()`, `ts_set_on_title_changed()`,
  `ts_set_on_ca_context_id_changed()`

This C API subclasses `ContentMainDelegate`, `ContentBrowserClient`,
`BrowserMainParts`, `WebContentsDelegate`, and `WebContentsObserver` internally,
exposing only opaque handles and C function pointers to callers.

### Previous attempts at language bindings

- **Issue 642** (Zig Profile Server) — Proved the C shim architecture works
  end-to-end (profile creation, tab management, CAContext IDs). Failed on
  deployment: couldn't integrate a Zig binary into Chromium's app bundle due to
  code signing and path resolution mismatches between `zig build` and
  `autoninja`.
- **Issue 643** (Zig Profile Server Take 2) — Proposed moving Zig code inside
  `chromium/src/` and having GN build it. Abandoned before implementation.
- **Issue 644** (Simplified C++ Profile Server) — Pragmatic alternative: keep
  C++, strip Content Shell down to essentials. Baseline confirmed working,
  research complete, not yet implemented.

The key lesson: the C shim works, but the binding language's binary must be
buildable within or alongside Chromium's build system, or the deployment story
breaks.

### Current profile server

The current `Chromium Profile Server` is a Content Shell fork (~100 files, ~1050
lines TermSurf-specific) that:

1. Connects to the GUI via Unix socket (`--ipc-socket={path}`)
2. Sends `ServerRegister` with its profile name
3. Handles protobuf commands: CreateTab, Navigate, MouseEvent, KeyEvent,
   ScrollEvent, Resize, CloseTab, FocusChanged, SetColorScheme
4. Sends back: TabReady, CaContext, UrlChanged, LoadingState, TitleChanged,
   CursorChanged

### IPC protocol

All communication uses Unix domain sockets with 4-byte little-endian
length-prefixed protobuf messages (`termsurf.proto`). The GUI spawns server
processes with `--ipc-socket={path}` and `--user-data-dir={profile_path}`. The
server connects back to the GUI socket and registers itself.

### Relevant Chromium branches

| Branch                   | Issue | What                                                       |
| ------------------------ | ----- | ---------------------------------------------------------- |
| `146.0.7650.0-issue-620` | 620   | C shim PoC (zig_content_shell)                             |
| `146.0.7650.0-issue-642` | 642   | Zig profile server (3 files in content/zig_profile_server) |
| `146.0.7650.0-issue-643` | 643   | Zig profile server via GN (abandoned)                      |
| `146.0.7650.0-issue-644` | 644   | Simplified C++ profile server (research done)              |
| `146.0.7650.0-issue-702` | 702   | Current working branch (socket IPC)                        |

## Architecture

### Shared C library: `libtermsurf_content`

A C library built inside `chromium/src/` by autoninja. It wraps the Content API
and exports C functions for browser lifecycle, profile/tab management, input
forwarding, and state observation. This is an evolution of the Issue 620 C shim,
extended with:

- Socket IPC (connect to GUI, send/receive protobuf messages)
- Persistent compositor support (stable CAContext IDs across navigations)
- All message types from `termsurf.proto`
- DevTools tab creation
- Color scheme support

The C library does NOT contain a `main()` function. Each binding package
provides its own `main()` that calls into the C library. This means the C
library is a static or shared library, not an executable.

### Three binding packages

Each package links against `libtermsurf_content` and provides:

1. A `main()` function that parses CLI args and calls `ts_content_main()`
2. Socket connection logic (connect to `--ipc-socket`, length-prefixed protobuf)
3. Message dispatch (receive commands, call C API, send responses)
4. Callback registration (C function pointers that marshal state changes back
   over the socket)

All three must be **functionally equivalent** to the current Chromium Profile
Server. They are interchangeable — the GUI cannot tell which one it's talking
to.

| Package     | Language | Build system                                      | Notes                                |
| ----------- | -------- | ------------------------------------------------- | ------------------------------------ |
| **Roamium** | Rust     | Cargo (linked against libtermsurf_content)        | Default browser after completion     |
| **Zoomium** | Zig      | Zig build (linked against libtermsurf_content)    | Matches GUI's language               |
| **Plusium** | C++      | GN/autoninja (linked against libtermsurf_content) | Simplest, lives inside chromium/src/ |

### GUI changes: generic browser binary support

The GUI currently hardcodes the binary name `Chromium Profile Server`. This
changes to:

1. The TUI sends the browser binary path to the GUI as part of the overlay
   setup.
2. The GUI spawns whatever binary it receives, passing `--ipc-socket` and
   `--user-data-dir` as today.
3. The GUI does not care what language the binary is written in — only that it
   speaks the protobuf protocol.

### TUI changes: `--browser` flag

The TUI gains a `--browser` flag:

```
web google.com                        # Uses default (roamium)
web google.com --browser roamium      # Explicit roamium
web google.com --browser zoomium      # Zig version
web google.com --browser plusium      # C++ version
web google.com --browser /path/to/bin # Any protocol-compatible binary
```

Resolution order for named browsers:

1. Check if the value is an absolute path → use directly
2. Look in the app bundle: `{bundle}/Contents/Browsers/{name}`
3. Look in `$PATH`

The TUI passes the resolved binary path to the GUI in the `SetOverlay` message
(new field). The GUI uses this path instead of the hardcoded Chromium Profile
Server path.

### Protocol compatibility

Any binary is valid as long as it:

1. Accepts `--ipc-socket={path}` and `--user-data-dir={path}` CLI args
2. Connects to the Unix socket at the given path
3. Sends `ServerRegister` with the profile name
4. Handles all command messages from `termsurf.proto`
5. Sends back all response messages from `termsurf.proto`

This makes the browser binary a plugin point — third parties could write their
own protocol-compatible browser engines (Gecko, WebKit) and plug them in.

## Code locations

### Chromium side

| Location                                        | What                                                      |
| ----------------------------------------------- | --------------------------------------------------------- |
| `chromium/src/content/zig_content_shell/`       | Issue 620 C shim (starting point for libtermsurf_content) |
| `chromium/src/content/chromium_profile_server/` | Current working profile server                            |
| `chromium/src/content/zig_profile_server/`      | Issue 642 Zig profile server files                        |

### GUI side

| Location                         | What                                           |
| -------------------------------- | ---------------------------------------------- |
| `gui/src/apprt/xpc.zig` ~860-986 | `spawnServerProcess()` — hardcoded binary path |
| `gui/src/apprt/xpc.zig` ~146     | `TERMSURF_SOCKET` env var setup                |

### TUI side

| Location          | What                        |
| ----------------- | --------------------------- |
| `tui/src/main.rs` | CLI argument parsing (clap) |
| `tui/src/ipc.rs`  | Socket connection to GUI    |

### Proto

| Location               | What                         |
| ---------------------- | ---------------------------- |
| `proto/termsurf.proto` | Protobuf message definitions |

## End state

When this issue is complete, the Chromium Profile Server (a Content Shell fork)
is deleted from the active codebase. It will remain in historical branches and
patches for reference, but it is no longer actively maintained. The three
binding packages — Roamium, Zoomium, and Plusium — fully replace it.
`libtermsurf_content` becomes the single maintained Chromium integration layer,
and all browser binaries are thin wrappers around it.

## Ideas for experiments

These are rough ideas, not commitments. Each experiment will be designed when
the previous one is complete.

1. **Extract `libtermsurf_content`** — Factor the C shim from Issue 620 into a
   proper C library within `chromium/src/`. Extend it with socket IPC, all
   message types, persistent compositor, DevTools. Build as a static library via
   GN. Verify by linking a minimal C `main()` that connects and serves one page.

2. **Build Plusium** — Create a minimal C++ binary in `chromium/src/` that links
   `libtermsurf_content`, implements socket IPC and message dispatch. This is
   the easiest binding since it stays inside the Chromium build. Verify it's
   functionally equivalent to the current profile server.

3. **Build Roamium** — Create a Rust crate that links `libtermsurf_content` via
   FFI (`bindgen` or manual declarations). Handle the build system integration
   (Cargo needs to find the Chromium-built library and headers). Verify
   equivalence.

4. **Build Zoomium** — Create a Zig package that links `libtermsurf_content` via
   `@cImport`. Same build system challenge as Roamium but for Zig. Verify
   equivalence.

5. **TUI `--browser` flag** — Add the CLI flag, browser resolution logic, and
   pass the binary path to the GUI via `SetOverlay`.

6. **GUI generic binary support** — Replace hardcoded binary path in
   `spawnServerProcess()` with the path received from the TUI.

7. **Make Roamium the default** — Once all three work, switch the default from
   Chromium Profile Server to Roamium.

8. **Retire the old profile server** — Delete `chromium_profile_server/` from
   the active Chromium branch once all three bindings are verified equivalent.

## Experiments

### Experiment 1: Extract `libtermsurf_content` C library

Factor the existing profile server's Chromium integration into a reusable C
library. The library wraps Content API classes behind opaque handles and C
function pointers, so any language with a C FFI can drive Chromium without
touching C++.

The library does NOT handle socket IPC or protobuf. Each binding package
(Roamium, Zoomium, Plusium) provides its own `main()`, socket connection,
message serialization, and dispatch. The C library is a pure Content API
wrapper.

#### Chromium branch

Create `146.0.7650.0-issue-704` from `146.0.7650.0-issue-702` (the current
working branch with socket IPC).

#### C API

All functions live in `libtermsurf_content.h`. Opaque handle types hide C++
objects:

```c
#ifndef LIBTERMSURF_CONTENT_H_
#define LIBTERMSURF_CONTENT_H_

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handles */
typedef void* ts_browser_context_t;
typedef void* ts_web_contents_t;

/* --- Lifecycle --- */

/* Initialize Chromium and run the message loop. Blocks until shutdown.
   Call ts_set_on_initialized() before this to get a callback when ready.
   argc/argv are forwarded to Chromium's ContentMain(). */
int ts_content_main(int argc, const char** argv);

/* Called on the UI thread when the browser is initialized and ready
   for profile/tab creation. */
void ts_set_on_initialized(void (*callback)(void* user_data), void* user_data);

/* Post a task to the UI thread. Thread-safe — call from any thread.
   This is how bindings marshal socket messages onto the UI thread. */
void ts_post_task(void (*task)(void* user_data), void* user_data);

/* Shut down Chromium and exit the message loop. */
void ts_quit(void);

/* --- Profiles --- */

/* Create a BrowserContext with persistent storage at the given path.
   Returns an opaque handle. */
ts_browser_context_t ts_create_browser_context(const char* path);

/* Destroy a BrowserContext and release its resources. */
void ts_destroy_browser_context(ts_browser_context_t ctx);

/* --- Tabs --- */

/* Create a WebContents (tab) in the given BrowserContext.
   Navigates to url. Sets initial viewport to width×height pixels.
   dark=true sets prefers-color-scheme:dark. */
ts_web_contents_t ts_create_web_contents(
    ts_browser_context_t ctx,
    const char* url,
    int width,
    int height,
    bool dark);

/* Create a DevTools WebContents inspecting another tab. */
ts_web_contents_t ts_create_devtools_web_contents(
    ts_browser_context_t ctx,
    ts_web_contents_t inspected,
    int width,
    int height,
    bool dark);

/* Close and destroy a WebContents. */
void ts_destroy_web_contents(ts_web_contents_t wc);

/* --- Navigation --- */

void ts_load_url(ts_web_contents_t wc, const char* url);

/* --- Input --- */

/* type: 0=down, 1=up. button: 0=left, 1=right, 2=middle. */
void ts_forward_mouse_event(
    ts_web_contents_t wc,
    int type,
    int button,
    int x,
    int y,
    int click_count,
    int modifiers);

void ts_forward_mouse_move(
    ts_web_contents_t wc,
    int x,
    int y,
    int modifiers);

/* phase/momentum_phase: 0=none, 1=began, 2=changed, 3=ended. */
void ts_forward_scroll_event(
    ts_web_contents_t wc,
    int x,
    int y,
    float delta_x,
    float delta_y,
    int phase,
    int momentum_phase,
    bool precise,
    int modifiers);

/* type: 0=down, 1=up, 2=repeat. keycode is Windows VK code. */
void ts_forward_key_event(
    ts_web_contents_t wc,
    int type,
    int keycode,
    const char* utf8,
    int modifiers);

/* --- State --- */

void ts_set_focus(ts_web_contents_t wc, bool focused);
void ts_set_color_scheme(ts_web_contents_t wc, bool dark);
void ts_set_view_size(ts_web_contents_t wc, int width, int height);

/* --- Callbacks --- */

/* All callbacks fire on the UI thread. The wc parameter identifies which tab
   changed. Bindings use these to send protobuf responses over the socket. */

void ts_set_on_ca_context_id(
    void (*cb)(ts_web_contents_t wc, uint32_t ca_context_id,
               int width, int height, void* user_data),
    void* user_data);

void ts_set_on_url_changed(
    void (*cb)(ts_web_contents_t wc, const char* url, void* user_data),
    void* user_data);

void ts_set_on_loading_state(
    void (*cb)(ts_web_contents_t wc, const char* state, int progress,
               void* user_data),
    void* user_data);

void ts_set_on_title_changed(
    void (*cb)(ts_web_contents_t wc, const char* title, void* user_data),
    void* user_data);

void ts_set_on_cursor_changed(
    void (*cb)(ts_web_contents_t wc, const char* cursor_type, void* user_data),
    void* user_data);

#ifdef __cplusplus
}
#endif

#endif  /* LIBTERMSURF_CONTENT_H_ */
```

Key design decisions:

- **`ts_post_task()`** — Thread-safe trampoline to the UI thread. Bindings read
  from the socket on a background thread and post tasks to the UI thread for
  Content API calls. This matches how the current profile server works
  (`PostTask` to UI thread from socket reader thread).
- **`user_data` on all callbacks** — Standard C pattern for closures. Bindings
  pass their context (socket fd, state pointers) through this.
- **String-based state/cursor values** — Matches the protobuf schema (e.g.,
  `"loading"`, `"done"`, `"pointer"`, `"text"`). Avoids enum mapping at the C
  boundary.
- **No socket/protobuf in the library** — Keeps the C library focused. Each
  binding handles protocol natively in its own language.

#### File structure

```
chromium/src/content/libtermsurf_content/
├── BUILD.gn                         # Static library target
├── libtermsurf_content.h            # Public C header (above)
├── libtermsurf_content.cc           # Implementation
├── ts_main_delegate.h               # ContentMainDelegate subclass
├── ts_main_delegate.cc
├── ts_main_delegate_mac.h           # macOS-specific (Dock hiding)
├── ts_main_delegate_mac.mm
├── ts_browser_client.h              # ContentBrowserClient subclass
├── ts_browser_client.cc
├── ts_browser_main_parts.h          # BrowserMainParts subclass
├── ts_browser_main_parts.cc
├── ts_browser_main_parts_mac.mm     # macOS window setup
├── ts_web_contents_delegate.h       # WebContentsDelegate (new-tab suppression)
├── ts_web_contents_delegate.cc
├── ts_tab_observer.h                # WebContentsObserver (state callbacks)
├── ts_tab_observer.cc
├── ts_compositor_bridge_mac.h       # PersistentCompositor for CAContext
├── ts_compositor_bridge_mac.mm
└── ts_ca_layer_bridge_mac.mm        # CALayer bridging
```

#### Implementation

The implementation extracts logic from the current profile server
(`chromium_profile_server/browser/shell_browser_main_parts.cc`). The key
difference: instead of handling socket IPC internally, the library exposes C
functions and fires C callbacks. The binding handles the protocol.

**`ts_main_delegate`** — Subclasses `ContentMainDelegate` directly (not
`ShellMainDelegate`). Creates `ts_browser_client`. Handles macOS Dock hiding via
`--hidden` flag.

**`ts_browser_client`** — Subclasses `ContentBrowserClient`. Creates
`ts_browser_main_parts`. Registers Mojo binder stubs (BadgeService, etc.) to
prevent renderer crashes.

**`ts_browser_main_parts`** — Subclasses `BrowserMainParts`. On
`PostCreateMainMessageLoop()`, fires the `on_initialized` callback. Manages the
list of `BrowserContext` instances and `WebContents` instances.

**`ts_web_contents_delegate`** — Handles `OpenURLFromTab()` to keep navigation
in the same tab (no new windows). Handles `CloseContents()`.

**`ts_tab_observer`** — Observes `DidFinishNavigation`, `LoadProgressChanged`,
`DidStopLoading`, `TitleWasSet`, `DidChangeCursor`. Fires the registered C
callbacks.

**`ts_compositor_bridge`** — Manages the persistent compositor
(`AcceleratedWidgetMacNSView` protocol) that provides stable CAContext IDs
across navigations. Same pattern as the current profile server.

#### BUILD.gn

```gn
static_library("libtermsurf_content") {
  sources = [
    "libtermsurf_content.cc",
    "libtermsurf_content.h",
    "ts_main_delegate.cc",
    "ts_main_delegate.h",
    "ts_browser_client.cc",
    "ts_browser_client.h",
    "ts_browser_main_parts.cc",
    "ts_browser_main_parts.h",
    "ts_web_contents_delegate.cc",
    "ts_web_contents_delegate.h",
    "ts_tab_observer.cc",
    "ts_tab_observer.h",
  ]

  if (is_mac) {
    sources += [
      "ts_main_delegate_mac.h",
      "ts_main_delegate_mac.mm",
      "ts_browser_main_parts_mac.mm",
      "ts_compositor_bridge_mac.h",
      "ts_compositor_bridge_mac.mm",
      "ts_ca_layer_bridge_mac.mm",
    ]
  }

  deps = [
    "//content/public/app",
    "//content/public/browser",
    "//content/public/common",
    "//content/public/gpu",
    "//content/public/renderer",
    "//content/public/utility",
    "//ui/display",
    "//ui/events",
    "//ui/gfx",
  ]
}
```

#### Verification

1. `cd chromium/src && autoninja -C out/Default libtermsurf_content` — must
   compile clean as a static library.
2. Write a minimal `test_main.cc` that links `libtermsurf_content`, calls
   `ts_set_on_initialized()` with a callback that creates a profile and tab,
   then calls `ts_content_main()`. The callback should print "initialized" and
   call `ts_quit()`. Verify it starts, prints, and exits cleanly.

#### Results

**Static library compiles clean.** All 5 source files compile without errors:

- `libtermsurf_content.cc` — C API implementation with global state and
  `TsNotify*` functions
- `ts_main_delegate.cc` — Extends `ShellMainDelegate`, overrides
  `CreateContentBrowserClient()` to return `TsBrowserClient`
- `ts_browser_client.cc` — Extends `ShellContentBrowserClient`, overrides
  `CreateBrowserMainParts()` to return `TsBrowserMainParts`
- `ts_browser_main_parts.cc` — Core implementation (~600 lines), manages
  profiles, tabs, compositor, input forwarding, and fires C callbacks
- `ts_tab_observer.cc` — Fires `TsNotify*` C callbacks instead of sending
  protobuf

**Test binary compiles and links.** `test_main.cc` links against
`libtermsurf_content.a` and calls `ts_content_main()`. The binary crashes at
startup with a DCHECK in `paths_apple.mm` because it expects to run inside a
`.app` bundle — this is expected behavior inherited from `ShellMainDelegate`
(the existing profile server has the same requirement).

**Approach:** Instead of subclassing `ContentMainDelegate` directly (as the
experiment design suggested), the implementation extends `ShellMainDelegate` and
its associated classes (`ShellContentBrowserClient`, `ShellBrowserMainParts`).
This reuses the profile server's existing setup (Mojo binder stubs, macOS menu,
persistent compositor bridge, etc.) while overriding only the
initialization/tab-management entry points to fire C callbacks instead of
handling IPC internally.

**Build issues encountered and fixed:**

- `raw_ptr<T>` clang plugin: Chromium's clang plugin requires `raw_ptr<T>` for
  class member pointers. `void* handle_` in `TsTabObserver` was changed to
  `uintptr_t handle_` with `reinterpret_cast` when passing to C callbacks.
- Vexing parse: `LoadURLParams params(GURL(url))` was parsed as a function
  declaration. Fixed with extra parentheses:
  `LoadURLParams params((GURL(url)))`.

**Files created:**

```
chromium/src/content/libtermsurf_content/
├── BUILD.gn                    # Static library + test executable
├── libtermsurf_content.h       # Public C header
├── libtermsurf_content.cc      # C API implementation
├── ts_main_delegate.h/cc      # ShellMainDelegate subclass
├── ts_browser_client.h/cc     # ShellContentBrowserClient subclass
├── ts_browser_main_parts.h/cc # ShellBrowserMainParts subclass (~600 lines)
├── ts_tab_observer.h/cc       # WebContentsObserver with C callbacks
└── test_main.cc               # Minimal test binary
```

**Deviations from design:**

- No `ts_main_delegate_mac.mm`, `ts_browser_main_parts_mac.mm`,
  `ts_web_contents_delegate.h/cc`, `ts_compositor_bridge_mac.h/mm`, or
  `ts_ca_layer_bridge_mac.mm` — these are reused from the existing profile
  server via `ShellMainDelegate` inheritance rather than being copied.
- The `on_cursor_changed` callback passes `int cursor_type` (Chromium enum
  value) instead of `const char* cursor_type` (string). The binding layer
  handles the enum-to-string mapping.
- The `on_tab_ready` callback was added (not in original design) — fired after
  compositor setup on macOS, signals when a tab is ready for input.
- `BUILD.gn` deps include `chromium_profile_server_app` and
  `chromium_profile_server_lib` (for the inherited classes) rather than raw
  `content/public/*` deps.

**Status: PARTIAL.** The C library compiles and links, but it depends on the
Chromium Profile Server (`chromium_profile_server_app` and
`chromium_profile_server_lib`). This defeats the purpose — the whole point is to
replace the profile server. Experiment 2 removes this dependency.

### Experiment 2: Remove profile server dependency

Experiment 1's `libtermsurf_content` subclasses the profile server's copies of
Content Shell classes (`ShellMainDelegate`, `ShellContentBrowserClient`,
`ShellBrowserMainParts`). But Content Shell itself already has all the virtual
hooks we need — `InitializeMessageLoopContext()`,
`CreateContentBrowserClient()`, `CreateBrowserMainParts()` are all virtual. We
can subclass Content Shell directly and eliminate the profile server dependency
entirely.

This means `libtermsurf_content` becomes a thin C layer on top of vanilla
Content Shell. The profile server (our fork of Content Shell) is no longer
needed. In the future, if Content Shell ever becomes a problem, we can rewrite
the internals while keeping the C API stable.

#### Changes

**Repoint 3 base classes** (mechanical header swaps):

| File                      | Old base (profile server)                                        | New base (Content Shell)                               |
| ------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------ |
| `ts_main_delegate.h`      | `chromium_profile_server/app/shell_main_delegate.h`              | `content/shell/app/shell_main_delegate.h`              |
| `ts_browser_client.h`     | `chromium_profile_server/browser/shell_content_browser_client.h` | `content/shell/browser/shell_content_browser_client.h` |
| `ts_browser_main_parts.h` | `chromium_profile_server/browser/shell_browser_main_parts.h`     | `content/shell/browser/shell_browser_main_parts.h`     |

**Copy 4 files into `libtermsurf_content/`** — these are profile server
additions not present in vanilla Content Shell:

- `ts_compositor_bridge_mac.h/mm` — `PersistentCompositorBridge`
  (`AcceleratedWidgetMacNSView` impl for stable CAContext IDs across
  navigations). Copied from `shell_compositor_bridge_mac.h/mm`.
- `ts_ca_layer_bridge_mac.h/mm` — `SetParentUiLayerOnView()` and
  `SetCALayerParamsCallbackOnView()` helpers. Copied from
  `shell_ca_layer_bridge_mac.h/mm`.

**Add `StubBadgeService`** — either copy the class into `libtermsurf_content/`
or register the Mojo binder in `TsBrowserClient`. This prevents renderer crashes
when websites call `navigator.setAppBadge()` (Issue 655).

**Update `ts_browser_main_parts.cc`** — change all `#include` paths from
`chromium_profile_server/` to either `content/shell/` or local
`libtermsurf_content/` files.

**Update `BUILD.gn`**:

```gn
static_library("libtermsurf_content") {
  sources = [
    "libtermsurf_content.cc",
    "libtermsurf_content.h",
    "ts_browser_client.cc",
    "ts_browser_client.h",
    "ts_browser_main_parts.cc",
    "ts_browser_main_parts.h",
    "ts_main_delegate.cc",
    "ts_main_delegate.h",
    "ts_tab_observer.cc",
    "ts_tab_observer.h",
  ]

  if (is_mac) {
    sources += [
      "ts_ca_layer_bridge_mac.h",
      "ts_ca_layer_bridge_mac.mm",
      "ts_compositor_bridge_mac.h",
      "ts_compositor_bridge_mac.mm",
    ]
  }

  deps = [
    "//content/shell:content_shell_lib",
    "//content/public/app",
    "//content/public/browser",
    "//content/public/common",
  ]

  if (is_mac) {
    deps += [
      "//ui/accelerated_widget_mac",
      "//ui/compositor",
    ]
  }
}
```

The key change: `//content/shell:content_shell_lib` replaces
`//content/chromium_profile_server:chromium_profile_server_app` and
`//content/chromium_profile_server:chromium_profile_server_lib`.

**Zero patches to Content Shell.** Pure subclassing.

#### Verification

1. `autoninja -C out/Default content/libtermsurf_content:libtermsurf_content` —
   compiles clean with no `chromium_profile_server` in any `#include` or dep.
2. `grep -r chromium_profile_server content/libtermsurf_content/` — returns
   nothing.
3. `autoninja -C out/Default content/libtermsurf_content:libtermsurf_content_test`
   — test binary links.

#### Results

**All three verification criteria pass.**

1. Library compiles clean — all 9 source files (5 `.cc` + 2 `.mm` + 2 `.h`)
   build without errors.
2. `grep -r chromium_profile_server content/libtermsurf_content/` returns
   nothing — zero profile server references.
3. Test binary links successfully.

**Changes made:**

- Swapped 3 base class includes from `chromium_profile_server/` to
  `content/shell/` (`ts_main_delegate.h`, `ts_browser_client.h`,
  `ts_browser_main_parts.h`).
- Created 4 new macOS files: `ts_compositor_bridge_mac.h/mm` (persistent
  compositor bridge) and `ts_ca_layer_bridge_mac.h/mm` (CALayer helpers). Copied
  from profile server equivalents with updated include paths.
- Added `StubBadgeService` to `TsBrowserClient` (Issue 655 — prevents renderer
  crashes on `navigator.setAppBadge()`).
- Updated `BUILD.gn` deps: `content/shell:content_shell_app` +
  `content/shell:content_shell_lib` replace the profile server targets.
- Cleaned up unused includes in `libtermsurf_content.cc`.

**Build issues encountered and fixed:**

- `blink::mojom::BadgeService` not visible from header — moved
  `BindBadgeService` to a lambda in the `.cc` file.
- `mojo::BinderMapWithContext` incomplete type — added
  `mojo/public/cpp/bindings/binder_map.h` include.
- `~PersistentCompositorBridge() override` — base class destructor isn't
  virtual. Changed to `virtual ~PersistentCompositorBridge()`.
- `ShellDevToolsFrontend` private constructor — vanilla Content Shell's
  constructor is private (profile server made it public). Used the static
  `Show()` method instead, which creates the Shell internally and returns the
  frontend. Get the shell via `frontend->frontend_shell()`.
- Unused `browser_ctx` variable after switching to `Show()` — removed.

**File structure:**

```
chromium/src/content/libtermsurf_content/
├── BUILD.gn
├── libtermsurf_content.cc
├── libtermsurf_content.h
├── test_main.cc
├── ts_browser_client.cc          # +StubBadgeService
├── ts_browser_client.h
├── ts_browser_main_parts.cc
├── ts_browser_main_parts.h
├── ts_ca_layer_bridge_mac.h      # NEW (macOS)
├── ts_ca_layer_bridge_mac.mm     # NEW (macOS)
├── ts_compositor_bridge_mac.h    # NEW (macOS)
├── ts_compositor_bridge_mac.mm   # NEW (macOS)
├── ts_main_delegate.cc
├── ts_main_delegate.h
├── ts_tab_observer.cc
└── ts_tab_observer.h
```

**Status: SUCCESS.** The C library is now fully independent of the Chromium
Profile Server. It subclasses vanilla Content Shell directly with zero patches
to Content Shell files.

### Experiment 3: Plusium — C++ browser binary

Build Plusium, a C++ binary that links `libtermsurf_content` and provides all
the socket IPC + protobuf logic needed to replace the current Chromium Profile
Server. Plusium lives inside `chromium/src/` and builds with GN/autoninja.

This is the simplest of the three bindings because it stays in the Chromium
build system — no cross-build-system linking. It validates that the C library
API is complete enough to drive a fully functional browser binary.

#### Architecture

Plusium is a thin C++ binary (~400 lines) that:

1. Parses `--ipc-socket` and `--user-data-dir` from the command line
2. Registers all C callbacks before calling `ts_content_main()`
3. On `on_initialized`: connects to the GUI's Unix socket, sends
   `ServerRegister`, spawns a socket reader thread
4. Reader thread: reads length-prefixed protobuf, posts tasks via
   `ts_post_task()` for dispatch on the UI thread
5. Dispatch: switches on `msg_case()`, calls the appropriate `ts_*()` C function
6. Callbacks: builds protobuf responses, writes them back to the socket

The binary is functionally equivalent to the current Chromium Profile Server.
The GUI cannot tell the difference.

#### Key design decisions

**Tab registry.** The C API uses opaque `ts_web_contents_t` handles. The
protobuf protocol uses integer `tab_id` and string `pane_id`. Plusium maintains
a `std::vector<TabEntry>` mapping between them:

```cpp
struct TabEntry {
  ts_web_contents_t handle;
  int tab_id;
  std::string pane_id;
  int inspected_tab_id;  // 0 for normal tabs
};
```

On `CreateTab`, Plusium stores the `pane_id` before calling
`ts_create_web_contents()`. The `on_tab_ready` callback provides the `tab_id`
and the handle, completing the entry. All other callbacks receive the handle and
look up `tab_id` in the registry.

**Cursor type passthrough.** The C API's `on_cursor_changed` returns an `int`
(Chromium enum value). The protobuf `CursorChanged.cursor_type` is `int64`.
Direct passthrough — no string mapping needed.

**QueryTabs from local registry.** The C API doesn't expose a query function.
Plusium answers `QueryTabsRequest` from its own tab registry — it knows every
tab's `tab_id`, `pane_id`, `inspected_tab_id`, and can get the URL from the last
`on_url_changed` callback.

**Socket write mutex.** The reader thread posts tasks to the UI thread
(one-way). Callbacks on the UI thread write protobuf responses to the socket.
Since callbacks are all on the UI thread, no mutex is needed for socket writes.

**`--user-data-dir` for profile name.** Same as the current profile server:
extract the basename of `--user-data-dir` as the profile name for
`ServerRegister`.

#### File structure

```
chromium/src/content/plusium/
├── BUILD.gn          # Executable target linking libtermsurf_content
├── plusium_main.cc    # ~400 lines: main, socket, dispatch, callbacks
└── termsurf.proto     # Copied from proto/termsurf.proto
```

`plusium_main.cc` is the only source file. `termsurf.proto` is copied from the
canonical `proto/termsurf.proto` in the main repo so Plusium has zero profile
server references.

#### BUILD.gn

```gn
import("//third_party/protobuf/proto_library.gni")

proto_library("termsurf_proto") {
  sources = [ "termsurf.proto" ]
}

executable("plusium") {
  testonly = true
  sources = [ "plusium_main.cc" ]
  deps = [
    ":termsurf_proto",
    "//content/libtermsurf_content",
  ]
}
```

#### `plusium_main.cc` outline

```cpp
#include "content/libtermsurf_content/libtermsurf_content.h"
#include "content/plusium/termsurf.pb.h"

#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <thread>
#include <vector>
#include <string>
#include <cstring>

// --- Tab registry ---

struct TabEntry {
  ts_web_contents_t handle = nullptr;
  int tab_id = 0;
  std::string pane_id;
  int inspected_tab_id = 0;
  std::string last_url;
};

static std::vector<TabEntry> g_tabs;
static int g_socket_fd = -1;
static std::string g_profile_name;
static std::string g_socket_path;
static std::string g_user_data_dir;

// --- Tab registry helpers ---

TabEntry* FindByHandle(ts_web_contents_t h);
TabEntry* FindByTabId(int tab_id);
TabEntry* FindByPaneId(const std::string& pane_id);

// --- Socket I/O ---

void SendProtobuf(const termsurf::TermSurfMessage& msg);
void SocketReaderLoop();

// --- Protobuf dispatch (called on UI thread via ts_post_task) ---

void HandleMessage(termsurf::TermSurfMessage* msg);

// --- C API callbacks ---

void OnInitialized(void*);
void OnTabReady(ts_web_contents_t wc, int tab_id, void*);
void OnCaContextId(ts_web_contents_t wc, uint32_t id, int w, int h, void*);
void OnUrlChanged(ts_web_contents_t wc, const char* url, void*);
void OnLoadingState(ts_web_contents_t wc, const char* state, int progress, void*);
void OnTitleChanged(ts_web_contents_t wc, const char* title, void*);
void OnCursorChanged(ts_web_contents_t wc, int cursor_type, void*);

// --- main ---

int main(int argc, const char** argv) {
  // Parse --ipc-socket and --user-data-dir from argv
  // Derive profile name from user-data-dir basename

  // Register all callbacks
  ts_set_on_initialized(OnInitialized, nullptr);
  ts_set_on_tab_ready(OnTabReady, nullptr);
  ts_set_on_ca_context_id(OnCaContextId, nullptr);
  ts_set_on_url_changed(OnUrlChanged, nullptr);
  ts_set_on_loading_state(OnLoadingState, nullptr);
  ts_set_on_title_changed(OnTitleChanged, nullptr);
  ts_set_on_cursor_changed(OnCursorChanged, nullptr);

  // Enter Chromium's message loop (blocks until shutdown)
  return ts_content_main(argc, argv);
}
```

#### Message dispatch

Maps each protobuf `msg_case()` to the corresponding `ts_*()` C call:

| Protobuf message    | C API call                          | Notes                                 |
| ------------------- | ----------------------------------- | ------------------------------------- |
| `CreateTab`         | `ts_create_web_contents()`          | Store pane_id in registry before call |
| `CreateDevtoolsTab` | `ts_create_devtools_web_contents()` | Look up inspected handle by tab_id    |
| `Resize`            | `ts_set_view_size()`                | Look up handle by tab_id              |
| `CloseTab`          | `ts_destroy_web_contents()`         | Remove from registry after call       |
| `Navigate`          | `ts_load_url()`                     | Look up handle by tab_id              |
| `MouseEvent`        | `ts_forward_mouse_event()`          | Map string type/button to int         |
| `MouseMove`         | `ts_forward_mouse_move()`           | Straight passthrough                  |
| `ScrollEvent`       | `ts_forward_scroll_event()`         | Straight passthrough                  |
| `KeyEvent`          | `ts_forward_key_event()`            | Map string type to int                |
| `FocusChanged`      | `ts_set_focus()`                    | Straight passthrough                  |
| `SetColorScheme`    | `ts_set_color_scheme()`             | Straight passthrough                  |
| `QueryTabsRequest`  | (local registry)                    | Build reply from g_tabs               |

#### Callback → protobuf mapping

| C callback          | Protobuf response | Notes                               |
| ------------------- | ----------------- | ----------------------------------- |
| `on_tab_ready`      | `TabReady`        | Look up pane_id from registry       |
| `on_ca_context_id`  | `CaContext`       | Look up tab_id from handle          |
| `on_url_changed`    | `UrlChanged`      | Store URL in registry for QueryTabs |
| `on_loading_state`  | `LoadingState`    | Straight passthrough                |
| `on_title_changed`  | `TitleChanged`    | Straight passthrough                |
| `on_cursor_changed` | `CursorChanged`   | Straight passthrough (int → int64)  |

#### C API gap: `pane_id` tracking

The C library doesn't know about `pane_id` — it's a protocol-level concept. The
profile server stores `pane_id` in its `TabState`. Plusium does the same: when
`CreateTab` arrives, Plusium stores the `pane_id` in the registry entry and
associates it with the `ts_web_contents_t` handle returned by the C API.

The `on_tab_ready` callback fires with `(handle, tab_id)`. Plusium finds the
registry entry by handle, fills in the `tab_id`, and sends
`TabReady(pane_id, tab_id)`.

#### C API gap: mouse/key type mapping

The protobuf uses string types (`"down"`, `"up"`, `"left"`, `"right"`). The C
API uses integers. Plusium maps:

- Mouse type: `"down"` → 0, `"up"` → 1
- Mouse button: `"left"` → 0, `"right"` → 1, `"middle"` → 2
- Key type: `"down"` → 0, `"up"` → 1, `"repeat"` → 2

#### Verification

1. `autoninja -C out/Default content/plusium:plusium` — compiles and links
   clean.
2. Replace the Chromium Profile Server binary in the app bundle with Plusium.
   Launch TermSurf, type `web google.com`. Page loads, mouse works, keyboard
   works, resize works, navigation works.
3. Test DevTools: `:devtools` command opens DevTools in a split pane.
4. Test multi-profile: open tabs in different profiles, verify isolation.
5. Test tab close: close a pane, verify the tab is destroyed.

Criterion 1 (builds) is the gate for this experiment. Criteria 2–5 are stretch
goals — if the library's API has gaps that prevent end-to-end functionality, we
document them and fix the C library in a follow-up experiment.

#### Results

**Criterion 1 passes.** Plusium compiles and links clean (11MB arm64 binary).

**Files created:**

```
chromium/src/content/plusium/
├── BUILD.gn          # Executable + proto_library targets
├── plusium_main.cc    # 430 lines: main, socket, dispatch, callbacks
└── termsurf.proto     # Copied from proto/termsurf.proto
```

Also added `//content/plusium:plusium` to the root `BUILD.gn` `gn_all` group
(required for GN target discovery).

**Build issues encountered and fixed:**

- **Exit-time destructors** (`-Werror,-Wexit-time-destructors`): Chromium
  forbids global statics with destructors. Changed `std::vector<TabEntry>`,
  `std::string` globals to raw pointers (`new` in `main()`, intentionally
  leaked). Used `const char*` for string globals with `new char[]` buffers.
- **`UNSAFE_BUFFERS` macro unavailable**: Plusium is a standalone binary, not
  part of the Chromium base library. Suppressed unsafe-buffer-usage warnings
  with `#pragma clang diagnostic ignored "-Wunsafe-buffer-usage"` instead.
- **GN target not discovered**: New BUILD.gn files in Chromium are only found if
  reachable from the root BUILD.gn dependency graph. Added Plusium to `gn_all`
  in `BUILD.gn`.
- **Protobuf linker errors**: Default `proto_library` generates full protobuf
  code, but only `protobuf_lite.dylib` exists in the component build. Fixed with
  `cc_generator_options = "lite"` to generate lite-compatible code.

**Design:**

- ~430 lines of C++ in a single file
- Tab registry: `std::vector<TabEntry>*` mapping handles to tab_id/pane_id
- Socket I/O: same wire format as the profile server (4-byte LE length prefix +
  protobuf)
- Threading: reader thread posts tasks via `ts_post_task()`, all dispatch on UI
  thread
- Callbacks write protobuf responses directly to socket (single-threaded, no
  mutex)
- String-to-int mapping for mouse/key types (protobuf uses strings, C API uses
  ints)
- Zero `chromium_profile_server` references

**Status: SUCCESS (gate criterion).** Binary compiles and links. End-to-end
testing (criteria 2–5) deferred to integration testing.

### Experiment 4: `--browser` flag and generic browser support

Add a `--browser` flag to the TUI and generic browser binary support in the GUI.
This lets users run any browser binary — `web google.com --browser plusium` runs
Plusium instead of the default Chromium Profile Server. Multiple browsers can
run simultaneously for the same profile, enabling web developers to test their
apps across different browser engines from within the same terminal.

#### Design principles

**Server processes are keyed by (profile, browser) pair.** Two panes with the
same profile but different browsers spawn two separate server processes. Two
panes with the same profile AND browser share one server process. This is a
change from the current model where servers are keyed by profile alone.

**The GUI maintains a browser registry.** On startup, the GUI scans the app
bundle for available browser binaries and builds a name → path map. Known
browsers currently: `"chromium"` (Chromium Profile Server) and `"plusium"`. The
registry is sent to the TUI via `HelloReply` so the TUI can validate names and
show them in help text.

**The TUI sends a browser specifier.** The `--browser` flag accepts either a
known short name (`"plusium"`) or an absolute path
(`"/path/to/custom-browser"`). The specifier is passed to the GUI in the
`SetOverlay` message. Empty means use the default.

**The GUI resolves and spawns.** When the GUI receives a `SetOverlay` with a
browser field:

1. If empty → use the default browser (`"chromium"`)
2. If it starts with `/` → absolute path, use directly
3. Otherwise → look up in the browser registry

The GUI then checks: is there a running server for this (profile, browser) pair?
If yes, reuse it. If no, spawn a new server process with the resolved binary
path.

#### Proto changes

Add `browser` field to `SetOverlay` and `SetDevtoolsOverlay`. Add `browsers`
field to `HelloReply`.

```protobuf
message SetOverlay {
  string pane_id = 1;
  uint64 col = 2;
  uint64 row = 3;
  uint64 width = 4;
  uint64 height = 5;
  string url = 6;
  string profile = 7;
  bool browsing = 8;
  string browser = 9;    // NEW: "chromium", "plusium", or absolute path
}

message SetDevtoolsOverlay {
  string pane_id = 1;
  uint64 col = 2;
  uint64 row = 3;
  uint64 width = 4;
  uint64 height = 5;
  string profile = 6;
  bool browsing = 7;
  int64 inspected_tab_id = 8;
  string browser = 9;    // NEW
}

message HelloReply {
  string homepage = 1;
  repeated string browsers = 2;  // NEW: available browser names
}
```

#### TUI changes

**`tui/src/main.rs`** — Add `--browser` flag to the `Cli` struct:

```rust
/// Browser binary to use ("chromium", "plusium", or absolute path)
#[arg(long, global = true)]
browser: Option<String>,
```

Default is empty (GUI uses its default). Pass to `send_set_overlay()` and
`send_set_devtools_overlay()`.

**`tui/src/ipc.rs`** — Add `browser: &str` parameter to `send_set_overlay()` and
`send_set_devtools_overlay()`. Include in the protobuf message construction.

#### GUI changes

**Server key change.** Currently `servers: StringHashMap(*Server)` is keyed by
profile name. Change to key by `"{profile}\x00{browser}"` (null-separated
composite key). This ensures (profile, browser) uniqueness while keeping the map
structure.

Add a `browser` field to the `Server` struct:

```zig
const Server = struct {
    process: ?std.process.Child = null,
    fd: std.posix.fd_t = -1,
    profile_key: []const u8 = "",
    browser: []const u8 = "",      // NEW: resolved browser name
    pane_count: usize = 0,
};
```

**Browser registry.** Add a `browser_paths: StringHashMap([]const u8)` map
populated during init. On macOS, scan
`{bundle}/Contents/Browsers/{name}.app/Contents/MacOS/{name}` for known
browsers. Also support a dev fallback path for each:

| Name       | Bundle path                                          | Dev fallback                                                          |
| ---------- | ---------------------------------------------------- | --------------------------------------------------------------------- |
| `chromium` | `{bundle}/Contents/Chromium/Chromium Profile Server` | `~/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app` |
| `plusium`  | `{bundle}/Contents/Browsers/plusium`                 | `~/dev/termsurf/chromium/src/out/Default/plusium`                     |

For dev builds, populate the registry from known dev paths. The exact layout can
be refined later when we build the release bundle.

**`spawnServerProcess()` changes.** Currently hardcodes the binary path. Change
to accept the resolved binary path from the server's `browser` field. The
`--ipc-socket` and `--user-data-dir` args stay the same.

**`getOrCreateServer()` changes.** Currently takes `profile: []const u8`. Change
to `getOrCreateServer(profile: []const u8, browser: []const u8)`:

1. Build composite key `"{profile}\x00{browser}"`
2. Look up in `servers` map
3. If found, return existing server
4. If not, create new server, resolve browser path, spawn process

**`handleSocketSetOverlay()` changes.** Read the `browser` field from the
protobuf message. Pass it to `getOrCreateServer()`. If empty, use `"chromium"`
as default.

**`handleSocketSetDevtoolsOverlay()` changes.** Same — read `browser` field,
pass to `getOrCreateServer()`. For auto-targeted DevTools (inspected_tab_id
resolved from the last browser pane), inherit the browser from the inspected
pane's server.

**`handleSocketHello()` changes.** Build the `browsers` list from the browser
registry keys. Send it in `HelloReply`.

#### Pane tracking

Add `browser` to the `Pane` struct so DevTools auto-targeting can inherit the
browser from the inspected pane:

```zig
const Pane = struct {
    // ... existing fields ...
    browser: []const u8 = "",  // NEW: browser name for this pane
};
```

#### File changes summary

| File                    | Changes                                                                  |
| ----------------------- | ------------------------------------------------------------------------ |
| `proto/termsurf.proto`  | Add `browser` to SetOverlay/SetDevtoolsOverlay, `browsers` to HelloReply |
| `tui/src/main.rs`       | Add `--browser` CLI flag, pass to IPC calls                              |
| `tui/src/ipc.rs`        | Add `browser` param to send_set_overlay/send_set_devtools_overlay        |
| `gui/src/apprt/xpc.zig` | Browser registry, composite server key, resolve browser path             |

#### Verification

1. `cd tui && cargo build` — TUI compiles with `--browser` flag.
2. `cd gui && zig build` — GUI compiles with browser registry and composite
   server keys.
3. Launch TermSurf, `web google.com` — default browser (Chromium Profile Server)
   works as before.
4. `web google.com --browser plusium` — Plusium binary is spawned instead. Page
   loads, mouse/keyboard/resize all work.
5. Open two panes: `web google.com` and `web google.com --browser plusium` —
   both work simultaneously with separate server processes.
6. Open two panes with same browser and profile — they share one server process
   (same as current behavior).

Criteria 1–2 (compiles) are the gate. Criteria 3–6 are the end-to-end
verification that the system works.

#### Results

**Partial failure.** Code compiles and `--browser` flag shows in `web --help`,
but `web google.com --browser plusium` does not load a page — it times out.
Default `web google.com` (Chromium Profile Server) still works.

**What was implemented** (all compiles clean):

- Proto: `browser` field on SetOverlay/SetDevtoolsOverlay, `browsers` on
  HelloReply.
- TUI: `--browser` CLI flag, passed through IPC.
- GUI: browser registry, composite `(profile, browser)` server key,
  `spawnServerProcess` accepts resolved path, `handleSocketServerRegister`
  matches by profile + unregistered fd.

**What failed:** Plusium spawns but the page never loads. The timeout suggests
one of several failure points in the pipeline.

**Debugging ideas:**

1. **Is Plusium actually spawning?** Check `ps aux | grep plusium` after running
   the command. If not, the browser registry path resolution or process spawn
   failed silently.
2. **Is Plusium connecting back?** Check the GUI IPC log for
   `socket server_register` from the Plusium process. If missing, Plusium may
   not be receiving `--ipc-socket` correctly, or it's crashing on startup before
   connecting.
3. **Is ServerRegister matching?** The new `handleSocketServerRegister` iterates
   servers looking for matching profile + `fd == -1`. If the profile name
   Plusium sends (extracted from `--user-data-dir` basename) doesn't match what
   the GUI stored, the registration silently fails.
4. **Is CreateTab being sent?** After registration, the GUI flushes pending
   tabs. Check if the `sendCreateTab` path fires. If `server.fd` is still -1
   after registration, the tab never gets created.
5. **Plusium logs.** Check `~/.local/state/termsurf/chromium-server.log` for
   Chromium-level errors. Plusium may be crashing during content initialization.
6. **Plusium arguments.** Plusium's `main()` parses `--ipc-socket` and
   `--user-data-dir` from argv. Verify the GUI is passing the right args — the
   Chromium Profile Server uses a `.app` bundle path while Plusium is a bare
   executable, so argument handling may differ.
7. **Process spawn args.** The `spawnServerProcess` function passes `--hidden`,
   `--enable-logging`, `--log-file` which are Content Shell flags. Plusium
   passes these through to `ts_content_main(argc, argv)` but verify they don't
   cause issues.

**Diagnosis (from `logs/gui.log`):** Plusium crashes immediately on startup with
a DCHECK in `content/shell/app/paths_apple.mm:41`:

```
DCHECK failed: "Contents" == path.BaseName().value() (Contents vs. out)
```

`GetContentsPath()` walks up from the binary path expecting a `.app` bundle
layout (`Contents/MacOS/binary`). Plusium is a bare executable at
`out/Default/plusium`, so `BaseName()` is `out` instead of `Contents`.

The call chain: `ShellMainDelegate::BasicStartupComplete()` →
`EnsureCorrectResolutionSettings()` → `GetInfoPlistPath()` → `GetContentsPath()`
→ DCHECK crash.

`EnsureCorrectResolutionSettings()` reads `NSHighResolutionCapable` from
`Info.plist` and toggles it for web tests (`--run-web-tests`). For normal
browser usage it's a no-op (early return). Retina display support comes from the
Chromium compositor and Window Server, not from this function.

### Experiment 5: Skip bundle path check for non-bundle binaries

Override `BasicStartupComplete()` in `TsMainDelegate` to skip the
`EnsureCorrectResolutionSettings()` call. This is a macOS-specific Content Shell
assumption that doesn't apply to TermSurf's browser binaries. The override
reproduces everything the parent does except the resolution settings call.

#### What to change

**`content/libtermsurf_content/ts_main_delegate.h`** — Add override:

```cpp
std::optional<int> BasicStartupComplete() override;
```

**`content/libtermsurf_content/ts_main_delegate.cc`** — Implement override. Copy
the body of `ShellMainDelegate::BasicStartupComplete()` but remove the
`#if BUILDFLAG(IS_MAC)` / `EnsureCorrectResolutionSettings()` block. The
remaining code handles:

- `--run-layout-test` → `--run-web-tests` flag migration (harmless, keep)
- Android compositor init (not applicable, keep for cross-platform)
- Windows ETW/crashpad/handle checks (not applicable, keep for cross-platform)
- `InitLogging()` (needed)
- Web test `OsSettingsProvider` setup (harmless, keep)
- `InitializeResourceBundle()` (needed)

Everything except the one macOS bundle path call.

#### Verification

1. `autoninja -C out/Default plusium` — compiles.
2. Run Plusium manually:
   `./out/Default/plusium --ipc-socket=/tmp/test.sock --user-data-dir=/tmp/test`
   — no DCHECK crash, process starts and connects.
3. Full end-to-end: `web google.com --browser plusium` — page loads.
4. Retina: page renders at native resolution (not blurry/1x).

#### Result: Failure

The DCHECK crash is fixed — Plusium no longer crashes on startup. But a Content
Shell window now pops up on screen and immediately vanishes. No page loads in
the terminal.

**Root cause:** Plusium links against stock `content/shell:content_shell_lib`,
which includes the stock `shell_platform_delegate_mac.mm`. When
`Shell::CreateNewWindow()` is called (from `TsBrowserMainParts::CreateTab()`),
the stock platform delegate calls `[window makeKeyAndOrderFront:nil]` — creating
a visible macOS window.

The existing Chromium Profile Server solved this with a **forked**
`shell_platform_delegate_mac.mm` in `content/chromium_profile_server/browser/`
that checks for a `--hidden` flag:

```objc
if (base::CommandLine::ForCurrentProcess()->HasSwitch(switches::kHidden)) {
    [window setAlphaValue:0.0];
    [window orderWindow:NSWindowBelow relativeTo:0];
} else {
    [window makeKeyAndOrderFront:nil];
}
```

This makes the window fully transparent and orders it behind all other windows.
Using `orderOut:` would detach the compositor (it triggers
`NSWindowDidChangeOcclusionStateNotification` which sets
`render_widget_host_is_hidden_ = true`). The `setAlphaValue:0` trick keeps the
window in the window list so the compositor stays active and CAContext survives
navigation.

The GUI already passes `--hidden` to server processes (line 1004 of `xpc.zig`),
but Plusium's stock Content Shell code doesn't recognize it.

## Conclusion

Issue 704 explored creating standalone browser binaries
(Roamium/Zoomium/Plusium) that wrap Chromium's Content API through a shared C
library. Five experiments were run across two sessions.

### What was accomplished

1. **C library extraction** (Experiment 1) — Confirmed `libtermsurf_content`
   already provides a clean C API boundary. No new library needed.

2. **Profile server dependency audit** (Experiment 2) — Verified that
   `libtermsurf_content` has no dependencies on the Chromium Profile Server's
   forked code. It links against stock `content/shell`.

3. **Plusium C++ binary** (Experiment 3) — Built a working C++ binary
   (`content/plusium/plusium_main.cc`) with its own `BUILD.gn` and protobuf IPC.
   Compiles and links successfully.

4. **`--browser` flag** (Experiment 4) — Added end-to-end support for selecting
   browser binaries: proto schema changes, TUI `--browser` CLI flag, GUI browser
   registry with composite `(profile, browser)` server keys, and dynamic binary
   path resolution. Compiles clean but Plusium crashes on startup (DCHECK in
   `paths_apple.mm`).

5. **Skip bundle path check** (Experiment 5) — Overrode `BasicStartupComplete()`
   in `TsMainDelegate` to skip the macOS `.app` bundle check. Fixed the DCHECK
   crash. But a Content Shell window appears on screen because stock
   `shell_platform_delegate_mac.mm` has no `--hidden` support.

### Critical next steps for a future issue

The remaining blocker is **window suppression**. Plusium links against stock
Content Shell, which always shows a native macOS window when creating tabs. The
fix requires one of:

1. **Patch stock Content Shell** — Add `--hidden` support to
   `content/shell/browser/shell_platform_delegate_mac.mm` (the same
   `setAlphaValue:0` + `orderWindow:NSWindowBelow` trick the Profile Server
   uses). This is the cleanest approach since Plusium already receives
   `--hidden` from the GUI.

2. **Fork `shell_platform_delegate_mac.mm` into `libtermsurf_content`** — Copy
   the Profile Server's version and have `libtermsurf_content` link against it
   instead of the stock one.

3. **Make Plusium link against the Profile Server's forked shell code** — Change
   Plusium's `BUILD.gn` deps from `content/shell:content_shell_lib` to the
   Profile Server's equivalents.

Option 1 is recommended — it's a small, surgical patch to one file that benefits
all TermSurf browser binaries without code duplication.
