+++
status = "closed"
opened = "2026-01-25"
closed = "2026-01-25"
+++

# TermSurf 3.0 Webpage Rendering

## Background

This document continues from [303-xpc.md](./303-xpc.md), which solved
cross-process GPU texture sharing on macOS.

### What We Accomplished (303-xpc)

**The Problem:** TermSurf 3.0 runs CEF in a separate process (profile server)
for browser isolation. The GUI needs to display textures rendered by CEF, but
macOS deprecated global IOSurface ID lookup in 2015. There was no obvious way to
share GPU textures between unrelated processes.

**The Solution:** XPC with Mach port transfer. After investigating and rejecting
several approaches (global IOSurface IDs, process ancestry, bootstrap
registration), we determined that XPC is the only supported mechanism for
transferring Mach port rights between processes on modern macOS.

**What We Built:**

| Component              | Purpose                                                               |
| ---------------------- | --------------------------------------------------------------------- |
| `termsurf-xpc`         | Rust bindings for XPC (connections, listeners, endpoints, Mach ports) |
| `termsurf-launcher`    | XPC service that spawns profile servers and relays endpoints          |
| `termsurf-test-sender` | Test process that creates a pink IOSurface and sends it via XPC       |
| `webview_xpc.rs`       | GUI-side XPC manager for receiving Mach ports                         |
| `webview_shader.wgsl`  | Shader for rendering webview textures                                 |

**Validation:** Running `web google.com` displayed a pink 100x100 texture
stretched to fill the terminal window. This proved the complete IPC pipeline:

```
web CLI → Unix socket → GUI → XPC → launcher → test-sender
                                                    │
                              IOSurface Mach port ──┘
                                                    │
GUI ← IOSurfaceLookupFromMachPort ← XPC ────────────┘
  │
  └── wgpu texture import → render pipeline → pink screen
```

### New Goal

Replace the pink test texture with a real webpage rendered by CEF.

**Critical requirement:** Profile isolation must work from the start. This is
the entire reason ts3 exists. Each webview must use a named profile with its own
cookies, storage, and cache directory.

**Success looks like:**

```
$ web --profile myprofile google.com
```

- Google.com renders in the terminal pane (not pink)
- `~/.config/termsurf/cef/myprofile/` directory is created
- Different `--profile` values create different directories
- Profiles are isolated (logging into Google in one profile doesn't affect
  others)

### Next Steps (After This Document)

Once basic webpage rendering with profiles works:

1. **Multiple pages** — Open multiple webviews with different profiles
   simultaneously
2. **Keyboard input** — Type in form fields, use keyboard shortcuts
3. **Mouse input** — Click links, scroll, hover states
4. **Resize handling** — CEF resizes when pane resizes, sends new IOSurface
5. **Navigation** — Back, forward, reload, URL changes
6. **Page lifecycle** — Handle page loads, errors, redirects
7. **DevTools** — Open Chrome DevTools for debugging

## Experiments

### Experiment 1: CEF Profile Server (Display Only)

**Status:** FAILED

**Goal:** Create `termsurf-profile`, a CEF-based profile server that renders
real webpages and sends them to the GUI via XPC. Verify that profile directories
are created correctly.

**Failure:** The implementation was completed (new `termsurf-profile` package,
URL/profile threading through the pipeline, updated build scripts), but the app
still displays the pink 100x100 test surface instead of a real webpage. The root
cause is that macOS caches XPC service registrations — launchd continues loading
the old `termsurf-launcher` binary (from a previous `LauncherTest.app` build)
which spawns `termsurf-test-sender` instead of the new `termsurf-profile`. Even
clean rebuilds with `--clean` do not clear the stale XPC service cache. The
fundamental issue is that the XPC service discovery mechanism is not under the
app's control, making it unreliable for development iteration.

**Scope:** Display only. No keyboard input, no mouse input, no scrolling, no
clicking. The page renders once and remains static. Interactivity is a separate
future experiment.

#### What the User Sees

```
$ web --profile myprofile google.com
```

- Terminal pane shows Google's homepage (not pink)
- Page is static (no scrolling, clicking, or typing — display only)
- Page is essentially a screenshot of the first render
- `~/.config/termsurf/cef/myprofile/` exists with CEF data files
- Ctrl+C exits cleanly

#### Architecture

Same as 303-xpc Experiment 2, but `termsurf-profile` replaces
`termsurf-test-sender`:

```
web CLI                    GUI                      Launcher              termsurf-profile
───────                    ───                      ────────              ────────────────
    │                       │                          │                         │
    │── open_webview ──────>│                          │                         │
    │   {url, profile}      │                          │                         │
    │                       │── spawn_profile ────────>│                         │
    │                       │   {session, endpoint}    │── spawn ───────────────>│
    │                       │                          │   --profile myprofile   │
    │                       │                          │   --url google.com      │
    │                       │                          │   --session-id UUID     │
    │                       │                          │                         │
    │                       │                          │<── claim_session ───────│
    │                       │                          │── endpoint ────────────>│
    │                       │                          │                         │
    │                       │<══════════ XPC (direct) ════════════════════════>│
    │                       │                          │                         │
    │                       │                          │    CEF init:            │
    │                       │                          │    cache_path =         │
    │                       │                          │    ~/.config/termsurf/  │
    │                       │                          │    cef/myprofile/       │
    │                       │                          │                         │
    │                       │                          │    Create browser       │
    │                       │                          │    Navigate to URL      │
    │                       │                          │                         │
    │                       │<── display_surface ──────────────────────────────│
    │                       │    {mach_port, w, h}     │    on_accelerated_paint │
    │                       │                          │                         │
    │                       │    Import IOSurface      │                         │
    │                       │    Render to pane        │                         │
    │                       │                          │                         │
    │<── response ─────────│                          │                         │
```

#### CEF Multi-Process Architecture

CEF inherits Chromium's multi-process design. When `termsurf-profile` calls
`cef_initialize()`, CEF spawns several child subprocesses:

| Subprocess | Purpose                        |
| ---------- | ------------------------------ |
| GPU        | Hardware-accelerated rendering |
| Renderer   | V8 JavaScript, DOM, layout     |
| Utility    | Network, audio, etc.           |
| Alerts     | System dialogs (macOS only)    |

These subprocesses run a **helper binary** — a minimal executable that calls
`execute_process()` and lets CEF determine the subprocess role from command-line
arguments. The helper binary already exists in cef-rs as `cefsimple_helper`.

All profile processes share the same helper binary from the app bundle:

```
TermSurf.app/Contents/
├── MacOS/
│   ├── wezterm-gui               (main GUI)
│   ├── termsurf-profile          (profile server)
│   └── termsurf-test-sender      (test, will be deleted)
├── Frameworks/
│   ├── Chromium Embedded Framework.framework/
│   ├── TermSurf Helper.app/              ◄── shared by all profiles
│   ├── TermSurf Helper (GPU).app/
│   ├── TermSurf Helper (Renderer).app/
│   ├── TermSurf Helper (Plugin).app/
│   └── TermSurf Helper (Alerts).app/
└── XPCServices/
    └── com.termsurf.launcher.xpc/
```

#### Profile Isolation Strategy

**Why ts3 succeeds where ts2 couldn't:** ts2 discovered that CEF's per-browser
request contexts with custom `cache_path` don't work reliably. ts2 was forced to
use a single global context for all browsers.

ts3 solves this through **process separation**. Each profile runs in its own
`termsurf-profile` process with its own `cef_initialize()` call and its own
`root_cache_path`. CEF is fully isolated at the OS process level — no shared
state between profiles.

```
termsurf-profile --profile myprofile
    └── CEF instance (root_cache_path = ~/.config/termsurf/cef/myprofile/)
        ├── GPU subprocess
        ├── Renderer subprocess
        └── Utility subprocess

termsurf-profile --profile work
    └── CEF instance (root_cache_path = ~/.config/termsurf/cef/work/)
        ├── GPU subprocess
        ├── Renderer subprocess
        └── Utility subprocess
```

#### Components

##### 1. termsurf-profile (New Package)

**Location:** `ts3/termsurf-profile/`

CEF-based profile server. Combines:

- XPC session claiming from `termsurf-test-sender`
- CEF initialization with profile-specific cache path
- `on_accelerated_paint` handler that sends IOSurface via XPC

```rust
// ts3/termsurf-profile/src/main.rs (sketch)
use cef::*;
use clap::Parser;
use termsurf_xpc::*;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    profile: String,

    #[arg(long)]
    url: String,

    #[arg(long)]
    session_id: String,
}

fn main() {
    let args = Args::parse();
    let cef_args = cef::args::Args::new();

    // 0. Load CEF framework (macOS only)
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = cef::library_loader::LibraryLoader::new(
            &std::env::current_exe().unwrap(),
            false, // not a helper
        );
        assert!(loader.load());
        loader
    };

    // 1. Handle CEF subprocess (returns early if this is a subprocess)
    let exit_code = cef::execute_process(
        Some(cef_args.as_main_args()),
        None::<&mut App>,
        std::ptr::null_mut(),
    );
    if exit_code >= 0 {
        std::process::exit(exit_code);
    }

    // 2. Claim session and connect to GUI via XPC (same as test-sender)
    let gui = claim_and_connect(&args.session_id);

    // 3. Compute paths
    let exe = std::env::current_exe().unwrap();
    let app_contents = exe.parent().unwrap().parent().unwrap();

    let helper_path = app_contents
        .join("Frameworks")
        .join("TermSurf Helper.app")
        .join("Contents/MacOS/TermSurf Helper");

    let cache_path = dirs::config_dir()
        .unwrap()
        .join("termsurf/cef")
        .join(&args.profile);

    // 4. Initialize CEF with profile-specific settings
    let settings = Settings {
        windowless_rendering_enabled: 1,
        // No external_message_pump — we call run_message_loop() below,
        // which means CEF owns the event loop. No competing loop exists.
        no_sandbox: 1,                  // Required for development
        root_cache_path: CefString::from(cache_path.to_str().unwrap()),
        browser_subprocess_path: CefString::from(helper_path.to_str().unwrap()),
        persist_session_cookies: 1,
        ..Default::default()
    };

    let mut app = create_app();
    cef::initialize(
        Some(cef_args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ).expect("CEF initialization failed");

    // 5. Create render handler that sends IOSurface via XPC
    let render_handler = ProfileRenderHandler::new(gui.clone());

    // 6. Create browser with off-screen rendering
    let window_info = WindowInfo {
        windowless_rendering_enabled: 1,
        shared_texture_enabled: 1, // Critical: enables IOSurface on macOS
        ..Default::default()
    };

    let browser_settings = BrowserSettings {
        windowless_frame_rate: 60,
        ..Default::default()
    };

    let browser = cef::browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut build_client(render_handler)),
        Some(&args.url.into()),
        Some(&browser_settings),
        None, // extra_info
        None, // request_context (uses global with our root_cache_path)
    );

    // 7. Install signal handler for clean shutdown
    ctrlc::set_handler(|| {
        cef::quit_message_loop();
    }).expect("Failed to set Ctrl+C handler");

    // 8. Run CEF message loop (blocks until quit_message_loop is called)
    cef::run_message_loop();
    cef::shutdown();
}

struct ProfileRenderHandler {
    gui: Arc<XpcConnection>,
    last_handle: AtomicPtr<c_void>, // Track IOSurface handle for dedup
}

impl RenderHandler for ProfileRenderHandler {
    fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
        // Return initial size (resize not supported in this experiment)
        if let Some(rect) = rect {
            rect.width = 800;
            rect.height = 600;
        }
    }

    fn screen_info(
        &self,
        _browser: Option<&mut Browser>,
        screen_info: Option<&mut ScreenInfo>,
    ) -> i32 {
        if let Some(info) = screen_info {
            // macOS base DPI is 72; Retina is typically 144 (2x)
            info.device_scale_factor = 2.0;
            return 1;
        }
        0
    }

    fn on_accelerated_paint(
        &self,
        _browser: Option<&mut Browser>,
        _type: PaintElementType,
        _dirty_rects: Option<&[Rect]>,
        info: Option<&AcceleratedPaintInfo>,
    ) {
        let Some(info) = info else { return };

        // Dedup: only send when IOSurface handle changes (avoids flooding
        // XPC with redundant Mach port transfers on every frame)
        let handle = info.shared_texture_io_surface as *mut c_void;
        let prev = self.last_handle.swap(handle, Ordering::Relaxed);
        if handle == prev {
            return;
        }

        // Create Mach port from IOSurface
        let port = termsurf_xpc::iosurface::create_mach_port(info.shared_texture_io_surface);
        if port == 0 { return; }

        // Send to GUI via XPC
        let msg = XpcDictionary::new();
        msg.set_string("action", "display_surface");
        msg.set_mach_send("iosurface_port", port);
        msg.set_i64("width", info.extra.coded_size.width);
        msg.set_i64("height", info.extra.coded_size.height);
        self.gui.send(&msg);
    }
}
```

**Key differences from the earlier sketch:**

- Loads CEF framework via `LibraryLoader` (macOS requirement)
- Calls `execute_process()` first for subprocess handling
- Sets `browser_subprocess_path` to shared helper binary
- Uses `run_message_loop()` (no external pump needed — no competing event loop)
- Implements `view_rect()` and `screen_info()` for proper DPI handling
- Uses `shared_texture_enabled: 1` in WindowInfo for IOSurface
- Sets `windowless_frame_rate: 60`
- Deduplicates `on_accelerated_paint` by tracking IOSurface handle changes

**Paint Callback Optimization:**

CEF calls `on_accelerated_paint` on every frame — cursor blinks, animations, and
repaints all trigger it. ts2 and the cef-rs OSR example process every paint
without deduplication because they are in-process (no IPC overhead). In ts3,
each paint would create a Mach port via `IOSurfaceCreateMachPort` and transfer
it over XPC to another process. At 60fps, that's 60 Mach port transfers/second
even for a static page.

The dedup check (`last_handle` comparison) avoids this: when CEF repaints into
the same IOSurface buffer, the handle pointer doesn't change, so we skip the XPC
send. With CEF's double-buffering (alternating IOSurface handles), we still send
on buffer swaps, which is acceptable for MVP.

Future optimization: have the GUI read directly from a shared IOSurface without
per-frame Mach port transfers (the GUI imports once and re-reads the same
surface).

##### 2. CEF Helper Binary

The helper binary already exists in cef-rs as `cefsimple_helper`. It is bundled
into the app by the build scripts. All profile processes point to the same
helper via `browser_subprocess_path`.

The helper is minimal:

```rust
fn main() {
    let args = Args::new();
    let _loader = LibraryLoader::new(&std::env::current_exe().unwrap(), true);
    execute_process(Some(args.as_main_args()), None::<&mut App>, std::ptr::null_mut());
}
```

##### 3. Launcher Modification

Update `termsurf-launcher` to spawn `termsurf-profile` instead of
`termsurf-test-sender`. Pass `--profile`, `--url`, and `--session-id` arguments.

The profile and URL must be passed from the GUI to the launcher in the
`spawn_profile` message.

##### 4. GUI Modification

Update `webview_socket.rs` to extract the profile name from `open_webview` and
pass it to the XPC manager.

Update `webview_xpc.rs` to include profile and URL in the `spawn_profile`
message to the launcher.

##### 5. Web CLI Modification

Add `--profile` flag to the `web` command. Include profile in the `open_webview`
message sent to the GUI.

```
$ web --profile myprofile google.com
$ web google.com  # Uses "default" profile
```

#### CEF Initialization Details

**Profile directory structure:**

```
~/.config/termsurf/cef/
├── myprofile/
│   ├── Cache/
│   ├── Cookies
│   ├── Local Storage/
│   └── ...
├── otherprofile/
│   └── ...
└── default/
    └── ...
```

**CefSettings (complete):**

```rust
Settings {
    // Profile-specific storage
    root_cache_path: "~/.config/termsurf/cef/{profile}/",

    // Enable off-screen rendering (no visible window)
    windowless_rendering_enabled: 1,

    // No external_message_pump — termsurf-profile has no competing event loop,
    // so CEF owns the loop via run_message_loop().

    // Path to shared helper binary
    browser_subprocess_path: ".../TermSurf Helper.app/Contents/MacOS/TermSurf Helper",

    // Disable sandbox for development
    no_sandbox: 1,

    // Persist cookies across sessions
    persist_session_cookies: 1,
}
```

**Browser creation:**

```rust
let window_info = WindowInfo {
    windowless_rendering_enabled: 1,
    shared_texture_enabled: 1,      // Critical: enables IOSurface on macOS
    external_begin_frame_enabled: 0, // Let CEF control frame timing
    ..Default::default()
};

let browser_settings = BrowserSettings {
    windowless_frame_rate: 60,
    ..Default::default()
};

cef::browser_host_create_browser_sync(
    Some(&window_info),
    Some(&mut client),  // Has our RenderHandler
    Some(&url.into()),
    Some(&browser_settings),
    None,  // extra_info
    None,  // request_context (uses global with our root_cache_path)
);
```

#### Device Scale Factor

CEF operates in logical pixels (DIP — device-independent pixels). macOS has a
base DPI of 72, so Retina displays have a scale factor of 2.0.

The `screen_info()` handler must report the correct scale factor so CEF renders
at the right resolution. For this experiment, we hardcode 2.0 (Retina). A future
experiment will read the actual DPI from the GUI.

#### Message Pump

`termsurf-profile` uses `cef::run_message_loop()`, which lets CEF own and manage
the event loop. This is the correct choice because `termsurf-profile` is a
dedicated CEF process with no competing event loop (unlike ts2, which shares a
thread with WezTerm's GUI).

**Why NOT `external_message_pump`:** ts2 and the cef-rs OSR example both use
`external_message_pump: 1` because they integrate CEF into an existing
application loop (WezTerm's GUI loop and winit's event loop, respectively). They
must call `do_message_loop_work()` on a timer. `termsurf-profile` has no such
constraint — XPC uses GCD dispatch queues that run independently of CEF's
message loop, so there is no conflict.

#### Mach Port Lifecycle

Mach ports are a finite kernel resource. Leaking them causes
`__THE_SYSTEM_HAS_NO_PORTS_AVAILABLE__` crashes (Chrome has hit this). The
sender and receiver have different ownership rules:

**Sender side (`termsurf-profile`):**

- `IOSurfaceCreateMachPort()` creates a send right
- `xpc_dictionary_set_mach_send()` moves the port into XPC — XPC takes
  ownership, sender does NOT need to deallocate

**Receiver side (GUI):**

- `xpc_dictionary_copy_mach_send()` creates a NEW send right — **caller MUST
  deallocate** via `mach_port_deallocate(mach_task_self(), port)`
- After `IOSurfaceLookupFromMachPort(port)`, the IOSurface is referenced
  independently through the IOSurfaceRef — the Mach port can (and must) be
  deallocated immediately

**Required changes to `termsurf-xpc`:**

1. **`termsurf-xpc/src/ffi.rs`** — Add FFI bindings:

   ```rust
   extern "C" {
       pub fn mach_port_deallocate(task: mach_port_t, name: mach_port_t) -> kern_return_t;
       pub fn mach_task_self_() -> mach_port_t; // Note: actual symbol has trailing _
   }
   ```

2. **`termsurf-xpc/src/iosurface.rs`** — Add deallocation helper:

   ```rust
   pub fn deallocate_mach_port(port: mach_port_t) {
       unsafe {
           ffi::mach_port_deallocate(ffi::mach_task_self_(), port);
       }
   }
   ```

3. **GUI receiver** (`webview_xpc.rs`) — After `IOSurfaceLookupFromMachPort`
   succeeds, immediately call
   `termsurf_xpc::iosurface::deallocate_mach_port(port)`. The IOSurface is now
   referenced through the IOSurfaceRef, not the Mach port.

4. **GUI import caching** — Instead of reimporting from Mach port every frame
   (as `draw.rs:218` currently does), import once when a new surface arrives and
   cache the wgpu texture/bind group. Only re-import when a NEW Mach port
   arrives. This eliminates the need to keep Mach ports alive across frames.

#### Clean Shutdown

`cef::run_message_loop()` blocks indefinitely. Without a signal handler, Ctrl+C
sends SIGINT which terminates the process without running `cef::shutdown()`,
risking profile directory corruption (incomplete writes to cookies, local
storage, etc.).

**Solution:** Install a signal handler that calls `cef::quit_message_loop()`
(which is thread-safe per CEF docs). After `run_message_loop()` returns,
`cef::shutdown()` runs for clean cleanup.

```rust
// In main(), before run_message_loop():
ctrlc::set_handler(|| {
    // Thread-safe: quit_message_loop posts to CEF's message loop
    cef::quit_message_loop();
}).expect("Failed to set Ctrl+C handler");

cef::run_message_loop();
cef::shutdown();
```

Add `ctrlc` to `termsurf-profile/Cargo.toml` dependencies.

#### Files to Create

| File                               | Purpose            |
| ---------------------------------- | ------------------ |
| `ts3/termsurf-profile/Cargo.toml`  | Package manifest   |
| `ts3/termsurf-profile/src/main.rs` | CEF profile server |

**Dependencies for `termsurf-profile/Cargo.toml`:**

- `cef` (from workspace)
- `clap` with `derive` feature
- `termsurf-xpc` (from workspace — XPC session claiming, Mach port helpers)
- `ctrlc` (signal handling for clean shutdown)
- `dirs` (config directory resolution)

#### Files to Modify

| File                                               | Changes                                                      |
| -------------------------------------------------- | ------------------------------------------------------------ |
| `ts3/termsurf-launcher/src/main.rs`                | Spawn `termsurf-profile`, pass profile/URL args              |
| `ts3/termsurf-web/src/main.rs`                     | Add `--profile` flag, include in open_webview                |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Extract profile from request                                 |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Pass profile/URL to launcher                                 |
| `ts3/Cargo.toml`                                   | Add termsurf-profile to workspace                            |
| `ts3/termsurf-xpc/src/ffi.rs`                      | Add `mach_port_deallocate` and `mach_task_self` FFI bindings |
| `ts3/termsurf-xpc/src/iosurface.rs`                | Add `deallocate_mach_port()` helper                          |
| Build scripts                                      | Bundle termsurf-profile and helper in app                    |

#### Success Criteria

- [ ] `web --profile myprofile google.com` renders Google homepage in pane
- [ ] `~/.config/termsurf/cef/myprofile/` directory exists after running
- [ ] `web --profile other google.com` creates `~/.config/termsurf/cef/other/`
- [ ] Page content is visible (not pink, not black, not error screen)
- [ ] Ctrl+C exits cleanly
- [ ] No CEF crashes or GPU errors in logs

**Out of scope for this experiment:**

- Keyboard input (typing in search box)
- Mouse input (clicking links, scrolling)
- Page resize (window resize updates texture)
- Navigation (back, forward, URL changes)

#### What This Proves

1. **CEF initialization works** in the profile server process
2. **Profile isolation works** — each profile gets its own directory
3. **on_accelerated_paint works** — CEF sends IOSurface to our handler
4. **End-to-end rendering works** — real webpage pixels reach the screen

This experiment validates rendering only. Interactivity (keyboard, mouse) will
be proven in subsequent experiments.

#### After This Experiment

With webpage rendering working:

1. Delete `termsurf-test-sender` (no longer needed)
2. Proceed to keyboard/mouse input handling
3. Add resize support (CEF resize → new IOSurface → GUI update)

---

### Experiment 2: Fix Stale XPC Service Cache

**Status:** FAILED

**Goal:** Add a cleanup step to the build scripts so that stale launchd
registrations for `com.termsurf.launcher` are removed before launching the app.
This unblocks Experiment 1's code, which is already implemented but has never
actually run.

**Failure:** Two issues encountered:

1. Running the binary directly (`$APP_BUNDLE/Contents/MacOS/wezterm-gui`) caused
   "XPC connection invalid" because launchd couldn't discover the embedded XPC
   service without LaunchServices registration. Fixed by changing `--open` to
   use `open "$APP_BUNDLE"`.
2. After that fix, `web https://google.com` timed out waiting for a surface. No
   pink screen, no webpage — the profile server either never started, never
   connected, or never sent an IOSurface. The `launchctl bootout` + `open` fixes
   were necessary but not sufficient.

#### Problem

macOS caches XPC service registrations in launchd. When `LauncherTest.app` was
run during Experiment 2 of 303-xpc, launchd registered the old launcher binary
at:

```
/Users/ryan/dev/termsurf/ts3/termsurf-launcher/LauncherTest.app/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/termsurf-launcher
```

This registration persists across builds. Even after rebuilding
`wezterm-gui.app` with the new launcher (which spawns `termsurf-profile` instead
of `termsurf-test-sender`), launchd continues loading the old binary. The
result: every `web` command still produces the 100x100 pink square from
`termsurf-test-sender`.

A `launchctl print` confirms the stale registration:

```
$ launchctl print gui/$(id -u)/com.termsurf.launcher
program = .../LauncherTest.app/.../termsurf-launcher
path = /private/tmp/com.termsurf.launcher.plist
state = running
last exit code = (never exited)
```

#### Solution

Add two lines to the top of each build script (after flag parsing, before any
build steps):

```bash
launchctl bootout "gui/$(id -u)/com.termsurf.launcher" 2>/dev/null || true
rm -f /private/tmp/com.termsurf.launcher.plist
```

- `launchctl bootout` kills the running process and removes the launchd
  registration. The `|| true` prevents `set -e` from aborting when the service
  isn't registered (first build, or already cleaned).
- `rm -f` removes the stale plist that was written to `/private/tmp/` by the old
  test scripts.

After this cleanup, when `wezterm-gui.app` launches and connects to
`com.termsurf.launcher`, launchd will discover the XPC service from the app
bundle's `Contents/XPCServices/` directory — which now contains the updated
launcher that spawns `termsurf-profile`.

#### Files to Modify

| File                                          | Changes                                 |
| --------------------------------------------- | --------------------------------------- |
| `ts3/scripts/build-debug.sh`                  | Add launchctl bootout + rm before build |
| `ts3/scripts/build-release.sh`                | Add launchctl bootout + rm before build |
| `ts3/termsurf-launcher/scripts/build-test.sh` | Add launchctl bootout + rm before build |

No code changes. All Experiment 1 code is preserved as-is.

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open
# In terminal:
web google.com
```

#### Success Criteria

Same as Experiment 1 (which this unblocks):

- [ ] `web google.com` renders Google homepage in pane (not pink, not black)
- [ ] Surface size is 1600x1200 (800x600 at 2x Retina), not 100x100
- [ ] `~/.config/termsurf/cef/default/` directory exists with CEF data files
- [ ] `web --profile myprofile google.com` creates separate profile directory
- [ ] Ctrl+C exits cleanly
- [ ] Repeated `build-debug.sh --open` cycles work without manual cleanup

---

### Experiment 3: Add Debug Logging

**Status:** SUCCESS

**Goal:** Add file-based logging to all three processes (GUI, launcher, profile
server) so we can see exactly where the pipeline breaks. Experiments 1 and 2
failed with no visible output because `open` discards stdout/stderr.

**Result:** GUI log (`/tmp/termsurf-gui.log`) captured successfully via
`open --stdout --stderr`. It revealed that the XPC launcher connection goes
invalid immediately after connecting
(`Launcher connection error: XPC connection
invalid`), confirming the launcher
XPC service never starts. No launcher or profile logs were created, which per
the diagnostic guide means "XPC service never started." The logging
infrastructure works and will carry forward into future experiments.

#### Problem

When `wezterm-gui.app` is launched via `open` (required for XPC service
discovery), all stdout/stderr from the GUI, launcher, and profile server are
lost. We cannot diagnose the pipeline failure without logs.

#### Solution

Redirect each process's output to log files in `/tmp/`:

##### 1. Build scripts: redirect GUI logs

**Files:** `ts3/scripts/build-debug.sh`, `ts3/scripts/build-release.sh`

Change:

```bash
open "$APP_BUNDLE"
```

To:

```bash
open --stdout /tmp/termsurf-gui.log --stderr /tmp/termsurf-gui.log "$APP_BUNDLE"
echo "Logs: /tmp/termsurf-gui.log"
```

##### 2. Launcher: redirect stdout/stderr to file at startup

**File:** `ts3/termsurf-launcher/src/main.rs`

At the top of `main()`, before any `println!`, redirect stdout and stderr to
`/tmp/termsurf-launcher.log` using `dup2`:

```rust
extern "C" {
    fn dup2(oldfd: i32, newfd: i32) -> i32;
}

fn redirect_output(path: &str) {
    use std::os::unix::io::AsRawFd;
    let file = std::fs::File::create(path).expect("Failed to create log file");
    let fd = file.as_raw_fd();
    unsafe {
        dup2(fd, 1); // stdout
        dup2(fd, 2); // stderr
    }
    std::mem::forget(file); // keep fd open
}
```

All existing `println!` / `eprintln!` calls continue to work — they just write
to the file instead of the (discarded) stdout.

##### 3. Launcher: redirect profile server output to file

**File:** `ts3/termsurf-launcher/src/main.rs`

When spawning the profile server, redirect its stdout/stderr:

```rust
let profile_log = std::fs::File::create(
    format!("/tmp/termsurf-profile-{}.log", session_id)
).unwrap_or_else(|_| std::fs::File::create("/tmp/termsurf-profile.log").unwrap());

Command::new(&profile_bin_path)
    .args(["--session-id", &session_id])
    .args(["--url", &url])
    .args(["--profile", &profile])
    .stdout(profile_log.try_clone().unwrap())
    .stderr(profile_log)
    .spawn()
```

#### Log Files Produced

| File                             | Source            | What it shows                                                |
| -------------------------------- | ----------------- | ------------------------------------------------------------ |
| `/tmp/termsurf-gui.log`          | wezterm-gui       | XPC manager, socket server, render pipeline                  |
| `/tmp/termsurf-launcher.log`     | termsurf-launcher | Service startup, spawn requests, session claims              |
| `/tmp/termsurf-profile-{id}.log` | termsurf-profile  | CEF init, browser creation, paint callbacks, Mach port sends |

#### Files to Modify

| File                                | Changes                                                     |
| ----------------------------------- | ----------------------------------------------------------- |
| `ts3/scripts/build-debug.sh`        | `open --stdout --stderr` redirect to log file               |
| `ts3/scripts/build-release.sh`      | Same as debug                                               |
| `ts3/termsurf-launcher/src/main.rs` | `dup2` redirect at startup + `.stdout()/.stderr()` on spawn |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open
# In terminal:
web google.com
# After timeout, read the logs:
cat /tmp/termsurf-gui.log
cat /tmp/termsurf-launcher.log
cat /tmp/termsurf-profile-*.log
```

#### Diagnostic Guide

The logs will show which step in the pipeline failed:

- No launcher log file → XPC service never started
- Launcher log but no spawn → `spawn_profile` message never arrived
- Spawn logged but no profile log → profile binary crashed at startup
- Profile log but no CEF init → CEF framework failed to load
- CEF init but no paint → browser didn't render
- Paint but no Mach port → IOSurface transfer failed

---

### Experiment 4: Restore launchd Mach Service Registration

**Status:** FAILED

**Goal:** Re-register the launcher as a launchd Mach service in the build
scripts, restoring the mechanism that made the pink screen work in Experiment 2.

**Result:** The launchd registration fix restored the XPC pipeline. The launcher
log (`/tmp/termsurf-launcher.log`) now exists and shows the full flow: service
starts, receives `spawn_profile`, spawns the profile server, receives
`claim_session`, and returns the GUI endpoint successfully. The GUI log no
longer shows "XPC connection invalid." The profile server log
(`/tmp/termsurf-profile-pane-0-31693.log`) shows it starts, loads CEF, connects
to the GUI, and finds the helper binary. It then crashes at CEF initialization:

```
[FATAL:cef/libcef_dll/ctocpp/app_ctocpp.cc:118] CefApp_0_CToCpp called with invalid version -1
```

The XPC fix works. The `web google.com` command still times out because the
profile server crashes before rendering. The failure is now a cef-rs binding
version mismatch, not an XPC issue.

#### Root Cause

The pink screen worked because `run-test.sh` creates a launchd plist at
`/tmp/com.termsurf.launcher.plist` and loads it via `launchctl load`. This tells
launchd: "when someone connects to the Mach service `com.termsurf.launcher`,
launch this binary." The launcher code uses `xpc_connection_create_mach_service`
on both sides (listener and client), which requires this launchd registration.

When the code was moved into `build-debug.sh`, the registration was removed
(`launchctl bootout`) but never re-created. The launcher binary was placed in
`Contents/XPCServices/` (embedded XPC service model), but the code still uses
Mach service APIs that talk to launchd's registry — not embedded service
discovery. Result: launchd says "never heard of it," connection invalid,
launcher never starts.

#### Fix

Add launchd plist creation and `launchctl bootstrap` to the build scripts, after
bundling and signing, before launching. This mirrors what `run-test.sh` does.

##### 1. Build scripts: register Mach service with launchd

**Files:** `ts3/scripts/build-debug.sh`, `ts3/scripts/build-release.sh`

After the `codesign` step and before the `open` step, add:

```bash
# 10. Register XPC launcher as launchd Mach service
LAUNCHER_BIN="$APP_BUNDLE/Contents/XPCServices/com.termsurf.launcher.xpc/Contents/MacOS/termsurf-launcher"
PLIST_PATH="/tmp/com.termsurf.launcher.plist"

cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.launcher</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.launcher</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>$LAUNCHER_BIN</string>
    </array>
</dict>
</plist>
EOF

launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
echo "Registered com.termsurf.launcher with launchd"
```

The existing `launchctl bootout` at the top of the script already cleans up
stale registrations from previous runs, so the sequence is:

1. `bootout` (top of script) — remove old registration
2. Build, bundle, sign
3. `bootstrap` (before launch) — register with new binary path
4. `open` — launch app, GUI connects to Mach service, launchd starts launcher

##### 2. No code changes

The launcher, GUI, profile server, and termsurf-xpc code are unchanged. The Mach
service APIs (`connect_mach_service`, `new_mach_service`) are correct for this
model — they just need launchd to know about the service.

#### Files to Modify

| File                           | Changes                                    |
| ------------------------------ | ------------------------------------------ |
| `ts3/scripts/build-debug.sh`   | Add plist creation + `launchctl bootstrap` |
| `ts3/scripts/build-release.sh` | Same                                       |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open
# In terminal:
web google.com
# Check all three logs:
cat /tmp/termsurf-gui.log
cat /tmp/termsurf-launcher.log
cat /tmp/termsurf-profile-*.log
```

#### Expected Log Progression

With the Mach service registered, the logs should advance beyond Experiment 3:

- `/tmp/termsurf-gui.log` — No more "connection invalid"; should show
  `spawn_profile` sent and surface received
- `/tmp/termsurf-launcher.log` — Should exist and show "Starting...", connection
  events, spawn requests, session claims
- `/tmp/termsurf-profile-*.log` — Should exist and show CEF initialization,
  browser creation, accelerated paint callbacks

If the launcher log appears but the profile log doesn't, the next failure point
is CEF framework loading or the profile server's own XPC claim-session call.

#### Success Criteria

- [ ] `/tmp/termsurf-launcher.log` exists and shows "Launcher: Starting..."
- [ ] `/tmp/termsurf-profile-*.log` exists and shows "Profile: Starting..."
- [ ] `web google.com` does not timeout (or fails at a later stage, past XPC)
- [ ] `launchctl list | grep com.termsurf.launcher` shows the service registered

---

### Experiment 5: Fix CEF API Version Initialization

**Status:** SUCCESS

**Goal:** Get past the CEF initialization crash by adding the missing
`api_hash()` call that ts2 has and ts3's profile server lacks.

**Result:** Adding `cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0)` before
creating the App object fixed the CEF initialization crash. The profile server
now initializes CEF, creates a browser, renders google.com, and sends the
IOSurface to the GUI. An image of google.com appeared in the terminal.

#### Root Cause

The profile server crashes at `cef::initialize()` with:

```
[FATAL:cef/libcef_dll/ctocpp/app_ctocpp.cc:118] CefApp_0_CToCpp called with invalid version -1
```

ts2's `init_cef()` in `wezterm-gui/src/main.rs` calls this before creating the
App object:

```rust
// Configure CEF API version (required before creating App objects)
let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
```

ts3's `termsurf-profile` does not make this call. Without it, CEF objects are
tagged with version `-1` (uninitialized), causing the fatal error.

#### Fix

One line added to `termsurf-profile/src/main.rs`.

**File:** `ts3/termsurf-profile/src/main.rs`

In `run_profile_server()`, after loading the CEF framework and before calling
`cef::execute_process()`, add:

```rust
// Configure CEF API version (required before creating App objects)
let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
```

This mirrors ts2's `init_cef()` exactly.

#### Files to Modify

| File                               | Changes               |
| ---------------------------------- | --------------------- |
| `ts3/termsurf-profile/src/main.rs` | Add `api_hash()` call |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open
# In terminal:
web google.com
# Check profile log for CEF progress:
cat /tmp/termsurf-profile-*.log
```

#### Success Criteria

- [ ] Profile log shows "Profile: CEF initialized" (gets past `cef::initialize`)
- [ ] No `CefApp_0_CToCpp called with invalid version` error in profile log
- [ ] Pipeline progresses to browser creation or a later stage

---

## What We Accomplished

Five experiments took the pipeline from a pink test square to rendering
google.com in the terminal.

### The Pipeline (Working)

```
web CLI → Unix socket → GUI → XPC → launcher → termsurf-profile
                                                       │
                                            CEF renders webpage
                                                       │
                                      IOSurface Mach port via XPC
                                                       │
GUI ← IOSurfaceLookupFromMachPort ← XPC ───────────────┘
  │
  └── wgpu texture import → render pipeline → google.com on screen
```

### What We Built

| Component            | What it does                                                  |
| -------------------- | ------------------------------------------------------------- |
| `termsurf-profile`   | CEF profile server — renders webpages, sends IOSurface to GUI |
| Debug logging        | All three processes log to `/tmp/` for diagnostics            |
| launchd registration | Build scripts register the launcher Mach service with launchd |
| CEF API version fix  | `api_hash()` call required before creating CEF App objects    |

### What Each Experiment Solved

| # | Experiment                         | Status  | What it fixed                                     |
| - | ---------------------------------- | ------- | ------------------------------------------------- |
| 1 | CEF Profile Server                 | FAILED  | Wrote the profile server; couldn't test (no logs) |
| 2 | Fix Stale XPC Service Cache        | FAILED  | Added launchctl bootout; still no logs            |
| 3 | Add Debug Logging                  | SUCCESS | `/tmp/` log files for all three processes         |
| 4 | Restore launchd Mach Registration  | FAILED  | XPC pipeline works; CEF crashes at init           |
| 5 | Fix CEF API Version Initialization | SUCCESS | `api_hash()` call; google.com renders             |

Experiments 1-2 failed silently because there was no logging. Experiment 3 added
logging, which revealed the launcher never started (XPC connection invalid).
Experiment 4 fixed the XPC issue by restoring launchd registration, which
revealed the CEF version crash. Experiment 5 fixed CEF with a one-line
`api_hash()` call copied from ts2.

### What Remains

From the "New Goal" at the top of this document:

- [x] Google.com renders in the terminal pane (not pink)
- [ ] `~/.config/termsurf/cef/myprofile/` directory is created per profile
- [ ] Different `--profile` values create different directories
- [ ] Profiles are isolated (separate cookies, storage, cache)

And from "Next Steps":

- [ ] Multiple webviews with different profiles simultaneously
- [ ] Keyboard input (form fields, shortcuts)
- [ ] Mouse input (clicks, scroll, hover)
- [ ] Resize handling (CEF resizes with pane, sends new IOSurface)
- [ ] Navigation (back, forward, reload, URL changes)
- [ ] Page lifecycle (loads, errors, redirects)
- [ ] DevTools
