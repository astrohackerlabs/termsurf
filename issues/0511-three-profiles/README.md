+++
status = "closed"
opened = "2026-02-16"
closed = "2026-02-16"
+++

# Issue 511: Three Profiles

## Background

Issue 510 proved two different browser profiles render side by side in the same
terminal window at 60fps, each with fully isolated sessions. But each pane still
spawns its own Chromium Profile Server process, even when two panes share the
same profile name. This works for the two-profile demo (one pane per profile),
but it breaks the moment two panes use `--profile work`.

Chromium acquires a lock on `--user-data-dir` at startup. A second process with
the same `--user-data-dir` will fail to initialize. Two panes sharing a profile
**must** share a single server process. Server reuse is not an optimization — it
is a correctness requirement.

The Chromium Profile Server already supports multiple tabs. Issue 503 Experiment
3 proved that one server process can host N WebContents from the same
BrowserContext, each with an independent FrameSinkVideoCapturer streaming at
60fps. The `CreateTab` method can be called multiple times, each adding a new
Shell + VideoConsumer + per-tab XPC connection. `CloseTab` fires automatically
when a tab's connection drops, removing the tab without shutting down the
server. The infrastructure exists — it just isn't wired up.

## Goal

Three panes in the same terminal window:

- Pane A: `--profile work` (server spawns)
- Pane B: `--profile personal` (second server spawns)
- Pane C: `--profile work` (reuses Pane A's server, sends `create_tab`)

This proves two capabilities:

1. **Two different profiles in the same window.** Already proven in Issue 510,
   but re-confirmed here with a third pane in the mix.
2. **Two panes sharing the same profile.** An ordinary feature that all users
   will expect to work flawlessly. Both panes share one server process, one
   BrowserContext, one `--user-data-dir` — but each gets its own WebContents
   rendering at 60fps.

## Product requirements

### Server lifecycle

Each profile gets exactly one Chromium Profile Server process. The lifecycle:

- **Spawn** when the first pane requests a profile that has no running server.
- **Add tab** when a subsequent pane requests a profile that already has a
  running server. No new process — send `create_tab` on the existing control
  connection.
- **Remove tab** when a pane disconnects. The server closes that tab's
  WebContents and video consumer.
- **Shut down** when the last tab closes (N reaches 0). The server process
  exits.

N can be any positive integer. A server with 5 tabs manages 5 independent
WebContents, each streaming frames to a different terminal pane.

### Frame routing

Each pane must receive only its own frames. The current architecture stamps
`pane_id` on every `display_surface` message, and the compositor routes by
`pane_id` to the correct Ghostty surface. This already works — the challenge is
making `pane_id` per-tab instead of per-process.

Currently the server receives `--pane-id` once from the command line and stamps
it on every video consumer. With server reuse, the server manages tabs for
multiple panes. Each tab's `pane_id` must come from the `create_tab` message,
not from the command line.

### Resize routing

The current `resize` message on the control connection resizes `tabs_[0]` — the
first (and only) tab. With multiple tabs, resize must route to the correct tab.
Each pane sends resize independently when the terminal is resized, so the
message must identify which tab to resize.

### Display

No UI changes. The URL bar already shows the profile name. Each pane renders
independently.

## Current state

### What already works

| Component                  | Status  | Notes                                               |
| -------------------------- | ------- | --------------------------------------------------- |
| `CreateTab` (server)       | Working | Adds Shell + VideoConsumer + per-tab XPC connection |
| `CloseTab` (server)        | Working | Fires when tab connection drops, server continues   |
| Multi-tab 60fps            | Proven  | Issue 503 Experiment 3: N WebContents, N capturers  |
| `display_surface` routing  | Working | `pane_id` in every message, compositor routes by it |
| Profile propagation        | Working | Issue 510: `web` sends profile, app extracts it     |
| Per-profile data directory | Working | Issue 510: `--user-data-dir` per profile            |
| XPC serialization          | Working | Issue 510: all peers on same serial queue           |

### What needs to change

**1. App: Profile-keyed server tracking.**

`CompositorXPC.swift` maps everything by pane UUID:

```swift
private var serverProcesses: [UUID: Process] = [:]
private var serverControlConnections: [UUID: xpc_connection_t] = [:]
```

A second pane requesting `--profile work` spawns a new server because
`serverProcesses[newUUID]` is nil. The mapping needs to change from pane UUID to
profile name so the app can detect an existing server for the same profile.

**2. App: Server reuse in `handleSetOverlay`.**

When `set_overlay` arrives with a URL and a profile name:

- If no server exists for this profile: spawn one, store the URL as pending.
- If a server already exists: send `create_tab` immediately on the existing
  control connection. No spawn needed.

**3. App: Disconnect logic.**

Currently `handleDisconnect` kills the server process when any web peer
disconnects. With server reuse, a disconnect should only remove one tab from the
server. The server should be killed (or allowed to exit) only when all panes for
that profile have disconnected.

**4. Server: Per-tab `pane_id`.**

`pane_id_` is a process-level field set once from `--pane-id`:

```cpp
std::string pane_id_;  // set from command line, shared by all tabs
```

Every tab's video consumer gets the same `pane_id_` via `SetPaneId(pane_id_)`.
For server reuse, each tab needs its own `pane_id` so the compositor can route
frames to the correct Ghostty surface. The `create_tab` message must include the
target pane UUID, and each video consumer must store its own.

**5. Server: Per-tab resize.**

`ResizeCapture` only operates on `tabs_[0]`:

```cpp
auto& tab = tabs_[0];
```

With multiple tabs, resize must accept a pane identifier and route to the
correct `TabState`. The `resize` message on the control connection must include
`pane_id`.

**6. Server: `--pane-id` removal.**

The server no longer belongs to a single pane. The `--pane-id` command-line flag
should be removed. The server identifies itself by its profile (the app already
knows which profile it spawned the server for). Each tab gets its `pane_id` from
`create_tab`.

**7. Server: `server_register` update.**

Currently `server_register` sends `pane_id`:

```cpp
xpc_dictionary_set_string(reg, "action", "server_register");
xpc_dictionary_set_string(reg, "pane_id", pane_id_.c_str());
```

With `--pane-id` removed, the server needs another way to identify itself to the
compositor. The simplest option: send `profile` (the basename of the
`--user-data-dir` path). Alternatively, since the compositor already knows which
profile it spawned the server for, `server_register` could be purely a handshake
with no routing information.

### What should work without changes

**xpc-gateway** — Pure stateless rendezvous. No profile or tab awareness.

**Metal renderer** — Composites overlays by pane UUID. Each pane independently
receives IOSurface frames. No changes needed.

**`web` CLI** — Already sends `--profile` in `set_overlay`. No changes needed.

**Profile name validation** — Already implemented in Issue 510.

## XPC protocol changes

### `web` -> app: `set_overlay`

Unchanged from Issue 510.

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

### app -> server: `create_tab`

Add `pane_id` so the server can stamp each tab's frames with the correct pane
UUID. Remove `tab_id` — each pane corresponds 1-to-1 with a tab, so `pane_id` is
the natural identifier for both.

```
{ action: "create_tab",
  url: "http://...",
  pane_id: "<uuid>",         // (new) identifies pane and tab
  pixel_width: N,
  pixel_height: N }
```

### app -> server: `resize`

Add `pane_id` so the server knows which tab to resize.

```
{ action: "resize",
  pane_id: "<uuid>",         // (new) which tab to resize
  pixel_width: N,
  pixel_height: N }
```

### server -> app: `server_register`

Replace `pane_id` with `profile`.

```
{ action: "server_register",
  profile: "<name>" }        // (changed) profile name instead of pane_id
```

### server -> app: `tab_ready`

Replace `tab_id` with `pane_id`.

```
{ action: "tab_ready",
  pane_id: "<uuid>" }
```

### server -> app: `display_surface`

Unchanged in structure — `pane_id` is already present. The difference is that
it's now per-tab instead of per-process.

```
{ action: "display_surface",
  iosurface_port: <mach_port>,
  pane_id: "<uuid>" }
```

## Architecture note: pane-to-tab mapping

Each terminal pane corresponds 1-to-1 with a Chromium tab (WebContents). There
is no scenario where one pane has multiple tabs or one tab spans multiple panes.
`pane_id` is the single identifier used everywhere — in XPC messages, in frame
routing, and in resize routing. There is no separate `tab_id`.

The server's `tabs_` vector currently stores `TabState` with Shell,
VideoConsumer, and tab_connection. For server reuse, each `TabState` also needs
the `pane_id` of the terminal pane it's rendering for:

```cpp
struct TabState {
    raw_ptr<Shell> shell;
    std::unique_ptr<ShellVideoConsumer> video_consumer;
    xpc_connection_t tab_connection = nullptr;
    std::string pane_id;    // (new) identifies pane and routes frames + resize
};
```

The flow for a shared server:

1. Pane A sends `set_overlay` with `profile=work`. Compositor spawns server.
2. Server sends `server_register`. Compositor stores control connection keyed by
   profile.
3. Compositor sends `create_tab` with `pane_id=A`. Server creates Tab 1, stamps
   A on its frames.
4. Pane C sends `set_overlay` with `profile=work`. Compositor finds existing
   server for `work`.
5. Compositor sends `create_tab` with `pane_id=C`. Server creates Tab 2, stamps
   C on its frames.
6. Both tabs stream at 60fps. Compositor routes by `pane_id` — no confusion.
7. Pane A disconnects. Compositor tells server to close Tab 1. Server keeps
   running for Tab 2.
8. Pane C disconnects. Compositor tells server to close Tab 2. Server has 0 tabs
   and exits.

## Ideas for future experiments

1. **Per-tab pane_id in Chromium.** Remove `--pane-id`, add `pane_id` to
   `create_tab`, store it per-TabState, pass to each VideoConsumer. Test with a
   single pane to verify frames still route correctly.

2. **Per-tab resize.** Add `pane_id` to `resize` message, look up the correct
   TabState, resize that tab's view and capturer. Test with a single pane.

3. **Profile-keyed server tracking in the app.** Restructure
   `CompositorXPC.swift` to map servers by profile name. When a second pane
   requests the same profile, send `create_tab` instead of spawning. Test with
   two panes, same profile.

4. **Server shutdown on last tab close.** The server currently idles forever
   with zero tabs. Add auto-exit when `tabs_` becomes empty after a `CloseTab`.
   Test by closing both panes and verifying the server process exits.

5. **Three panes, two profiles.** The full demo: panes A and C with `work`, pane
   B with `personal`. Two server processes, three frame streams, all at 60fps.

## Experiments

### Experiment 1: Server reuse + three panes

#### Goal

Three split panes in the same window — two with `--profile work`, one with
`--profile personal` — render independent Chromium sessions at 60fps. The two
`work` panes share a single server process. Closing one `work` pane keeps the
other alive. Closing the last pane for a profile terminates that server.

#### Chromium branch

Create a new branch from the current Issue 509 branch:

```bash
cd ~/dev/termsurf/chromium/src
git checkout -b 146.0.7650.0-issue-511 146.0.7650.0-issue-509
```

After committing the Chromium changes, update `docs/chromium.md` to add the new
branch to the Branches table.

#### Changes

Five files in the Chromium fork, one file in the app. No changes to `web` or
xpc-gateway.

##### 1. `shell_switches.h` — Remove `kPaneId`

Delete the `kPaneId` constant (line 69):

```cpp
// Remove:
inline constexpr char kPaneId[] = "pane-id";
```

The server no longer receives `--pane-id` from the command line. Each tab gets
its `pane_id` from the `create_tab` message instead.

##### 2. `shell_browser_main_parts.h` — Update signatures, add `pane_id` to TabState

Remove `pane_id_` member. Add `pane_id` to `TabState`. Update method signatures:

Change `TabState` from:

```cpp
struct TabState {
    TabState();
    ~TabState();
    raw_ptr<Shell> shell;
    std::unique_ptr<ShellVideoConsumer> video_consumer;
    xpc_connection_t tab_connection = nullptr;
};
```

to:

```cpp
struct TabState {
    TabState();
    ~TabState();
    raw_ptr<Shell> shell;
    std::unique_ptr<ShellVideoConsumer> video_consumer;
    xpc_connection_t tab_connection = nullptr;
    std::string pane_id;
};
```

Change `StartDynamicMode` signature from:

```cpp
void StartDynamicMode(const std::string& gateway_name,
                      const std::string& pane_id);
```

to:

```cpp
void StartDynamicMode(const std::string& gateway_name);
```

Change `CreateTab` signature from:

```cpp
void CreateTab(const GURL& url, const std::string& tab_id,
               int pixel_width, int pixel_height);
```

to:

```cpp
void CreateTab(const GURL& url, const std::string& pane_id,
               int pixel_width, int pixel_height);
```

Change `ResizeCapture` signature from:

```cpp
void ResizeCapture(int pixel_width, int pixel_height);
```

to:

```cpp
void ResizeCapture(const std::string& pane_id,
                   int pixel_width, int pixel_height);
```

Remove `pane_id_` member:

```cpp
// Remove:
std::string pane_id_;
```

##### 3. `shell_browser_main_parts.cc` — Per-tab pane_id, per-tab resize, auto-exit

**`InitializeMessageLoopContext`** — Stop passing `pane_id`:

Change:

```cpp
std::string pane_id;
if (cmd->HasSwitch(switches::kPaneId))
  pane_id = cmd->GetSwitchValueASCII(switches::kPaneId);
StartDynamicMode(cmd->GetSwitchValueASCII(switches::kXpcService), pane_id);
```

to:

```cpp
StartDynamicMode(cmd->GetSwitchValueASCII(switches::kXpcService));
```

**`StartDynamicMode`** — Remove `pane_id` parameter, derive profile from
`--user-data-dir`, send profile in `server_register`:

Change signature from:

```cpp
void ShellBrowserMainParts::StartDynamicMode(
    const std::string& gateway_name,
    const std::string& pane_id) {
  pane_id_ = pane_id;
```

to:

```cpp
void ShellBrowserMainParts::StartDynamicMode(
    const std::string& gateway_name) {
```

Change `server_register` message from:

```cpp
xpc_dictionary_set_string(reg, "action", "server_register");
xpc_dictionary_set_string(reg, "pane_id", pane_id_.c_str());
```

to:

```cpp
// Derive profile name from --user-data-dir basename.
std::string profile;
base::CommandLine* cmd = base::CommandLine::ForCurrentProcess();
if (cmd->HasSwitch(switches::kContentShellUserDataDir)) {
  base::FilePath data_dir = cmd->GetSwitchValuePath(switches::kContentShellUserDataDir);
  profile = data_dir.BaseName().value();
}

xpc_dictionary_set_string(reg, "action", "server_register");
xpc_dictionary_set_string(reg, "profile", profile.c_str());
```

Change the log line from:

```cpp
LOG(INFO) << "[ProfileServer] Connected to app via gateway, pane="
          << pane_id_;
```

to:

```cpp
LOG(INFO) << "[ProfileServer] Connected to app via gateway, profile="
          << profile;
```

**Control connection handler** — Extract `pane_id` from `create_tab`, pass to
`ResizeCapture`:

Change `create_tab` extraction from:

```cpp
const char* tab_id_str = xpc_dictionary_get_string(event, "tab_id");
std::string url(url_str ? url_str : "about:blank");
std::string tab_id(tab_id_str ? tab_id_str : "");
```

to:

```cpp
const char* pane_id_str = xpc_dictionary_get_string(event, "pane_id");
std::string url(url_str ? url_str : "about:blank");
std::string pane_id(pane_id_str ? pane_id_str : "");
```

Change `CreateTab` binding from:

```cpp
base::BindOnce(&ShellBrowserMainParts::CreateTab,
               base::Unretained(self), GURL(url), tab_id,
               pw, ph));
```

to:

```cpp
base::BindOnce(&ShellBrowserMainParts::CreateTab,
               base::Unretained(self), GURL(url), pane_id,
               pw, ph));
```

Change `resize` handling from:

```cpp
} else if (action && std::string_view(action) == "resize") {
  int pw = (int)xpc_dictionary_get_uint64(event, "pixel_width");
  int ph = (int)xpc_dictionary_get_uint64(event, "pixel_height");
  content::GetUIThreadTaskRunner({})->PostTask(
      FROM_HERE,
      base::BindOnce(&ShellBrowserMainParts::ResizeCapture,
                     base::Unretained(self), pw, ph));
}
```

to:

```cpp
} else if (action && std::string_view(action) == "resize") {
  const char* rpane = xpc_dictionary_get_string(event, "pane_id");
  std::string resize_pane(rpane ? rpane : "");
  int pw = (int)xpc_dictionary_get_uint64(event, "pixel_width");
  int ph = (int)xpc_dictionary_get_uint64(event, "pixel_height");
  content::GetUIThreadTaskRunner({})->PostTask(
      FROM_HERE,
      base::BindOnce(&ShellBrowserMainParts::ResizeCapture,
                     base::Unretained(self), resize_pane, pw, ph));
}
```

**`CreateTab`** — Use per-tab `pane_id`, store in TabState:

Change signature and body. Replace `tab_id` with `pane_id` throughout:

```cpp
void ShellBrowserMainParts::CreateTab(const GURL& url,
                                      const std::string& pane_id,
                                      int pixel_width,
                                      int pixel_height) {
```

Change `SetPaneId` from:

```cpp
video_consumer->SetPaneId(pane_id_);
```

to:

```cpp
video_consumer->SetPaneId(pane_id);
```

Change `tab_ready` message from:

```cpp
xpc_dictionary_set_string(msg, "action", "tab_ready");
xpc_dictionary_set_string(msg, "tab_id", tab_id.c_str());
```

to:

```cpp
xpc_dictionary_set_string(msg, "action", "tab_ready");
xpc_dictionary_set_string(msg, "pane_id", pane_id.c_str());
```

Change TabState storage from:

```cpp
auto tab = std::make_unique<TabState>();
tab->shell = shell;
tab->video_consumer = std::move(video_consumer);
tab->tab_connection = tab_conn;
tabs_.push_back(std::move(tab));

LOG(INFO) << "[ProfileServer] Tab '" << tab_id << "' ready, "
          << tabs_.size() << " tab(s) active";
```

to:

```cpp
auto tab = std::make_unique<TabState>();
tab->shell = shell;
tab->video_consumer = std::move(video_consumer);
tab->tab_connection = tab_conn;
tab->pane_id = pane_id;
tabs_.push_back(std::move(tab));

LOG(INFO) << "[ProfileServer] Tab for pane " << pane_id << " ready, "
          << tabs_.size() << " tab(s) active";
```

Also update the create log line from:

```cpp
LOG(INFO) << "[ProfileServer] Created tab '" << tab_id
          << "' for URL: " << url.spec();
```

to:

```cpp
LOG(INFO) << "[ProfileServer] Created tab for pane " << pane_id
          << ", URL: " << url.spec();
```

**`ResizeCapture`** — Route by `pane_id` instead of hardcoding `tabs_[0]`:

Change from:

```cpp
void ShellBrowserMainParts::ResizeCapture(int pixel_width, int pixel_height) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);

  if (tabs_.empty() || pixel_width <= 0 || pixel_height <= 0)
    return;

  auto& tab = tabs_[0];
```

to:

```cpp
void ShellBrowserMainParts::ResizeCapture(const std::string& pane_id,
                                          int pixel_width,
                                          int pixel_height) {
  DCHECK_CURRENTLY_ON(BrowserThread::UI);

  if (pane_id.empty() || pixel_width <= 0 || pixel_height <= 0)
    return;

  // Find the tab for this pane.
  TabState* tab = nullptr;
  for (auto& t : tabs_) {
    if (t->pane_id == pane_id) {
      tab = t.get();
      break;
    }
  }
  if (!tab)
    return;
```

Update the rest of `ResizeCapture` to use `tab->` instead of `tab->` (already a
pointer, just remove the `auto&` dereference — the field accesses remain the
same: `tab->shell`, `tab->video_consumer`).

**`CloseTab`** — Add auto-exit when last tab closes:

After the existing `tabs_.erase(it)` and `return`, add a check. Change:

```cpp
      tabs_.erase(it);
      return;
    }
  }
}
```

to:

```cpp
      tabs_.erase(it);

      // Auto-exit when last tab closes.
      if (tabs_.empty()) {
        LOG(INFO) << "[ProfileServer] No tabs remaining, exiting";
        Shell::Shutdown();
      }
      return;
    }
  }
}
```

##### 4. `CompositorXPC.swift` — Profile-keyed server tracking + server reuse

This is the bulk of the Swift changes.

**Replace pane-keyed server dictionaries with profile-keyed ones.** Change:

```swift
/// Maps pane UUID → Chromium Profile Server process (Issue 509).
private var serverProcesses: [UUID: Process] = [:]

/// Maps pane UUID → server control connection (for sending create_tab).
private var serverControlConnections: [UUID: xpc_connection_t] = [:]

/// Maps pane UUID → URL to load (stored until server registers).
private var pendingURLs: [UUID: String] = [:]
```

to:

```swift
/// Maps profile name → Chromium Profile Server process.
private var serverProcesses: [String: Process] = [:]

/// Maps profile name → server control connection (for sending create_tab).
private var serverControlConnections: [String: xpc_connection_t] = [:]

/// Maps pane UUID → (profile, url) pending server registration.
private var pendingTabs: [UUID: (profile: String, url: String)] = [:]

/// Maps pane UUID → profile name (for disconnect cleanup).
private var paneProfiles: [UUID: String] = [:]
```

**Replace `pendingPixelSizes` type** — unchanged in type, just keep as-is:

```swift
/// Maps pane UUID → pending pixel size for create_tab (Issue 509 Experiment 4).
private var pendingPixelSizes: [UUID: (UInt64, UInt64)] = [:]
```

**`handleSetOverlay`** — Rewrite the URL branch. When a `set_overlay` with URL
arrives:

1. Store `paneProfiles[uuid] = profile`.
2. Cache the C surface, compute pixel dims (same as before).
3. If `serverControlConnections[profile]` exists → server is already running and
   registered. Send `create_tab` immediately.
4. Else if `serverProcesses[profile]` exists → server is spawned but hasn't
   registered yet. Store in `pendingTabs` (will be sent when `server_register`
   arrives).
5. Else → no server for this profile. Spawn one, store in `pendingTabs`.

Replace the existing URL branch (from `if let urlPtr = urlPtr {` through the
`spawnServer` call) with:

```swift
if let urlPtr = urlPtr {
    let url = String(cString: urlPtr)
    let profilePtr = xpc_dictionary_get_string(msg, "profile")
    let profile = profilePtr.map { String(cString: $0) } ?? "default"

    // Track which profile this pane belongs to.
    paneProfiles[uuid] = profile

    // If this pane already has a server (resize case), just update.
    if cachedCSurfaces[uuid] != nil {
        if let cSurface = cachedCSurfaces[uuid] {
            ghostty_surface_set_overlay(cSurface, col, row, width, height)

            var cellWidth: UInt32 = 0
            var cellHeight: UInt32 = 0
            ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)
            let pixelWidth = UInt64(width) * UInt64(cellWidth)
            let pixelHeight = UInt64(height) * UInt64(cellHeight)

            if let controlConn = serverControlConnections[profile] {
                let msg = xpc_dictionary_create(nil, nil, 0)
                xpc_dictionary_set_string(msg, "action", "resize")
                xpc_dictionary_set_string(msg, "pane_id", paneIdStr)
                xpc_dictionary_set_uint64(msg, "pixel_width", pixelWidth)
                xpc_dictionary_set_uint64(msg, "pixel_height", pixelHeight)
                xpc_connection_send_message(controlConn, msg)
            }
        }
        return
    }

    fputs("[Compositor] set_overlay with URL \(url) for pane \(paneIdStr) profile \(profile)\n", stderr)

    // Get the C surface pointer.
    var cSurfaceOpt: ghostty_surface_t? = nil
    DispatchQueue.main.sync { [weak self] in
        cSurfaceOpt = self?.appDelegate?.findSurface(forUUID: uuid)?.surface
    }
    guard let cSurface = cSurfaceOpt else {
        fputs("[Compositor] surface not found for pane \(paneIdStr)\n", stderr)
        return
    }
    cachedCSurfaces[uuid] = cSurface
    ghostty_surface_set_overlay(cSurface, col, row, width, height)

    // Compute pixel dimensions.
    var cellWidth: UInt32 = 0
    var cellHeight: UInt32 = 0
    ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)
    let pixelWidth = UInt64(width) * UInt64(cellWidth)
    let pixelHeight = UInt64(height) * UInt64(cellHeight)
    pendingPixelSizes[uuid] = (pixelWidth, pixelHeight)

    if let controlConn = serverControlConnections[profile] {
        // Server already registered — send create_tab immediately.
        sendCreateTab(controlConn, paneId: paneIdStr, url: url, uuid: uuid)
    } else {
        // Store as pending (sent when server_register arrives).
        pendingTabs[uuid] = (profile: profile, url: url)

        if serverProcesses[profile] == nil {
            // No server for this profile — spawn one.
            spawnServer(forProfile: profile)
        }
        // Else: server spawned but not yet registered. pendingTabs will be
        // consumed when server_register arrives.
    }
```

**Add `sendCreateTab` helper:**

```swift
private func sendCreateTab(_ controlConn: xpc_connection_t, paneId: String, url: String, uuid: UUID) {
    let pixelSize = pendingPixelSizes.removeValue(forKey: uuid)
    let msg = xpc_dictionary_create(nil, nil, 0)
    xpc_dictionary_set_string(msg, "action", "create_tab")
    xpc_dictionary_set_string(msg, "url", url)
    xpc_dictionary_set_string(msg, "pane_id", paneId)
    if let (pw, ph) = pixelSize {
        xpc_dictionary_set_uint64(msg, "pixel_width", pw)
        xpc_dictionary_set_uint64(msg, "pixel_height", ph)
    }
    xpc_connection_send_message(controlConn, msg)
    fputs("[Compositor] Sending create_tab url=\(url) pane_id=\(paneId) pixel=\(pixelSize?.0 ?? 0)x\(pixelSize?.1 ?? 0)\n", stderr)
}
```

**`handleServerRegister`** — Key by profile, flush all pending tabs for that
profile:

Replace the entire method with:

```swift
private func handleServerRegister(_ msg: xpc_object_t, from peer: xpc_connection_t) {
    guard let profilePtr = xpc_dictionary_get_string(msg, "profile") else {
        fputs("[Compositor] server_register missing profile\n", stderr)
        return
    }
    let profile = String(cString: profilePtr)

    fputs("[Compositor] server_register from profile \(profile)\n", stderr)

    // Store the control connection keyed by profile.
    serverControlConnections[profile] = peer

    // Flush all pending tabs for this profile.
    for (uuid, pending) in pendingTabs {
        if pending.profile == profile {
            sendCreateTab(peer, paneId: uuid.uuidString, url: pending.url, uuid: uuid)
        }
    }
    pendingTabs = pendingTabs.filter { $0.value.profile != profile }
}
```

**`spawnServer`** — Key by profile, remove `--pane-id`:

Change signature from:

```swift
private func spawnServer(forPane uuid: UUID, profile: String) {
```

to:

```swift
private func spawnServer(forProfile profile: String) {
```

Remove `--pane-id` from the arguments. Change:

```swift
process.arguments = [
    "--xpc-service=com.termsurf.xpc-gateway",
    "--pane-id=\(uuid.uuidString)",
    "--user-data-dir=\(profilePath)",
    "--hidden"
]
```

to:

```swift
process.arguments = [
    "--xpc-service=com.termsurf.xpc-gateway",
    "--user-data-dir=\(profilePath)",
    "--hidden"
]
```

Change storage from `serverProcesses[uuid]` to `serverProcesses[profile]`:

```swift
serverProcesses[profile] = process
fputs("[Compositor] Spawned server PID \(process.processIdentifier) for profile \(profile)\n", stderr)
```

**`handleDisconnect`** — Remove one pane, only kill server when it's the last
pane for that profile:

Replace the web peer branch with:

```swift
if let uuid = peerPaneIds.removeValue(forKey: peerId) {
    fputs("[Compositor] Web process disconnected for pane \(uuid.uuidString)\n", stderr)

    let profile = paneProfiles.removeValue(forKey: uuid)

    // Clear the overlay.
    currentSurfaces.removeValue(forKey: uuid)
    pendingPixelSizes.removeValue(forKey: uuid)
    pendingTabs.removeValue(forKey: uuid)
    if let cSurface = cachedCSurfaces.removeValue(forKey: uuid) {
        ghostty_surface_clear_overlay(cSurface)
    }

    // If no other panes use this profile, kill the server.
    if let profile = profile {
        let otherPanesForProfile = paneProfiles.values.contains(where: { $0 == profile })
        if !otherPanesForProfile {
            if let process = serverProcesses.removeValue(forKey: profile) {
                process.terminate()
                fputs("[Compositor] Terminated server PID \(process.processIdentifier) for profile \(profile)\n", stderr)
            }
            serverControlConnections.removeValue(forKey: profile)
        }
    }
```

##### 5. `docs/chromium.md` — Add branch

Add a row to the Branches table:

```
| `146.0.7650.0-issue-511` | [Issue 511](issues/0511-three-profiles.md) | Per-tab pane routing        |
```

#### Build

```bash
# Build Chromium (after committing changes on the new branch)
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default chromium_profile_server

# Build TermSurf
cd ~/dev/termsurf/ts5 && zig build

# No web changes needed.
```

#### Test

```bash
# Clean logs.
> ~/dev/termsurf/logs/overlay.log

# Start test page server.
cd ~/dev/termsurf/ts4/box-demo && bun run server.ts &

# Open TermSurf.
open ~/dev/termsurf/ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log

# Pane A:
cargo run -p web -- http://localhost:9407 --profile work

# Split pane. Pane B:
cargo run -p web -- http://localhost:9407 --profile personal

# Split pane. Pane C:
cargo run -p web -- http://localhost:9407 --profile work

# Let all three run for at least 30 seconds.

# Then close Pane A (ctrl-c). Verify Pane C keeps rendering.
# Then close Pane C. Verify the work server exits.
# Then close Pane B. Verify the personal server exits.
```

#### Pass criteria

1. All three panes render the box-demo at 60fps simultaneously.
2. Exactly two `Chromium Profile Server` processes are running (one for `work`,
   one for `personal`).
3. Panes A and C share the same server process (same PID in logs).
4. Profile data directories exist at
   `~/.config/termsurf/chromium-profiles/work/` and
   `~/.config/termsurf/chromium-profiles/personal/`.
5. The URL bar shows the correct profile name in each pane.
6. Closing Pane A does not affect Pane C — it keeps rendering at 60fps.
7. Closing Pane C (the last `work` pane) terminates the `work` server process.
8. Closing Pane B terminates the `personal` server process.
9. No crashes in the log.

#### Result

**Pass.** Three panes rendered simultaneously at 60fps — two sharing `work`, one
on `personal`. Exactly two server processes spawned (one per profile). The two
`work` panes shared a single server process confirmed by matching PIDs in logs.
Closing one `work` pane left the other rendering. Closing the last pane for each
profile terminated its server cleanly. No crashes.

## Conclusion

### How we got here

Issue 510 proved two different profiles render side by side at 60fps, but it
papered over a fundamental limitation: each pane spawned its own server process
regardless of profile name. That works when every profile has exactly one pane,
but Chromium locks `--user-data-dir` at startup — a second process with the same
data directory will fail. Server reuse was not an optimization to defer. It was
a correctness requirement.

The Chromium Profile Server already had the multi-tab infrastructure from Issue
503 Experiment 3: `CreateTab` adds independent WebContents with their own frame
capturers, `CloseTab` removes them without killing the server, and each tab gets
its own per-tab XPC connection. What was missing was the wiring: the app tracked
servers by pane UUID instead of profile name, the server stamped a single
process-level `pane_id` on every frame, and resize was hardcoded to `tabs_[0]`.

Experiment 1 rewired both sides in one pass. On the Chromium side: removed
`--pane-id`, made `pane_id` per-tab (from `create_tab`), routed resize by
`pane_id`, derived profile from `--user-data-dir` basename for
`server_register`, and added auto-exit when the last tab closes. On the app
side: replaced pane-keyed dictionaries with profile-keyed ones, added server
reuse logic (second pane with same profile sends `create_tab` to existing
server), and changed disconnect to only kill the server when the last pane for
that profile disconnects.

### What we accomplished

Three panes in the same terminal window — two sharing `--profile work`, one on
`--profile personal` — each rendering an independent Chromium session at 60fps.
Two server processes, three frame streams, full profile isolation, correct
lifecycle management. This validates the complete multi-profile, multi-pane
architecture.

| Experiment | Goal                       | Result |
| ---------- | -------------------------- | ------ |
| 1          | Server reuse + three panes | Pass   |

### What's next

- **Input forwarding.** Keyboard and mouse events to Chromium WebContents —
  without this, webpages are view-only.
- **Session isolation verification.** Load pages that write to localStorage in
  both profiles and confirm data isolation persists across restarts.
- **In-process Chromium.** The endgame: embed Chromium directly via the Content
  API instead of streaming over XPC. The streaming architecture validated every
  other piece of the pipeline — profiles, lifecycle, frame routing, resize — so
  the transition can focus purely on embedding.
