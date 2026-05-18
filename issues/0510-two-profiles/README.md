+++
status = "closed"
opened = "2026-02-16"
closed = "2026-02-16"
+++

# Issue 510: Two Profiles Side by Side

## Background

Multiple browser profiles in the same window is a core TermSurf product
requirement. Few browsers support this — Chrome doesn't. It took five
generations (ts1–ts5) and hundreds of experiments to develop an architecture
that works. The breakthrough was forking Chromium and using its Content API
directly, where `BrowserContext` natively supports multiple isolated instances
with separate cookies, localStorage, and cache (Issue 406).

Issue 509 proved the full streaming pipeline: Chromium renders a webpage,
streams IOSurface frames at 60fps over XPC, and the Metal renderer composites
them at pixel-perfect Retina resolution inside a terminal pane. The pipeline
handles resize, correct sRGB colors, and clean lifecycle management.

This issue demonstrates the two-profile capability by rendering two different
browser profiles side by side in split panes in the same window. Each profile
gets its own Chromium Profile Server process with its own `--user-data-dir`,
producing fully isolated browser sessions — different cookies, different
localStorage, different cache.

## Product requirements

### Profile naming

A profile name must:

- Consist of lowercase alphanumeric characters only (`a-z`, `0-9`)
- Start with a letter (`a-z`)
- Be non-empty

This is intentionally strict. Profile names are compatible with variable names
in software, filesystem paths, URL slugs, and configuration keys. This gives
maximum flexibility for future use. Examples: `default`, `work`, `personal`,
`guest`, `dev`.

The `web` CLI accepts `--profile <name>` (default: `default`). The profile name
must be validated before use.

### Profile data isolation

Each profile maps to a separate Chromium `--user-data-dir`:

```
~/.config/termsurf/chromium-profiles/<name>/
```

Two panes with `--profile work` share the same server process and browser
session (same cookies, same localStorage). Two panes with different profiles
(`work` vs `personal`) get separate server processes with separate data.

### Display

The `web` TUI already renders the profile name in the URL bar's top-right
corner. This is purely cosmetic — no changes needed to the UI layout itself.

## Current state

### What already works

| Component                  | Status  | Notes                                             |
| -------------------------- | ------- | ------------------------------------------------- |
| `web` CLI `--profile` flag | Working | Parses flag, displays in URL bar                  |
| Profile name in URL bar    | Working | Renders icon + name in top-right                  |
| Chromium `--user-data-dir` | Working | Per-process data isolation                        |
| One server per pane        | Working | Spawns, streams, terminates cleanly               |
| IOSurface streaming        | Working | 60fps, pixel-perfect Retina                       |
| Dynamic resize             | Working | XPC `resize` message, never stretch               |
| xpc-gateway                | Working | Stateless rendezvous, no profile awareness needed |

### What needs to change

**1. `web` must send the profile name over XPC.**

Currently `set_overlay` does not include the profile name. The app has no way to
know which profile the `web` process is using. The profile name must be added to
`set_overlay` so the app can route to the correct server process.

**2. The app must route server processes by profile, not just by pane.**

Currently `CompositorXPC.swift` maps everything by pane UUID. It spawns one
server per pane with a hardcoded profile path (`profiles/default`). Two panes
with the same profile should share one server process (and thus one browser
session). Two panes with different profiles must get different server processes.

The server process mapping needs to change from `[UUID: Process]` to something
that groups panes by profile. When a second pane requests `--profile work` and a
server for `work` is already running, the app should reuse that server and send
a second `create_tab` rather than spawning a new process.

**3. The hardcoded profile path must use the actual profile name.**

`CompositorXPC.swift` line 407 hardcodes `profiles/default`. This must become
`profiles/<name>` using the profile name from the `set_overlay` message.

### What should work without changes

**xpc-gateway** — Pure stateless rendezvous. Returns the app's endpoint to any
process that asks. No profile awareness needed.

**Chromium Profile Server** — Already accepts `--user-data-dir` as a flag. Each
instance is a separate process with a separate data directory. No source changes
should be needed — just pass a different path per profile.

**Metal renderer** — Already composites multiple overlays by pane UUID. Each
pane independently receives IOSurface frames and renders them. No changes
needed.

## XPC protocol changes

### `web` → app: `set_overlay`

Add `profile` field:

```
{ action: "set_overlay",
  pane_id: "<uuid>",
  col: N,
  row: N,
  width: N,
  height: N,
  url: "http://...",
  profile: "work" }          // (new) profile name
```

### app → server: `create_tab`

Unchanged — the app already knows the profile because it spawned the server with
the correct `--user-data-dir`. The tab just needs a URL.

```
{ action: "create_tab",
  url: "http://...",
  tab_id: "<uuid>",
  pixel_width: N,
  pixel_height: N }
```

### app → server: `resize`

Unchanged.

```
{ action: "resize",
  pixel_width: N,
  pixel_height: N }
```

### server → app: `server_register`, `tab_ready`, `display_surface`

Unchanged.

## Architecture note: one server per profile

The current code spawns one server per pane. The two-profile demo needs one
server per profile. This means:

- Pane A (`--profile work`) → spawns server with
  `--user-data-dir=~/.config/termsurf/chromium-profiles/work`
- Pane B (`--profile personal`) → spawns server with
  `--user-data-dir=~/.config/termsurf/chromium-profiles/personal`
- Pane C (`--profile work`) → reuses server from pane A, sends `create_tab`

For the initial demo (two profiles, one pane each), pane-per-server and
profile-per-server are equivalent — each profile has exactly one pane. Server
reuse (pane C scenario) is a future optimization. The demo should still be
designed with this in mind, but it's not required to pass.

## Ideas for future experiments

1. **Profile name validation + XPC propagation.** Add validation to `web`,
   include `profile` in `set_overlay`, and update `CompositorXPC.swift` to
   extract it and use it for the `--user-data-dir` path. Test with a single pane
   using `--profile work` and verify the data goes to
   `~/.config/termsurf/chromium-profiles/work/`.

2. **Two profiles side by side.** Open two split panes, run `web` with different
   `--profile` flags in each, verify two separate server processes spawn with
   separate data directories, and both render independently at 60fps.

3. **Session isolation verification.** Load a page that writes to localStorage
   in both profiles. Verify each profile sees only its own data. Close and
   reopen — verify persistence within each profile and isolation between them.

## Experiment 1: Profile propagation + two profiles side by side

### Goal

Two split panes in the same window, each running `web` with a different
`--profile` flag, render independent Chromium sessions at 60fps. Each profile
gets its own server process with its own data directory. Profile names are
validated before use.

### Changes

Four files, all small.

#### 1. `web/src/main.rs` — Validate profile name

After parsing `--profile`, validate before proceeding. Add after line 45 (after
the `while` loop, before the `url` unwrap):

```rust
// Validate profile name: lowercase alphanumeric, starts with a letter.
if profile.is_empty()
    || !profile.bytes().next().unwrap().is_ascii_lowercase()
    || !profile.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
{
    eprintln!("Error: profile name must be lowercase alphanumeric, starting with a letter");
    std::process::exit(1);
}
```

Pass `&profile` to `send_set_overlay` (line 84–91):

```rust
conn.send_set_overlay(
    pid,
    viewport_rect.x,
    viewport_rect.y,
    viewport_rect.width,
    viewport_rect.height,
    &url,
    &profile,
);
```

#### 2. `web/src/xpc.rs` — Add `profile` to `send_set_overlay`

Update signature (line 169):

```rust
pub fn send_set_overlay(&self, pane_id: &str, col: u16, row: u16, width: u16, height: u16, url: &str, profile: &str) {
```

Add after the `url` lines (after line 196):

```rust
let profile_key = CString::new("profile").unwrap();
let profile_c = CString::new(profile).unwrap();
xpc_dictionary_set_string(dict, profile_key.as_ptr(), profile_c.as_ptr());
```

#### 3. `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — Extract profile, use for data dir

**In `handleSetOverlay`**, extract the profile name from the message. Add after
the URL extraction (after line 194):

```swift
let profilePtr = xpc_dictionary_get_string(msg, "profile")
let profile = profilePtr.map { String(cString: $0) } ?? "default"
```

Pass `profile` to `spawnServer`. Change line 251 from:

```swift
spawnServer(forPane: uuid)
```

to:

```swift
spawnServer(forPane: uuid, profile: profile)
```

**In `spawnServer`**, accept the profile parameter and use it for the data
directory path. Change signature from:

```swift
private func spawnServer(forPane uuid: UUID) {
```

to:

```swift
private func spawnServer(forPane uuid: UUID, profile: String) {
```

Change line 407 from:

```swift
let profilePath = "\(home)/.config/termsurf/profiles/default"
```

to:

```swift
let profilePath = "\(home)/.config/termsurf/chromium-profiles/\(profile)"
```

#### 4. No Chromium changes

The Chromium Profile Server already accepts `--user-data-dir` as a flag. Each
process is fully isolated. No source changes needed.

### XPC message (updated)

**`set_overlay`** — now includes `profile`:

```
{ action: "set_overlay",
  pane_id: "<uuid>",
  col: N,
  row: N,
  width: N,
  height: N,
  url: "http://...",
  profile: "work" }
```

All other messages unchanged.

### Build

```bash
# Build web TUI
cd web && cargo build

# Build TermSurf
cd ts5 && zig build

# No Chromium build needed.
```

### Test

```bash
# Start test page server
cd ts4/box-demo && bun run server.ts &

# Open TermSurf
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log

# In one pane:
cargo run -p web -- http://localhost:9407 --profile work

# Split pane, in the other:
cargo run -p web -- http://localhost:9407 --profile personal
```

### Pass criteria

1. Both panes render the box-demo at 60fps simultaneously.
2. Two separate `Chromium Profile Server` processes are running (one per
   profile).
3. Profile data directories exist at
   `~/.config/termsurf/chromium-profiles/work/` and
   `~/.config/termsurf/chromium-profiles/personal/`.
4. The URL bar shows the correct profile name in each pane.
5. Closing both `web` processes terminates both server processes cleanly.
6. Invalid profile names (e.g., `Work`, `123`, `work!`) are rejected by the
   `web` CLI.

### Result

**Partial.** Two profiles spawned correctly (PID 28886 and PID 28940), both
streaming at ~60fps with separate `--user-data-dir` paths. The URL bar showed
the correct profile name in each pane. After ~4 seconds of dual streaming, the
app crashed:

```
*** Terminating app due to uncaught exception 'NSInvalidArgumentException',
reason: '-[NSTaggedPointerString count]: unrecognized selector sent to
instance 0x8000000000000000'
```

Stack trace shows the crash in `handleDisplaySurface` → Swift Dictionary
subscript setter (`currentSurfaces[uuid] = ioSurface`). Root cause: concurrent
access to shared state from multiple XPC peer connections. See Experiment 2.

## Experiment 2: Serialize peer connections on the XPC queue

### Goal

Fix the concurrency crash from Experiment 1. Two profiles stream at 60fps
without crashing.

### Problem

The anonymous listener is created with a serial queue:

```swift
let queue = DispatchQueue(label: "com.termsurf.compositor.xpc")
let listener = xpc_connection_create(nil, queue)
```

But incoming peer connections never have their target queue set:

```swift
xpc_connection_set_event_handler(peerConn) { ... }
xpc_connection_resume(peerConn)
// No xpc_connection_set_target_queue!
```

In C-level XPC, peer connections do not inherit the listener's queue — they
default to the global concurrent queue. With two servers streaming
`display_surface` at 60fps each (120 messages/second combined), their event
handlers run concurrently on arbitrary threads. Both handlers mutate
`currentSurfaces`, `cachedCSurfaces`, and other shared dictionaries without
synchronization. The crash — `[NSTaggedPointerString count]` on a Dictionary
subscript setter — is a classic symptom of concurrent Swift Dictionary mutation.

With only one server (Issue 509), this never manifested because there was only
one display_surface stream. Two servers exposed the race immediately.

### Changes

One file, one line.

#### 1. `ts5/macos/Sources/Ghostty/CompositorXPC.swift`

Add `xpc_connection_set_target_queue` before `xpc_connection_resume` for each
peer. In the listener's event handler (line ~108), change:

```swift
xpc_connection_resume(peerConn)
```

to:

```swift
xpc_connection_set_target_queue(peerConn, queue)
xpc_connection_resume(peerConn)
```

This requires capturing `queue` in the closure. The `queue` variable is already
a local in `start()` — it just needs to be captured by the listener's event
handler.

This ensures all peer event handlers — web processes, server control
connections, and server tab connections — execute on the same serial queue.
Shared state mutations are automatically serialized. No locks needed.

#### 2. Clean log file

Truncate `logs/overlay.log` before testing so the log only contains this
experiment's output.

```bash
> ~/dev/termsurf/logs/overlay.log
```

### Build

```bash
cd ts5 && zig build
# No web or Chromium changes.
```

### Test

```bash
# Clean logs.
> ~/dev/termsurf/logs/overlay.log

# Start test page.
cd ts4/box-demo && bun run server.ts &

# Launch app.
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log

# In one pane:
cargo run -p web -- http://localhost:9407 --profile work

# Split pane, in the other:
cargo run -p web -- http://localhost:9407 --profile personal

# Let both run for at least 30 seconds.
```

### Pass criteria

1. Both profiles render at 60fps for >30 seconds without crashing.
2. Logs show two separate server PIDs, each streaming independently.
3. Clean exit: closing both `web` processes terminates both servers.
4. No `NSInvalidArgumentException` or other crashes in the log.

### Result

**Pass.** Two profiles streamed at 60fps simultaneously without crashing. Both
server processes spawned with separate data directories and terminated cleanly
on exit.

## Conclusion

### How we got here

This was the fastest issue in the project's history. The architecture built
across Issues 507–509 — XPC gateway, server lifecycle, IOSurface streaming,
pixel-perfect rendering — turned out to be almost entirely reusable. The changes
were minimal: add `profile` to one XPC message, swap a hardcoded path for a
variable, and fix a concurrency bug that only appeared with two simultaneous
streams.

Experiment 1 added profile name validation to the `web` CLI, propagated the
profile name through the XPC `set_overlay` message, and used it to construct
per-profile data directories. Two profiles spawned and streamed correctly, but
crashed after ~4 seconds — a concurrency bug in the XPC event handling.

Experiment 2 fixed the crash with one line: `xpc_connection_set_target_queue`.
In C-level XPC, peer connections default to the global concurrent queue, not the
listener's serial queue. With two servers streaming `display_surface` at 60fps
each, their event handlers raced on shared Swift dictionaries. Pinning all peers
to the same serial queue serialized all mutations. The fix applies retroactively
to Issue 509 — it was always latent, just never triggered with a single server.

### What we accomplished

Two browser profiles rendering side by side in the same terminal window at
60fps, each with fully isolated browser sessions (separate cookies,
localStorage, and cache). This is the core product capability that motivated the
entire TermSurf project.

| Experiment | Goal                               | Result                      |
| ---------- | ---------------------------------- | --------------------------- |
| 1          | Profile propagation + two profiles | Partial (concurrency crash) |
| 2          | Serialize XPC peer connections     | Pass                        |

### What's next

The two-profile demo validates the architecture. Remaining work:

- **Session isolation verification.** Load pages that write to localStorage in
  both profiles and confirm data isolation persists across restarts.
- **Input forwarding.** Keyboard and mouse events to Chromium WebContents —
  without this, webpages are view-only.
- **Server reuse.** Two panes with the same `--profile` should share one server
  process. Currently each pane spawns its own server regardless of profile name.
- **In-process Chromium.** The endgame: embed Chromium directly via the Content
  API instead of streaming over XPC. The streaming architecture is a stepping
  stone that validated every other piece of the pipeline.
