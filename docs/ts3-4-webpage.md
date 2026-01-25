# TermSurf 3.0 Webpage Rendering

## Background

This document continues from [ts3-3-xpc.md](./ts3-3-xpc.md), which solved
cross-process GPU texture sharing on macOS.

### What We Accomplished (ts3-3-xpc)

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

### Experiment 3: CEF Profile Server

**Status:** PLANNED

**Goal:** Create `termsurf-profile`, a CEF-based profile server that renders
real webpages and sends them to the GUI via XPC. Verify that profile directories
are created correctly.

#### What the User Sees

```
$ web --profile myprofile google.com
```

- Terminal pane shows Google's homepage (not pink)
- `~/.config/termsurf/cef/myprofile/` exists with CEF data files
- Ctrl+C exits cleanly

#### Architecture

Same as Experiment 2, but `termsurf-profile` replaces `termsurf-test-sender`:

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

#### Components

##### 1. termsurf-profile (New Package)

**Location:** `ts3/termsurf-profile/`

CEF-based profile server. Combines:

- XPC session claiming from `termsurf-test-sender`
- CEF initialization and browser creation
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

    // 1. Claim session and connect to GUI (same as test-sender)
    let gui = claim_and_connect(&args.session_id);

    // 2. Initialize CEF with profile-specific cache path
    let cache_path = dirs::config_dir()
        .unwrap()
        .join("termsurf/cef")
        .join(&args.profile);

    let settings = CefSettings {
        cache_path: cache_path.to_str().unwrap().into(),
        // ... other settings
    };

    cef::initialize(&settings);

    // 3. Create render handler that sends IOSurface via XPC
    let render_handler = ProfileRenderHandler::new(gui.clone());

    // 4. Create browser and navigate
    let browser = create_browser_sync(
        &args.url,
        render_handler,
        // ... other handlers
    );

    // 5. Run CEF message loop
    cef::run_message_loop();
}

struct ProfileRenderHandler {
    gui: XpcConnection,
}

impl RenderHandler for ProfileRenderHandler {
    fn on_accelerated_paint(&self, info: &AcceleratedPaintInfo) {
        // Get Mach port from IOSurface
        let port = IOSurfaceCreateMachPort(info.shared_texture_io_surface);

        // Send to GUI
        let msg = XpcDictionary::new();
        msg.set_string("action", "display_surface");
        msg.set_mach_send("iosurface_port", port);
        msg.set_i64("width", info.width);
        msg.set_i64("height", info.height);
        self.gui.send(&msg);
    }
}
```

##### 2. Launcher Modification

Update `termsurf-launcher` to spawn `termsurf-profile` instead of
`termsurf-test-sender`. Pass `--profile`, `--url`, and `--session-id` arguments.

The profile and URL must be passed from the GUI to the launcher in the
`spawn_profile` message.

##### 3. GUI Modification

Update `webview_socket.rs` to extract the profile name from `open_webview` and
pass it to the XPC manager.

Update `webview_xpc.rs` to include profile and URL in the `spawn_profile`
message to the launcher.

##### 4. Web CLI Modification

Add `--profile` flag to the `web` command. Include profile in the `open_webview`
message sent to the GUI.

```
$ web --profile myprofile google.com
$ web google.com  # Uses default profile
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

**Key CEF settings:**

```rust
CefSettings {
    // Profile-specific storage
    cache_path: "~/.config/termsurf/cef/{profile}/",

    // Enable off-screen rendering
    windowless_rendering_enabled: true,

    // Use GPU acceleration
    // (Required for on_accelerated_paint to receive IOSurface)
}
```

**Browser creation:**

```rust
let window_info = CefWindowInfo {
    windowless_rendering_enabled: true,
    shared_texture_enabled: true,  // Critical for IOSurface
    // ...
};

let browser_settings = CefBrowserSettings {
    // Default settings OK for now
};

CefBrowserHost::create_browser_sync(
    window_info,
    client,  // Has our RenderHandler
    url,
    browser_settings,
    None,  // extra_info
    None,  // request_context (uses global with our cache_path)
);
```

#### Files to Create

| File                               | Purpose            |
| ---------------------------------- | ------------------ |
| `ts3/termsurf-profile/Cargo.toml`  | Package manifest   |
| `ts3/termsurf-profile/src/main.rs` | CEF profile server |

#### Files to Modify

| File                                               | Changes                                         |
| -------------------------------------------------- | ----------------------------------------------- |
| `ts3/termsurf-launcher/src/main.rs`                | Spawn `termsurf-profile`, pass profile/URL args |
| `ts3/termsurf-web/src/main.rs`                     | Add `--profile` flag, include in open_webview   |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Extract profile from request                    |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Pass profile/URL to launcher                    |
| `ts3/Cargo.toml`                                   | Add termsurf-profile to workspace               |
| Build scripts                                      | Bundle termsurf-profile in app                  |

#### Success Criteria

- [ ] `web --profile myprofile google.com` renders Google homepage in pane
- [ ] `~/.config/termsurf/cef/myprofile/` directory exists after running
- [ ] `web --profile other google.com` creates `~/.config/termsurf/cef/other/`
- [ ] Page content is visible (not pink, not black, not error screen)
- [ ] Ctrl+C exits cleanly
- [ ] No CEF crashes or GPU errors in logs

#### What This Proves

1. **CEF initialization works** in the profile server process
2. **Profile isolation works** — each profile gets its own directory
3. **on_accelerated_paint works** — CEF sends IOSurface to our handler
4. **End-to-end rendering works** — real webpage pixels reach the screen
5. **Architecture is viable** — ready for interactivity features

#### After This Experiment

With webpage rendering working:

1. Delete `termsurf-test-sender` (no longer needed)
2. Proceed to keyboard/mouse input handling
3. Add resize support (CEF resize → new IOSurface → GUI update)
