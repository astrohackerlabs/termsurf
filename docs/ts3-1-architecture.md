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

**Results:** SUCCESS (2025-01-24)

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

**Status:** Not Started

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

- [ ] Browser subprocess creates socket at expected path
- [ ] Coordinator connects to socket successfully
- [ ] Ping request receives pong response
- [ ] Socket file cleaned up on subprocess exit
- [ ] Error handling for socket already exists (stale socket)

**Future extensions (not part of this experiment):**

- `open` command to create browser instances
- `navigate` command to change URLs
- Event streaming for console output
- Texture handle passing for rendering

**Results:** (to be filled in after experiment)
