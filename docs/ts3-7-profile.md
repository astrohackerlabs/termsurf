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

**Status:** FAILED

**Goal:** Simplify the architecture by eliminating the separate launcher
process. The GUI becomes the Mach service that profile servers connect to. This
simplification must happen before implementing multi-profile support.

**Rationale:** The launcher exists only because it was designed that way, not
because of any technical requirement. The GUI can register as a Mach service,
spawn profile processes directly, and handle endpoint relay itself. Merging
eliminates one process and one IPC hop:

```
Before: CLI → GUI → Launcher (spawn) → Profile → Launcher (claim) → GUI
After:  CLI → GUI (spawn) → Profile → GUI (claim)
```

#### Failure Analysis

**Crash:** `EXC_BREAKPOINT` with `_xpc_api_misuse` at
`xpc_connection_set_event_handler`

**Root cause:** XPC API misuse when the GUI (acting as a Mach service) receives
connections from profile servers. The crash occurs at line 52 of
`webview_xpc.rs` in the `set_new_connection_handler` closure when calling
`set_event_handler` on peer connections.

**Why it failed:**

1. **Mach service peer connection semantics differ from anonymous listeners.**
   When using `XpcListener::new_mach_service()`, connections received in
   `set_new_connection_handler` have different lifecycle and handler semantics
   than anonymous XPC listeners. Calling `xpc_connection_set_event_handler` on
   peer connections in this context triggers API misuse.

2. **Stale profile servers from previous runs.** A profile server from a
   previous session connected immediately at GUI startup (log shows "New
   connection from profile server" before any `web` command), triggering the
   handler code path before the GUI was ready.

3. **The GUI is not a proper XPC service.** The launcher was a dedicated XPC
   service binary managed by launchd with proper lifecycle control. When the GUI
   tries to be a Mach service, it's just a regular app with a registered Mach
   service name — the XPC framework expects services registered via launchd to
   follow specific patterns that a GUI app doesn't follow.

#### Decision: Keep the Launcher

The separate launcher process is architecturally necessary, not just a design
choice. The initial rationale was wrong: the launcher exists because it's the
correct macOS pattern, not because "it was designed that way."

**Why the launcher is simpler:**

1. **It's the correct XPC pattern.** XPC services bundled in
   `Contents/XPCServices/` are designed to be spawned by launchd on-demand,
   managed by the system, and run as simple focused binaries. The launcher is
   ~100 lines of straightforward code that does one thing well.

2. **Merging adds complexity, not removes it.** Every workaround for the Mach
   service registration issue (Unix socket handshakes, custom bootstrap
   registration, connection handler hacks) adds complexity. "One fewer process"
   is an illusory benefit when the process is tiny and launchd-managed.

3. **Fighting the platform is always harder.** macOS XPC services work a
   specific way. The launcher follows that pattern. The GUI-as-service approach
   fights against it.

**What the launcher provides:**

- Proper XPC service lifecycle management by launchd
- Isolation from GUI crashes/restarts
- Correct Mach service semantics for peer connections
- A clean separation of concerns

The launcher stays. The experiment is closed.

---

#### Attempted Changes (Not Merged)

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

The rest of the profile server remains unchanged — it still sends
`claim_session` and receives the endpoint in the reply.

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

| Action | File                                            |
| ------ | ----------------------------------------------- |
| Modify | `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` |
| Modify | `ts3/termsurf-profile/src/main.rs`              |
| Modify | `ts3/scripts/build-debug.sh`                    |
| Modify | `ts3/scripts/build-release.sh`                  |
| Modify | `ts3/Cargo.toml`                                |
| Modify | `CLAUDE.md`                                     |
| Modify | `docs/ts3-3-xpc.md`                             |
| Delete | `ts3/termsurf-launcher/` (entire directory)     |

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

---

### Experiment 2: One Process Per Profile

**Status:** PLANNED

**Goal:** Implement the core architectural requirement: exactly one
`termsurf-profile` process per browser profile, with multiple webviews (CEF
browsers) sharing that process.

**What this enables:**

- `web google.com` then `web github.com` → same process, two browsers
- `web --profile work gitlab.com` → different process for `work` profile
- Shared cookies/storage within a profile (like Chrome tabs)
- No CEF `SingletonLock` crashes from duplicate profile processes

#### Architecture Overview

```
                              ┌─────────────────────────────────────┐
                              │           Launcher                  │
                              │                                     │
                              │  running_profiles: {                │
                              │    "default" → ProfileEndpoint,     │
                              │    "work" → ProfileEndpoint,        │
                              │  }                                  │
                              └─────────────────────────────────────┘
                                    │                    ▲
                     spawn_profile  │                    │ register_profile
                     (or forward)   │                    │
                                    ▼                    │
┌──────────┐        ┌───────────────────────────────────────────────────┐
│   GUI    │◄──────►│              Profile Server (default)             │
│          │  XPC   │                                                   │
│ pane 1 ◄─┼────────┤  browsers: {                                      │
│ pane 2 ◄─┼────────┤    "session-1" → Browser (google.com) → pane 1   │
│          │        │    "session-2" → Browser (github.com) → pane 2   │
└──────────┘        │  }                                                │
                    └───────────────────────────────────────────────────┘
```

#### Flow: First Webview for a Profile

```
1. CLI sends "open_webview" to GUI (profile=default, url=google.com)
2. GUI creates anonymous XPC listener for pane 1, gets endpoint
3. GUI sends "spawn_profile" to launcher with gui_endpoint, url, profile, dimensions
4. Launcher checks running_profiles["default"] → not found
5. Launcher stores gui_endpoint in pending_sessions, spawns termsurf-profile
6. Profile initializes CEF with root_cache_path for "default"
7. Profile creates XPC listener, sends "register_profile" to launcher
8. Launcher stores profile's endpoint in running_profiles["default"]
9. Profile claims session from launcher, gets gui_endpoint
10. Profile creates browser for google.com, connects to GUI
11. on_accelerated_paint sends IOSurface Mach port to GUI pane 1
```

#### Flow: Second Webview for Same Profile

```
1. CLI sends "open_webview" to GUI (profile=default, url=github.com)
2. GUI creates anonymous XPC listener for pane 2, gets endpoint
3. GUI sends "spawn_profile" to launcher with gui_endpoint, url, profile, dimensions
4. Launcher checks running_profiles["default"] → FOUND
5. Launcher sends "create_browser" to existing profile process
   (includes gui_endpoint, url, dimensions, session_id)
6. Profile receives create_browser on its XPC listener
7. Profile creates second browser for github.com
8. Profile connects new browser's RenderHandler to GUI pane 2's endpoint
9. on_accelerated_paint sends IOSurface Mach port to GUI pane 2
```

No new process spawned. Both browsers share the same CEF context.

#### Changes

**1. Launcher: Track running profiles**

**File:** `ts3/termsurf-launcher/src/main.rs`

Add a map to track which profiles have running processes:

```rust
struct LauncherState {
    pending_sessions: Mutex<HashMap<String, XpcEndpoint>>,  // existing
    running_profiles: Mutex<HashMap<String, XpcEndpoint>>,  // NEW: profile → endpoint
}
```

**2. Launcher: Handle register_profile action**

When a profile server finishes CEF init, it registers itself:

```rust
"register_profile" => {
    let profile = msg.get_string("profile").unwrap();
    let endpoint = msg.copy_endpoint("endpoint").unwrap();

    state.running_profiles.lock().unwrap()
        .insert(profile.to_string(), endpoint);

    log::info!("[Launcher] Profile '{}' registered", profile);
}
```

**3. Launcher: Route spawn_profile to existing process**

Modify `spawn_profile` handler to check for existing process first:

```rust
"spawn_profile" => {
    let profile = msg.get_string("profile").unwrap();
    let session_id = msg.get_string("session_id").unwrap();
    let gui_endpoint = msg.copy_endpoint("gui_endpoint").unwrap();
    let url = msg.get_string("url").unwrap();
    let width = msg.get_i64("width") as u32;
    let height = msg.get_i64("height") as u32;
    let scale = msg.get_string("scale").unwrap();

    // Store GUI endpoint for claiming
    state.pending_sessions.lock().unwrap()
        .insert(session_id.to_string(), gui_endpoint.clone());

    // Check if profile process already running
    let existing = state.running_profiles.lock().unwrap()
        .get(profile).cloned();

    if let Some(profile_endpoint) = existing {
        // Forward to existing process
        log::info!("[Launcher] Forwarding to existing profile '{}'", profile);

        let conn = XpcConnection::from_endpoint(profile_endpoint)?;
        let create_msg = XpcDictionary::new();
        create_msg.set_string("action", "create_browser");
        create_msg.set_string("session_id", session_id);
        create_msg.set_endpoint("gui_endpoint", gui_endpoint);
        create_msg.set_string("url", url);
        create_msg.set_i64("width", width as i64);
        create_msg.set_i64("height", height as i64);
        create_msg.set_string("scale", scale);
        conn.send(&create_msg);
    } else {
        // Spawn new process (existing code)
        log::info!("[Launcher] Spawning new profile '{}'", profile);
        // ... spawn termsurf-profile with CLI args ...
    }
}
```

**4. Profile server: Register with launcher after CEF init**

**File:** `ts3/termsurf-profile/src/main.rs`

After CEF initializes, create a listener and register:

```rust
// After cef::initialize() succeeds...

// Create listener for incoming create_browser commands
let profile_listener = XpcListener::new_anonymous()?;
let profile_endpoint = profile_listener.get_endpoint()?;

// Register with launcher
let register_msg = XpcDictionary::new();
register_msg.set_string("action", "register_profile");
register_msg.set_string("profile", &args.profile);
register_msg.set_endpoint("endpoint", profile_endpoint);
launcher.send(&register_msg);

// Set up handler for create_browser commands
set_new_connection_handler(&profile_listener, move |conn| {
    set_event_handler(&conn, move |event| {
        if let Ok(msg) = event {
            let action = msg.get_string("action").unwrap_or_default();
            if action == "create_browser" {
                // Marshal to CEF UI thread
                let url = msg.get_string("url").unwrap().to_string();
                let session_id = msg.get_string("session_id").unwrap().to_string();
                let gui_endpoint = msg.copy_endpoint("gui_endpoint").unwrap();
                let width = msg.get_i64("width") as u32;
                let height = msg.get_i64("height") as u32;

                cef::post_task(ThreadId::UI, move || {
                    create_browser(&url, &session_id, gui_endpoint, width, height);
                });
            }
        }
    });
    conn.resume();
});
profile_listener.resume();
```

**5. Profile server: Multi-browser state**

Refactor from single-browser to multi-browser state:

```rust
// OLD: Single browser
struct SharedState {
    url: String,
    width: AtomicU32,
    height: AtomicU32,
    gui: Arc<XpcConnection>,
    last_handle: AtomicPtr<c_void>,
}

// NEW: Multiple browsers
struct BrowserState {
    session_id: String,
    gui: Arc<XpcConnection>,
    width: AtomicU32,
    height: AtomicU32,
    last_handle: AtomicPtr<c_void>,
}

struct ProfileState {
    scale: f32,
    browsers: Mutex<HashMap<i32, Arc<BrowserState>>>,  // browser_id → state
}

static PROFILE_STATE: OnceLock<Arc<ProfileState>> = OnceLock::new();
```

**6. Profile server: Create browser function**

Extract browser creation into a reusable function:

```rust
fn create_browser(
    url: &str,
    session_id: &str,
    gui_endpoint: XpcEndpoint,
    width: u32,
    height: u32,
) {
    let profile_state = PROFILE_STATE.get().unwrap();

    // Connect to GUI for this browser
    let gui = XpcConnection::from_endpoint(gui_endpoint).unwrap();
    gui.resume();

    // Create browser-specific state
    let browser_state = Arc::new(BrowserState {
        session_id: session_id.to_string(),
        gui: Arc::new(gui),
        width: AtomicU32::new(width),
        height: AtomicU32::new(height),
        last_handle: AtomicPtr::new(std::ptr::null_mut()),
    });

    // Create render handler for this browser
    let render_handler = RenderHandler::new(RenderHandlerInner {
        state: Arc::clone(&browser_state),
        scale: profile_state.scale,
    });

    // Create client and browser
    let client = Client::builder()
        .render_handler(render_handler)
        .life_span_handler(/* ... */)
        .build();

    let window_info = WindowInfo::new_for_offscreen(width, height);
    let browser_settings = BrowserSettings::new();

    let browser = cef::BrowserHost::create_browser_sync(
        &window_info,
        Some(client),
        url,
        &browser_settings,
        None,
        None,
    ).unwrap();

    // Store browser state keyed by browser ID
    let browser_id = browser.get_identifier();
    profile_state.browsers.lock().unwrap()
        .insert(browser_id, browser_state);
}
```

**7. Profile server: Route paint callbacks to correct browser**

Each `RenderHandlerInner` holds its own `BrowserState`, so paint callbacks
automatically go to the correct GUI endpoint. No routing needed — it's built
into the structure.

**8. GUI: No changes required**

The GUI already creates a separate XPC listener per pane. It doesn't know or
care whether the profile server is new or reused.

#### Files to Modify

| Action | File                                | Change                          |
| ------ | ----------------------------------- | ------------------------------- |
| Modify | `ts3/termsurf-launcher/src/main.rs` | Add running_profiles, routing   |
| Modify | `ts3/termsurf-profile/src/main.rs`  | Multi-browser, register_profile |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open

# First webview - spawns profile process
web google.com
ps aux | grep termsurf-profile
# Should show 1 process

# Second webview same profile - reuses process
web github.com
ps aux | grep termsurf-profile
# Should STILL show 1 process (same PID)

# Both panes should render their respective pages
# Check logs
cat /tmp/termsurf-launcher.log
# Should show: "Forwarding to existing profile 'default'"

cat /tmp/termsurf-profile-*.log
# Should show: two "create_browser" logs, two IOSurface sends
```

#### Success Criteria

- [ ] First `web` command spawns a profile process
- [ ] Second `web` command (same profile) reuses existing process
- [ ] Both webviews render in their respective panes
- [ ] Different profiles spawn separate processes
- [ ] Launcher logs show "Forwarding to existing profile" on second request
- [ ] Profile logs show multiple browser creations
- [ ] No CEF SingletonLock errors
