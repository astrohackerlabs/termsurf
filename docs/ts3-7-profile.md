# TermSurf 3.0 One-Process-Per-Profile

## Background

### Progress So Far

ts3 has established a working pipeline for rendering webpages in terminal panes:

- **ts3-1 through ts3-3:** Designed the out-of-process architecture. The GUI
  (WezTerm) communicates with a launcher XPC service, which spawns profile
  server processes. Profile servers run CEF off-screen rendering and send
  IOSurface Mach ports back to the GUI for display.
- **ts3-4:** Got a webpage (google.com) rendering in a terminal pane. The full
  pipeline works: CLI → Unix socket → GUI → XPC → launcher → profile server →
  CEF → IOSurface → Mach port → GUI → wgpu → screen.
- **ts3-5:** Fixed profile path isolation. Each profile stores its CEF data at
  `~/.config/termsurf/cef/<profile>/` instead of the macOS-specific
  `~/Library/Application Support/`.
- **ts3-6:** Removed hardcoded 800x600 dimensions. The GUI now reads pane pixel
  dimensions and DPI from the Mux, computes logical size and scale factor, and
  passes them to the profile server at startup. CEF renders at the correct pane
  size on Retina displays.

### The Problem

The current code spawns a new `termsurf-profile` process for every `web`
command. This violates the foundational architectural constraint of ts3: **there
must be exactly one process per browser profile.**

CEF's `SingletonLock` file prevents two processes from opening the same
`root_cache_path`. If a user runs `web google.com` and then `web github.com`
with the same profile, the second process will crash or fail to initialize.

This is not a bug in our code -- it is how CEF and Chromium are designed. One
`root_cache_path` = one process. This constraint is the entire reason ts3 moved
CEF out-of-process: to support multiple profiles, each needs its own process.

## Goal

Implement one-process-per-profile so that multiple webviews can share a single
CEF process, like tabs in a browser.

**Product requirements:**

1. A user can open many different webviews for the same profile (e.g.,
   `web google.com` and `web github.com` both using the `default` profile). Each
   webview renders in its own pane with its own size and URL.
2. A user can open webviews across many different profiles (e.g., `default`,
   `work`, `personal`). Each profile gets its own process with its own cookies,
   storage, and cache.
3. There is always exactly one `termsurf-profile` process per profile,
   containing exactly one CEF instance. Multiple webviews within that process
   are separate CEF browser instances sharing the same CEF context.
4. All cross-process GPU texture sharing continues to use XPC Mach port
   transfer. Each webview has its own IOSurface and its own Mach port sent to
   the GUI.

**Success looks like:**

- `web google.com` opens in pane 1 -- profile process starts, page renders
- `web github.com` opens in pane 2 (same profile) -- no new process, second
  browser created in the existing profile process, page renders in pane 2
- `web --profile work gitlab.com` opens in pane 3 -- new profile process starts
  for `work`, page renders in pane 3
- All three panes display their respective pages simultaneously
- Closing a pane destroys only that browser, not the entire profile process
- Closing all panes for a profile shuts down that profile process

## Tasks

- [ ] Launcher tracks running profile processes (PID + connection per profile)
- [ ] Launcher routes `spawn_profile` to existing process if profile is running
- [ ] Profile server accepts "create browser" commands for additional webviews
- [ ] Profile server manages multiple browsers with separate sizes, URLs, and
      IOSurfaces
- [ ] Each browser's IOSurface Mach port is sent to the correct GUI pane
- [ ] GUI correctly maps incoming surfaces to the right pane when multiple
      webviews share a profile process
- [ ] Closing a pane sends a "destroy browser" command to the profile server
- [ ] Profile server shuts down when its last browser is destroyed

## Deferred Work

The following features were planned in ts3-6 but are blocked until
one-process-per-profile is implemented. They will be addressed in subsequent
documents after this architecture is in place:

- **Dynamic resize** -- Send new pane dimensions to the profile server via XPC
  when the window resizes or panes are split. Requires bidirectional XPC
  communication (GUI → profile) and calling `host.was_resized()` on the correct
  browser instance. ts2's settle delay (30ms) is a fallback if bouncing recurs.
- **Keyboard input** -- Forward keystrokes to CEF for typing in form fields and
  using keyboard shortcuts.
- **Mouse input** -- Forward clicks, scrolling, and hover events to CEF for
  interacting with page elements.
- **Navigation** -- Back, forward, reload, and URL bar changes.
- **Page lifecycle** -- Handle page loads, errors, redirects, and title updates.
- **DevTools** -- Open Chrome DevTools for debugging webview content.

## Research: Current Architecture and What Must Change

### Current Flow (Single Browser Per Process)

```
1. CLI sends "open_webview" to GUI via Unix socket
2. GUI creates anonymous XPC listener for this pane, gets endpoint
3. GUI sends "spawn_profile" to launcher (includes gui_endpoint, URL, dimensions)
4. Launcher stores gui_endpoint, spawns new termsurf-profile process
5. Profile process starts, claims session from launcher (gets gui_endpoint)
6. Profile connects to GUI via endpoint
7. Profile initializes CEF, creates ONE browser in on_context_initialized
8. on_accelerated_paint sends IOSurface Mach port to GUI
9. GUI receives surface, maps to pane via session_id
```

Every `web` command repeats steps 1-9, spawning a new process every time. Step 4
always spawns -- there is no check for an existing process.

### What the Launcher Must Do

The launcher must become a **router**. When `spawn_profile` arrives:

- **First request for a profile:** Spawn the process (current behavior).
- **Subsequent requests for the same profile:** Forward the request to the
  existing process as a `create_browser` command.

To do this, the launcher needs:

1. A `running_profiles` map: `HashMap<String, ProfileProcessInfo>` where
   `ProfileProcessInfo` contains the profile process's XPC endpoint.
2. A `register_profile` action: after the profile server initializes CEF, it
   creates its own anonymous XPC listener and sends the endpoint to the
   launcher.
3. Modified `spawn_profile` logic: check `running_profiles` first.

### What the Profile Server Must Do

The profile server must become **multi-browser**. Currently it creates one
browser at startup and runs forever. It must:

1. After CEF init, create an anonymous XPC listener and register it with the
   launcher via `register_profile`.
2. Listen for `create_browser` commands on that listener. Each command includes
   a GUI endpoint, URL, width, height, and scale.
3. For each browser, create a separate `Client` + `RenderHandler` pair connected
   to that browser's GUI endpoint. Each render handler sends IOSurface Mach
   ports to its own pane.
4. The initial browser (from CLI args) is created in `on_context_initialized` as
   before. Subsequent browsers are created via XPC commands.

**Thread safety:** XPC callbacks run on libdispatch queues, not the CEF UI
thread. Browser creation must be marshalled to the CEF UI thread using
`cef::post_task(ThreadId::UI, ...)`.

**Shared state refactor:** Currently `SharedState` holds a single `url`,
`width`, `height`, and `gui` connection. This must become multi-browser:

```rust
struct BrowserState {
    gui: Arc<XpcConnection>,
    width: AtomicU32,
    height: AtomicU32,
    last_handle: AtomicPtr<c_void>,
}

struct ProfileState {
    scale: f32,
    browsers: Mutex<HashMap<String, Arc<BrowserState>>>,  // keyed by session_id
}
```

Each `RenderHandlerInner` holds an `Arc<BrowserState>` instead of the global
`SharedState`.

### What the GUI Must Change

Almost nothing. The GUI already creates a separate anonymous XPC listener per
pane, each with its own `session_id` → `pane_id` mapping. Whether the profile
server is new or reused, the GUI's listener receives the `display_surface`
message and maps it to the right pane.

The only change: `request_profile_spawn` currently always sends `spawn_profile`
to the launcher. This still works -- the launcher decides whether to spawn or
forward. The GUI doesn't need to know.

## Experiments

### Experiment 1: Merge Launcher into GUI

**Status:** PLANNED

**Goal:** Simplify the architecture by eliminating the separate launcher process.
The GUI becomes the Mach service that profile servers connect to. This
simplification must happen before implementing multi-profile support.

**Rationale:** The launcher exists only because it was designed that way, not
because of any technical requirement. The GUI can register as a Mach service,
spawn profile processes directly, and handle endpoint relay itself. Merging
eliminates one process and one IPC hop:

```
Before: CLI → GUI → Launcher (spawn) → Profile → Launcher (claim) → GUI
After:  CLI → GUI (spawn) → Profile → GUI (claim)
```

#### Changes

**1. GUI: Add Mach service listener and session handling**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

Add to `XpcManager`:

- A Mach service listener for `com.termsurf.gui`
- A `sessions` map to store GUI endpoints temporarily
- A `claim_session` handler
- Process spawning code (moved from launcher)
- Running profiles tracking (for later multi-profile support)

```rust
struct XpcManager {
    // Remove: _launcher: XpcConnection,
    service_listener: XpcListener,  // Mach service listener for com.termsurf.gui
    sessions: Mutex<HashMap<String, XpcEndpoint>>,  // session_id -> GUI endpoint
    running_profiles: Mutex<HashMap<String, XpcEndpoint>>,  // profile -> endpoint (for later)
    // ... existing fields ...
}

impl XpcManager {
    fn new() -> anyhow::Result<Self> {
        // Create Mach service listener instead of connecting to launcher
        let service_listener = XpcListener::new_mach_service("com.termsurf.gui")?;

        // Set up handler for incoming connections (from profile servers)
        set_new_connection_handler(&service_listener, move |conn| {
            // Handle claim_session requests from profile servers
            set_event_handler(&conn, move |event| {
                if let Ok(msg) = event {
                    let action = msg.get_string("action").unwrap_or_default();
                    if action == "claim_session" {
                        // Look up and return the GUI endpoint for this session
                        // ... (see below)
                    }
                }
            });
            conn.resume();
        });
        service_listener.resume();

        Ok(Self {
            service_listener,
            sessions: Mutex::new(HashMap::new()),
            running_profiles: Mutex::new(HashMap::new()),
            // ...
        })
    }
}
```

**2. GUI: Spawn profile processes directly**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

Modify `request_profile_spawn` to spawn the profile process directly instead of
sending a message to the launcher:

```rust
pub fn request_profile_spawn(
    self: &Arc<Self>,
    pane_id: PaneId,
    url: &str,
    profile: &str,
    width: u32,
    height: u32,
    scale: f32,
) -> anyhow::Result<String> {
    let session_id = format!("pane-{}-{}", pane_id, std::process::id());

    // Create anonymous listener for this pane (existing code)
    let listener = XpcListener::new_anonymous()?;
    let endpoint = listener.get_endpoint()?;

    // Store endpoint for profile to claim
    self.sessions.lock().unwrap().insert(session_id.clone(), endpoint);

    // Set up handler for incoming surface messages (existing code)
    // ...

    // Spawn profile server directly (moved from launcher)
    let profile_bin = Self::get_profile_binary_path()?;
    let log_path = format!("/tmp/termsurf-profile-{}.log", session_id);

    let mut cmd = Command::new(&profile_bin);
    cmd.args(["--session-id", &session_id])
        .args(["--url", url])
        .args(["--profile", profile])
        .args(["--width", &width.to_string()])
        .args(["--height", &height.to_string()])
        .args(["--scale", &format!("{}", scale)]);

    if let Ok(log_file) = File::create(&log_path) {
        cmd.stdout(log_file.try_clone()?).stderr(log_file);
    }

    cmd.spawn()?;

    Ok(session_id)
}

fn get_profile_binary_path() -> anyhow::Result<PathBuf> {
    // GUI is at: .app/Contents/MacOS/wezterm-gui
    // Profile is at: .app/Contents/MacOS/termsurf-profile
    let exe = std::env::current_exe()?;
    Ok(exe.parent().unwrap().join("termsurf-profile"))
}
```

**3. GUI: Handle claim_session requests**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

When a profile server connects and sends `claim_session`:

```rust
"claim_session" => {
    let session_id = msg.get_string("session_id").unwrap();

    let endpoint = {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.remove(&session_id)
    };

    let reply = XpcDictionary::create_reply(&msg)?;
    if let Some(ep) = endpoint {
        reply.set_endpoint("endpoint", ep);
        log::info!("[XPC Manager] Session {} claimed", session_id);
    } else {
        reply.set_string("error", "session not found");
        log::warn!("[XPC Manager] Session {} not found", session_id);
    }
    conn.send(&reply);
}
```

**4. Profile server: Connect to GUI instead of launcher**

**File:** `ts3/termsurf-profile/src/main.rs`

Change one line:

```rust
// Before
let launcher = XpcConnection::connect_mach_service("com.termsurf.launcher")?;

// After
let gui = XpcConnection::connect_mach_service("com.termsurf.gui")?;
```

The rest of the profile server remains unchanged — it still sends `claim_session`
and receives the endpoint in the reply.

**5. Build scripts: Register GUI as Mach service**

**File:** `ts3/scripts/build-debug.sh`

Remove XPC service bundling for launcher. Change launchd registration:

```bash
# Remove these lines:
mkdir -p "$APP_BUNDLE/Contents/XPCServices/com.termsurf.launcher.xpc/..."
cp ... termsurf-launcher ...

# Change launchd plist to register GUI:
cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.gui</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.gui</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>$APP_BUNDLE/Contents/MacOS/wezterm-gui</string>
    </array>
</dict>
</plist>
EOF

launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
```

**6. Build scripts: Same changes for release**

**File:** `ts3/scripts/build-release.sh`

Apply the same changes as build-debug.sh.

**7. Delete launcher crate**

Remove the entire `ts3/termsurf-launcher/` directory and remove it from
`ts3/Cargo.toml` workspace members.

**8. Update documentation**

- `CLAUDE.md`: Remove launcher from key binaries, update topology diagram
- `docs/ts3-3-xpc.md`: Update architecture description

#### Files to Modify

| Action | File                                               |
| ------ | -------------------------------------------------- |
| Modify | `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    |
| Modify | `ts3/termsurf-profile/src/main.rs`                 |
| Modify | `ts3/scripts/build-debug.sh`                       |
| Modify | `ts3/scripts/build-release.sh`                     |
| Modify | `ts3/Cargo.toml`                                   |
| Modify | `CLAUDE.md`                                        |
| Modify | `docs/ts3-3-xpc.md`                                |
| Delete | `ts3/termsurf-launcher/` (entire directory)        |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open

# Test basic webview still works
web google.com

# Check logs -- no more launcher log
cat /tmp/termsurf-gui.log
# Should show: claim_session handling, process spawning

cat /tmp/termsurf-profile-*.log
# Should show: connected to com.termsurf.gui (not com.termsurf.launcher)

# Verify no launcher process
ps aux | grep termsurf-launcher
# Should return nothing
```

#### Success Criteria

- [ ] `web google.com` renders a page (basic functionality preserved)
- [ ] No `termsurf-launcher` process running
- [ ] Profile server logs show connection to `com.termsurf.gui`
- [ ] GUI logs show `claim_session` handling and process spawning
- [ ] No `/tmp/termsurf-launcher.log` file created
- [ ] Build scripts no longer reference launcher
