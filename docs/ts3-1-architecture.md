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
