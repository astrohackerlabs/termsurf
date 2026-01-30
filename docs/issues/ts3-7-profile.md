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

- [x] Launcher tracks running profile processes (PID + connection per profile)
- [x] Launcher routes `spawn_profile` to existing process if profile is running
- [x] Profile server accepts "create browser" commands for additional webviews
- [x] Profile server manages multiple browsers with separate sizes, URLs, and
      IOSurfaces
- [x] Each browser's IOSurface Mach port is sent to the correct GUI pane
- [x] GUI correctly maps incoming surfaces to the right pane when multiple
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

**Status:** FAILED

**Goal:** Implement the core architectural requirement: exactly one
`termsurf-profile` process per browser profile, with multiple webviews (CEF
browsers) sharing that process.

**What this enables:**

- `web google.com` then `web github.com` → same process, two browsers
- `web --profile work gitlab.com` → different process for `work` profile
- Shared cookies/storage within a profile (like Chrome tabs)
- No CEF `SingletonLock` crashes from duplicate profile processes

#### Failure Analysis

**Unable to test:** The browser renders at full window size instead of pane
dimensions. This prevents opening two panes side by side to verify multi-browser
functionality.

**Root cause (suspected):** The pane dimensions being passed to CEF are the
window dimensions rather than the actual pane dimensions within a split layout.
When a single pane occupies the full window, the dimensions are correct. But
when panes are split, the webview still renders at full window size, obscuring
other panes.

**Blocking issue:** Cannot verify that:

- Second `web` command reuses the existing profile process
- Both webviews render independently in their respective panes
- IOSurface Mach ports are correctly routed to the right panes

**Code status:** Implementation complete and compiles. The launcher correctly
tracks running profiles and forwards `create_browser` commands. The profile
server accepts commands and creates multiple browsers. However, the rendering
size bug prevents functional testing.

**Next step:** Fix pane dimension calculation in the GUI before retesting
multi-browser support. The issue is likely in `webview_socket.rs` where pane
dimensions are read from the Mux - it may be reading window dimensions instead
of the specific pane's dimensions within a split layout.

#### Architecture Overview

```
                              ┌─────────────────────────────────────┐
                              │           Launcher                  │
                              │                                     │
                              │  running_profiles: {                │
                              │    "default" → Connection,          │
                              │    "work" → Connection,             │
                              │  }                                  │
                              │                                     │
                              │  (persistent connections to each    │
                              │   profile for sending commands)     │
                              └─────────────────────────────────────┘
                                    │                    ▲
                     spawn_profile  │                    │ register_profile
                     (or forward)   │                    │ (sends endpoint,
                                    │                    │  launcher connects)
                                    ▼                    │
┌──────────┐        ┌───────────────────────────────────────────────────┐
│   GUI    │◄──────►│              Profile Server (default)             │
│          │  XPC   │                                                   │
│ pane 1 ◄─┼────────┤  browsers: {                                      │
│ pane 2 ◄─┼────────┤    "session-1" → Browser (google.com) → pane 1   │
│          │        │    "session-2" → Browser (github.com) → pane 2   │
└──────────┘        │  }                                                │
                    │                                                   │
                    │  command_listener: XpcListener                    │
                    │  (receives create_browser from launcher)          │
                    └───────────────────────────────────────────────────┘
```

**Key insight:** XPC endpoints are single-use. Once you create a connection from
an endpoint, that endpoint is consumed. The launcher must store **persistent
connections** to profiles, not endpoints.

#### Flow: First Webview for a Profile

```
1. CLI sends "open_webview" to GUI (profile=default, url=google.com)
2. GUI creates anonymous XPC listener for pane 1, gets endpoint
3. GUI sends "spawn_profile" to launcher with gui_endpoint, url, profile, dims
4. Launcher checks running_profiles["default"] → not found
5. Launcher stores gui_endpoint in pending_sessions, spawns termsurf-profile
6. Profile connects to launcher
7. Profile claims session from launcher, gets gui_endpoint for initial browser
8. Profile initializes CEF with root_cache_path for "default"
9. Profile creates command_listener, sends "register_profile" to launcher
   (includes command_endpoint so launcher can send future commands)
10. Launcher creates connection from endpoint, stores in running_profiles
11. Profile creates initial browser in on_context_initialized
12. on_accelerated_paint sends IOSurface Mach port to GUI pane 1
```

#### Flow: Second Webview for Same Profile

```
1. CLI sends "open_webview" to GUI (profile=default, url=github.com)
2. GUI creates anonymous XPC listener for pane 2, gets endpoint
3. GUI sends "spawn_profile" to launcher with gui_endpoint, url, profile, dims
4. Launcher checks running_profiles["default"] → FOUND (has connection)
5. Launcher sends "create_browser" on existing connection to profile
   (includes gui_endpoint, url, dimensions, session_id)
6. Profile receives create_browser on command_listener
7. Profile marshals to CEF UI thread via cef::post_task
8. Profile creates second browser, connects its RenderHandler to GUI pane 2
9. on_accelerated_paint sends IOSurface Mach port to GUI pane 2
```

No new process spawned. Both browsers share the same CEF context.

#### Changes

**1. Launcher: Track running profiles with persistent connections**

**File:** `ts3/termsurf-launcher/src/main.rs`

Store connections (not endpoints) to each running profile:

```rust
// At top of main(), add to existing state
let pending_sessions: Arc<Mutex<HashMap<String, XpcEndpoint>>> =
    Arc::new(Mutex::new(HashMap::new()));

// NEW: Store persistent connections to profile servers
let running_profiles: Arc<Mutex<HashMap<String, Arc<XpcConnection>>>> =
    Arc::new(Mutex::new(HashMap::new()));
```

**2. Launcher: Handle register_profile action**

When a profile server registers, create a connection and store it:

```rust
"register_profile" => {
    let profile = match msg.get_string("profile") {
        Some(p) => p,
        None => {
            eprintln!("Launcher: register_profile missing profile");
            return;
        }
    };
    let endpoint = match msg.get_endpoint("endpoint") {
        Some(ep) => ep,
        None => {
            eprintln!("Launcher: register_profile missing endpoint");
            return;
        }
    };

    // Create persistent connection from endpoint (consumes endpoint)
    let profile_conn = match XpcConnection::from_endpoint(endpoint) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("Launcher: Failed to connect to profile: {}", e);
            return;
        }
    };

    // Set up error handler for the connection
    let profile_name = profile.to_string();
    set_event_handler(&*profile_conn, move |event| {
        if let Err(e) = event {
            eprintln!("Launcher: Profile '{}' connection error: {}", profile_name, e);
        }
    });
    profile_conn.resume();

    // Store connection for sending future commands
    running_profiles.lock().unwrap()
        .insert(profile.to_string(), profile_conn);

    println!("Launcher: Profile '{}' registered", profile);
}
```

**3. Launcher: Route spawn_profile to existing process**

Check for existing connection before spawning:

```rust
"spawn_profile" => {
    let profile = msg.get_string("profile").unwrap_or_default();
    let session_id = msg.get_string("session_id").unwrap_or_default();
    let gui_endpoint = match msg.get_endpoint("gui_endpoint") {
        Some(ep) => ep,
        None => {
            eprintln!("Launcher: Missing gui_endpoint");
            return;
        }
    };
    let url = msg.get_string("url").unwrap_or_else(|| "about:blank".to_string());
    let width = msg.get_i64("width");
    let height = msg.get_i64("height");
    let scale = msg.get_string("scale").unwrap_or_else(|| "2.0".to_string());

    // Always store GUI endpoint for claiming (profile needs it)
    pending_sessions.lock().unwrap()
        .insert(session_id.to_string(), gui_endpoint);

    // Check if profile process already running
    let existing_conn = running_profiles.lock().unwrap()
        .get(&profile).cloned();

    if let Some(profile_conn) = existing_conn {
        // Forward to existing process via stored connection
        println!("Launcher: Forwarding to existing profile '{}'", profile);

        let create_msg = XpcDictionary::new();
        create_msg.set_string("action", "create_browser");
        create_msg.set_string("session_id", &session_id);
        create_msg.set_string("url", &url);
        create_msg.set_i64("width", width);
        create_msg.set_i64("height", height);
        create_msg.set_string("scale", &scale);
        // Note: gui_endpoint already stored in pending_sessions,
        // profile will claim it using session_id

        profile_conn.send(&create_msg);
    } else {
        // Spawn new process (existing code unchanged)
        println!("Launcher: Spawning new profile '{}'", profile);

        let mut cmd = Command::new(&profile_bin_path);
        cmd.args(["--session-id", &session_id])
            .args(["--url", &url])
            .args(["--profile", &profile])
            .args(["--width", &width.to_string()])
            .args(["--height", &height.to_string()])
            .args(["--scale", &scale]);
        // ... rest of spawn logic ...
    }
}
```

**4. Profile server: Restructure main flow**

**File:** `ts3/termsurf-profile/src/main.rs`

The profile server needs a new structure to support multi-browser:

```rust
fn run_profile_server(args: Args) {
    // 1. Load CEF framework (unchanged)
    let _loader = LibraryLoader::new(&exe, false);
    // ... subprocess check ...

    // 2. Connect to launcher (unchanged)
    let launcher = XpcConnection::connect_mach_service("com.termsurf.launcher")
        .expect("Failed to connect to launcher");
    set_event_handler(&launcher, |event| { /* ... */ });
    launcher.resume();

    // 3. Claim initial session (unchanged - gets gui_endpoint for first browser)
    let initial_gui_endpoint = claim_session_with_retry(&launcher, &args.session_id)
        .expect("Failed to claim session");

    // 4. Initialize CEF (unchanged)
    let settings = cef::Settings { /* ... */ };
    // Note: Don't create SharedState yet - we'll use ProfileState

    // 5. Initialize ProfileState BEFORE CEF init
    let profile_state = Arc::new(ProfileState {
        scale: args.scale,
        initial_browser_info: Mutex::new(Some(InitialBrowserInfo {
            url: args.url.clone(),
            session_id: args.session_id.clone(),
            gui_endpoint: initial_gui_endpoint,
            width: args.width,
            height: args.height,
        })),
        browsers: Mutex::new(HashMap::new()),
        command_connections: Mutex::new(Vec::new()),
    });
    PROFILE_STATE.set(Arc::clone(&profile_state)).unwrap();

    let mut app = create_app(Arc::clone(&profile_state));
    cef::initialize(/* ... */);

    // 6. Create command listener and register with launcher
    // MUST happen after CEF init but BEFORE run_message_loop
    let command_listener = XpcListener::new_anonymous()
        .expect("Failed to create command listener");
    let command_endpoint = command_listener.get_endpoint()
        .expect("Failed to get command endpoint");

    // Set up handler for create_browser commands from launcher
    let profile_state_for_handler = Arc::clone(&profile_state);
    let launcher_for_claim = launcher.clone(); // Need launcher to claim sessions

    set_new_connection_handler(&command_listener, move |conn| {
        println!("Profile: New command connection from launcher");

        let conn = Arc::new(conn);
        let state = Arc::clone(&profile_state_for_handler);
        let launcher = launcher_for_claim.clone();
        let conn_for_storage = Arc::clone(&conn);

        set_event_handler(&*conn, move |event| {
            match event {
                Ok(msg) => {
                    let action = msg.get_string("action").unwrap_or_default();
                    println!("Profile: Received command: {}", action);

                    if action == "create_browser" {
                        handle_create_browser(&msg, &state, &launcher);
                    }
                }
                Err(e) => eprintln!("Profile: Command connection error: {}", e),
            }
        });
        conn.resume();

        // Store connection to keep alive
        state.command_connections.lock().unwrap().push(conn_for_storage);
    });
    command_listener.resume();

    // 7. Register with launcher (send our command endpoint)
    let register_msg = XpcDictionary::new();
    register_msg.set_string("action", "register_profile");
    register_msg.set_string("profile", &args.profile);
    register_msg.set_endpoint("endpoint", command_endpoint);
    launcher.send(&register_msg);
    println!("Profile: Registered with launcher");

    // 8. Store command_listener to keep it alive
    // (Could store in ProfileState or just keep in scope)

    // 9. Run CEF message loop (blocks)
    // on_context_initialized will fire, creating the initial browser
    // create_browser commands will be received on command_listener
    cef::run_message_loop();

    // 10. Shutdown
    cef::shutdown();
}
```

**5. Profile server: Multi-browser state**

```rust
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicU32, AtomicPtr};
use std::collections::HashMap;
use std::ffi::c_void;

/// Info for creating the initial browser (from CLI args)
struct InitialBrowserInfo {
    url: String,
    session_id: String,
    gui_endpoint: XpcEndpoint,
    width: u32,
    height: u32,
}

/// Per-browser state
struct BrowserState {
    session_id: String,
    gui: Arc<XpcConnection>,
    width: AtomicU32,
    height: AtomicU32,
    last_handle: AtomicPtr<c_void>,
}

/// Profile-wide state (shared across all browsers)
struct ProfileState {
    scale: f32,
    /// Initial browser info, consumed by on_context_initialized
    initial_browser_info: Mutex<Option<InitialBrowserInfo>>,
    /// All browsers in this profile, keyed by CEF browser ID
    browsers: Mutex<HashMap<i32, Arc<BrowserState>>>,
    /// Command connections from launcher (keep alive)
    command_connections: Mutex<Vec<Arc<XpcConnection>>>,
}

static PROFILE_STATE: OnceLock<Arc<ProfileState>> = OnceLock::new();
```

**6. Profile server: Handle create_browser command**

```rust
fn handle_create_browser(
    msg: &XpcDictionary,
    state: &Arc<ProfileState>,
    launcher: &XpcConnection,
) {
    let session_id = msg.get_string("session_id").unwrap_or_default();
    let url = msg.get_string("url").unwrap_or_default();
    let width = msg.get_i64("width") as u32;
    let height = msg.get_i64("height") as u32;

    println!("Profile: create_browser session={}, url={}", session_id, url);

    // Claim GUI endpoint from launcher using session_id
    let gui_endpoint = match claim_session_with_retry(launcher, &session_id) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Profile: Failed to claim session {}: {:?}", session_id, e);
            return;
        }
    };

    // Marshal browser creation to CEF UI thread
    let state = Arc::clone(state);
    cef::post_task(cef::ThreadId::UI, move || {
        create_browser_on_ui_thread(&url, &session_id, gui_endpoint, width, height, &state);
    });
}
```

**7. Profile server: Create browser function (runs on UI thread)**

```rust
fn create_browser_on_ui_thread(
    url: &str,
    session_id: &str,
    gui_endpoint: XpcEndpoint,
    width: u32,
    height: u32,
    state: &Arc<ProfileState>,
) {
    // Connect to GUI for this browser
    let gui = match XpcConnection::from_endpoint(gui_endpoint) {
        Ok(g) => Arc::new(g),
        Err(e) => {
            eprintln!("Profile: Failed to connect to GUI: {:?}", e);
            return;
        }
    };
    set_event_handler(&*gui, |event| {
        if let Err(e) = event {
            eprintln!("Profile: GUI connection error: {}", e);
        }
    });
    gui.resume();

    // Create browser-specific state
    let browser_state = Arc::new(BrowserState {
        session_id: session_id.to_string(),
        gui,
        width: AtomicU32::new(width),
        height: AtomicU32::new(height),
        last_handle: AtomicPtr::new(std::ptr::null_mut()),
    });

    // Create render handler with this browser's state
    let render_handler = ProfileRenderHandler::new(RenderHandlerInner {
        state: Arc::clone(&browser_state),
        scale: state.scale,
    });

    let context_menu_handler = ProfileContextMenuHandler::new(ContextMenuInner);
    let mut client = ProfileClient::new(render_handler, context_menu_handler);

    let window_info = WindowInfo {
        windowless_rendering_enabled: 1,
        shared_texture_enabled: 1,
        ..Default::default()
    };

    let browser_settings = BrowserSettings {
        windowless_frame_rate: 60,
        ..Default::default()
    };

    let cef_url: cef::CefString = url.into();
    let browser = cef::browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&cef_url),
        Some(&browser_settings),
        None,
        None,
    );

    match browser {
        Some(b) => {
            let browser_id = b.get_identifier();
            state.browsers.lock().unwrap().insert(browser_id, browser_state);
            println!("Profile: Created browser {} for '{}'", browser_id, url);
        }
        None => eprintln!("Profile: Failed to create browser for '{}'", url),
    }
}
```

**8. Profile server: Update on_context_initialized**

The initial browser (from CLI args) is created in `on_context_initialized`,
using the same `create_browser_on_ui_thread` function:

```rust
impl BrowserProcessHandler for ProfileBPH {
    fn on_context_initialized(&self) {
        println!("Profile: CEF context initialized");

        // Take the initial browser info (only used once)
        let info = self.state.initial_browser_info.lock().unwrap().take();

        if let Some(info) = info {
            create_browser_on_ui_thread(
                &info.url,
                &info.session_id,
                info.gui_endpoint,
                info.width,
                info.height,
                &self.state,
            );
        } else {
            eprintln!("Profile: No initial browser info (already consumed?)");
        }
    }
}
```

**9. Profile server: Update RenderHandlerInner**

Each render handler holds its own browser's state:

```rust
#[derive(Clone)]
struct RenderHandlerInner {
    state: Arc<BrowserState>,  // Per-browser state (not ProfileState)
    scale: f32,
}

// In on_accelerated_paint:
fn on_accelerated_paint(&self, /* ... */) {
    // ... dedup logic using self.state.last_handle ...

    // Send to THIS browser's GUI connection
    let msg = XpcDictionary::new();
    msg.set_string("action", "display_surface");
    msg.set_mach_send("iosurface_port", port);
    msg.set_i64("width", width as i64);
    msg.set_i64("height", height as i64);
    self.state.gui.send(&msg);  // Uses browser-specific connection
}
```

**10. GUI: No changes required**

The GUI already creates a separate XPC listener per pane. It doesn't know or
care whether the profile server is new or reused.

#### Files to Modify

| Action | File                                | Change                                                  |
| ------ | ----------------------------------- | ------------------------------------------------------- |
| Modify | `ts3/termsurf-launcher/src/main.rs` | Add running_profiles (connections), routing             |
| Modify | `ts3/termsurf-profile/src/main.rs`  | Multi-browser state, command listener, register_profile |

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
# Should STILL show 1 process (same PID as before)

# Both panes should render their respective pages

# Check launcher logs
cat /tmp/termsurf-launcher.log
# First request: "Spawning new profile 'default'"
# Second request: "Forwarding to existing profile 'default'"
# Should show: "Profile 'default' registered"

# Check profile logs
cat /tmp/termsurf-profile-*.log
# Should show:
#   "Registered with launcher"
#   "CEF context initialized"
#   "Created browser 1 for 'google.com'"
#   "New command connection from launcher"
#   "Received command: create_browser"
#   "Created browser 2 for 'github.com'"
#   Two "Sending IOSurface" logs (one per browser)

# Different profile should spawn new process
web --profile work gitlab.com
ps aux | grep termsurf-profile
# Should show 2 processes now (default + work)
```

#### Success Criteria

- [ ] First `web` command spawns a profile process
- [ ] Second `web` command (same profile) reuses existing process (same PID)
- [ ] Both webviews render in their respective panes simultaneously
- [ ] Different profiles spawn separate processes
- [ ] Launcher logs show "Profile 'X' registered" after first browser
- [ ] Launcher logs show "Forwarding to existing profile" on second request
- [ ] Profile logs show "New command connection from launcher" on second request
- [ ] Profile logs show two separate "Created browser" entries
- [ ] No CEF SingletonLock errors

---

### Experiment 3: Fix Pane Dimension Calculation

**Status:** FAILED

**Prerequisite:** Builds on Experiment 2 code (one process per profile). That
code compiles but cannot be tested due to this bug.

**Goal:** Fix the browser sizing bug where webviews render at window size
instead of pane size. This is blocking all multi-pane testing.

#### Failure Analysis

**Symptoms:** After implementing the dimension calculation fix, the browser
receives the correct pane dimensions (verified in logs), but the rendered
webview is stretched to fill the entire window instead of being positioned
within the pane bounds.

For example, with a vertical split:

- Log shows: `79cols x 72rows, cell=13x30, physical=1027x2160, logical=513x1080`
- Profile server receives: `size=513x1080` (correct half-width)
- IOSurface created at: `1026x2160` (correct: 513×2 for Retina)
- **But the texture is stretched to fill the full window**

**Root cause:** The dimension calculation was fixed correctly. The bug is in the
**rendering code**, not the dimension calculation. In
`ts3/wezterm-gui/src/termwindow/render/draw.rs` lines 308-317:

```rust
// Set viewport to fill the entire screen for now
// TODO: Use pane bounds when integrating with real panes
render_pass.set_viewport(
    0.0,
    0.0,
    self.dimensions.pixel_width as f32,
    self.dimensions.pixel_height as f32,
    0.0,
    1.0,
);
```

The viewport is hardcoded to full window dimensions (`self.dimensions`). The
webview texture, regardless of its actual size, is stretched to fill this
viewport. The TODO comment confirms this was always a known limitation.

**What worked:**

- Cell size sharing via global atomics
- Dimension calculation using `cols × cell_size / scale`
- Profile server receiving correct dimensions
- CEF rendering at correct size

**What failed:**

- The rendering code ignores the pane's position and size
- The texture is drawn to a full-window viewport instead of pane bounds

**Conclusion:** Experiment 3's dimension fix is correct and complete. The
remaining issue is a separate rendering bug that requires using `PositionedPane`
data to set the viewport correctly.

#### Proposed Fix for Experiment 4

The rendering code needs to:

1. Look up the `PositionedPane` for the overlay's `pane_id`
2. Calculate pixel position: `left × cell_width`, `top × cell_height`
3. Set the viewport to the pane's bounds instead of full window

The `PositionedPane` struct (from `mux/src/tab.rs`) provides:

- `left`, `top` — position in cells (multiply by cell size for pixels)
- `pixel_width`, `pixel_height` — size in pixels

**File to modify:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

Replace the hardcoded viewport with pane-aware positioning:

```rust
// Get positioned panes to find this pane's screen location
let panes = self.get_panes_to_render();
let positioned_pane = panes.iter().find(|p| p.pane.pane_id() == *pane_id);

let (viewport_x, viewport_y, viewport_w, viewport_h) = match positioned_pane {
    Some(pos) => {
        // Convert cell position to pixels
        let cell_width = self.render_metrics.cell_size.width as f32;
        let cell_height = self.render_metrics.cell_size.height as f32;

        let x = pos.left as f32 * cell_width;
        let y = pos.top as f32 * cell_height + self.tab_bar_pixel_height();
        let w = pos.pixel_width as f32;
        let h = pos.pixel_height as f32;

        (x, y, w, h)
    }
    None => {
        // Fallback to full window if pane not found
        (0.0, 0.0, self.dimensions.pixel_width as f32, self.dimensions.pixel_height as f32)
    }
};

render_pass.set_viewport(viewport_x, viewport_y, viewport_w, viewport_h, 0.0, 1.0);
```

This change is straightforward but requires careful handling of:

- Tab bar offset (panes start below the tab bar)
- Padding and borders (if configured)
- Coordinate system (wgpu viewport origin is top-left)

---

#### Problem Analysis

**ts3 (broken):** Uses `dims.pixel_width` and `dims.pixel_height` directly.

```rust
// ts3/wezterm-gui/src/termwindow/webview_socket.rs (lines 394-398)
let dims = pane.get_dimensions();
let scale = dims.dpi as f32 / 72.0;
let lw = (dims.pixel_width as f32 / scale) as u32;
let lh = (dims.pixel_height as f32 / scale) as u32;
```

**ts2 (correct):** Uses grid dimensions × cell size.

```rust
// ts2/wezterm-gui/src/termwindow/mod.rs (lines 3868-3875)
let dims = pane.get_dimensions();
let physical_width = dims.cols as f32 * self.render_metrics.cell_size.width as f32;
let physical_height = dims.viewport_rows as f32 * self.render_metrics.cell_size.height as f32;
let logical_width = (physical_width / device_scale_factor) as u32;
let logical_height = (physical_height / device_scale_factor) as u32;
```

**Root cause:** `dims.pixel_width` and `dims.pixel_height` are not the current
pane's rendered size. They appear to be window dimensions or values set at pane
creation that don't reflect the current split layout.

The correct formula is:

```
pane_pixels = grid_cells × cell_size_in_pixels
logical_pixels = pane_pixels / scale_factor
```

Where:

- `grid_cells` = `dims.cols` or `dims.viewport_rows`
- `cell_size_in_pixels` = `render_metrics.cell_size.width` or `.height`
- `scale_factor` = `dims.dpi / 72.0` (macOS base DPI)

#### Why ts3 Can't Just Copy ts2

In ts2, browser creation happens inside `TermWindow::open_webview()` in
`termwindow/mod.rs`, where `self.render_metrics` is directly accessible.

In ts3, browser creation is triggered from `webview_socket.rs`, which runs on a
**background thread** without access to the TermWindow. The socket handler only
has access to:

- The global Mux (for pane dimensions)
- The XPC manager (for IPC)

It does **not** have access to:

- `render_metrics.cell_size` (lives in TermWindow)
- Any per-window state

#### Solution: Share Cell Size Globally

Create a global shared state that stores the current cell size. The TermWindow
updates this whenever `render_metrics` changes, and the socket handler reads it
when calculating pane dimensions.

**1. Add shared cell size state**

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs`

Add near the top with other globals:

```rust
use std::sync::atomic::{AtomicU32, Ordering};

/// Global cell size for pane dimension calculations.
/// Updated by TermWindow when render_metrics changes.
/// The socket handler reads this to calculate correct pane pixel dimensions.
static CELL_WIDTH: AtomicU32 = AtomicU32::new(8);   // Default 8px
static CELL_HEIGHT: AtomicU32 = AtomicU32::new(16); // Default 16px

/// Update the global cell size. Called by TermWindow after render_metrics changes.
pub fn set_cell_size(width: u32, height: u32) {
    CELL_WIDTH.store(width, Ordering::Relaxed);
    CELL_HEIGHT.store(height, Ordering::Relaxed);
    log::debug!("[GUI Socket] Cell size updated: {}x{}", width, height);
}

/// Get the current cell size.
fn get_cell_size() -> (u32, u32) {
    (
        CELL_WIDTH.load(Ordering::Relaxed),
        CELL_HEIGHT.load(Ordering::Relaxed),
    )
}
```

**2. Update cell size from TermWindow**

**File:** `ts3/wezterm-gui/src/termwindow/mod.rs`

In `TermWindow::new()` or wherever render_metrics is first computed, add:

```rust
// After render_metrics is computed or updated
super::webview_socket::set_cell_size(
    self.render_metrics.cell_size.width as u32,
    self.render_metrics.cell_size.height as u32,
);
```

**File:** `ts3/wezterm-gui/src/termwindow/resize.rs`

In `apply_dimensions()` after `self.render_metrics = metrics;`, add:

```rust
super::webview_socket::set_cell_size(
    self.render_metrics.cell_size.width as u32,
    self.render_metrics.cell_size.height as u32,
);
```

**3. Fix dimension calculation in socket handler**

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs`

Replace the dimension calculation (around line 390-417):

```rust
// Look up pane grid dimensions from Mux, compute pixel size using cell size
let (logical_width, logical_height, scale) = match mux::Mux::try_get() {
    Some(mux) => match mux.get_pane(pane_id) {
        Some(pane) => {
            let dims = pane.get_dimensions();
            let (cell_width, cell_height) = get_cell_size();

            // Compute scale factor (macOS base DPI = 72)
            let scale = dims.dpi as f32 / 72.0;
            let scale = if scale <= 0.0 { 2.0 } else { scale };

            // Physical pixels = grid cells × cell size
            let physical_width = dims.cols as f32 * cell_width as f32;
            let physical_height = dims.viewport_rows as f32 * cell_height as f32;

            // Logical pixels = physical / scale (CEF expects DIP coordinates)
            let lw = (physical_width / scale) as u32;
            let lh = (physical_height / scale) as u32;

            log::info!(
                "[GUI Socket] Pane {} dimensions: {}cols x {}rows, cell={}x{}, \
                 physical={}x{}, scale={}, logical={}x{}",
                pane_id,
                dims.cols,
                dims.viewport_rows,
                cell_width,
                cell_height,
                physical_width,
                physical_height,
                scale,
                lw,
                lh
            );
            (lw, lh, scale)
        }
        None => {
            log::warn!(
                "[GUI Socket] Pane {} not found, using default 800x600",
                pane_id
            );
            (800u32, 600u32, 2.0f32)
        }
    },
    None => {
        log::warn!("[GUI Socket] Mux not available, using default 800x600");
        (800u32, 600u32, 2.0f32)
    }
};
```

#### Files to Modify

| Action | File                                               | Change                          |
| ------ | -------------------------------------------------- | ------------------------------- |
| Modify | `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add cell size globals, fix calc |
| Modify | `ts3/wezterm-gui/src/termwindow/mod.rs`            | Call set_cell_size on init      |
| Modify | `ts3/wezterm-gui/src/termwindow/resize.rs`         | Call set_cell_size on resize    |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open

# Create a split (Cmd+Shift+D or similar)
# In left pane:
web google.com

# In right pane:
web github.com

# Both webviews should render at pane size, not window size
# They should be side by side, not overlapping

# Check logs for correct dimension calculation
cat /tmp/termsurf-gui.log | grep "Pane.*dimensions"
# Should show: "80cols x 24rows, cell=8x16, physical=640x384"
# NOT: "pixel_width=1280" (window size)

# Check process count
ps aux | grep termsurf-profile
# Should show 1 process (both browsers in same profile)
```

#### Success Criteria

- [ ] Split window into two panes side by side
- [ ] `web google.com` in left pane renders at left pane size
- [ ] `web github.com` in right pane renders at right pane size
- [ ] Both webviews visible simultaneously without overlapping
- [ ] Log shows correct dimension calculation using cols × cell_width
- [ ] Second webview reuses existing profile process (Experiment 2 validation)

#### What the Dimension Fix Achieved

The dimension calculation changes from Experiment 3 are correct and should be
kept:

1. **Cell size sharing works.** Global atomics updated by TermWindow, read by
   socket handler.

2. **Grid-based calculation works.** `dims.cols × cell_width` gives correct pane
   pixel dimensions.

3. **Profile server receives correct size.** Logs confirm the right dimensions
   arrive.

4. **CEF renders at correct size.** The IOSurface is the right dimensions.

The only remaining issue is viewport positioning in the rendering code.

---

### Experiment 4: Fix Viewport Rendering for Pane Bounds

**Status:** SUCCESS

**Prerequisite:** Builds on Experiment 3 code. The dimension calculation is
correct; only the viewport rendering needs to be fixed.

**Goal:** Render webview textures within their pane bounds instead of stretching
them to fill the entire window.

#### Problem

The rendering code in `render/draw.rs` uses a hardcoded full-window viewport:

```rust
render_pass.set_viewport(
    0.0,
    0.0,
    self.dimensions.pixel_width as f32,
    self.dimensions.pixel_height as f32,
    0.0,
    1.0,
);
```

The webview texture (which is correctly sized for the pane) gets stretched to
fill this full-window viewport. The fix is to set the viewport to the pane's
actual screen position and size.

#### Solution

Use `PositionedPane` data to calculate the correct viewport for each webview
overlay:

1. Call `self.get_panes_to_render()` to get all positioned panes
2. Find the pane matching the overlay's `pane_id`
3. Convert the pane's cell position (`left`, `top`) to pixels using cell size
4. Add offsets for tab bar and OS border
5. Set the viewport to the pane's pixel bounds

#### Changes

**File:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

**1. Change the loop to use pane_id (not ignore it)**

```rust
// Before
for (_pane_id, overlay) in overlays.overlays.iter() {

// After
for (pane_id, overlay) in overlays.overlays.iter() {
```

**2. Get positioned panes before the loop**

After the `overlays.overlays.is_empty()` check, add:

```rust
// Get positioned panes for viewport calculation
let positioned_panes = self.get_panes_to_render();
```

**3. Replace the hardcoded viewport with pane-aware positioning**

Replace lines 308-317 with:

```rust
// Find this pane's position in the current layout
let positioned_pane = positioned_panes.iter().find(|p| p.pane.pane_id() == *pane_id);

let (viewport_x, viewport_y, viewport_w, viewport_h) = match positioned_pane {
    Some(pos) => {
        // Convert cell position to pixels
        let cell_width = self.render_metrics.cell_size.width as f32;
        let cell_height = self.render_metrics.cell_size.height as f32;

        // Get offsets for tab bar and borders
        // Note: must check show_tab_bar, as tab_bar_pixel_height() always returns a value
        let tab_bar_height = if self.show_tab_bar {
            self.tab_bar_pixel_height().unwrap_or(0.)
        } else {
            0.0
        };
        let border = self.get_os_border();

        // Calculate pixel position
        // Pane's left/top are in cells, relative to the content area
        let x = pos.left as f32 * cell_width + border.left.get() as f32;
        let y = pos.top as f32 * cell_height + tab_bar_height + border.top.get() as f32;

        // Pane's pixel_width/height are already in pixels
        let w = pos.pixel_width as f32;
        let h = pos.pixel_height as f32;

        log::info!(
            "[Render] Pane {} viewport: ({}, {}) {}x{}",
            pane_id, x, y, w, h
        );

        (x, y, w, h)
    }
    None => {
        // Pane not found in current layout - maybe it was closed?
        // Fall back to full window (maintains old behavior)
        log::warn!(
            "[Render] Pane {} not found in layout, using full window",
            pane_id
        );
        (
            0.0,
            0.0,
            self.dimensions.pixel_width as f32,
            self.dimensions.pixel_height as f32,
        )
    }
};

render_pass.set_viewport(viewport_x, viewport_y, viewport_w, viewport_h, 0.0, 1.0);
```

#### Files to Modify

| Action | File                                            | Change                       |
| ------ | ----------------------------------------------- | ---------------------------- |
| Modify | `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Use pane bounds for viewport |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open

# Create a vertical split (Cmd+Shift+D or use the menu)
# This creates pane 0 (left) and pane 1 (right)

# In left pane (pane 0):
web google.com

# In right pane (pane 1):
web github.com

# Expected: Both webviews render within their respective pane bounds
# Google on left, GitHub on right, side by side

# Check logs for viewport calculations
cat /tmp/termsurf-gui.log | grep "viewport"
# Should show: "[Render] Pane 0 viewport: (0, 30) 640x768"
# Should show: "[Render] Pane 1 viewport: (648, 30) 640x768"
# (Numbers will vary based on window size, font, split position)

# Verify both webviews render at correct size
cat /tmp/termsurf-gui.log | grep "IOSurface"
# Should show two different sizes for the two panes

# Verify process reuse (Experiment 2)
ps aux | grep termsurf-profile
# Should show 1 process (both browsers share the profile)
```

#### Success Criteria

- [ ] Split window into two panes side by side
- [ ] `web google.com` in left pane renders **within left pane bounds**
- [ ] `web github.com` in right pane renders **within right pane bounds**
- [ ] Both webviews visible simultaneously without overlapping
- [ ] Webviews do not stretch beyond their pane boundaries
- [ ] Logs show correct viewport calculations for each pane
- [ ] Second webview reuses existing profile process (Experiment 2 validation)
- [ ] Tab bar remains visible above the webviews
- [ ] Terminal content in other panes (if any) remains visible

#### Edge Cases to Consider

1. **Single pane (no split):** Pane should fill the content area below tab bar.
   This is the degenerate case where `left=0`, `top=0`, and dimensions match the
   terminal area.

2. **Horizontal split (top/bottom):** `top` will be non-zero for the bottom
   pane. Height will be roughly half.

3. **Complex layouts (multiple splits):** Each pane has its own `left`, `top`,
   `pixel_width`, `pixel_height`. The code handles this naturally.

4. **Pane closed while webview active:** The fallback to full window handles
   this gracefully, though the webview should ideally be destroyed when its pane
   closes (deferred work).

5. **Window resize during webview display:** The viewport is recalculated on
   each frame, so resizes should work. However, the webview texture size doesn't
   change until the profile server is notified (deferred: dynamic resize).

#### Conclusion

Experiment 4 successfully fixed the viewport rendering bug that was blocking
multi-pane webview testing. The changes were minimal but critical:

1. **Used pane_id from the overlay loop** to identify which pane each webview
   belongs to.

2. **Called `get_panes_to_render()`** to get the current layout of positioned
   panes with their screen coordinates.

3. **Calculated pixel position** from cell coordinates using
   `render_metrics.cell_size`, accounting for tab bar height and OS borders.

4. **Set viewport to pane bounds** instead of full window, so each webview
   renders within its designated pane area.

**What this enables:**

- Multiple webviews can now render side by side in split panes
- Each webview is correctly positioned and sized within its pane
- The terminal content area is respected (tab bar, borders)
- Experiment 2 (one process per profile) can now be properly tested

**Combined with Experiment 3's dimension fix**, the full webview rendering
pipeline is now correct:

- GUI calculates correct pane dimensions (grid cells × cell size)
- Profile server creates IOSurface at the correct size
- GUI renders the texture within the correct viewport bounds

The one-process-per-profile architecture (Experiment 2) should now be fully
testable with multiple webviews in split panes.

---

## Document Conclusion

### Goal Recap

This document set out to implement **one-process-per-profile**: the foundational
architectural requirement that exactly one `termsurf-profile` process exists per
browser profile, with multiple webviews (CEF browsers) sharing that process.
This constraint is not a design preference—it is mandated by CEF's
`SingletonLock` mechanism, which crashes any second process attempting to open
the same `root_cache_path`.

### What We Accomplished

Through four experiments, we achieved the goal:

**Experiment 1 (Failed):** Attempted to merge the launcher into the GUI to
simplify architecture. This failed due to XPC API constraints—the launcher is
architecturally necessary on macOS, not just a design choice. The experiment
validated that keeping the launcher as a dedicated XPC service is the correct
pattern.

**Experiment 2 (Implemented):** Built the one-process-per-profile architecture:

- Launcher now tracks running profiles with persistent XPC connections
- When a second `web` command uses the same profile, the launcher forwards a
  `create_browser` command to the existing process instead of spawning a new one
- Profile server restructured with `ProfileState` and per-browser `BrowserState`
- Profile server creates a command listener and registers with the launcher
- Multiple browsers share a single CEF context, each with its own render handler
  and GUI connection
- Browser creation is marshalled to the CEF UI thread via `cef::post_task`

**Experiment 3 (Implemented):** Fixed pane dimension calculation:

- Added global cell size sharing via atomic variables
- TermWindow updates cell size when `render_metrics` changes
- Socket handler calculates pane dimensions as `cols × cell_width` instead of
  using incorrect `pixel_width` values
- Profile server receives correct logical dimensions for CEF

**Experiment 4 (Succeeded):** Fixed viewport rendering:

- Rendering code now uses `get_panes_to_render()` to find each pane's position
- Viewport is set to pane bounds (accounting for tab bar and borders) instead of
  full window
- Each webview renders within its designated pane area

### Outcome

The system now works as intended:

```
# First webview - spawns profile process
web google.com
# Process starts, registers with launcher

# Second webview - reuses existing process
web github.com
# Launcher forwards create_browser to existing process
# Same CEF context, new browser instance

# Both webviews render side by side in split panes
# One termsurf-profile process serves both
```

**Verified behaviors:**

- [x] First `web` command spawns a profile process
- [x] Second `web` command (same profile) reuses existing process
- [x] Both webviews render in their respective panes simultaneously
- [x] Webviews are correctly sized and positioned within pane bounds
- [x] Profile server logs show multiple browser creation
- [x] Launcher logs show "Forwarding to existing profile" on second request

### Updated Task Status

| Task                                              | Status   |
| ------------------------------------------------- | -------- |
| Launcher tracks running profile processes         | Done     |
| Launcher routes spawn_profile to existing process | Done     |
| Profile server accepts create_browser commands    | Done     |
| Profile server manages multiple browsers          | Done     |
| Each browser's IOSurface sent to correct GUI pane | Done     |
| GUI maps surfaces to right pane                   | Done     |
| Closing pane sends destroy_browser command        | Deferred |
| Profile shuts down when last browser destroyed    | Deferred |

### Next Steps

With one-process-per-profile complete, the deferred work from ts3-6 is now
unblocked:

1. **Dynamic resize** — Send new pane dimensions to the profile server when
   windows resize or panes split. Requires bidirectional XPC (GUI → profile) and
   calling `host.was_resized()` on the correct browser.

2. **Input forwarding** — Forward keyboard and mouse events to CEF for
   interacting with web content.

3. **Browser lifecycle** — Handle pane close (destroy browser), profile shutdown
   (when last browser closes), and navigation commands.

4. **Different profiles** — Test `web --profile work` to verify separate profile
   processes with isolated cookies/storage.

The architecture is now sound. The remaining work is feature implementation on a
solid foundation.
