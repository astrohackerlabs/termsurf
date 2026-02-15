# Issue 503: One, Two, Three

## Background: One Process Per Profile

The constraint that two processes cannot share the same profile data directory
has been discovered, re-discovered, and documented across eight issue documents
spanning three generations of TermSurf. This section consolidates those
findings.

### ts2: Discovery (Issues 208, 209)

Issue 208 found that CEF's Chrome runtime (post-M128) deliberately ignores
custom `cache_path` settings. The `root_cache_path` IS the profile — one
process, one profile, no exceptions. Issue 209 confirmed this by attempting to
use Chrome's native profile naming (`Default`, `Profile 1`, etc.) with CEF.
Custom profiles fail silently; only the Default profile works. This is
documented CEF behavior, not a bug.

### ts3: Architecture around the constraint (Issues 301, 305, 306, 307)

Issue 301 ("Lessons from ts2") identified this as the core constraint that
necessitated the entire ts3 architecture: out-of-process CEF, one process per
profile. Issue 305 confirmed the mechanism — CEF uses a `SingletonLock` file in
the profile directory; a second process will crash or fail to initialize. Issue
306 discovered that the ts3 code was violating this constraint by spawning a new
`termsurf-profile` process for every `web` command. Running `web google.com`
then `web github.com` with the same profile would crash the second process on
SingletonLock. The fix: detect an existing profile process and send a "create
browser" command to it. Issue 307 formalized this as "the foundational
architectural constraint of ts3" — exactly one `termsurf-profile` process per
browser profile, with multiple webviews within that process sharing cookies and
storage like tabs in a browser.

### ts4: The CEF vs Chromium distinction (Issues 406, 407)

Issue 406 made the critical discovery: **the one-profile-per-process constraint
is CEF-specific, not a Chromium limitation.** Chromium's Content API
(`content::BrowserContext`) fully supports multiple profiles with different
storage paths in the same process. Chrome itself does this routinely. Electron
proves it via `session.fromPartition()`. CEF adds its own constraints on top of
Chromium. This finding killed CEF and led to ts4.

Issue 407 proved it in practice — the in-process Chromium PoC ran two
`BrowserContext` instances with different storage paths, each with its own
cookies, localStorage, and cache, all in one process at 60fps.

### What this means for Issue 503

Multiple `BrowserContext` instances (different profiles) coexist in one process
— proven in Issue 407. But Issue 503 asks a different question: can multiple
`WebContents` from the **same** `BrowserContext` each have their own
`FrameSinkVideoCapturer` delivering independent IOSurface streams? This is the
multi-tab case. The profile server must host an unlimited number of WebContents
per profile, each captured independently and sent over its own XPC connection.

## Problem

The ts4 proof-of-concept demonstrated two **different** browser profiles
rendering side by side at 60fps (Issue 414, Issue 501). But TermSurf needs to
support a harder case: two tabs from the **same** profile in the same window.

There is exactly **one profile server process per profile** — this is a hard
constraint, not a design preference. Two Chromium processes cannot share the
same user data directory (SingletonLock). This was proven in ts2/ts3 with CEF
and remains true with the Content API. A profile server must therefore handle
**multiple WebContents** within a single process, each with its own
`FrameSinkVideoCapturer` delivering an independent IOSurface stream via XPC.

We've never tested this. The current Chromium Profile Server creates exactly one
`Shell` with one `WebContents` and one `ShellVideoConsumer` in
`InitializeMessageLoopContext()`. It needs to support an unlimited number.

Additionally, the working Swift compositor and box-demo test page live in
`ts4/`, but ts5 is the active development directory. These need to be ported to
`ts5/` so that ts5 has its own self-contained test infrastructure.

## Goals

1. **Port the box-demo and Swift compositor from ts4 to ts5.** The ts5 directory
   should have its own working copies that don't depend on ts4.

2. **Validate one profile, one tab.** A single Chromium Profile Server process
   with one WebContents rendering one page to one pane — the simplest case.

3. **Validate two profiles, one tab each.** Two Chromium Profile Server
   processes, each with a different `--user-data-dir`, each hosting one
   WebContents. Two panes in one window. This is the Issue 414/501 case,
   re-validated with the ts5 test infrastructure.

4. **Validate two profiles, three tabs.** Two Chromium Profile Server processes:
   one hosting **two** WebContents (same profile, two tabs), one hosting **one**
   WebContents (different profile). Three panes in one window. The compositor
   opens two XPC connections to the first profile server and one to the second.

## Architecture

### One profile server per profile

```
Profile Server A (profile-a data dir)
├── WebContents 1 → FrameSinkVideoCapturer → IOSurface → XPC connection 1
├── WebContents 2 → FrameSinkVideoCapturer → IOSurface → XPC connection 2
└── WebContents N → ...

Profile Server B (profile-b data dir)
├── WebContents 1 → FrameSinkVideoCapturer → IOSurface → XPC connection 1
└── WebContents N → ...
```

The compositor (Swift app) connects to profile servers via XPC. Each connection
represents one tab. A profile server can accept an unlimited number of
connections, creating a new WebContents and capturer for each.

### XPC connection model: one connection per tab

Two options were considered:

**Option A: One XPC connection per tab.** The compositor opens a new connection
to the profile server for each tab it wants. The profile server's listener fires
once per connection, creating a WebContents and capturer for each.

**Option B: One shared connection, multiplexed.** The compositor opens a single
connection to the profile server. All messages include a tab identifier. Frames
for all tabs flow over the same pipe.

**Decision: Option A.** Three reasons:

1. **Lifecycle is free.** Closing a connection = closing a tab. The profile
   server sees the connection die and tears down the WebContents + capturer. No
   need for explicit "close tab" messages or lifecycle protocol.

2. **No head-of-line blocking.** Each XPC connection has its own dispatch queue.
   If one tab's IOSurface Mach port transfer is slow, it doesn't delay another
   tab's frame delivery. With a shared connection, all tabs compete for the same
   pipe.

3. **Natural XPC model.** When the compositor creates two connections to the
   same Mach service, the profile server's listener fires twice with two
   separate `xpc_connection_t` peers. Each peer naturally maps to one
   WebContents. The existing single-tab code already handles one connection —
   multiple connections is a generalization, not a new abstraction.

The shared-connection approach would require adding a tab identifier to every
message, demuxing on both sides, and explicit lifecycle commands — all
complexity that XPC's connection model provides for free.

### What changes in the Chromium Profile Server

Currently, the profile server creates one WebContents at startup:

```cpp
void ShellBrowserMainParts::InitializeMessageLoopContext() {
  Shell* shell = Shell::CreateNewWindow(...);
  video_consumer_ = std::make_unique<ShellVideoConsumer>();
  video_consumer_->ObserveContents(shell->web_contents());
}
```

For multi-tab support, the profile server needs to:

1. **Listen for incoming XPC connections** requesting new tabs.
2. **Create a new WebContents** (via `Shell::CreateNewWindow()`) for each
   connection.
3. **Create a new ShellVideoConsumer** per WebContents, each sending frames over
   its own XPC connection back to the compositor.

The existing single-tab startup path can remain as a default, but the server
must also accept "create tab" commands over XPC.

## What needs to be ported

### Box demo (`ts4/box-demo/` -> `ts5/box-demo/`)

The test page: a spinning blue square with a localStorage-based identity string
and FPS counter. Served by a Bun HTTP server on port 9407.

Files:

- `server.ts` — Bun HTTP server (12 lines)
- `public/index.html` — Test page (105 lines)

No changes needed — copy as-is.

### Swift compositor (`ts4/two-profiles-swift/` -> adapted for ts5)

The receiver app: an XPC Mach service that accepts IOSurface Mach ports from
Chromium Profile Server processes and composites them into a Metal window using
CADisplayLink.

Source files:

- `Package.swift` — SwiftPM manifest
- `Sources/Receiver/main.swift` — XPC listener, Metal pipeline, rendering (328
  lines)
- `Sources/Receiver/Shaders.metal` — Vertex + fragment shaders (33 lines)
- `com.termsurf.two-profiles-swift.plist` — Launchd agent definition

The two-pane compositor from ts4 must be adapted into three separate apps for
the three test cases.

## Three Swift apps

### `ts5/one-profile/` — One pane, one profile server

The simplest case. One Chromium Profile Server process with one WebContents
sends IOSurface frames to a Swift compositor that renders a single pane filling
the whole window.

- XPC service name: `com.termsurf.one-profile`
- Window: single pane (800x600)
- Profile servers: 1
- WebContents per server: 1
- Session IDs: `profile-a`

### `ts5/two-profiles/` — Two panes, two profile servers

Two Chromium Profile Server processes, each with a different `--user-data-dir`,
each hosting one WebContents. Two panes side by side. The two panes should show
different localStorage identities, proving profile isolation.

- XPC service name: `com.termsurf.two-profiles`
- Window: two panes side by side (1600x600)
- Profile servers: 2
- WebContents per server: 1
- Session IDs: `profile-a`, `profile-b`
- User data dirs: `~/.config/termsurf/poc/profile-a`,
  `~/.config/termsurf/poc/profile-b`

### `ts5/three-profiles/` — Three panes, two profile servers

The new test case. Two Chromium Profile Server processes: the first hosts
**two** WebContents (same profile, two tabs), the second hosts **one**
WebContents (different profile). Three panes in one window.

The two same-profile panes should show the **same** localStorage identity (they
share a `BrowserContext` with the same storage). The third pane should show a
**different** identity.

- XPC service name: `com.termsurf.three-profiles`
- Window: three panes side by side (2400x600)
- Profile servers: 2
- WebContents per server: 2 (profile-a), 1 (profile-b)
- Session IDs: `profile-a1`, `profile-a2`, `profile-b`
- User data dirs: `~/.config/termsurf/poc/profile-a`,
  `~/.config/termsurf/poc/profile-b`

## Success criteria

All three apps:

1. Build with `swift build` (+ `make` for Metal shaders).
2. Render at ~60fps sustained.
3. No Dock icon for Chromium Profile Server processes.
4. Correct profile isolation (same-profile panes share identity, different
   profiles have different identities).
5. For three-profiles: two panes from the same profile show the same
   localStorage identity, confirming they share the same `BrowserContext`.

## Experiments
