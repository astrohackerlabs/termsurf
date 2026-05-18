+++
status = "closed"
opened = "2026-01-24"
closed = "2026-01-24"
+++

# TermSurf 3.0 Architecture

## Overview

TermSurf 3.0 (ts3) is a terminal emulator with integrated browser capabilities,
built on WezTerm + CEF (Chromium Embedded Framework). This document describes
the core architecture, specifically the process model for browser integration.

## Background

### Why ts3?

TermSurf 2.0 (ts2) validated that WezTerm + CEF integration works:

- IOSurface texture sharing on macOS
- Keyboard, mouse, and scroll input handling
- Multiple browser instances in a single process
- Browser resize handling

However, our experiments revealed a fundamental limitation: **CEF can only
initialize once per process with a single `root_cache_path`**. This means a
shared browser daemon cannot support multiple isolated profiles (different
cookies, storage, login sessions).

ts3 addresses this with a new process model.

### Lessons from ts2

1. A shared CEF daemon forces all browsers to share one profile context
2. In order to support multiple profiles, we MUST separate the browser process
   from the window, and we MUST attach exactly one process per profile
3. CEF prevents two processes from opening the same profile (to avoid data
   corruption), so the `web` command must coordinate access to browser
   subprocesses

## Process Model

### Architecture

```
termsurf (main terminal process)
    │
    ├── termsurf web https://a.com ──┐
    ├── termsurf web https://b.com ──┴──► browser-subprocess (profile=default)
    │                                         └── CEF helper processes
    │
    ├── termsurf web --profile=work https://c.com ──► browser-subprocess (profile=work)
    │                                                     └── CEF helper processes
    │
    └── termsurf web --profile=personal https://d.com ──► browser-subprocess (profile=personal)
                                                              └── CEF helper processes
```

The `termsurf web` command is a **coordinator**:

- If a browser subprocess for the requested profile exists, connect to it
- If not, spawn a new browser subprocess for that profile
- Send commands to the subprocess to open URLs, navigate, etc.

### Key Principles

1. **One browser subprocess per profile**: Each profile gets its own browser
   subprocess with its own CEF context, enabling true isolation (separate
   cookies, storage, sessions).

2. **Multiple panes per profile**: A single browser subprocess can host multiple
   browser panes/tabs that share the same profile. This is efficient - you don't
   spawn a new process for each tab.

3. **The `web` command is a coordinator**: The `web` command does not run CEF
   directly. Instead, it spawns or connects to browser subprocesses based on the
   requested profile. This is necessary because CEF prevents two processes from
   opening the same profile directory.

4. **Cross-process texture sharing**: Browser content is rendered off-screen by
   CEF and shared with the main terminal process via platform-native APIs. This
   allows compositing browser panes alongside terminal panes. cef-rs supports:
   - **macOS**: IOSurface via Metal (currently testing)
   - **Linux**: DMA-BUF via Vulkan external memory
   - **Windows**: D3D11 shared textures via Vulkan interop

### Communication

The main terminal process and browser subprocesses communicate via:

- Unix domain sockets for commands (navigate, go back, reload, etc.)
- Platform-native texture handles for zero-copy sharing (IOSurface, DMA-BUF,
  D3D11)

### Profile Isolation

Each profile has:

- Its own CEF `root_cache_path` (cookies, local storage, cache)
- Its own browser subprocess
- Complete isolation from other profiles

Users can:

- Have multiple tabs/panes open in the same profile (shared session)
- Have tabs/panes in different profiles (isolated sessions)
- Log into the same site with different accounts in different profiles

## Components

### Main Terminal Process (WezTerm-based)

- Window management and compositing
- Terminal emulation
- Receives textures from browser subprocesses
- Routes input events to appropriate subprocess

### Web Command Coordinator (`termsurf web`)

- CLI entry point for browser operations
- Checks if a browser subprocess for the requested profile is running
- Spawns new browser subprocess if needed, or connects to existing one
- Forwards commands (open URL, navigate, reload, etc.) to the subprocess

### Browser Subprocess

- Long-lived process, one per profile
- Initializes CEF with profile-specific cache path
- Manages one or more browser instances (panes/tabs)
- Renders to off-screen shared textures
- Handles browser-specific input (when pane is focused)
- Streams console output back to terminal (optional)

### CEF Helper Processes

- Managed internally by CEF
- GPU process, renderer processes, etc.
- No direct interaction with TermSurf code

## Validated Technology (from ts2/cef-rs)

The following has been validated and is ready for ts3:

| Component                | Status     | Notes                        |
| ------------------------ | ---------- | ---------------------------- |
| IOSurface texture import | Working    | Zero-copy texture sharing    |
| Keyboard input           | Working    | All key events handled       |
| Mouse input              | Working    | Click, move, scroll          |
| Multiple browsers        | Working    | Per-instance texture routing |
| Browser resize           | Working    | Dynamic resize support       |
| Context menu             | Suppressed | Prevents windowing conflicts |

## Open Questions

1. **Browser subprocess binary**: Is the browser subprocess a separate binary,
   or a mode of the main `termsurf` binary (e.g.,
   `termsurf browser-subprocess`)?
2. **Subprocess discovery**: How does the `web` command find existing browser
   subprocesses? PID files? Unix socket naming convention?
3. **Pane creation flow**: How does the main process signal a browser subprocess
   to create a new pane?
4. **Texture handle passing**: How are texture handles passed from subprocess to
   main process?
5. **Focus management**: How does the main process know which browser pane has
   focus?
6. **Subprocess lifecycle**: When does a browser subprocess exit? When all its
   panes close?

## Future Considerations

- **Linux/Windows testing**: cef-rs has cross-platform texture sharing support,
  but we are only testing on macOS for now.
- **Profile management UI**: How users create, switch, and manage profiles.
- **DevTools**: Exposing Chrome DevTools for browser panes.

## Experiments

### Experiment 1: Profile Loading

**Status:** SUCCESS

**Goal:** Validate that the `web` CLI can load CEF with different profile
directories, confirming our core architecture is sound.

**Hypothesis:** CEF will initialize successfully with profile-specific cache
paths, and we can run multiple browser subprocesses with different profiles
simultaneously.

**Test cases:**

1. `web` - loads default profile (`~/.config/termsurf/cef/default/`)
2. `web --profile work` - loads work profile (`~/.config/termsurf/cef/work/`)
3. `web --incognito` - loads with no persistent profile
4. Run two instances with different profiles simultaneously
5. Verify CEF rejects two processes opening the same profile

**Implementation:**

- Add `--profile <name>` flag (default: "default")
- Add `--incognito` flag (mutually exclusive with `--profile`)
- Validate profile name: lowercase alphanumeric, must start with letter
- Pass profile to subprocess via `--browser-subprocess --profile <name>`
- Set CEF `root_cache_path` to `~/.config/termsurf/cef/<profile>/`
- For incognito, use empty cache path or temp directory

**Success criteria:**

- [x] `web` prints "loaded CEF" with profile=default
- [x] `web --profile work` prints "loaded CEF" with profile=work
- [x] `web --incognito` prints "loaded CEF" with no profile
- [x] Two processes with different profiles run simultaneously
- [x] Two processes with the same profile: second fails gracefully

**Results:** SUCCESS (2026-01-24)

All test cases passed. Key findings:

1. **Profile isolation works**: Each profile gets its own directory under
   `~/.config/termsurf/cef/<profile>/` with separate cookies, storage, etc.

2. **CEF enforces single-process-per-profile**: CEF automatically creates a
   `SingletonLock` file in the profile directory. If a second process tries to
   open the same profile, CEF fails with: "Failed to create SingletonLock: File
   exists" and "Aborting now to avoid profile corruption."

3. **Incognito works**: Empty `root_cache_path` triggers CEF's in-memory storage
   mode (with a warning about singleton behavior, which is expected).

4. **Validation works**: Profile names must be lowercase alphanumeric starting
   with a letter. `--profile` and `--incognito` are mutually exclusive.

This validates our core architecture: one CEF process per profile with automatic
conflict detection.

### Experiment 2: Socket Communication (Ping/Pong)

**Status:** SUCCESS

**Goal:** Validate Unix domain socket communication between the coordinator and
browser subprocess, establishing the foundation for command passing and
eventually texture handle sharing.

**Background:** Both ts1 and ts2 use Unix domain sockets with newline-delimited
JSON for IPC. This pattern is proven and we'll adopt it for ts3.

**Hypothesis:** The browser subprocess can create a socket, listen for
connections, and respond to ping requests from the coordinator.

**Socket naming convention:**

```
~/.config/termsurf/sockets/{profile}.sock
```

Examples:

- `~/.config/termsurf/sockets/default.sock`
- `~/.config/termsurf/sockets/work.sock`
- Incognito: `~/.config/termsurf/sockets/incognito-{uuid}.sock` (unique per
  instance)

**Protocol format:** Newline-delimited JSON (same as ts1/ts2)

Request:

```json
{"id": "uuid", "action": "ping"}
```

Response:

```json
{"id": "uuid", "status": "ok", "data": {"pong": true}}
```

**Test flow:**

1. Coordinator spawns browser subprocess with `--profile test`
2. Browser subprocess:
   - Loads CEF (as in Experiment 1)
   - Creates socket at `~/.config/termsurf/sockets/test.sock`
   - Listens for connections
3. Coordinator:
   - Waits briefly for socket to appear
   - Connects to socket
   - Sends ping request
   - Receives pong response
   - Prints success
4. Both processes exit cleanly

**Implementation:**

- Add socket server to browser subprocess (listen after CEF init)
- Add socket client to coordinator (connect after spawning subprocess)
- Define protocol types: `Request`, `Response`
- Implement ping/pong handler
- Clean up socket file on exit

**Success criteria:**

- [x] Browser subprocess creates socket at expected path
- [x] Coordinator connects to socket successfully
- [x] Ping request receives pong response
- [x] Socket file cleaned up on subprocess exit
- [x] Error handling for socket already exists (stale socket)

**Future extensions (not part of this experiment):**

- `open` command to create browser instances
- `navigate` command to change URLs
- Event streaming for console output
- Texture handle passing for rendering

**Results:** SUCCESS (2026-01-24)

All test cases passed. Key findings:

1. **Socket creation works**: Browser subprocess creates socket at
   `~/.config/termsurf/sockets/{profile}.sock` immediately after CEF
   initialization.

2. **Communication works**: Newline-delimited JSON protocol enables
   bidirectional request/response communication. Ping request:
   `{"id":"uuid","action":"ping"}` receives response:
   `{"id":"uuid","status":"ok","data":{"pong":true}}`.

3. **Incognito sockets work**: Incognito mode uses unique UUID-based socket
   paths (`incognito-{uuid}.sock`) to enable multiple concurrent incognito
   sessions.

4. **Cleanup works**: Socket files are properly removed when the subprocess
   exits, preventing stale socket accumulation.

5. **Stale socket handling**: If a stale socket exists from a crashed process,
   it is removed before binding to prevent "address already in use" errors.

This validates our IPC architecture. The coordinator can spawn and communicate
with browser subprocesses, establishing the foundation for browser management
commands and eventually texture handle passing.

### Experiment 3: Multi-Pane Subprocess Sharing

**Status:** SUCCESS

**Goal:** Validate that multiple `web` invocations with the same profile share
one subprocess, while different profiles get separate subprocesses. This is
critical for the multi-pane architecture where browser panes in the same profile
should share cookies, sessions, and state.

**Background:** Experiment 2 validated basic socket communication, but each
`web` invocation spawned a new subprocess and blocked waiting for it. For real
use, we need:

1. Subprocess reuse: Multiple panes with the same profile share one subprocess
2. Long-lived subprocesses: Stay alive across client connections
3. Non-blocking coordinators: Don't wait for subprocess to exit

**Test cases:**

1. Run `web --profile=test` in terminal 1 → spawns new subprocess
2. Run `web --profile=test` in terminal 2 → connects to existing subprocess (no
   new spawn)
3. Run `web --profile=other` in terminal 3 → spawns different subprocess
4. Close terminal 1 → subprocess stays alive (terminal 2 still connected)
5. Close terminal 2 → subprocess exits (no more browsers open)

**Implementation changes:**

1. **Subprocess (long-lived)**:

   - Accept multiple connections concurrently
   - Run CEF message pump in main thread
   - Handle socket connections in separate thread
   - Track open browser instances with reference counting
   - Exit when browser count reaches zero

2. **Coordinator (non-blocking)**:

   - Check if socket exists and is connectable before spawning
   - If connected, reuse existing subprocess
   - If not, spawn new subprocess and wait for socket
   - Send `open_browser` command, receive `browser_id`
   - Don't block waiting for subprocess to exit

3. **Protocol extensions**:
   ```json
   {"id": "uuid", "action": "open_browser", "data": {"url": "https://..."}}
   {"id": "uuid", "status": "ok", "data": {"browser_id": 1}}

   {"id": "uuid", "action": "close_browser", "data": {"browser_id": 1}}
   {"id": "uuid", "status": "ok"}
   ```

**Test flow:**

```
# Terminal 1
$ web --profile=test
Connecting to existing subprocess... not found
Spawning subprocess for profile=test
Waiting for socket...
Connected to subprocess (pid=12345)
Opening browser... browser_id=1
[stays connected, can send more commands]

# Terminal 2 (while terminal 1 is still running)
$ web --profile=test
Connecting to existing subprocess... connected! (pid=12345)
Opening browser... browser_id=2
[shares subprocess with terminal 1]

# Verify with ps
$ ps aux | grep "web --browser-subprocess"
ryan  12345  web --browser-subprocess --profile test
# Only ONE subprocess for profile=test
```

**Success criteria:**

- [x] Second `web --profile=test` reuses existing subprocess (no "Spawning"
      message, no CEF init)
- [x] Different profile (`--profile=other`) spawns separate subprocess
- [x] Subprocess survives when first client disconnects
- [x] Subprocess exits when last browser is closed
- [x] `ps aux | grep web` shows expected number of processes (one per active
      profile)
- [x] Stale socket from crashed subprocess is detected and cleaned up

**Design decisions:**

1. **Subprocess spawning**: The coordinator spawns the subprocess with
   `Command::new().spawn()` and does not wait for it. When the coordinator
   exits, the subprocess becomes an orphan (reparented to launchd/init) and
   continues running independently. No daemonization needed.

2. **Browser lifecycle**: The browser and `web` CLI are tied together. The flow
   is: user runs `web open google.com` → browser opens → console output streams
   to terminal → user closes browser → `web` CLI exits. The subprocess exits
   when its last browser closes.

3. **Connection detection**: Rely on socket EOF detection when clients
   disconnect. No heartbeat mechanism needed.

**Results:** SUCCESS (2026-01-24)

All test cases passed. Key implementation details:

1. **Threaded connection handling**: The subprocess uses a non-blocking listener
   to accept connections, spawning a new thread for each client. Each thread
   handles requests for that connection until the client disconnects.

2. **Shared browser state**: An `Arc<BrowserState>` tracks browser count and
   next browser ID across all connection threads. Atomic operations ensure
   thread-safe updates.

3. **Shutdown coordination**: When `close_browser` decrements the count to zero,
   it returns `was_last=true`. The connection handler sets a shared shutdown
   flag, causing the accept loop to exit. The subprocess then waits for all
   connection threads to finish before cleaning up.

4. **Socket-based discovery**: The coordinator first attempts to connect to the
   socket. If successful, it reuses the existing subprocess. If the socket
   exists but connection fails (stale socket from crash), it removes the socket
   and spawns a new subprocess.

5. **Background spawning**: The coordinator spawns the subprocess without
   waiting for it (`Stdio::null()` for all handles). The subprocess runs
   independently and outlives the spawning coordinator.

This validates the core multi-pane architecture. Multiple browser panes using
the same profile share one subprocess (and thus one CEF context with shared
cookies/storage), while different profiles are fully isolated in separate
processes.

### Experiment 4: Connection-Based Webview Lifecycle

**Status:** SUCCESS

**Goal:** Simplify the protocol and webview lifecycle:

1. Rename `open_browser` to `open` (less confusing)
2. Remove `close_browser` command (connection close = webview close)
3. Handle coordinator crashes gracefully

**Background:** Experiment 3 uses `open_browser` and `close_browser` commands.
This has two problems:

1. The name `open_browser` is confusing - we're opening a webview, not a browser
2. If a coordinator crashes without sending `close_browser`, the subprocess
   never exits (orphaned subprocess)

**Terminology:**

- `web` - the coordinator CLI
- `subprocess` - the CEF process that manages webviews
- `webview` - an individual web content view owned by a connection

**Problem with current approach:**

```
$ web https://example.com --profile test &
# Sends open_browser, gets browser_id=1

$ kill -9 $!   # Coordinator killed without cleanup
# Connection EOF detected
# But subprocess waits forever for close_browser that never comes
```

**Proposed solution:**

1. Rename `open_browser` → `open` (we're opening a webview in the subprocess)
2. Remove `close_browser` entirely - tie webview lifetime to connection lifetime
3. When connection ends (for any reason), the webview is automatically closed

**Key insight:** The OS guarantees that when a process dies (clean exit, crash,
SIGKILL), all its file descriptors are closed. The subprocess sees EOF on the
socket. We use this as the signal to clean up.

**Protocol changes:**

Before (Experiment 3):

```json
{"id": "uuid", "action": "open_browser", "data": {"url": "https://..."}}
{"id": "uuid", "status": "ok", "data": {"browser_id": 1}}

{"id": "uuid", "action": "close_browser", "data": {"browser_id": 1}}
{"id": "uuid", "status": "ok"}
```

After (Experiment 4):

```json
{"id": "uuid", "action": "open", "data": {"url": "https://..."}}
{"id": "uuid", "status": "ok"}
```

No ID needed - the subprocess tracks which connection owns which webview
internally. No close command needed - disconnecting closes the webview.

**Implementation changes:**

1. **Rename command**: `open_browser` → `open`

2. **Track ownership internally**: Subprocess associates each webview with its
   connection. No need to expose IDs to coordinator.

3. **Cleanup on disconnect**: When the connection handler's read loop exits (EOF
   or error), close all webviews owned by that connection.

4. **Remove close_browser**: The explicit command is no longer needed.

**Lifecycle flow:**

```
web (coordinator)                    subprocess
    |                                    |
    |-- connect ------------------------>| (new connection thread)
    |-- open {"url": "..."} ------------>| create webview, track owner
    |<- ok ------------------------------|
    |                                    |
    |   [webview open, events stream]    |
    |                                    |
    |-- disconnect (EOF) --------------->| close owned webviews
    |   (clean exit, crash, or SIGKILL)  | if no webviews left: shutdown
```

**Test cases:**

1. `web` opens webview, then exits cleanly → webview closed, subprocess exits
2. `web` opens webview, then is killed with SIGTERM → webview closed, subprocess
   exits
3. `web` opens webview, then is killed with SIGKILL → webview closed, subprocess
   exits
4. Two `web` instances connect, first exits → first webview closed, subprocess
   stays alive
5. Two `web` instances connect, both exit → both webviews closed, subprocess
   exits

**Success criteria:**

- [x] Clean `web` exit closes webview automatically
- [x] SIGTERM'd `web` closes webview automatically
- [x] SIGKILL'd `web` closes webview automatically
- [x] No orphaned subprocesses after `web` death
- [x] `open_browser` renamed to `open`
- [x] `close_browser` command removed from protocol
- [x] Multiple `web` instances can coexist, each with their own webview

**Benefits:**

1. **Clearer naming**: `open` is less confusing than `open_browser`
2. **Crash-proof**: No way to leak webviews - connection death always triggers
   cleanup
3. **Simpler protocol**: Fewer commands, no IDs to track
4. **Uniform behavior**: Same cleanup path for clean exit and crash

**Results:** SUCCESS (2026-01-24)

All test cases passed. Key findings:

1. **EOF detection works**: When a coordinator disconnects (for any reason), the
   subprocess detects EOF on the socket read and triggers cleanup. This works
   for clean exits, SIGTERM, and even SIGKILL.

2. **Ownership tracking works**: Each connection thread tracks its owned
   webviews in a local `Vec<u64>`. On disconnect, it iterates and closes each
   webview. This ensures no leaks regardless of how the coordinator exits.

3. **Subprocess lifecycle correct**: The subprocess exits only when the last
   webview is closed. Multiple coordinators can connect and disconnect
   independently; the subprocess stays alive as long as any webview remains.

4. **Simplified protocol**: The `open` command (renamed from `open_browser`)
   creates a webview. There is no `close_browser` command - disconnecting
   automatically closes owned webviews. The response no longer includes a
   `webview_id` since the subprocess tracks ownership internally.

5. **No orphaned subprocesses**: Even with SIGKILL (which cannot be caught), the
   OS closes the socket, the subprocess detects EOF, and cleanup proceeds
   normally. There is no way to leak a subprocess.

This validates our connection-based lifecycle model. The architecture is now
crash-proof and simpler to use.

### Experiment 5: Rename "Subprocess" to "Profile Server"

**Status:** SUCCESS

**Goal:** Improve terminology by renaming "subprocess" to "profile server"
throughout the codebase. The CEF process is specifically attached to a profile,
so "profile server" more accurately describes its role.

**Background:** The current code uses "subprocess" to refer to the CEF process
that manages webviews for a profile. This is a generic implementation term that
doesn't convey meaning. "Profile server" is more accurate - it's a server that
handles webview requests for a specific profile.

**Terminology:**

- `web` - the coordinator CLI (unchanged)
- `profile server` - the CEF process that serves webviews for a profile
  (previously "subprocess")
- `webview` - an individual web content view (unchanged)

**Changes:**

1. **CLI flag**: `--browser-subprocess` → `--profile-server`

2. **Variable names**: `is_subprocess` → `is_profile_server`

3. **Function names**:
   - `run_subprocess()` → `run_profile_server()`
   - `spawn_subprocess()` → `spawn_profile_server()`

4. **Log prefix**: `[Subprocess]` → `[Profile]`

5. **User-facing messages**:
   - "Connected to existing subprocess" → "Connected to existing profile server"
   - "Spawning new subprocess" → "Starting profile server"
   - "Connected to subprocess" → "Connected to profile server"
   - "Subprocess status" → "Profile server status"
   - "Failed to spawn subprocess" → "Failed to start profile server"

6. **Comments**: Update references to "subprocess" → "profile server"

**Not changing:**

- `browser_subprocess_path` in CEF settings - this is a CEF API field name

**Success criteria:**

- [x] `--browser-subprocess` renamed to `--profile-server`
- [x] All function/variable names updated
- [x] Log prefix changed to `[Profile]`
- [x] User-facing messages updated
- [x] Code compiles and tests pass
- [x] Existing functionality unchanged

**Results:** SUCCESS (2026-01-24)

All changes applied and tested. The terminology is now consistent:

- CLI flag: `--profile-server` (was `--browser-subprocess`)
- Functions: `run_profile_server()`, `spawn_profile_server()`
- Variable: `is_profile_server`
- Log prefix: `[Profile]`
- Messages: "Connected to existing profile server", "Starting profile server",
  "Profile server status"

The rename clarifies that this process is the CEF context for a specific
profile, not just a generic subprocess.

### Experiment 6: Display Webview in GUI

**Status:** FAILURE

**Goal:** Render a webpage in the profile server and display it in the GUI
(wezterm-gui), with the coordinator acting as intermediary. This is the first
end-to-end test of the full architecture.

**Background:** Experiments 1-5 established:

- Profile server can initialize CEF and manage webviews
- Coordinator can communicate with profile server via socket
- Webview lifecycle is tied to coordinator connection

Now we need to connect the GUI. The GUI (wezterm-gui) is the graphical
application that displays terminal panes. We want webviews to appear in the same
space as terminal panes, rendered by the profile server and composited by the
GUI.

**Research findings from ts2:**

1. **IPC mechanism**: ts2 uses Unix domain sockets (not WezTerm's built-in IPC,
   which is one-way and can't stream console.log back). Socket path is stored in
   `TERMSURF_SOCKET` env var, set by the GUI for child processes.

2. **Pane identification**: ts2 uses `WEZTERM_PANE` (WezTerm's built-in numeric
   pane ID). ts1 used a custom `TERMSURF_PANE_ID` string. We'll use
   `WEZTERM_PANE` since we're building on WezTerm.

3. **Resize handling**: ts2 uses a "settle-and-rerender" pattern - waits 30ms
   after bounds change before resizing CEF to avoid intermediate resize calls.
   For MVP, we skip this and just stretch the texture.

4. **Focus handling**: ts2 has two modes (Browse/Control). For MVP, we skip
   interactivity entirely.

**Architecture:**

```
┌─────────────┐     socket      ┌─────────────────┐
│ coordinator │ <-------------> │  profile server │
│   (web)     │                 │  (CEF process)  │
└─────────────┘                 └─────────────────┘
       │                                │
       │ socket                         │ IOSurface
       │ (TERMSURF_SOCKET)              │ (shared texture)
       ▼                                ▼
┌─────────────────────────────────────────────────┐
│                      GUI                         │
│                  (wezterm-gui)                   │
│  ┌─────────────┐  ┌─────────────┐               │
│  │ terminal    │  │  webview    │               │
│  │ pane        │  │  (texture)  │               │
│  └─────────────┘  └─────────────┘               │
└─────────────────────────────────────────────────┘
```

**Key insight:** The coordinator is the intermediary for control flow, but the
texture memory is shared directly via IOSurface (macOS). The coordinator passes
the texture handle (an integer), not the actual pixels.

**Environment variables (set by GUI):**

- `TERMSURF_SOCKET` - Path to GUI's Unix domain socket (e.g.,
  `/tmp/termsurf-{pid}.sock`)
- `WEZTERM_PANE` - Numeric pane ID (WezTerm built-in)

**Data flow (MVP):**

1. User runs `web google.com` in a terminal pane
2. Coordinator reads `TERMSURF_SOCKET` and `WEZTERM_PANE` from environment
3. Coordinator connects to profile server, sends `open` with URL and pane size
4. Profile server creates CEF browser at requested size, renders to IOSurface
5. Profile server returns IOSurface handle to coordinator
6. Coordinator connects to GUI socket, sends `display_webview` with texture
   handle and pane ID
7. GUI imports IOSurface and composites it over the terminal pane
8. User sees google.com rendered in the pane
9. User presses ctrl+c, coordinator disconnects, GUI hides webview

**Protocol: Coordinator <-> Profile Server**

Request (coordinator sends):

```json
{
  "id": "uuid",
  "action": "open",
  "data": {"url": "https://google.com", "width": 800, "height": 600}
}
```

Response (profile server returns):

```json
{
  "id": "uuid",
  "status": "ok",
  "data": {"texture_handle": 12345, "width": 800, "height": 600}
}
```

**Protocol: Coordinator <-> GUI**

Same socket protocol as ts2 (newline-delimited JSON):

```json
{"id": "uuid", "action": "display_webview", "pane_id": 42, "data": {"texture_handle": 12345, "width": 800, "height": 600}}
{"id": "uuid", "status": "ok"}
```

```json
{"id": "uuid", "action": "hide_webview", "pane_id": 42}
{"id": "uuid", "status": "ok"}
```

**Implementation steps:**

1. **GUI: Create socket server**

   - Create Unix socket at `/tmp/termsurf-{pid}.sock`
   - Set `TERMSURF_SOCKET` env var for child processes
   - Handle `display_webview` and `hide_webview` actions
   - Port relevant code from `ts2/wezterm-gui/src/termsurf_socket/`

2. **Profile server: Render to IOSurface**

   - Use cef-rs OSR to create browser at requested size
   - Render to IOSurface (already validated in cef-rs examples)
   - Return IOSurfaceID in `open` response

3. **Coordinator: Bridge profile server and GUI**

   - Read `TERMSURF_SOCKET` and `WEZTERM_PANE` from environment
   - Connect to profile server, send `open` with URL and pane dimensions
   - Receive texture handle from profile server
   - Connect to GUI socket, send `display_webview`
   - Wait for ctrl+c, then send `hide_webview` and exit

4. **GUI: Import and display texture**
   - Receive `display_webview` with texture handle and pane ID
   - Import IOSurface using handle
   - Composite texture over terminal pane content
   - On `hide_webview`, remove texture overlay

**Color profile handling (critical):**

ts2 encountered washed-out colors due to double gamma correction. The fix:

- **Problem:** Chromium renders in sRGB (already gamma-corrected). If GPU sees
  texture as linear (`Bgra8Unorm`), it applies gamma correction again.
- **Solution:** Use sRGB texture view formats to tell GPU the data is already
  sRGB-encoded.

Profile server side (cef-rs already handles this):

```rust
// Texture created with sRGB view formats allowed
view_formats: &[wgpu::TextureFormat::Bgra8UnormSrgb]
```

GUI side (must do this when importing):

```rust
// Create texture view with sRGB format
let view = texture.create_view(&wgpu::TextureViewDescriptor {
    format: Some(wgpu::TextureFormat::Bgra8UnormSrgb),
    ..Default::default()
});
```

This prevents double gamma correction and ensures colors match what Chromium
intended. The fix is already in cef-rs (commit `2b3aa8a3d`), but the GUI must
also use sRGB views when importing the IOSurface texture.

**MVP scope (this experiment):**

- Single webview, single pane
- Render once at initial size
- No interactivity (no mouse, no keyboard to webview)
- Stretch texture if pane resizes (no CEF resize)
- ctrl+c to exit
- Correct DPI/color profile handling

**Deferred for later experiments:**

- Input handling (mouse, keyboard)
- Proper resize (tell CEF to resize, not just stretch)
- Console.log streaming
- Multiple webviews
- Control bar / browse mode / control mode
- Focus management

**Success criteria:**

- [x] GUI creates socket and sets `TERMSURF_GUI_SOCKET`
- [x] Profile server renders webpage to IOSurface
- [x] Profile server returns texture handle (IOSurface ID) to coordinator
- [x] Coordinator passes texture handle to GUI
- [ ] GUI imports IOSurface and displays webpage in correct pane - **NOT
      IMPLEMENTED**
- [ ] Webpage is visible and correctly sized - **NOT VISIBLE**
- [ ] Colors are correct (no washed-out appearance from double gamma
      correction) - **UNTESTABLE**
- [x] ctrl+c exits coordinator and hides webview

**Results:** FAILURE (2026-01-24)

**What worked:**

1. **Profile server CEF rendering**: Accelerated OSR works -
   `on_accelerated_paint` fires (not software fallback), IOSurface IDs are
   captured successfully.

2. **Socket communication**: Coordinator connects to both profile server socket
   and GUI socket. Full protocol round-trip works.

3. **GUI socket timing**: Fixed by starting server early in `main.rs` before any
   windows/shells spawn (learned from ts2's `start_server()` pattern).

4. **Single-threaded profile server**: Fixed CEF threading requirement (browser
   creation must happen on main thread) by replacing threaded connection
   handling with a single-threaded poll loop.

5. **Protocol flow**: Complete data flow works - profile server creates browser,
   captures IOSurface ID, coordinator receives it and forwards to GUI socket,
   GUI acknowledges receipt.

**What failed:**

1. **GUI rendering not implemented**: Phase 4 (texture import and rendering in
   `pane.rs`/`draw.rs`) was never completed. The GUI receives the IOSurface ID
   but doesn't import or render it.

2. **IOSurface ID instability**: Discovered that CEF uses multiple buffers
   (double/triple buffering). The IOSurface ID changes every frame (246, 282,
   289, etc.). The ID captured initially becomes stale immediately.

**Fundamental architectural issue:**

Cross-process IOSurface sharing is problematic because CEF cycles through
multiple IOSurface buffers. The IOSurface ID we capture and send to the GUI
becomes stale by the next frame. To make this work, we'd need to continuously
stream updated IOSurface IDs at 60fps, adding significant IPC overhead.

In ts2's in-process architecture, this isn't a problem because the render
handler runs inside wezterm-gui and updates the texture directly in
`on_accelerated_paint` every frame - no IPC needed for texture updates.

**Lessons learned:**

1. **Socket timing matters**: The GUI socket server must start in `main()`
   before any windows or panes spawn, otherwise child processes won't inherit
   the `TERMSURF_GUI_SOCKET` environment variable.

2. **Use PID-based socket paths**: Window-ID-based paths require the window to
   exist first, creating a chicken-and-egg problem. PID-based paths (like ts2
   uses) can be created before any windows exist.

3. **CEF threading requirements**: Browser creation must happen on the thread
   that called `cef_initialize`. The original multi-threaded socket server
   violated this. Single-threaded poll loop solves it.

4. **Don't disable GPU compositing**: The `disable-gpu-compositing` flag forces
   software rendering. Without it, CEF uses accelerated OSR with IOSurface.

5. **Cross-process texture sharing has inherent challenges**: The buffer cycling
   issue is fundamental to how CEF works. ts2's in-process approach sidesteps
   this entirely.

**Files changed:**

- `cef-rs/cef/src/osr_texture_import/iosurface_ipc.rs` - New: IOSurface ID
  lookup functions (`get_iosurface_id`, `lookup_iosurface_by_id`)
- `cef-rs/cef/src/osr_texture_import/iosurface.rs` - Added `from_id()` method
- `ts3/termsurf-web/src/main.rs` - Major rewrite: CEF browser management with
  `on_accelerated_paint`, single-threaded socket server, coordinator bridge
- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` - New: GUI socket server
  with global singleton pattern (matching ts2)
- `ts3/wezterm-gui/src/termwindow/mod.rs` - Added webview_socket module
- `ts3/wezterm-gui/src/main.rs` - Early socket server startup
- `ts3/mux/src/domain.rs` - Propagate `TERMSURF_GUI_SOCKET` to child processes

**Recommendation:**

Consider reverting to ts2's in-process architecture (CEF inside wezterm-gui)
rather than pursuing cross-process texture sharing. The profile isolation
benefit may not justify the complexity and performance overhead of continuously
streaming IOSurface IDs across processes. Alternative approaches to explore:

1. Accept single-profile limitation (ts2 approach)
2. Use software rendering and shared memory (slower but stable IDs)
3. Investigate CEF's `shared_texture_handle` stability guarantees
4. Consider a hybrid: CEF in-process, profile switching via CEF restart

### Experiment 7: Render Static Webview Texture

**Status:** FAILURE

**Goal:** Complete the missing piece from Experiment 6: render a static webview
texture in the GUI by looking up the IOSurface by ID from another process.

**What was implemented:**

1. CEF shader (`cef_shader.wgsl`) - fullscreen triangle for texture rendering
2. WebGpu pipeline (`webgpu.rs`) - `webview_render_pipeline` and bind group
   layout
3. Overlay rendering (`draw.rs`) - `render_webview_overlays()` function
4. Pane detection (`pane.rs`) - skip terminal rendering when overlay active
5. IOSurface IPC (`iosurface_ipc.rs`) - `retain_iosurface()` /
   `release_iosurface()`
6. Default frontend changed to WebGpu (`frontend.rs`)

**What worked:**

- Socket communication - GUI receives overlay requests with correct IOSurface ID
  ✓
- Pane detection - `paint_pane()` correctly skips terminal rendering ✓
- WebGpu backend - now default (was OpenGL before) ✓
- CEF rendering - profile server captures IOSurface IDs ✓
- Render infrastructure - shader, pipeline, bind group all compile ✓

**What failed:**

- **Cross-process IOSurface sharing** - `IOSurfaceLookup()` consistently returns
  NULL. The GUI process cannot access the IOSurface created by the profile
  server's CEF process.

**Attempts to fix:**

1. Tried `IOSurfaceIncrementUseCount` / `IOSurfaceDecrementUseCount` - no effect
2. Tried `CFRetain` / `CFRelease` - no effect
3. The IOSurface lookup continues to fail regardless of retention strategy

**Root cause:**

In **ts2**, CEF runs **in the same process** as wezterm-gui. The IOSurface
handle from `on_accelerated_paint` is directly usable - no cross-process sharing
needed.

In **ts3**, CEF runs in a **separate profile server process**. We assumed
IOSurface's global ID system would allow cross-process lookup via
`IOSurfaceLookup()`, but this doesn't work. Possible reasons:

1. CEF's IOSurfaces may not be registered in the global namespace
2. CEF's GPU subprocess (which creates the IOSurfaces) may not share them
   globally
3. Sandbox/entitlement restrictions may prevent cross-process access
4. IOSurfaces created by Chromium's GPU process may be process-local

**Key insight:**

The ts3 architecture fundamentally differs from ts2. The assumption that
IOSurface cross-process sharing would "just work" was incorrect. IOSurface IDs
are valid within CEF's process but not accessible from wezterm-gui.

**Success criteria results:**

- [x] Socket communication works
- [x] Pane detection works
- [x] Render pipeline compiles
- [ ] **IOSurface imported successfully by ID in GUI process** ← FAILED
- [ ] Texture rendered at correct pane position
- [ ] Webpage content visible

**Possible paths forward:**

1. **Move CEF back into wezterm-gui process** (like ts2) - eliminates
   cross-process sharing entirely, but loses profile isolation benefits
2. **Copy pixels via shared memory** - profile server copies pixel data to
   shared memory, GUI reads it; slower but reliable
3. **Investigate Chromium's IOSurface creation** - see if there's a way to make
   CEF's IOSurfaces globally shareable
4. **Use Mach ports for IOSurface transfer** - macOS IPC mechanism for passing
   IOSurface references between processes

**Lessons learned:**

1. IOSurface cross-process sharing via global IDs is not universal - depends on
   how the IOSurface was created
2. CEF's multi-process architecture (browser process, GPU process, renderer
   processes) adds complexity to texture sharing
3. The ts2 approach of running CEF in-process avoids these issues entirely

### Experiment 8: Clean Up Failed Cross-Process Code

**Status:** SUCCESS

**Goal:** Remove the broken cross-process IOSurface rendering code from
Experiment 7 while preserving working infrastructure. This creates a clean
foundation for future experiments.

**Background:** Experiment 7 attempted cross-process IOSurface sharing but
failed because `IOSurfaceLookup()` returns NULL for IOSurfaces created by CEF's
GPU process. The codebase now has error-producing code that clutters logs and
makes the app appear broken.

**Approach:** Keep the working parts (socket servers, communication protocol,
overlay state tracking) and remove the broken parts (IOSurface lookup, texture
import, rendering).

**What to KEEP:**

1. **GUI socket server** (`wezterm-gui/src/termwindow/webview_socket.rs`)
   - Server creation and `TERMSURF_GUI_SOCKET` env var ✓
   - Connection handling ✓
   - Message parsing (`display_webview`, `close_webview`) ✓
   - `WebviewOverlayState` struct ✓
   - This infrastructure works and will be useful for future experiments

2. **Profile server socket communication** (`termsurf-web/src/main.rs`)
   - Socket server for profile server ✓
   - `open` command handling ✓
   - CEF browser creation ✓
   - The `get_status` and `ping` commands ✓

3. **Coordinator bridge** (`termsurf-web/src/main.rs`)
   - Connection to profile server ✓
   - Connection to GUI socket ✓
   - Message forwarding ✓

4. **Environment variable propagation** (`mux/src/domain.rs`)
   - `TERMSURF_GUI_SOCKET` passed to child processes ✓

**What to REMOVE:**

1. **GUI texture rendering** (`wezterm-gui/src/termwindow/render/draw.rs`)
   - Remove `render_webview_overlays()` function
   - Remove IOSurface lookup calls (`lookup_iosurface_by_id`)
   - Remove texture import code
   - Remove bind group creation

2. **Webview render pipeline** (`wezterm-gui/src/termwindow/webgpu.rs`)
   - Remove `webview_render_pipeline`
   - Remove `webview_bind_group_layout`
   - Remove shader module loading

3. **CEF shader** (`wezterm-gui/src/cef_shader.wgsl`)
   - Delete this file entirely

4. **Pane rendering skip** (`wezterm-gui/src/termwindow/render/pane.rs`)
   - Remove the overlay detection that skips terminal rendering
   - Terminal should render normally even when overlay is registered

5. **IOSurface retention** (`termsurf-web/src/main.rs`)
   - Remove `retain_iosurface()` calls in `BrowserState`
   - Remove `retained_handle` field
   - Keep `iosurface_id` field (for logging/debugging, but don't rely on it)

6. **IOSurface IPC retain/release**
   (`cef-rs/cef/src/osr_texture_import/iosurface_ipc.rs`)
   - Remove `retain_iosurface()` function
   - Remove `release_iosurface()` function
   - Keep `get_iosurface_id()` and `lookup_iosurface_by_id()` (may be useful
     later)

7. **Frontend default change** (`config/src/frontend.rs`)
   - Revert default from `WebGpu` back to `OpenGL`
   - This was changed for Experiment 7 but isn't needed without texture
     rendering

**What to SIMPLIFY:**

1. **Overlay state behavior**: When a `display_webview` message is received,
   still register the overlay in `WebviewOverlayState`, but don't attempt to
   render anything. This preserves the protocol for future use.

2. **Profile server response**: Still return `iosurface_id` in the `open`
   response for debugging visibility, but document that it's informational only.

**Implementation order:**

1. Remove texture rendering code from `draw.rs`
2. Remove pipeline/shader from `webgpu.rs`
3. Delete `cef_shader.wgsl`
4. Remove pane skip logic from `pane.rs`
5. Remove IOSurface retention from `termsurf-web/src/main.rs`
6. Remove retain/release from `iosurface_ipc.rs`
7. Revert frontend default to OpenGL
8. Test that `web` command no longer produces errors
9. Verify terminal still renders normally

**Success criteria:**

- [x] No `IOSurface NOT FOUND` errors in logs
- [x] No `[Webview]` error messages in GUI logs
- [x] Terminal renders normally when `web` command is running
- [x] Socket communication still works (GUI acknowledges `display_webview`)
- [x] Clean build (minor warnings about unused socket server functions - expected)
- [x] Profile server still captures IOSurface IDs (visible in logs, for
      debugging)

**Files to modify:**

| File                                                 | Action                                                   |
| ---------------------------------------------------- | -------------------------------------------------------- |
| `wezterm-gui/src/termwindow/render/draw.rs`          | Remove `render_webview_overlays()` and IOSurface imports |
| `wezterm-gui/src/termwindow/webgpu.rs`               | Remove webview pipeline and bind group layout            |
| `wezterm-gui/src/cef_shader.wgsl`                    | Delete                                                   |
| `wezterm-gui/src/termwindow/render/pane.rs`          | Remove overlay detection code                            |
| `termsurf-web/src/main.rs`                           | Remove IOSurface retention code                          |
| `cef-rs/cef/src/osr_texture_import/iosurface_ipc.rs` | Remove retain/release functions                          |
| `config/src/frontend.rs`                             | Revert default to OpenGL                                 |

**Expected outcome:**

A clean codebase where:

- The `web` command runs without errors
- Socket infrastructure is preserved for future experiments
- Terminal functionality is unaffected
- Logs show successful socket communication without texture-related errors

**Results:** SUCCESS (2026-01-24)

All broken cross-process texture sharing code has been removed while preserving
working infrastructure:

1. **Removed from GUI:**
   - `render_webview_overlays()` function and all IOSurface lookup code
   - Webview render pipeline and bind group layout from `webgpu.rs`
   - `cef_shader.wgsl` file (deleted)
   - Overlay detection in `pane.rs` that skipped terminal rendering

2. **Removed from profile server:**
   - IOSurface retention code (`retained_handle`, `set_iosurface`, `release`)
   - The `BrowserState` struct now only tracks IOSurface info for
     logging/debugging

3. **Removed from cef-rs:**
   - `retain_iosurface()` and `release_iosurface()` functions from
     `iosurface_ipc.rs`
   - Updated module documentation to note cross-process sharing limitation

4. **Reverted frontend default:** Changed from `WebGpu` back to `OpenGL`

5. **Preserved working infrastructure:**
   - GUI socket server and `WebviewOverlayState` (for future use)
   - Profile server socket communication and CEF browser creation
   - Coordinator bridge connecting both servers
   - Environment variable propagation (`TERMSURF_GUI_SOCKET`)

The codebase now compiles cleanly with only minor warnings about unused socket
server functions (expected, as these are preserved for future experiments).

**Future direction:**

With this cleanup complete, we can explore alternative approaches:

1. <del>Move CEF back into wezterm-gui (like ts2) for in-process texture
   sharing</del> - Absolutely not.
2. Investigate Mach port-based IOSurface transfer
3. Use shared memory to copy pixel data between processes
4. <del>Accept single-profile limitation and use ts2 architecture</del> -
   Absolutely not.
