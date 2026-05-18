+++
status = "closed"
opened = "2026-02-15"
closed = "2026-02-15"
+++

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

### Experiment 1: One profile — port box-demo and build one-pane compositor

#### Goal

Port the box-demo test page from ts4 to ts5. Build a one-pane Swift compositor
(`ts5/one-profile/`) that receives IOSurface frames from a single Chromium
Profile Server process and renders them in a Metal window. This establishes the
ts5 test infrastructure and validates the simplest case.

#### Branch

No Chromium changes — this experiment only adds files to the main repo.

#### Changes

##### `ts5/box-demo/` — Copy from ts4

Copy `ts4/box-demo/` to `ts5/box-demo/` as-is:

- `server.ts` — Bun HTTP server on port 9407
- `public/index.html` — Spinning blue square with localStorage identity and FPS

No modifications needed.

##### `ts5/one-profile/` — New Swift app

Adapt `ts4/two-profiles-swift/` into a single-pane compositor:

- `Package.swift` — SwiftPM manifest, target name `OneProfile`
- `Sources/OneProfile/main.swift` — XPC listener, Metal pipeline, rendering
- `Sources/OneProfile/Shaders.metal` — Vertex + fragment shaders (unchanged)
- `com.termsurf.one-profile.plist` — Launchd agent definition
- `Makefile` — Compile Metal shaders + `swift build`

Changes from the ts4 two-profiles-swift source:

1. **One pane, not two.** Remove the left/right pane split. The single pane
   fills the entire window. Remove the `Pane` enum, the `paneForSession()`
   mapping, and the dual-viewport rendering logic.
2. **Window size.** 800x600 (single pane) instead of 1600x600 (two panes).
3. **XPC service name.** `com.termsurf.one-profile`.
4. **Target name.** `OneProfile` instead of `Receiver`.
5. **Log path.** `~/dev/termsurf/logs/one-profile.log`.
6. **Single texture.** One `gCurrentTexture` instead of an array of two.

#### Build and Run

```bash
# 1. Start test page server
cd ts5/box-demo && bun run server.ts &

# 2. Build one-profile compositor
cd ts5/one-profile && make

# 3. Register as launchd service
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts5/one-profile/com.termsurf.one-profile.plist

# 4. Start one Chromium Profile Server
cd chromium/src
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.one-profile \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407
```

#### Pass Criteria

1. Box-demo server runs on port 9407.
2. One-profile compositor builds with `make` (shaders + `swift build`).
3. Compositor window shows the spinning blue square at ~60fps.
4. No Dock icon for the Chromium Profile Server process.
5. localStorage identity string is visible in the rendered page.

#### Result: Pass

Build: `make` compiled Metal shaders and Swift app with zero errors (one warning
about `.metallib` file, fixed by adding it to the exclude list in
`Package.swift`).

Compositor log (receiver side):

```
[OneProfile] 60 frames (59.0 fps) | IOSurface 1600x1200
[OneProfile] 60 frames (60.0 fps) | IOSurface 1600x1200
[OneProfile] 61 frames (60.0 fps) | IOSurface 1600x1200
[OneProfile] 60 frames (59.7 fps) | IOSurface 1600x1200
[OneProfile] 61 frames (60.3 fps) | IOSurface 1600x1200
```

Profile server log (sender side):

```
[ShellVideoConsumer] Attached to FrameSinkId FrameSinkId(5, 3), starting capture
[ShellVideoConsumer] 62 frames in 1.00931s (61.4283 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 60 frames in 1.01486s (59.1215 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01645s (60.0127 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01622s (60.0261 fps) | IOSurface 1600x1200
```

60fps on both sides. No Dock icon. The spinning blue square and localStorage
identity rendered correctly in the single-pane compositor window.

#### Conclusion

The ts5 test infrastructure is established. Box-demo is ported, and
`ts5/one-profile/` is a working single-pane Swift compositor (~240 lines)
adapted from the ts4 two-profiles-swift source. The one-profile baseline
validates the full pipeline: Chromium Profile Server → FrameSinkVideoCapturer →
IOSurface → XPC Mach port → Metal texture → CADisplayLink rendering at 60fps.

### Experiment 2: Two profiles — port the two-pane compositor

#### Goal

Port the ts4 two-profiles-swift compositor to `ts5/two-profiles/`. Two Chromium
Profile Server processes, each with a different `--user-data-dir`, each hosting
one WebContents. Two panes side by side in one window. The two panes should show
different localStorage identities, proving profile isolation.

This is the Issue 414/501 case, re-validated with the ts5 test infrastructure.

#### Branch

No Chromium changes — this experiment only adds files to the main repo.

#### Changes

##### `ts5/two-profiles/` — New Swift app

Port `ts4/two-profiles-swift/` with naming updates:

- `Package.swift` — SwiftPM manifest, target name `TwoProfiles`
- `Sources/TwoProfiles/main.swift` — XPC listener, Metal pipeline, two-pane
  rendering
- `Sources/TwoProfiles/Shaders.metal` — Vertex + fragment shaders (copy from
  one-profile)
- `com.termsurf.two-profiles.plist` — Launchd agent definition
- `Makefile` — Compile Metal shaders + `swift build`

Changes from the ts4 two-profiles-swift source:

1. **XPC service name.** `com.termsurf.two-profiles` instead of
   `com.termsurf.two-profiles-swift`.
2. **Target name.** `TwoProfiles` instead of `Receiver`.
3. **Log prefix.** `[TwoProfiles]` instead of `[Receiver]`.
4. **Log path.** `~/dev/termsurf/logs/two-profiles.log`.
5. **Window title.** `Two Profiles`.
6. **Exclude metallib.** Add `shaders.metallib` to the Package.swift exclude
   list (lesson from Experiment 1).
7. **Binary path in plist.** Points to
   `ts5/two-profiles/.build/debug/TwoProfiles`.

The two-pane rendering logic (left/right viewports, `Pane` enum,
`paneForSession()` mapping) carries over unchanged from ts4.

#### Build and Run

```bash
# 1. Start test page server (if not already running)
cd ts5/box-demo && bun run server.ts &

# 2. Build two-profiles compositor
cd ts5/two-profiles && make

# 3. Register as launchd service
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts5/two-profiles/com.termsurf.two-profiles.plist

# 4. Start two Chromium Profile Servers
cd chromium/src
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 &

out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407
```

#### Pass Criteria

1. Two-profiles compositor builds with `make` (shaders + `swift build`).
2. Compositor window shows two side-by-side panes, each with a spinning blue
   square at ~60fps.
3. The two panes show **different** localStorage identity strings (profile
   isolation).
4. No Dock icon for either Chromium Profile Server process.
5. Both profile servers log ~60fps on the sender side.

#### Result: Pass

Build: `make` compiled Metal shaders and Swift app with zero errors.

Compositor log (receiver side):

```
[TwoProfiles] L: 60 (60.0 fps) R: 60 (60.0 fps) | IOSurface 1600x1200
[TwoProfiles] L: 61 (60.0 fps) R: 60 (59.0 fps) | IOSurface 1600x1200
[TwoProfiles] L: 60 (60.0 fps) R: 61 (61.0 fps) | IOSurface 1600x1200
[TwoProfiles] L: 60 (60.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
[TwoProfiles] L: 60 (59.7 fps) R: 60 (59.7 fps) | IOSurface 1600x1200
```

Profile server A log (sender side):

```
[ShellVideoConsumer] 61 frames in 1.01659s (60.0046 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01691s (59.9855 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01829s (59.9042 fps) | IOSurface 1600x1200
```

Profile server B log (sender side):

```
[ShellVideoConsumer] 61 frames in 1.01659s (60.0044 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 60 frames in 1.00022s (59.9866 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 60 frames in 1.00021s (59.9873 fps) | IOSurface 1600x1200
```

60fps on all three streams (compositor left, compositor right, both senders). No
Dock icons. Two side-by-side panes with different localStorage identities,
confirming profile isolation.

#### Conclusion

`ts5/two-profiles/` is a working two-pane Swift compositor (~280 lines) ported
from ts4. The Issue 414/501 two-profiles case is re-validated with the ts5 test
infrastructure. Both profile servers deliver independent IOSurface streams at
60fps, composited into a single Metal window with left/right viewports.

### Experiment 3: Three panes — dynamic XPC tab creation

#### Goal

Validate that a single Chromium Profile Server process can dynamically create
**multiple** WebContents from the same `BrowserContext`, each with its own
`FrameSinkVideoCapturer` delivering an independent IOSurface stream. This is the
multi-tab case — the core question of Issue 503.

This experiment also introduces the dynamic tab protocol. The compositor stays
as the Mach service (listener). Profile servers connect as clients, just like
Experiments 1–2. The difference: instead of creating WebContents at startup, the
profile server opens a **control connection**, and the compositor sends
`create_tab` commands back on it. For each tab, the profile server opens a
**dedicated tab connection** for bidirectional per-tab communication (frames
out, keyboard/mouse in).

Two profile server processes:

- **Profile server A** (profile-a data dir): connects to the compositor,
  receives **two** `create_tab` commands, opens two tab connections.
- **Profile server B** (profile-b data dir): connects to the compositor,
  receives **one** `create_tab` command, opens one tab connection.

The compositor renders three panes. The left and center panes (profile-a) should
show the **same** localStorage identity. The right pane (profile-b) should show
a **different** identity.

#### Two connection types

The compositor is the Mach service. Profile servers connect to it. Each profile
server opens 1 + N connections (1 control + N tab connections):

```
Profile Server A ──control──▶ Compositor (Mach service)
                  ──tab-1───▶
                  ──tab-2───▶

Profile Server B ──control──▶ Compositor (Mach service)
                  ──tab-1───▶
```

The **control connection** is opened first. The profile server sends a
`register` message identifying itself. The compositor sends `create_tab`
commands back on this channel. The control connection is a command channel — no
frame data flows on it.

Each **tab connection** is opened by the profile server in response to a
`create_tab` command. It sends a `tab_ready` message correlating it with the
`create_tab` request. After that, frames flow profile-server → compositor and
(eventually) input events flow compositor → profile-server on this connection.
Each tab connection maps to one pane.

#### XPC protocol

##### Control connection (one per profile server)

Profile server → Compositor:

```
{"action": "register", "profile": "profile-a"}
```

Compositor → Profile server:

```
{"action": "create_tab", "url": "http://localhost:9407", "tab_id": "left"}
{"action": "create_tab", "url": "http://localhost:9407", "tab_id": "center"}
```

##### Tab connection (one per tab)

Profile server → Compositor (first message):

```
{"action": "tab_ready", "tab_id": "left"}
```

Profile server → Compositor (60fps):

```
{"action": "display_surface", "iosurface_port": <mach_port>, "width": N, "height": N}
```

The compositor maps `tab_id` to a pane when it receives `tab_ready`. All
subsequent `display_surface` messages on that connection route to that pane.

No `session_id` field on frame messages. The connection itself is the identity.

##### Lifecycle

- Compositor closes a tab connection → profile server tears down that tab's
  WebContents + capturer.
- Compositor closes the control connection → profile server shuts down (no
  reason to live without a compositor).

#### Branch

`146.0.7650.0-issue-503`, branched off `146.0.7650.0-issue-502`.

#### Chromium changes

The profile server currently creates one WebContents at startup and connects to
an external Mach service as an XPC client. For dynamic mode, it connects with a
control channel, waits for `create_tab` commands, and opens additional
connections for each tab.

##### Cleanup

The old single-tab startup path is removed entirely. `GetStartupURL()` is
deleted. The `--session-id` switch is deleted (no longer needed — connections
are the identity). `InitializeMessageLoopContext()` always enters dynamic mode.
This makes the new Chromium branch (`146.0.7650.0-issue-503`) incompatible with
the `ts5/one-profile/` and `ts5/two-profiles/` apps. The profile server will
connect and send `register` instead of immediately streaming frames, and those
compositors don't speak the new protocol. To re-run Experiments 1–2, use the
`146.0.7650.0-issue-502` branch.

##### `content/chromium_profile_server/browser/shell_browser_main_parts.h`

Replace the single `video_consumer_` with per-tab state:

```cpp
struct TabState {
  raw_ptr<Shell> shell;
  std::unique_ptr<ShellVideoConsumer> video_consumer;
  xpc_connection_t tab_connection;
};

std::vector<std::unique_ptr<TabState>> tabs_;
xpc_connection_t control_connection_ = nullptr;
```

Add methods:

```cpp
void StartDynamicMode(const std::string& service_name);
void HandleControlMessage(xpc_object_t msg);
void CreateTab(const GURL& url, const std::string& tab_id);
void CloseTab(xpc_connection_t conn);
```

##### `content/chromium_profile_server/browser/shell_browser_main_parts.cc`

`InitializeMessageLoopContext()` calls `StartDynamicMode()` with the
`--xpc-service` value.

`StartDynamicMode()`:

1. Opens a control connection to the compositor's Mach service via
   `xpc_connection_create_mach_service()` (client mode, no listener flag).
2. Sets up an event handler for incoming messages (`create_tab` commands).
3. Sends a `register` message with the profile name (derived from the basename
   of `--user-data-dir`).
4. Does NOT create any WebContents.

`HandleControlMessage()` on receiving a `create_tab` dictionary:

1. Extracts `url` and `tab_id`.
2. PostTasks `CreateTab()` to the UI thread.

`CreateTab()`:

1. Creates a Shell + WebContents via `Shell::CreateNewWindow()`.
2. Creates a ShellVideoConsumer.
3. Opens a NEW connection to the compositor's Mach service.
4. Sends `{"action": "tab_ready", "tab_id": "..."}` on the new connection.
5. Hands the new connection to the ShellVideoConsumer via `SetConnection()`.
6. Calls `ObserveContents()` → `RenderViewReady()` → `Attach()` → frames start
   flowing.
7. Sets up an error handler on the new connection — on close, PostTasks
   `CloseTab()` to tear down the tab.
8. Stores the TabState.

`CloseTab()` finds the TabState for the connection, stops the capturer, closes
the Shell, and removes the entry.

`PostMainMessageLoopRun()` clears `tabs_` and releases connections.

##### `content/chromium_profile_server/browser/shell_video_consumer.h`

Replace `ConnectToService()` and `SetSessionId()` with `SetConnection()`:

```cpp
// Use an XPC connection opened by the caller.
void SetConnection(xpc_connection_t conn);
```

Remove `session_id_` field entirely.

##### `content/chromium_profile_server/browser/shell_video_consumer.cc`

`SetConnection()` stores the connection (with `xpc_retain`). `OnFrameCaptured()`
sends frames on it. Remove `ConnectToService()`, `SetSessionId()`, and
session-id tagging from frame messages.

#### Swift compositor: `ts5/three-profiles/`

A three-pane compositor. Like Experiments 1–2, it's a Mach service (listener).
Profile servers connect to it. The difference: it now handles two connection
types (control and tab) and sends `create_tab` commands.

- `Package.swift` — SwiftPM manifest, target name `ThreeProfiles`
- `Sources/ThreeProfiles/main.swift` — XPC listener, control/tab protocol, Metal
  pipeline, three-pane rendering
- `Sources/ThreeProfiles/Shaders.metal` — Vertex + fragment shaders (copy)
- `com.termsurf.three-profiles.plist` — Launchd agent definition
- `Makefile` — Compile Metal shaders + `swift build`

##### Connection handling

The compositor's XPC listener accepts all incoming connections. The first
message on each connection determines its type:

- `{"action": "register", ...}` → control connection. Store it, send
  `create_tab` commands.
- `{"action": "tab_ready", "tab_id": "..."}` → tab connection. Map the tab_id to
  a pane. All subsequent `display_surface` messages on this connection route to
  that pane.

For the experiment, the compositor has hardcoded topology:

- Profile `"profile-a"` registers → send two `create_tab` commands with tab_ids
  `"left"` and `"center"`.
- Profile `"profile-b"` registers → send one `create_tab` command with tab_id
  `"right"`.

Tab-id to pane mapping: `"left"` → `.left`, `"center"` → `.center`, `"right"` →
`.right`.

##### Rendering

Three panes side by side. `Pane` enum has `.left`, `.center`, `.right`.
Viewports split into thirds. Otherwise identical to two-profiles.

- Window: 2400x600
- Three textures, three surfaces, three frame counters
- FPS logging: `L: N (fps) C: N (fps) R: N (fps)`

#### Build and Run

```bash
# 1. Start test page server (if not already running)
cd ts5/box-demo && bun run server.ts &

# 2. Build Chromium with dynamic tab support
cd chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default one_profile

# 3. Build three-profiles compositor
cd ts5/three-profiles && make

# 4. Register compositor as launchd service
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts5/three-profiles/com.termsurf.three-profiles.plist

# 5. Start profile server A (connects to compositor, waits for create_tab)
cd chromium/src
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.three-profiles \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a &

# 6. Start profile server B
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.three-profiles \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b
```

Profile servers connect to the compositor and wait. The compositor sends
`create_tab` commands, profile servers open tab connections and start streaming
frames.

#### Pass Criteria

1. Chromium builds with dynamic tab changes (autoninja).
2. Three-profiles compositor builds with `make`.
3. Profile servers connect and register. Compositor sends `create_tab` commands.
   Profile servers open tab connections and start streaming.
4. Compositor window shows three side-by-side panes at ~60fps.
5. Left and center panes show the **same** localStorage identity (same profile,
   same `BrowserContext`).
6. Right pane shows a **different** localStorage identity (different profile).
7. No Dock icon for either Chromium Profile Server process.
8. Both profile servers log ~60fps per WebContents on the sender side.
9. Profile server A logs two attached capturers (two WebContents from one
   `BrowserContext`).

#### Result: Pass

Build: Chromium compiled with 13 steps (incremental). Two Chromium style errors
fixed during the build: `TabState` needed explicit out-of-line
constructor/destructor, and `strcmp` had to be replaced with `std::string_view`
comparison to satisfy `-Wunsafe-buffer-usage-in-libc-call`. Swift compositor
built with zero errors.

Profile server A log (sender side — two WebContents from one BrowserContext):

```
[ProfileServer] Connected to compositor: com.termsurf.three-profiles
[ProfileServer] Registered as profile: profile-a
[ProfileServer] Created tab 'left' for URL: http://localhost:9407/
[ProfileServer] Tab 'left' ready, 1 tab(s) active
[ProfileServer] Created tab 'center' for URL: http://localhost:9407/
[ProfileServer] Tab 'center' ready, 2 tab(s) active
[ShellVideoConsumer] Attached to FrameSinkId FrameSinkId(6, 3), starting capture
[ShellVideoConsumer] Attached to FrameSinkId FrameSinkId(5, 3), starting capture
[ShellVideoConsumer] 61 frames in 1.01042s (60.3706 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01231s (60.2585 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 60 frames in 1.00157s (59.9059 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01724s (59.966 fps) | IOSurface 1600x1200
```

Profile server B log (sender side — one WebContents):

```
[ProfileServer] Connected to compositor: com.termsurf.three-profiles
[ProfileServer] Registered as profile: profile-b
[ProfileServer] Created tab 'right' for URL: http://localhost:9407/
[ProfileServer] Tab 'right' ready, 1 tab(s) active
[ShellVideoConsumer] Attached to FrameSinkId FrameSinkId(5, 3), starting capture
[ShellVideoConsumer] 60 frames in 1.00012s (59.9925 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01602s (60.0384 fps) | IOSurface 1600x1200
[ShellVideoConsumer] 61 frames in 1.01649s (60.0105 fps) | IOSurface 1600x1200
```

Compositor log (receiver side — three panes):

```
[ThreeProfiles] Listening on com.termsurf.three-profiles...
[ThreeProfiles] Profile server registered: profile-a
[ThreeProfiles] Sent create_tab: tab_id=left url=http://localhost:9407
[ThreeProfiles] Sent create_tab: tab_id=center url=http://localhost:9407
[ThreeProfiles] Tab ready: tab_id=left -> pane left
[ThreeProfiles] Tab ready: tab_id=center -> pane center
[ThreeProfiles] Profile server registered: profile-b
[ThreeProfiles] Sent create_tab: tab_id=right url=http://localhost:9407
[ThreeProfiles] Tab ready: tab_id=right -> pane right
[ThreeProfiles] L: 61 (60.0 fps) C: 61 (60.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
[ThreeProfiles] L: 60 (59.3 fps) C: 61 (60.3 fps) R: 61 (60.3 fps) | IOSurface 1600x1200
[ThreeProfiles] L: 61 (60.3 fps) C: 61 (60.3 fps) R: 61 (60.3 fps) | IOSurface 1600x1200
[ThreeProfiles] L: 61 (60.0 fps) C: 60 (59.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
[ThreeProfiles] L: 61 (60.0 fps) C: 61 (60.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
```

60fps on all three streams. No Dock icons. The dynamic tab protocol worked
exactly as designed: profile servers connect, register, receive `create_tab`
commands, open per-tab connections, and start streaming.

Profile server A created **two** WebContents from the **same** BrowserContext,
each with its own FrameSinkVideoCapturer (FrameSinkId 6,3 and 5,3), each
streaming at 60fps over its own XPC tab connection.

#### Conclusion

The core question of Issue 503 is answered: **yes, a single Chromium Profile
Server process can host multiple WebContents from the same BrowserContext, each
with an independent FrameSinkVideoCapturer delivering IOSurface frames at 60fps
over its own XPC connection.**

The dynamic tab protocol (control connection + per-tab connections) works as
designed. Profile servers connect to the compositor, register, receive
`create_tab` commands, and open dedicated tab connections for each tab. The
compositor maps tab IDs to panes and renders all three at 60fps.

Key validated capabilities:

1. **Multiple WebContents per BrowserContext** — Profile server A created two
   WebContents from one profile, each with independent capture. This is the
   multi-tab case.
2. **Independent FrameSinkVideoCapturers** — Each WebContents has its own
   capturer delivering a separate IOSurface stream. No crosstalk.
3. **Dynamic tab creation via XPC** — Tabs are created on demand by the
   compositor, not hardcoded at startup. The profile server creates WebContents
   and capturers in response to `create_tab` commands.
4. **Per-tab XPC connections** — Each tab has its own connection. Connection
   identity replaces session IDs. No multiplexing overhead.
5. **Bidirectional connection model** — The compositor (Mach service listener)
   sends commands to profile servers (clients) on control connections. Profile
   servers open tab connections back. This is the foundation for keyboard/mouse
   input forwarding.
6. **Three independent 60fps streams** — Two from one profile, one from another,
   all rendering in a single Metal window.

### Experiment 4: Update One Profile for dynamic tab protocol

#### Goal

Update `ts5/one-profile/` to speak the dynamic tab protocol introduced in
Experiment 3. Currently incompatible with the `146.0.7650.0-issue-503` Chromium
branch because the old compositor expects profile servers to immediately stream
`display_surface` messages with `session_id` fields, while the new Chromium code
sends `register` and waits for `create_tab` commands.

No Chromium changes — Swift compositor only.

#### Changes

##### `ts5/one-profile/Sources/OneProfile/main.swift`

**New globals:**

```swift
var gControlConnection: xpc_connection_t?
var gTabConnection: xpc_connection_t?
```

**`handleMessage` signature:** Add a `peer` parameter so the handler knows which
connection sent each message (same pattern as three-profiles):

```swift
func handleMessage(_ msg: xpc_object_t, peer: xpc_connection_t)
```

Update the call site in `startXPCListener` to pass `peerConn`.

**`register` handler:** Replace the old `session_id`-based handler. When a
profile server sends `{"action": "register", "profile": "..."}`:

1. Store `peer` as `gControlConnection`.
2. Send
   `{"action": "create_tab", "url": "http://localhost:9407", "tab_id":
   "main"}`
   back on the control connection.
3. Log the profile name.

**New `tab_ready` handler:** When a profile server sends
`{"action":
"tab_ready", "tab_id": "main"}` on a new connection:

1. Store `peer` as `gTabConnection`.
2. Log the tab ID.

**`display_surface` handler:** No changes needed. Frames arrive on the tab
connection instead of the control connection, but the rendering logic is
identical — there's only one pane.

#### Build and Run

```bash
# 1. Start test page server (if not already running)
cd ts5/box-demo && bun run server.ts &

# 2. Build one-profile compositor
cd ts5/one-profile && make

# 3. Register as launchd service
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts5/one-profile/com.termsurf.one-profile.plist

# 4. Start one Chromium Profile Server (no URL arg, no --session-id)
cd chromium/src
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.one-profile \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a
```

#### Pass Criteria

1. One-profile compositor builds with `make`.
2. Profile server connects and sends `register`. Compositor sends `create_tab`.
   Profile server opens a tab connection, sends `tab_ready`, starts streaming.
3. Compositor window shows the spinning blue square at ~60fps.
4. No Dock icon for the Chromium Profile Server process.

#### Result

All pass criteria met:

1. `make` builds clean (0.77s).
2. Full protocol handshake works:
   - Profile server connects, sends `register` as `profile-a`
   - Compositor sends `create_tab` with `url=http://localhost:9407`,
     `tab_id=main`
   - Profile server creates tab, opens tab connection, sends `tab_ready`
   - Frames stream on the tab connection
3. Compositor window renders spinning blue square at ~60fps (59–61fps observed).
4. Profile server runs headless with `--hidden`, no Dock icon.

#### Conclusion

One-profile compositor updated to the dynamic tab protocol with minimal changes
(~15 lines modified). The protocol is backwards-compatible in the sense that the
same Chromium Profile Server binary works with both one-profile and
three-profiles compositors — the compositor decides how many tabs to create.

### Experiment 5: Update Two Profiles for dynamic tab protocol

#### Goal

Update `ts5/two-profiles/` to speak the dynamic tab protocol. Same motivation as
Experiment 4 — two-profiles is incompatible with the `146.0.7650.0-issue-503`
Chromium branch because it routes frames by `session_id` (removed in Experiment
3) instead of by connection identity.

Two profiles, two panes, two profile servers — each profile server gets one
`create_tab` command and opens one tab connection. Connection identity
determines which pane receives frames. No Chromium changes.

#### Changes

##### `ts5/two-profiles/Sources/TwoProfiles/main.swift`

**Remove `paneForSession`:** Delete the old `session_id`-based routing function.
Replace with `paneForTabId` that maps `tab_id` strings to panes:

```swift
func paneForTabId(_ tabId: String?) -> Pane? {
    switch tabId {
    case "left": return .left
    case "right": return .right
    default: return nil
    }
}
```

**New globals:**

```swift
var gTabConnections: [(conn: xpc_connection_t, pane: Pane)] = []
var gControlConnections: [xpc_connection_t] = []
```

**`handleMessage` signature:** Add `peer` parameter:

```swift
func handleMessage(_ msg: xpc_object_t, peer: xpc_connection_t)
```

Update the call site in `startXPCListener` to pass `peerConn`.

**`register` handler:** When a profile server sends
`{"action": "register", "profile": "..."}`:

1. Store `peer` in `gControlConnections`.
2. Send one `create_tab` command back. Map profile name to tab ID:
   - `profile-a` → `tab_id: "left"`
   - `profile-b` → `tab_id: "right"`
3. Log the profile name.

**New `tab_ready` handler:** When a profile server sends
`{"action": "tab_ready", "tab_id": "..."}` on a new connection:

1. Map `tab_id` to pane via `paneForTabId`.
2. Append `(conn: peer, pane: pane)` to `gTabConnections`.
3. Log the mapping.

**`display_surface` handler:** Replace `session_id` lookup with connection
identity lookup — iterate `gTabConnections` to find which pane this connection
maps to (same pattern as three-profiles).

#### Build and Run

```bash
# 1. Start test page server (if not already running)
cd ts4/box-demo && bun run server.ts &

# 2. Build two-profiles compositor
cd ts5/two-profiles && make

# 3. Register as launchd service
launchctl bootstrap gui/$(id -u) \
  ~/dev/termsurf/ts5/two-profiles/com.termsurf.two-profiles.plist

# 4. Start two Chromium Profile Servers
cd chromium/src
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a &
out/Default/Chromium\ Profile\ Server.app/Contents/MacOS/Chromium\ Profile\ Server \
  --hidden \
  --xpc-service=com.termsurf.two-profiles \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b &
```

#### Pass Criteria

1. Two-profiles compositor builds with `make`.
2. Both profile servers connect and complete the handshake: `register` →
   `create_tab` → `tab_ready` → streaming.
3. Left pane renders profile-a, right pane renders profile-b, both at ~60fps.
4. No Dock icons for the Chromium Profile Server processes.

#### Result

All pass criteria met:

1. `make` builds clean (0.92s).
2. Both profile servers complete the full handshake:
   - profile-a: register → create_tab (tab_id=left) → tab_ready → streaming
   - profile-b: register → create_tab (tab_id=right) → tab_ready → streaming
3. Both panes render at ~60fps (59–61fps observed on both).
4. Both profile servers run headless with `--hidden`, no Dock icons.

#### Conclusion

Two-profiles compositor updated to the dynamic tab protocol. Same pattern as
Experiment 4 (one-profile) but with connection-identity routing: each tab
connection maps to a pane via `gTabConnections`, replacing the old
`session_id`-based `paneForSession` function. All three compositors
(one-profile, two-profiles, three-profiles) now speak the same dynamic tab
protocol.

## Conclusion

All four goals accomplished across five experiments:

1. **Port to ts5** (Experiments 1–2): `ts5/one-profile/`, `ts5/two-profiles/`,
   and `ts5/three-profiles/` are self-contained Swift compositors with Metal
   rendering and XPC listeners. ts5 no longer depends on ts4 for testing.

2. **One profile, one tab** (Experiment 1, updated in Experiment 4): Single
   Chromium Profile Server, single WebContents, single pane at 60fps.

3. **Two profiles, one tab each** (Experiment 2, updated in Experiment 5): Two
   Profile Server processes with different `--user-data-dir` paths, each hosting
   one WebContents. Two panes at 60fps.

4. **Two profiles, three tabs** (Experiment 3): One Profile Server hosting
   **two** WebContents from the same BrowserContext, a second hosting one. Three
   independent FrameSinkVideoCapturers delivering three IOSurface streams over
   three XPC connections. Three panes at 60fps. This was the key unknown — it
   proves that multiple WebContents per BrowserContext each get independent
   capture at full framerate.

### Dynamic tab protocol

Experiment 3 introduced a compositor-driven protocol that replaced the original
session-ID-based approach. Experiments 4–5 backported it to the one-profile and
two-profiles compositors, so all three now speak the same protocol:

- Profile server connects and sends `register` with its profile name.
- Compositor sends `create_tab` commands with a URL and tab ID.
- Profile server opens a new XPC connection per tab, sends `tab_ready`.
- Frames flow on per-tab connections. Connection identity replaces session IDs.

The compositor decides how many tabs each profile gets. The Profile Server is
generic — it creates whatever tabs the compositor asks for.

### What's next

The multi-tab, multi-profile rendering pipeline is validated. The next step is
embedding this into ts5's Ghostty fork: replacing the Swift test compositors
with Ghostty's Metal renderer, and spawning Chromium Profile Server processes
from within the application.
