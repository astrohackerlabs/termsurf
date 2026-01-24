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
