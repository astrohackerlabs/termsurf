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
