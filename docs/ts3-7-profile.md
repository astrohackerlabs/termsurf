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
