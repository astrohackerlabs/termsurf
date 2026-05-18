+++
status = "closed"
opened = "2026-03-05"
closed = "2026-03-05"
+++

# Issue 707: Roamium — Rust reimplementation of Plusium

## Goal

Rewrite Plusium in Rust. The new binary, Roamium, must be 100% compatible with
Plusium — same IPC protocol, same C API calls, same behavior. The GUI should not
be able to tell the difference.

## Background

### What Plusium is

Plusium (`content/plusium/plusium_main.cc`) is a ~500-line C++ binary that wraps
Chromium's Content API through `libtermsurf_content`, a C library. It does three
things:

1. **Connects to the GUI** via Unix domain socket (`--ipc-socket=` flag)
2. **Reads protobuf messages** (length-prefixed, LE u32 + payload) and
   dispatches them to the C API
3. **Sends protobuf responses** back when Chromium fires callbacks (tab ready,
   URL changed, etc.)

The C API (`libtermsurf_content.h`) exports ~20 functions with simple C types:
`int`, `const char*`, `void*`, `bool`, `uint32_t`. No C++ types cross the
boundary.

### Why Rust

- The TUI is already Rust. Roamium reuses the same toolchain, proto definitions,
  and socket framing patterns.
- `prost` (already a TUI dependency) handles protobuf. `std::os::unix::net`
  handles sockets. FFI to the C API is trivial.
- Rust's ownership model prevents the class of bugs that caused Issue 706 (void
  pointer corruption across async boundaries).

### What needs to be reimplemented

Every feature of `plusium_main.cc` (511 lines):

**Argument parsing** — Extract `--ipc-socket=` and `--user-data-dir=` from argv.
Derive profile name from the basename of the user data dir path.

**Tab registry** — A `Vec<TabEntry>` holding `handle` (void pointer from C API),
`tab_id`, `pane_id`, `inspected_tab_id`, and `last_url` for each tab. Lookup by
handle and by tab_id.

**Socket connection** — On initialized callback, connect to the GUI's Unix
socket, send `ServerRegister` with the profile name, spawn a reader thread.

**Socket reader loop** — Read from socket into a buffer, extract length-prefixed
protobuf messages, parse with prost, post each to the UI thread via
`ts_post_task`.

**Message dispatch** — Handle 12 incoming message types:

| Message             | Action                                             |
| ------------------- | -------------------------------------------------- |
| `CreateTab`         | Push entry, call `ts_create_web_contents`          |
| `CreateDevtoolsTab` | Push entry, call `ts_create_devtools_web_contents` |
| `Resize`            | `ts_set_view_size`                                 |
| `CloseTab`          | `ts_destroy_web_contents`, remove entry            |
| `Navigate`          | `ts_load_url`                                      |
| `MouseEvent`        | `ts_forward_mouse_event`                           |
| `MouseMove`         | `ts_forward_mouse_move`                            |
| `ScrollEvent`       | `ts_forward_scroll_event`                          |
| `KeyEvent`          | `ts_forward_key_event`                             |
| `FocusChanged`      | `ts_set_focus`                                     |
| `SetColorScheme`    | `ts_set_color_scheme`                              |
| `QueryTabsRequest`  | Count tabs, build reply, send                      |

**Callbacks** — 6 C callbacks registered before `ts_content_main`:

| Callback          | Sends           |
| ----------------- | --------------- |
| `OnTabReady`      | `TabReady`      |
| `OnCaContextId`   | `CaContext`     |
| `OnUrlChanged`    | `UrlChanged`    |
| `OnLoadingState`  | `LoadingState`  |
| `OnTitleChanged`  | `TitleChanged`  |
| `OnCursorChanged` | `CursorChanged` |

**String-to-int mappings** — Mouse type (`down`/`up` → 0/1), mouse button
(`left`/`right`/`middle` → 0/1/2), key type (`down`/`up`/`repeat` → 0/1/2).

**Shutdown** — When the last tab is closed, call `ts_quit()`.

### C API surface

The full API from `libtermsurf_content.h` (20 functions):

```c
// Lifecycle
int ts_content_main(int argc, const char** argv);
void ts_set_on_initialized(void (*cb)(void*), void*);
void ts_post_task(void (*task)(void*), void*);
void ts_quit(void);

// Profiles
ts_browser_context_t ts_create_browser_context(const char* path);
void ts_destroy_browser_context(ts_browser_context_t ctx);

// Tabs
ts_web_contents_t ts_create_web_contents(ctx, url, w, h, dark);
ts_web_contents_t ts_create_devtools_web_contents(ctx, tab_id, w, h, dark);
void ts_destroy_web_contents(ts_web_contents_t wc);

// Navigation
void ts_load_url(ts_web_contents_t wc, const char* url);

// Input
void ts_forward_mouse_event(wc, type, button, x, y, click_count, mods);
void ts_forward_mouse_move(wc, x, y, mods);
void ts_forward_scroll_event(wc, x, y, dx, dy, phase, momentum, precise, mods);
void ts_forward_key_event(wc, type, keycode, utf8, mods);

// State
void ts_set_focus(ts_web_contents_t wc, bool focused);
void ts_set_color_scheme(ts_web_contents_t wc, bool dark);
void ts_set_view_size(ts_web_contents_t wc, int w, int h);

// Callbacks (6 setters, each takes fn pointer + user_data)
void ts_set_on_tab_ready(...);
void ts_set_on_ca_context_id(...);
void ts_set_on_url_changed(...);
void ts_set_on_loading_state(...);
void ts_set_on_title_changed(...);
void ts_set_on_cursor_changed(...);
```

Handles (`ts_web_contents_t`, `ts_browser_context_t`) are `void*`. Roamium
stores them as `*mut c_void` and passes them back verbatim — never dereferences
them.

### Existing Rust patterns (from TUI)

The TUI (`tui/src/ipc.rs`) already has:

- **prost** for protobuf (v0.14, with `prost-build` for codegen)
- **`build.rs`** that compiles `../proto/termsurf.proto`
- **Length-prefixed framing**: 4-byte LE u32 + payload, same as Plusium
- **Reader thread**: `std::os::unix::net::UnixStream`, buffered reads, frame
  extraction
- **Message dispatch**: `match` on `msg.msg`

Roamium reuses the same proto file and framing code. The main difference is
direction: the TUI is a client that sends requests, while Roamium is a server
that receives commands and sends events.

### Build considerations

Plusium is built inside Chromium's GN build system because it links against
`libtermsurf_content` (a static library) and `content_shell_lib` (Chromium
internals). Roamium needs the same linkage.

Options:

1. **Build Roamium with Cargo, link Chromium dylibs.** Since
   `is_component_build = true`, `libtermsurf_content`'s symbols end up in shared
   libraries (`libcontent.dylib`, etc.). Roamium's `build.rs` would point
   `rustc` at `chromium/src/out/Default/` for `-L` and `-l` flags.
2. **Build Roamium from GN.** Add a GN target that invokes `cargo build` and
   links the result. More complex but integrates into the existing build.
3. **Build a small C shim.** A tiny `roamium_main.c` that calls
   `ts_content_main()` (which Chromium needs for process setup), but delegates
   all logic to a Rust library linked in. This sidesteps the question of how
   Rust calls `ts_content_main` — the C shim handles Chromium bootstrap, and the
   Rust code handles everything else.

The biggest question is `ts_content_main(argc, argv)`. This function enters
Chromium's message loop and never returns (until shutdown). Plusium calls it
from `main()`. Roamium needs to do the same, but from Rust's `main()`. This is
straightforward FFI — Rust calls the `extern "C"` function and blocks.

### Key files

- `content/plusium/plusium_main.cc` — The C++ original (511 lines)
- `content/libtermsurf_content/libtermsurf_content.h` — The C API (168 lines)
- `proto/termsurf.proto` — Protobuf message definitions
- `tui/src/ipc.rs` — TUI's socket + protobuf patterns (reference)
- `tui/Cargo.toml` — TUI's dependencies (prost, etc.)
- `tui/build.rs` — TUI's proto codegen
- `content/plusium/BUILD.gn` — Plusium's GN build config
- `content/libtermsurf_content/BUILD.gn` — Library's GN build config

## Ideas for experiments

1. **Make libtermsurf_content a shared library.** Change `static_library` to
   `shared_library` in its `BUILD.gn`. Update Plusium's `BUILD.gn` to depend
   only on the dylib (remove direct `content_shell_app`/`content_shell_lib`
   deps). Verify Plusium still works. This is a prerequisite for Roamium — it
   proves the C library is a proper abstraction boundary, not just a source
   grouping.

2. **Standalone Rust binary with FFI bindings.** Create `roamium/` at the repo
   root (sibling to `gui/` and `tui/`). Write FFI bindings to
   `libtermsurf_content.h` (hand-written, ~20 `extern "C"` declarations). Reuse
   prost + the same proto file. Implement the full message loop. Build with
   Cargo, link `libtermsurf_content.dylib`. Test by swapping `--browser plusium`
   for `--browser roamium`.

3. **Shared proto crate.** Extract the proto compilation into a shared crate
   (`termsurf-proto/`) that both Roamium and the TUI depend on, eliminating
   duplicate `build.rs` codegen.

## Experiments

### Experiment 1: Make libtermsurf_content a shared library

Plusium currently links 430+ Chromium dylibs directly, even though it only calls
20 `ts_*` functions from `libtermsurf_content`. The C library was designed as
the abstraction boundary, but GN builds it as a `static_library` — the `.o`
files get baked into the `plusium` binary, and all of Chromium's transitive
dependencies leak through.

Change `libtermsurf_content` to a `shared_library` so it becomes a proper
boundary. Plusium (and later Roamium) links one dylib instead of 430.

#### What to change

**`content/libtermsurf_content/BUILD.gn`** — Change `static_library` to
`shared_library`.

**`content/plusium/BUILD.gn`** — Remove `//content/shell:content_shell_app` and
`//content/shell:content_shell_lib` from Plusium's `deps`. These are
`libtermsurf_content`'s internal dependencies — Plusium should not need them.
Keep only `:termsurf_proto` and `//content/libtermsurf_content`.

#### Verification

1. `autoninja -C out/Default plusium` — builds clean.
2. `ls -la out/Default/libtermsurf_content.dylib` — the dylib exists.
3. `otool -L out/Default/plusium` — Plusium links `libtermsurf_content.dylib`,
   not 430 Chromium dylibs directly.
4. `nm -gU out/Default/libtermsurf_content.dylib | grep ts_` — all 20 `ts_*`
   symbols are exported.
5. `web google.com --browser plusium` — browse, navigate, DevTools — all
   working.

#### Result: Success — Plusium links 5 dylibs instead of 430

Three changes:

1. `libtermsurf_content/BUILD.gn`: `static_library` → `shared_library`
2. `plusium/BUILD.gn`: Removed `content_shell_app` and `content_shell_lib` deps
3. `libtermsurf_content.h`: Added `TS_EXPORT` macro
   (`__attribute__((visibility("default")))`) to all 23 `ts_*` functions.
   Chromium builds with `-fvisibility=hidden`, so without this the symbols
   weren't exported.

Before: Plusium linked 430 Chromium dylibs directly.

After: Plusium links 5 dylibs:

- `libtermsurf_content.dylib` — our 23 C API functions
- `libthird_party_protobuf_protobuf_lite.dylib` — Plusium's own protobuf
- `libthird_party_abseil-cpp_absl.dylib` — protobuf dependency
- `libc++_chrome.dylib` — C++ stdlib
- `/usr/lib/libSystem.B.dylib` — system

All 23 `ts_*` symbols are exported from the dylib. Plusium works end-to-end:
browsing, navigation, DevTools.

The C library is now a proper abstraction boundary. Roamium can link one dylib
instead of dealing with Chromium's internal dependency graph.

### Experiment 2: Minimal Rust binary that links libtermsurf_content

Build a minimal Roamium binary that proves the full Cargo → dylib → Chromium
pipeline works: FFI bindings, linking, runtime dylib loading, and calling
`ts_content_main()`. No socket code, no protobuf — just enough to prove the
build system works end-to-end.

#### What to create

**`roamium/Cargo.toml`**

```toml
[package]
name = "roamium"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "roamium"
path = "src/main.rs"

[dependencies]
prost = "0.14"

[build-dependencies]
prost-build = "0.14"
```

**`roamium/build.rs`**

```rust
use std::env;
use std::path::PathBuf;

fn main() {
    // Chromium build output directory (relative to repo root).
    let chromium_out = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../chromium/src/out/Default")
        .canonicalize()
        .expect("chromium/src/out/Default must exist — build Chromium first");

    // Link-time: find libtermsurf_content.dylib.
    println!(
        "cargo:rustc-link-search=native={}",
        chromium_out.display()
    );
    println!("cargo:rustc-link-lib=dylib=termsurf_content");

    // Runtime: two rpaths.
    // 1. @loader_path/. — for release (dylib colocated with binary).
    // 2. Chromium build dir — for development (binary in target/, dylib in
    //    chromium/src/out/Default/).
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/.");
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        chromium_out.display()
    );

    // Compile protobuf (same pattern as TUI).
    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}
```

**`roamium/src/ffi.rs`** — Hand-written FFI bindings for libtermsurf_content.h.
Only the functions needed for the smoke test:

```rust
use std::ffi::c_void;
use std::os::raw::{c_char, c_int};

pub type TsBrowserContext = *mut c_void;
pub type TsWebContents = *mut c_void;

extern "C" {
    pub fn ts_content_main(argc: c_int, argv: *const *const c_char) -> c_int;
    pub fn ts_set_on_initialized(
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn ts_post_task(
        task: Option<unsafe extern "C" fn(*mut c_void)>,
        user_data: *mut c_void,
    );
    pub fn ts_quit();
    pub fn ts_create_browser_context(path: *const c_char) -> TsBrowserContext;
}
```

**`roamium/src/main.rs`** — Minimal smoke test:

```rust
mod ffi;

use std::ffi::c_void;
use std::ptr;

unsafe extern "C" fn on_initialized(_user_data: *mut c_void) {
    eprintln!("[Roamium] Chromium initialized — creating browser context");
    let ctx = unsafe { ffi::ts_create_browser_context(ptr::null()) };
    eprintln!("[Roamium] Browser context: {:?}", ctx);
    eprintln!("[Roamium] Smoke test passed — shutting down");
    unsafe { ffi::ts_quit() };
}

fn main() {
    let args: Vec<std::ffi::CString> = std::env::args()
        .map(|a| std::ffi::CString::new(a).unwrap())
        .collect();
    let argv: Vec<*const i8> = args.iter().map(|a| a.as_ptr()).collect();

    unsafe {
        ffi::ts_set_on_initialized(Some(on_initialized), ptr::null_mut());
    }

    eprintln!("[Roamium] Entering ts_content_main");
    let ret = unsafe {
        ffi::ts_content_main(argv.len() as i32, argv.as_ptr())
    };
    std::process::exit(ret);
}
```

#### Verification

1. `cd roamium && cargo build` — compiles and links clean.
2. `./target/debug/roamium --no-sandbox` — prints "Chromium initialized",
   creates a browser context, prints "Smoke test passed", exits cleanly.
3. `otool -L target/debug/roamium | grep termsurf` — links
   `libtermsurf_content.dylib`.
4. `otool -l target/debug/roamium | grep -A2 LC_RPATH` — has both
   `@loader_path/.` and the Chromium build dir.

#### Result: Success — Rust calls into Chromium via FFI

`cargo build` compiles and links clean. The binary links
`libtermsurf_content.dylib` and has both rpaths (`@loader_path/.` for release,
Chromium build dir for development).

Running from `out/Default/` (via symlink), the full chain works:

```
[Roamium] Entering ts_content_main
[Roamium] Chromium initialized — creating browser context
[Roamium] Browser context: 0x9e6e98780
[Roamium] Smoke test passed — shutting down
```

Rust → `extern "C"` → `libtermsurf_content.dylib` → Chromium's ContentMain →
initialized callback → `ts_create_browser_context` → `ts_quit`. The entire FFI
pipeline works.

**Child process issue:** Chromium spawns GPU, renderer, and network processes by
re-executing the binary. These child processes need `icudtl.dat` and other data
files next to the binary. When running from `roamium/target/debug/`, the files
aren't there. Solution: copy the built binary into `chromium/src/out/Default/`
after building — the same location where Plusium lives.

### Experiment 3: Build scripts and binary placement

The existing `build-debug.sh` and `build-release.sh` build the GUI, Chromium,
and TUI. They need to also build Roamium and copy the binary into
`chromium/src/out/Default/` so Chromium's child processes can find `icudtl.dat`
and other data files.

Also add a standalone `build-roamium.sh` for quick iteration during development.

Both existing scripts currently build `chromium_profile_server` as the Chromium
target. They should also build `plusium` (and eventually just Roamium will
replace both).

#### What to change

**`scripts/build-roamium.sh`** (new) — Standalone script for quick Roamium
builds:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"

if [ -x "$CHROMIUM_PROTOC" ]; then
  export PROTOC="$CHROMIUM_PROTOC"
fi

cd "$REPO_DIR/roamium"
cargo build "$@"

# Determine target dir based on --release flag.
if [[ " $* " == *" --release "* ]]; then
  SRC="$REPO_DIR/roamium/target/release/roamium"
else
  SRC="$REPO_DIR/roamium/target/debug/roamium"
fi

cp "$SRC" "$CHROMIUM_OUT/roamium"
echo "Copied roamium to $CHROMIUM_OUT/roamium"
```

**`scripts/build-debug.sh`** — Add Roamium section after TUI, copy binary to
`out/Default`.

**`scripts/build-release.sh`** — Same pattern but with `--release`.

Both scripts also define `CHROMIUM_OUT` once at the top and update the "Done"
output to include Roamium.

#### Verification

1. `scripts/build-roamium.sh` — builds and copies binary.
2. `scripts/build-roamium.sh --release` — builds release and copies.
3. `ls -la chromium/src/out/Default/roamium` — binary exists.
4. `chromium/src/out/Default/roamium --no-sandbox` — smoke test passes, no child
   process ICU errors.
5. `scripts/build-debug.sh` — builds everything including Roamium.
6. `scripts/build-release.sh` — builds everything including Roamium.

#### Result: Success — build scripts work, child processes fixed

Three scripts created/updated:

1. **`scripts/build-roamium.sh`** (new) — standalone build + copy. Passes
   through `cargo build` args (e.g., `--release`).
2. **`scripts/build-debug.sh`** — added Roamium section, builds `plusium` too,
   defines `CHROMIUM_OUT`.
3. **`scripts/build-release.sh`** — same changes with `--release`.

`build-roamium.sh` builds the binary and copies it to
`chromium/src/out/Default/roamium`. Running from there, the smoke test passes
cleanly — no child process ICU errors because `icudtl.dat` is in the same
directory.

Both existing scripts also now build `plusium` alongside
`chromium_profile_server` via
`autoninja -C out/Default chromium_profile_server plusium`.

### Experiment 4: Full IPC pipeline — socket, protobuf, threading

Prove the complete IPC pipeline works in Rust: socket connection,
length-prefixed protobuf framing, background reader thread, and `ts_post_task`
dispatch to Chromium's UI thread. Implement just enough to handle a real
end-to-end flow: connect → ServerRegister → CreateTab → TabReady → navigate →
browse.

This is the critical threading experiment. Plusium runs three things
concurrently:

1. **Main thread** — blocked in `ts_content_main()` (Chromium's message loop)
2. **Reader thread** — reads socket, decodes protobuf, posts tasks to main
   thread via `ts_post_task()`
3. **Callbacks** — fire on main thread, encode protobuf, write to socket

The question: can Rust safely bridge these threads through `ts_post_task`'s
`void*` interface?

#### What to implement

**Argument parsing** — Extract `--ipc-socket=` and `--user-data-dir=` from argv
before calling `ts_content_main()`. Derive profile name from basename of user
data dir.

**Complete FFI bindings** (`ffi.rs`) — All 20 functions from
`libtermsurf_content.h`. The smoke test only had 5; add the remaining 15:

- `ts_destroy_browser_context`
- `ts_create_web_contents`
- `ts_create_devtools_web_contents`
- `ts_destroy_web_contents`
- `ts_load_url`
- `ts_forward_mouse_event`
- `ts_forward_mouse_move`
- `ts_forward_scroll_event`
- `ts_forward_key_event`
- `ts_set_focus`
- `ts_set_color_scheme`
- `ts_set_view_size`
- `ts_set_on_tab_ready`
- `ts_set_on_ca_context_id`
- `ts_set_on_url_changed`
- `ts_set_on_loading_state`
- `ts_set_on_title_changed`
- `ts_set_on_cursor_changed`

**Tab registry** — `Vec<TabEntry>` with `handle: *mut c_void`, `tab_id: i64`,
`pane_id: String`, `inspected_tab_id: i64`, `last_url: String`. Lookup by handle
and by tab_id. Stored in a global (same pattern as Plusium's
`static std::vector<TabEntry>* g_tabs`).

**Socket connection** — In `on_initialized()`, connect to the GUI's Unix socket,
send `ServerRegister` with the profile name, spawn the reader thread.

**Protobuf framing** — 4-byte LE length prefix + prost encode/decode. Same wire
format as Plusium and TUI.

**Reader thread** — Read from socket into a buffer. Extract complete messages.
For each message, box it and post to the UI thread via `ts_post_task()`. The
`void*` user_data carries a `Box<TermSurfMessage>` — `Box::into_raw()` to send,
`Box::from_raw()` to receive.

**Message dispatch** — Handle all 12 incoming message types:

| Message             | C API call                        |
| ------------------- | --------------------------------- |
| `CreateTab`         | `ts_create_web_contents`          |
| `CreateDevtoolsTab` | `ts_create_devtools_web_contents` |
| `Resize`            | `ts_set_view_size`                |
| `CloseTab`          | `ts_destroy_web_contents`         |
| `Navigate`          | `ts_load_url`                     |
| `MouseEvent`        | `ts_forward_mouse_event`          |
| `MouseMove`         | `ts_forward_mouse_move`           |
| `ScrollEvent`       | `ts_forward_scroll_event`         |
| `KeyEvent`          | `ts_forward_key_event`            |
| `FocusChanged`      | `ts_set_focus`                    |
| `SetColorScheme`    | `ts_set_color_scheme`             |
| `QueryTabsRequest`  | Build reply from tab registry     |

**String-to-int mappings** — `"down"`/`"up"` → 0/1 for mouse type,
`"left"`/`"right"`/`"middle"` → 0/1/2 for button, `"down"`/`"up"`/`"repeat"` →
0/1/2 for key type.

**6 callbacks** — Register before `ts_content_main()`:

| Callback            | Sends protobuf  |
| ------------------- | --------------- |
| `on_tab_ready`      | `TabReady`      |
| `on_ca_context_id`  | `CaContext`     |
| `on_url_changed`    | `UrlChanged`    |
| `on_loading_state`  | `LoadingState`  |
| `on_title_changed`  | `TitleChanged`  |
| `on_cursor_changed` | `CursorChanged` |

Each callback looks up the tab by handle, builds the response message, and
writes it to the socket.

**Shutdown** — When CloseTab removes the last entry, call `ts_quit()`.

#### Threading model

```
Main thread (Chromium UI loop)
  ├── on_initialized() → connect socket, send ServerRegister, spawn reader
  ├── ts_post_task callbacks → handle_message() → C API calls
  └── C API callbacks → build protobuf → write to socket

Reader thread
  └── loop: read() → frame → decode → Box::into_raw() → ts_post_task()
```

The socket fd is shared between threads: reader thread reads, callbacks write.
Use `Arc<Mutex<UnixStream>>` or duplicate the fd (one for read, one for write) —
the latter is simpler and avoids contention.

#### File structure

```
roamium/src/
  main.rs    — argv parsing, on_initialized, callback registration
  ffi.rs     — all 20 extern "C" declarations
  ipc.rs     — socket connect, framing, reader thread, send_protobuf
  dispatch.rs — handle_message(), tab registry, string-to-int maps
  proto.rs   — prost include of generated code
```

#### Verification

1. `scripts/build-roamium.sh` — compiles clean.
2. Launch GUI, open a terminal pane, run `web google.com --browser roamium`.
3. Page loads, URL bar updates, title shows — proves CreateTab → TabReady →
   CaContext → UrlChanged → LoadingState → TitleChanged pipeline.
4. Click links, type in search — proves mouse/keyboard forwarding.
5. Close the tab — proves CloseTab → ts_quit shutdown.
6. DevTools: `:devtools` — proves CreateDevtoolsTab pipeline.
7. No crashes, no hangs, no leaked threads.

#### Result: Success — full IPC pipeline works in Rust

Roamium is a drop-in replacement for Plusium. The GUI cannot tell the
difference.

Five files, ~400 lines of Rust replacing 511 lines of C++:

- **`ffi.rs`** — All 20 `extern "C"` declarations matching
  `libtermsurf_content.h`
- **`proto.rs`** — prost include of generated protobuf code
- **`ipc.rs`** — Socket connect, length-prefixed framing, reader thread with
  `Box::into_raw()` → `ts_post_task()` → `Box::from_raw()` dispatch
- **`dispatch.rs`** — Tab registry, all 12 message handlers, all 6 callbacks,
  string-to-int mappings
- **`main.rs`** — Argv parsing, `OnceLock` globals, callback registration,
  `ts_content_main()` entry

The threading model works exactly as designed: reader thread decodes protobuf
and posts `Box<TermSurfMessage>` to the UI thread via `ts_post_task`. Callbacks
fire on the UI thread and write responses back through the socket. No contention
— the reader thread owns the read half, the UI thread owns the write half.

Log output confirms the full pipeline: ServerRegister → CreateTab → TabReady →
CaContext → UrlChanged → LoadingState → TitleChanged. Page loads, renders at
60fps via CALayerHost compositing.

Also added `"roamium"` to the GUI's browser registry in `xpc.zig` so
`--browser roamium` resolves correctly.

Two pre-existing Chromium warnings also appear (same as Plusium):

- `In memory database cannot use the given database directory` — leveldb proto
  database warning when using `--user-data-dir` with an in-memory profile.
  Harmless.
- `DisplayLinkMac ID is not available. Switch to DelayBasedTimeSource(Timer) for BeginFrameSource.`
  — Chromium's compositor falls back to timer-based frame scheduling because the
  process doesn't own a display link. Does not affect rendering — CALayerHost
  compositing bypasses this path entirely.

### Experiment 5: Full feature verification (debug + release)

Manual testing checklist. Every feature Plusium supports must work identically
in Roamium. Run the full checklist twice — once in debug mode, once in release
mode.

#### Setup

**Debug:**

```bash
cd gui && zig build
scripts/build-roamium.sh
```

**Release:**

```bash
cd gui && zig build -Doptimize=ReleaseFast
scripts/build-roamium.sh --release
```

Launch GUI, open a terminal pane.

#### Checklist

**Page load**

- [x] `web google.com --browser roamium` — page renders
- [x] URL bar shows `https://www.google.com`
- [x] Page title appears in tab header
- [x] Loading indicator shows progress and completes

**Navigation**

- [x] Click a link — new page loads, URL bar updates
- [x] Type a URL in the TUI and press Enter — navigates
- [x] Page title updates after navigation

**Mouse input**

- [x] Click — activates buttons, follows links
- [x] Right-click — no crash (context menu may not appear, that's expected)
- [x] Click and drag — text selection works
- [x] Scroll — page scrolls smoothly (trackpad and mouse wheel)
- [x] Hover — cursor changes over links (pointer), text (I-beam), default

**Keyboard input**

- [x] Type in a search box (Google search) — characters appear
- [x] Backspace — deletes characters
- [x] Enter — submits form
- [x] Tab — moves focus between form elements
- [x] Cmd+A — selects all text in input
- [x] Cmd+C / Cmd+V — copy/paste works

**Resize**

- [x] Resize the window — browser pane resizes, content reflows
- [x] Split panes — browser pane adjusts to new size

**DevTools**

- [x] `:devtools` — DevTools panel opens in a split pane
- [x] DevTools shows Elements/Console/Network tabs
- [x] Inspect element works (click in DevTools, highlights in page)

**Color scheme**

- [x] `:colorscheme dark` — page switches to dark mode (test on a site that
      supports it, e.g. `web github.com`)
- [x] `:colorscheme light` — page switches back

**Focus**

- [x] Click between terminal and browser panes — focus switches correctly
- [x] Browser pane shows active indicator when focused

**Close**

- [x] Close the browser pane — Roamium process exits cleanly
- [x] No zombie processes left (`ps aux | grep roamium`)

**Multi-tab**

- [x] Open a second `web` pane with the same profile — reuses the same Roamium
      process
- [x] Both tabs work independently (navigate, scroll, type)
- [x] Close one tab — the other keeps working
- [x] Close the last tab — Roamium exits

#### Verification

Run the full checklist above in debug mode. Record any failures, fix them, and
re-test. Then repeat the entire checklist in release mode.

#### Result: Success — all 27 tests pass in debug and release

Every feature works identically to Plusium. Page load, navigation, mouse input
(click, drag, scroll, hover cursors), keyboard input (typing, backspace, enter,
tab, Cmd+A/C/V), resize, DevTools, color scheme switching, focus, clean
shutdown, and multi-tab — all pass in both debug and release builds.

Roamium is a verified drop-in replacement for Plusium.

## Conclusion

Roamium works. A Rust binary, built entirely outside Chromium's build system,
controls a full Chromium browser through 20 C function calls. Pages load, render
at 60fps, respond to mouse and keyboard, open DevTools, switch color schemes,
manage multiple tabs, and shut down cleanly. The GUI cannot tell the difference
between Roamium and Plusium.

This completes a four-issue arc (704–707) that fundamentally changed how
TermSurf talks to Chromium:

- **Issue 704** extracted the C library (`libtermsurf_content`) and proved that
  a standalone binary could wrap it. Plusium was born — 430 lines of C++ that
  replaced the monolithic Chromium Profile Server's 1,050-line fork.
- **Issue 705** made Plusium production-ready across 10 experiments: fixing the
  tab_ready timing bug, dark mode, cursor changes, and completing the full
  feature audit. Everything worked except DevTools.
- **Issue 706** found and fixed the DevTools crash. The root cause was a `void*`
  pointer corrupted across the C API boundary when used asynchronously. The fix
  — passing `tab_id` integers instead of pointers — established the design rule
  that no C++ pointers cross the library boundary for async use.
- **Issue 707** proved the architecture works in a second language. Roamium
  reimplements Plusium in ~400 lines of Rust — fewer than the C++ original —
  with zero Chromium build system integration. Cargo builds it, a shell script
  copies it next to Chromium, and it just works.

The C library is the architectural keystone. `libtermsurf_content` exports 20
functions with C types only — `int`, `const char*`, `void*`, `bool`, `uint32_t`.
Any language with C FFI can control Chromium through it. Rust proved it in five
experiments: shared library extraction, FFI smoke test, build scripts, full IPC
pipeline, and comprehensive feature verification.

What this means for TermSurf: the browser is now a plugin. The GUI launches any
protocol-compatible binary — Roamium, Plusium, or a future Zoomium (Zig). Third
parties could write bindings in Go, Swift, or Python. The protocol is simple
(Unix socket + length-prefixed protobuf), the C library is stable, and the
boundary is clean. TermSurf is no longer coupled to a single Chromium fork
binary.
