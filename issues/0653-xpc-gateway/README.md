+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 653: XPC Gateway Isolation

## Goal

The `web` TUI must connect to the correct TermSurf app — debug builds connect to
the debug app, installed release builds connect to the installed app. Currently
both builds share a single XPC gateway, so only one can work at a time.

## Problem

The XPC gateway (`com.termsurf.xpc-gateway`) is a singleton Mach service
registered with `launchd`. Both debug and release builds of TermSurf connect to
the same gateway and call `register_app` to store their endpoint. The gateway
stores exactly one endpoint (`appEndpoint` in
`xpc-gateway/Sources/main.swift:61`). Whichever app registers last wins — the
other app's `web` processes silently connect to the wrong app.

Additionally, the launchd plist (`gui/macos/com.termsurf.xpc-gateway.plist:15`)
points to a stale path:

```xml
<string>/Users/ryan/dev/termsurf/ghost/xpc-gateway/.build/debug/xpc-gateway</string>
```

The `ghost/` directory was renamed to `gui/` in Issue 613. The gateway binary is
now at `gui/xpc-gateway/.build/debug/xpc-gateway`.

## How the XPC gateway works

The architecture has three actors:

1. **XPC gateway daemon** — A tiny Swift process that owns the
   `com.termsurf.xpc-gateway` Mach service. Registered with `launchd` via a
   plist. It holds an app endpoint and brokers connections between the app and
   `web` TUI processes.

2. **TermSurf app** — On startup, connects to the gateway and calls
   `register_app` with an anonymous listener endpoint (`xpc.zig:153-158`). This
   is how the app makes itself discoverable.

3. **`web` TUI** — Connects to the gateway and calls `connect` to get the app's
   endpoint. Then connects directly to the app via the returned endpoint. All
   subsequent communication (set_overlay, navigate, mode_changed, etc.) goes
   over this direct connection.

The gateway is a rendezvous point. Once the `web` process has the app's
endpoint, it talks directly to the app — the gateway is no longer involved.

### Key files

| File                                       | Purpose                                         |
| ------------------------------------------ | ----------------------------------------------- |
| `gui/xpc-gateway/Sources/main.swift`       | Gateway daemon source                           |
| `gui/macos/com.termsurf.xpc-gateway.plist` | launchd plist                                   |
| `gui/src/apprt/xpc.zig:125-161`            | App-side: connect to gateway, register endpoint |
| `gui/src/apprt/xpc.zig:140`                | Mach service name: `com.termsurf.xpc-gateway`   |
| `gui/src/apprt/xpc.zig:776`                | Chromium server `--xpc-service` argument        |
| `tui/src/xpc.rs`                           | TUI-side: connect to gateway, get endpoint      |

## What needs to change

### 1. Separate Mach service names

Debug and release builds must use different Mach service names so they don't
interfere:

- **Release**: `com.termsurf.xpc-gateway` (unchanged)
- **Debug**: `com.termsurf.debug.xpc-gateway`

The service name is used in three places:

- `gui/src/apprt/xpc.zig:140` — app connects to gateway
- `gui/src/apprt/xpc.zig:776` — passed to Chromium server as `--xpc-service=`
- `gui/xpc-gateway/Sources/main.swift:9` — gateway listens on the service name

The Zig code can use `comptime` to select the service name based on
`builtin.mode`, the same pattern used for the debug data directory in Issue 650.

### 2. Fix the stale plist path

The plist at `gui/macos/com.termsurf.xpc-gateway.plist:15` needs the correct
path to the gateway binary. Since we now need two plists (one per service name),
we need:

- `gui/macos/com.termsurf.xpc-gateway.plist` — release gateway, points to the
  gateway binary inside the installed app bundle
  (`/Applications/TermSurf.app/Contents/MacOS/xpc-gateway`)
- `gui/macos/com.termsurf.debug.xpc-gateway.plist` — debug gateway, points to
  the dev build (`gui/xpc-gateway/.build/debug/xpc-gateway`)

### 3. Bundle the gateway binary

For the installed app, the gateway binary should be bundled inside
`TermSurf.app/Contents/MacOS/xpc-gateway` (or `Contents/Helpers/`). The release
plist points to this bundled copy. The install script copies the gateway binary
and loads the plist with `launchctl`.

### 4. TUI service name selection

The `web` TUI must know which gateway to connect to. It currently hardcodes the
service name. Options:

- **Environment variable**: The TermSurf app sets `TERMSURF_XPC_SERVICE` in the
  terminal environment. The `web` TUI reads it.
- **Compile-time**: Build the TUI with the service name baked in. But the TUI is
  a single binary used by both debug and release.
- **Convention**: The TUI checks `TERMSURF_XPC_SERVICE`, falling back to
  `com.termsurf.xpc-gateway` if unset. The debug app sets
  `TERMSURF_XPC_SERVICE=com.termsurf.debug.xpc-gateway` in its environment.

The environment variable approach is simplest — the app already controls the
terminal environment.

## Experiments

### Experiment 1: Isolate debug and release gateways

**Goal:** Debug and release builds each get their own XPC gateway so `web`
connects to the correct app regardless of which builds are running.

#### Changes

**1. `gui/xpc-gateway/Sources/main.swift`** — Accept the service name as a
command-line argument instead of hardcoding it. The gateway binary is shared
between debug and release — the plist controls which service name it listens on:

```swift
let serviceName: String
if CommandLine.arguments.count > 1 {
    serviceName = CommandLine.arguments[1]
} else {
    serviceName = "com.termsurf.xpc-gateway"
}
```

This replaces the hardcoded `let serviceName = "com.termsurf.xpc-gateway"` at
line 9. The dispatch queue label also changes to use the dynamic service name.

**2. `gui/src/apprt/xpc.zig:140`** — Use `comptime` to select the Mach service
name based on build mode:

```zig
const xpc_service_name = if (comptime builtin.mode == .Debug)
    "com.termsurf.debug.xpc-gateway"
else
    "com.termsurf.xpc-gateway";

gateway = xpc_connection_create_mach_service(xpc_service_name, null, 0);
```

Same pattern as the debug data directory in Issue 650.

**3. `gui/src/apprt/xpc.zig:776`** — Update the `--xpc-service=` argument passed
to the Chromium server to use the same `comptime` service name:

```zig
"--xpc-service=" ++ (if (comptime builtin.mode == .Debug)
    "com.termsurf.debug.xpc-gateway"
else
    "com.termsurf.xpc-gateway"),
```

Since the `xpc_arg` is already formatted with `bufPrintZ`, this can use
`comptime` string concatenation directly.

**4. `gui/src/apprt/xpc.zig` (init function)** — Set `TERMSURF_XPC_SERVICE` in
the process environment so child terminal sessions inherit it. Debug builds set
it to `com.termsurf.debug.xpc-gateway`; release builds don't set it (the TUI
falls back to the default). Use `std.posix.setenv`:

```zig
if (comptime builtin.mode == .Debug) {
    _ = std.posix.setenv("TERMSURF_XPC_SERVICE", "com.termsurf.debug.xpc-gateway", true);
}
```

This goes in the `init` function, after connecting to the gateway. Every
terminal session spawned by the debug app will inherit this variable.

**5. `tui/src/xpc.rs:79`** — Read `TERMSURF_XPC_SERVICE` from the environment,
falling back to `com.termsurf.xpc-gateway`:

```rust
let service_name = std::env::var("TERMSURF_XPC_SERVICE")
    .unwrap_or_else(|_| "com.termsurf.xpc-gateway".to_string());
let gateway_name = CString::new(service_name).unwrap();
```

When launched from a debug app terminal, the TUI connects to the debug gateway.
When launched from a release app terminal (or standalone), it connects to the
release gateway.

**6. `gui/macos/com.termsurf.xpc-gateway.plist`** — Fix the stale path and make
this the release plist. Points to the bundled gateway binary and passes the
release service name as an argument:

```xml
<key>ProgramArguments</key>
<array>
    <string>/Applications/TermSurf.app/Contents/MacOS/xpc-gateway</string>
    <string>com.termsurf.xpc-gateway</string>
</array>
```

**7. `gui/macos/com.termsurf.debug.xpc-gateway.plist`** — New plist for the
debug gateway. Points to the dev build binary and passes the debug service name:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.debug.xpc-gateway</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.debug.xpc-gateway</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/ryan/dev/termsurf/gui/xpc-gateway/.build/debug/xpc-gateway</string>
        <string>com.termsurf.debug.xpc-gateway</string>
    </array>
    <key>StandardOutPath</key>
    <string>/Users/ryan/dev/termsurf/logs/xpc-gateway-debug.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/ryan/dev/termsurf/logs/xpc-gateway-debug.log</string>
</dict>
</plist>
```

**8. `install.sh`** — Bundle the gateway binary and load the release plist:

```bash
# Bundle xpc-gateway.
echo "==> Bundling xpc-gateway..."
cp "$REPO_DIR/gui/xpc-gateway/.build/debug/xpc-gateway" "$APP/Contents/MacOS/xpc-gateway"

# Load release xpc-gateway plist.
echo "==> Loading xpc-gateway launchd service..."
launchctl bootout "gui/$(id -u)/com.termsurf.xpc-gateway" 2>/dev/null || true
cp "$REPO_DIR/gui/macos/com.termsurf.xpc-gateway.plist" \
  "$HOME/Library/LaunchAgents/com.termsurf.xpc-gateway.plist"
launchctl bootstrap "gui/$(id -u)" \
  "$HOME/Library/LaunchAgents/com.termsurf.xpc-gateway.plist"
```

#### Verification

1. **Build the gateway**: `cd gui/xpc-gateway && swift build`. Verify it accepts
   a service name argument: `.build/debug/xpc-gateway com.termsurf.test` should
   print `Listening on com.termsurf.test`.
2. **Load debug plist**: Copy `com.termsurf.debug.xpc-gateway.plist` to
   `~/Library/LaunchAgents/` and `launchctl bootstrap gui/$(id -u)`. Verify
   `launchctl print gui/$(id -u)/com.termsurf.debug.xpc-gateway` shows the
   service running.
3. **Debug build**: `cd gui && zig build`. Launch the debug app. Open a terminal
   pane and run `echo $TERMSURF_XPC_SERVICE` — should print
   `com.termsurf.debug.xpc-gateway`.
4. **Debug `web`**: In the debug app's terminal, run `web https://google.com`.
   It should connect to the debug gateway and render the page.
5. **Install release**: Run `install.sh`. Launch the installed release app. Run
   `echo $TERMSURF_XPC_SERVICE` — should be empty (unset). Run
   `web https://google.com` — should connect to the release gateway and render.
6. **Both simultaneously**: Launch both the debug app and the installed release
   app. Run `web` in each. Both should connect to their respective gateways and
   render independently.

**Result: Partial.** All code changes compile and the service name isolation is
correct — debug and release use separate Mach service names throughout the stack
(gateway daemon, app, Chromium server, TUI). However, the experiment still
requires manual `launchctl` commands to register the plists. The app should
launch the gateway automatically without any manual setup.

#### What worked

- Gateway accepts service name as CLI argument (change 1)
- `comptime` service name selection in xpc.zig (changes 2–3)
- `TERMSURF_XPC_SERVICE` environment variable for the TUI (changes 4–5)
- Separate plists with correct paths and service names (changes 6–7)
- All three components build cleanly (gateway, GUI, TUI)

#### What didn't work

The experiment requires manually loading plists via `launchctl bootstrap`. This
is a regression from the ts5 era where `SMAppService` handled registration
automatically — the app called `SMAppService.agent(plistName:).register()` on
startup, and launchd auto-started the gateway on demand. That approach was never
ported from ts5 to gui/.

The `launchctl` commands in `install.sh` (change 8) and the verification steps
requiring manual plist loading are wrong. The app must be self-contained — just
`open "TermSurf Debug.app"` should work with zero prior setup.

#### Next step

Experiment 2 should port the SMAppService auto-registration from ts5 (Issue 506
Experiment 2) to gui/. The gateway binary and plist must be bundled inside the
app during `zig build`, and the app must call `SMAppService.register()` on
startup before connecting to the gateway.

### Experiment 2: Auto-register gateway via SMAppService

**Goal:** The app registers the XPC gateway with launchd on startup via
`SMAppService`. No manual `launchctl` commands needed — just `open` the app and
everything works.

#### Background

ts5 solved this in Issue 506 Experiment 2. Three pieces:

1. **Bundle plist** — A plist using `BundleProgram` (bundle-relative path)
   instead of `ProgramArguments` (absolute path) lives at
   `Contents/Library/LaunchAgents/` inside the app bundle.
2. **Bundle binary** — The gateway binary is copied into
   `Contents/MacOS/xpc-gateway` during `zig build`.
3. **SMAppService registration** — On startup, the app calls
   `SMAppService.agent(plistName:).register()`. This tells launchd about the
   agent. When any process connects to the Mach service name, launchd
   auto-starts the gateway.

The gui/ generation moved all XPC logic from Swift to Zig but never ported the
SMAppService registration. The stale launchd registration from the `ghost/` era
(manually loaded via `launchctl bootstrap`) has been keeping things working by
accident.

#### Changes

**1. `gui/macos/com.termsurf.xpc-gateway.bundle.plist`** — New bundle plist for
the release gateway. Uses `BundleProgram` instead of absolute paths:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.xpc-gateway</string>
    <key>BundleProgram</key>
    <string>Contents/MacOS/xpc-gateway</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.xpc-gateway</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>com.termsurf.xpc-gateway</string>
    </array>
</dict>
</plist>
```

`BundleProgram` tells launchd the binary path is relative to the app bundle.
`ProgramArguments` passes the service name as the first argument (which the
gateway reads from Experiment 1's change 1).

**2. `gui/macos/com.termsurf.debug.xpc-gateway.bundle.plist`** — Same but for
the debug gateway:

```xml
<key>Label</key>
<string>com.termsurf.debug.xpc-gateway</string>
<key>BundleProgram</key>
<string>Contents/MacOS/xpc-gateway</string>
<key>MachServices</key>
<dict>
    <key>com.termsurf.debug.xpc-gateway</key>
    <true/>
</dict>
<key>ProgramArguments</key>
<array>
    <string>com.termsurf.debug.xpc-gateway</string>
</array>
```

**3. `gui/src/build/GhosttyXcodebuild.zig`** — Add post-build steps to bundle
the gateway binary and the correct plist into the app. Ported directly from ts5:

```zig
// Bundle xpc-gateway binary and LaunchAgent plist (Issue 653).
const copy_gateway = copy_gw: {
    const step = RunStep.create(b, "copy xpc-gateway into bundle");
    step.addArgs(&.{"cp"});
    step.addFileArg(b.path("xpc-gateway/.build/debug/xpc-gateway"));
    step.addArg(b.fmt("{s}/Contents/MacOS/xpc-gateway", .{app_path}));
    step.step.dependOn(&build.step);
    break :copy_gw step;
};

const mkdir_la = mkdir_la: {
    const step = RunStep.create(b, "mkdir LaunchAgents in bundle");
    step.addArgs(&.{ "mkdir", "-p",
        b.fmt("{s}/Contents/Library/LaunchAgents", .{app_path}) });
    step.step.dependOn(&build.step);
    break :mkdir_la step;
};

const plist_name = if (config.optimize == .Debug)
    "com.termsurf.debug.xpc-gateway" else "com.termsurf.xpc-gateway";
const copy_plist = copy_pl: {
    const step = RunStep.create(b, "copy xpc-gateway plist into bundle");
    step.addArgs(&.{"cp"});
    step.addFileArg(b.path(
        b.fmt("macos/{s}.bundle.plist", .{plist_name})));
    step.addArg(b.fmt(
        "{s}/Contents/Library/LaunchAgents/{s}.plist",
        .{ app_path, plist_name }));
    step.step.dependOn(&mkdir_la.step);
    break :copy_pl step;
};

// Copy must wait for gateway files to be in the bundle.
copy.step.dependOn(&copy_gateway.step);
copy.step.dependOn(&copy_plist.step);
```

**4. `gui/macos/Sources/App/macOS/AppDelegate.swift`** — Register the gateway
LaunchAgent on startup, before the Zig-side `xpc.init` connects to it. Add this
at the top of `init()`, before `ghostty = Ghostty.App(...)`:

```swift
import ServiceManagement

// Register the xpc-gateway LaunchAgent (Issue 653).
#if DEBUG
let gatewayService = SMAppService.agent(
    plistName: "com.termsurf.debug.xpc-gateway.plist")
#else
let gatewayService = SMAppService.agent(
    plistName: "com.termsurf.xpc-gateway.plist")
#endif
switch gatewayService.status {
case .notRegistered, .notFound:
    do {
        try gatewayService.register()
    } catch {
        fputs("[TermSurf] Failed to register xpc-gateway: \(error)\n",
              stderr)
    }
case .enabled, .requiresApproval:
    break
@unknown default:
    break
}
```

This runs before `Ghostty.App()` calls `xpc.init`, ensuring the gateway is
registered with launchd before the app tries to connect to it.

**5. `install.sh`** — Remove the `launchctl` commands added in Experiment 1. The
app handles registration via SMAppService — no manual plist loading needed. Keep
the gateway binary bundling (it's now done by `zig build`, but `install.sh`
copies the whole app bundle, so it comes along for free).

#### Verification

1. **Clean slate**: Unregister any stale gateway registrations:
   ```bash
   launchctl bootout gui/$(id -u)/com.termsurf.xpc-gateway 2>/dev/null
   launchctl bootout gui/$(id -u)/com.termsurf.debug.xpc-gateway 2>/dev/null
   ```
2. **Build**: `cd gui/xpc-gateway && swift build && cd .. && zig build`.
3. **Debug app**: `open "gui/macos/build/Debug/TermSurf Debug.app"`. The gateway
   should auto-register. Check:
   `launchctl print gui/$(id -u)/com.termsurf.debug.xpc-gateway` shows the
   service.
4. **Debug `web`**: In the debug app's terminal, run `web https://google.com`.
   It connects and renders.
5. **Install + release**: Run `install.sh`, then
   `open /Applications/TermSurf.app`. Check:
   `launchctl print gui/$(id -u)/com.termsurf.xpc-gateway` shows the release
   service. `web https://google.com` works.
6. **Both simultaneously**: Both apps running, `web` works in each.

**Result: Fail.**

#### What happened

SMAppService auto-registration works. Both debug and release gateways register
and start on demand — `launchctl print` confirms the services are running and
`web` successfully connects to the compositor ("Connected to compositor" in TUI
output). The gateway isolation from Experiment 1 is also intact.

But the installed release build at `/Applications/TermSurf.app` can't load
pages. `web` connects to the compositor, then times out waiting for the page to
render. The same release build running from the repo directory
(`gui/zig-out/TermSurf.app`) loads pages fine.

We investigated code signing: `install.sh` adds Chromium helpers and the `web`
binary to the app bundle after the build system signs it, which invalidates the
code seal (`codesign -vvv` reports "a sealed resource is missing or invalid").
Adding `codesign --force --deep --sign -` to `install.sh` made the signature
valid again but did not fix the page loading timeout.

#### What we know

- The XPC gateway is running from
  `/Applications/TermSurf.app/Contents/MacOS/xpc-gateway` (confirmed via
  `lsof -p`).
- BTM shows the agent registered to `file:///Applications/TermSurf.app/` with
  correct label and executable path.
- `web` connects to the compositor — the gateway brokering step works.
- Pages time out only when running the installed copy at `/Applications/`.
- The repo build (`gui/zig-out/TermSurf.app`) works despite not having Chromium
  helpers bundled, meaning Chromium is found via an absolute path, not from the
  app bundle.

#### What we don't know

Why the installed release build times out loading pages when the identical
binary at the repo path doesn't. The XPC gateway changes are not the cause — the
gateway works correctly. The page loading failure is likely a separate issue
with how the installed app interacts with Chromium (path resolution, sandboxing,
code signing side effects) that predates or is orthogonal to this issue.

#### Conclusion

The SMAppService approach works for gateway auto-registration, but the installed
release build has a page loading regression that blocks verification. This needs
to be investigated separately — it may be a pre-existing `install.sh` issue
rather than a consequence of the gateway isolation changes.

### Experiment 3: Diagnose debug build gateway failure

**Goal:** Determine why `web` hangs after printing the pane ID in the debug
build. It never prints "Connected to compositor", which means it fails to
connect to the XPC gateway Mach service. The release build from the repo
(`gui/zig-out/TermSurf.app`) works, so the gateway code is functional —
something specific to the debug configuration is broken.

#### Symptom

```
ryan: web https://google.com
[web] TERMSURF_PANE_ID = D69A0E44-CCDA-4C64-B3DE-82EC3C5E432E
```

No "Connected to compositor" line. The TUI hangs indefinitely.

#### Diagnostic steps

1. **Check `TERMSURF_XPC_SERVICE` in the debug terminal.** Run
   `echo $TERMSURF_XPC_SERVICE` inside the debug app's terminal pane. It should
   print `com.termsurf.debug.xpc-gateway`. If it's empty, the env var isn't
   being set by xpc.zig and `web` is trying the release gateway name.

2. **Check if the debug gateway is registered with launchd.** Run:

   ```
   launchctl print gui/$(id -u)/com.termsurf.debug.xpc-gateway
   ```

   If it shows the service, note whether `state = running` or `state = waiting`.
   If it errors ("Could not find service"), SMAppService registration failed.

3. **Check BTM state for the debug agent.** Run:

   ```
   sfltool dumpbtm | grep -A 15 "com.termsurf.debug"
   ```

   Look for the parent bundle identifier and URL. Confirm it points to the debug
   app bundle (`com.termsurf.debug`), not the release app.

4. **Check the debug app's stderr for SMAppService output.** The AppDelegate
   code prints `[TermSurf] Registered xpc-gateway (status: ...)` or
   `[TermSurf] xpc-gateway register error ...` to stderr. Launch the debug app
   from the terminal to see this output:

   ```
   gui/macos/build/Debug/TermSurf\ Debug.app/Contents/MacOS/termsurf 2>&1 | grep TermSurf
   ```

5. **Check the debug bundle contents.** Verify the gateway binary and plist were
   bundled correctly by `zig build`:

   ```
   ls -la "gui/macos/build/Debug/TermSurf Debug.app/Contents/MacOS/xpc-gateway"
   ls -la "gui/macos/build/Debug/TermSurf Debug.app/Contents/Library/LaunchAgents/"
   ```

   The plist should be `com.termsurf.debug.xpc-gateway.plist`. Read it and
   verify the `Label`, `MachServices` key, and `ProgramArguments` all use
   `com.termsurf.debug.xpc-gateway`.

6. **Check the bundle plist's ProgramArguments.** The Experiment 2 fix added a
   dummy `xpc-gateway` as argv[0] so the service name lands at argv[1]. Verify
   the bundled plist has two entries in `ProgramArguments`:

   ```xml
   <array>
       <string>xpc-gateway</string>
       <string>com.termsurf.debug.xpc-gateway</string>
   </array>
   ```

   If it only has one entry, the gateway falls back to the release service name.

7. **Check if the release gateway is also running.** Run:
   ```
   ps aux | grep xpc-gateway | grep -v grep
   ```
   and:
   ```
   launchctl print gui/$(id -u)/com.termsurf.xpc-gateway
   ```
   If the release gateway is running and the debug one isn't, `web` (with
   `TERMSURF_XPC_SERVICE=com.termsurf.debug.xpc-gateway`) would try to connect
   to a nonexistent Mach service and hang.

#### Expected outcome

One or more of these checks will reveal the break. The most likely candidates:

- The env var isn't set (step 1) → `web` connects to the release gateway, which
  may not have an endpoint registered by the debug app.
- SMAppService registration failed (steps 2, 4) → the debug gateway never
  started.
- The bundle plist has wrong ProgramArguments (step 6) → the gateway listens on
  the release service name instead of debug.
- The gateway binary or plist wasn't bundled (step 5) → SMAppService has nothing
  to register.

**Result: Pass.** Root cause identified.

#### Findings

**Step 1 (env var):** Not directly testable without the debug app running
interactively. Source confirms `xpc.zig` sets `TERMSURF_XPC_SERVICE` at comptime
for Debug builds.

**Step 2 (launchd registration):** The debug gateway IS registered with launchd,
but launchd **cannot start it**:

```
state = spawn scheduled
job state = spawn failed
last exit code = 78: EX_CONFIG
runs = 33
properties = partial import | resolve program | has LWCR
```

Launchd has tried to spawn the gateway 33 times. Every attempt exits with code
78 (`EX_CONFIG` — configuration error). The `resolve program` property means
launchd is failing to resolve the `BundleProgram` path to an executable.

**Step 3 (BTM state):** BTM entry looks correct — parent is
`com.termsurf.debug`, URL points to the debug app bundle. However, there is a
**UUID mismatch**: launchd has `BTM uuid = 79002B4D-...` while `sfltool dumpbtm`
shows `UUID: A0FFA4F9-...` for the same agent. This stale UUID may be left over
from the Experiment 2 debugging sessions where the debug app accidentally
registered the release plist name.

**Step 4 (stderr):** Not captured — requires relaunching the debug app.

**Step 5 (bundle contents):** Both present and correct:

- `Contents/MacOS/xpc-gateway` — 83168 bytes, valid code signature
- `Contents/Library/LaunchAgents/com.termsurf.debug.xpc-gateway.plist` — correct

**Step 6 (ProgramArguments):** Correct — two entries: `xpc-gateway` (argv[0])
and `com.termsurf.debug.xpc-gateway` (argv[1]).

**Step 7 (release gateway):** Release gateway is running (pid 76212, from
`/Applications/TermSurf.app`). Debug gateway is NOT running.

**Additional tests:**

- The gateway binary runs fine when executed manually:
  `"TermSurf Debug.app/Contents/MacOS/xpc-gateway" com.termsurf.test.manual`
  prints `Listening on com.termsurf.test.manual`.
- Code signing is valid on both the debug app and the gateway binary.
- System log shows `service inactive: com.termsurf.debug.xpc-gateway` every 10
  seconds — launchd's retry loop.

#### Root cause

Launchd cannot resolve the `BundleProgram` path for the debug gateway. The
`last exit code = 78 (EX_CONFIG)` and `resolve program` property confirm this.
Two factors may be contributing:

1. **Space in bundle path.** The debug app lives at `TermSurf Debug.app` (with a
   space). The BTM URL is `file:///...TermSurf%20Debug.app/`. The release app at
   `/Applications/TermSurf.app` (no space) works. launchd's `BundleProgram`
   resolution may fail when the parent bundle path contains spaces.

2. **Stale BTM entry.** The UUID mismatch (`79002B4D` in launchd vs `A0FFA4F9`
   in BTM) suggests the BTM database has a stale record from earlier debugging.
   Running `sfltool resetbtm` and re-registering may fix this.

#### Next step

Experiment 4 should test both hypotheses: reset the BTM database to clear stale
entries, and if the space is the problem, either rename the debug app to remove
the space or switch from `BundleProgram` to absolute `ProgramArguments` for
debug builds.

### Experiment 4: Fix debug gateway spawn failure

**Goal:** Get the debug gateway to start via SMAppService so `web` works in the
debug build.

Experiment 3 found two possible causes for `exit code 78 (EX_CONFIG)`:

1. Stale BTM entry (UUID mismatch between launchd and sfltool)
2. Space in `TermSurf Debug.app` breaking `BundleProgram` path resolution

This experiment tests them in order — cheapest fix first.

#### Step 1: Reset BTM and retest

Clear the entire BTM database, relaunch the debug app, and check if the gateway
starts:

```bash
sfltool resetbtm
```

Then close and reopen the debug app. Check:

```bash
launchctl print gui/$(id -u)/com.termsurf.debug.xpc-gateway
```

If `state = running`, the stale BTM entry was the problem. Run
`web https://google.com` in the debug terminal to confirm end-to-end.

If `state = spawn scheduled` or `job state = spawn failed` with exit code 78
again, the BTM entry was not the issue — proceed to step 2.

#### Step 2: Remove the space from the debug app name

The debug app is built as `TermSurf Debug.app`. The release app is
`TermSurf.app`. launchd resolves `BundleProgram` relative to the parent bundle
URL, which is `file:///...TermSurf%20Debug.app/`. The URL-encoded space may
break path resolution.

Change the debug app name from `TermSurf Debug.app` to `TermSurf-Debug.app` in
the Xcode project configuration. The relevant setting is `PRODUCT_NAME` (or the
target/scheme name) in `gui/macos/`. Find where the debug build's app name is
set and remove the space.

After rebuilding (`zig build`), relaunch and check:

```bash
launchctl print gui/$(id -u)/com.termsurf.debug.xpc-gateway
```

If `state = running`, the space was the problem. Run `web https://google.com` to
confirm.

#### Verification

The debug gateway starts automatically when the debug app launches. Running
`web https://google.com` in the debug app's terminal connects and loads the
page. No manual `launchctl` commands needed.

**Result: Partial.**

#### What worked

- **BTM reset alone did not fix it** (step 1). After `sfltool resetbtm`, the
  debug gateway still failed with exit code 78 after 67 attempts (up from 33).
  The stale UUID was not the root cause.
- **Removing the space fixed the debug build** (step 2). Renaming from
  `TermSurf Debug.app` to `TermSurf-Debug.app` resolved the `BundleProgram` path
  resolution failure. The debug gateway now starts automatically via
  SMAppService and `web` loads pages.
- **Both debug and release gateways run simultaneously.** After the fix, both
  `com.termsurf.debug.xpc-gateway` and `com.termsurf.xpc-gateway` are running as
  separate launchd services.
- Switched SMAppService logging from `fputs` to `os_log` (`Logger`) so
  registration status is visible in Console.app and the unified log.

#### What didn't work

The installed release build at `/Applications/TermSurf.app` still can't load
pages. `web` connects to the compositor but pages time out. The debug build
(from repo) and the release build (from repo) both work — only the installed
copy fails. This is the same issue observed in Experiment 2.

#### Changes made

- `gui/macos/Ghostty.xcodeproj/project.pbxproj`: `PRODUCT_NAME` changed from
  `"TermSurf Debug"` to `"TermSurf-Debug"`.
- `gui/src/build/GhosttyXcodebuild.zig:52`: `app_name` changed from
  `"TermSurf Debug"` to `"TermSurf-Debug"`.
- `build-debug.sh:32`: Updated APP path.
- `install.sh:48`: Updated lsregister unregister path.
- `gui/macos/Sources/App/macOS/AppDelegate.swift`: Replaced `fputs` with
  `Logger` for SMAppService registration output.

#### Conclusion

The space in `TermSurf Debug.app` was causing launchd's `BundleProgram`
resolution to fail with `EX_CONFIG`. The rename to `TermSurf-Debug.app` fixes
the debug build completely. The installed release build's page loading failure
is a separate issue unrelated to XPC gateway isolation — it needs its own
investigation.

### Experiment 5: Diagnose installed build page timeout

**Goal:** Determine why `web` can load pages from the debug build and the
release build (both run from the repo), but NOT from the installed release build
at `/Applications/TermSurf.app`.

#### Symptom

- Debug build (`gui/macos/build/Debug/TermSurf-Debug.app`): `web` connects and
  pages load.
- Release build (`gui/zig-out/TermSurf.app`): `web` connects and pages load.
- Installed build (`/Applications/TermSurf.app`): `web` connects to the
  compositor ("Connected to compositor") but pages time out. No error, just
  hangs.

The XPC gateway works — `web` gets past the gateway and connects directly to the
app. The problem is somewhere between the compositor receiving the navigate
request and Chromium rendering the page.

#### Diagnostic steps

1. **Compare the gateway and compositor connections.** Run `web` from both the
   release repo build and the installed build. Capture the full output of both
   to compare. Are there differences in the connection flow beyond "Connected to
   compositor"?

2. **Check if Chromium starts.** After launching the installed app and running
   `web https://google.com`, check if any Chromium processes are running:

   ```
   ps aux | grep -i "chromium\|profile.server" | grep -v grep
   ```

   If no Chromium processes exist, the app can't find or launch the Chromium
   server. If they exist, the issue is downstream.

3. **Check how the app finds Chromium.** Read the Zig code that launches the
   Chromium Profile Server to understand how the path is resolved. Is it:
   - Hardcoded to the build directory (`chromium/src/out/Default/`)?
   - Relative to the app bundle (`Contents/Helpers/`)?
   - Read from a config file or environment variable?

   If hardcoded to the build directory, the installed app would still find
   Chromium (the build dir still exists), so path resolution alone wouldn't
   explain the failure. But if there's a secondary path used for resources (pak
   files, locales, v8 snapshot), those might differ.

4. **Check the Chromium server's stderr/stdout.** The Chromium server may be
   logging errors. Find where its output goes:

   ```
   /usr/bin/log show --predicate 'process CONTAINS "Chromium"' --last 5m --style compact
   ```

   Also check if the debug-logs skill has a configured log path for Chromium.

5. **Check environment differences.** When the app runs from the repo directory,
   the working directory and environment may differ from `/Applications/`.
   Check:
   - What is the app's working directory in each case?
   - Are there environment variables set differently?
   - Does the app inherit `PATH` or other vars that affect Chromium discovery?

6. **Check code signing impact.** Experiment 2 found that `install.sh` breaks
   the code signature by adding files to the bundle, and we added
   `codesign --force --deep --sign -` to re-sign. Verify:

   ```
   codesign -vvv /Applications/TermSurf.app
   ```

   Even if the signature is valid, the re-signing might change the app's
   identity in a way that affects XPC endpoint connections or Chromium IPC.

7. **Test a minimal install.** Copy the release build to `/Applications/`
   WITHOUT adding Chromium helpers or the web binary — just the bare app bundle.
   Launch it, then run `web` from the repo's cargo build. If pages load, the
   issue is caused by `install.sh`'s bundle modifications. If pages still don't
   load, the issue is the `/Applications/` path itself.

#### Expected outcome

Step 2 will narrow the problem to either "Chromium doesn't start" or "Chromium
starts but can't render." Step 3 will reveal the path resolution logic. Step 7
will isolate whether `install.sh` modifications cause the failure or the
`/Applications/` location itself is the problem.

**Result: Pass.** Root cause identified.

#### Findings

**Step 2 (Chromium processes):** No TermSurf Chromium processes were running
after the installed app launched and `web` timed out. The bundled Chromium
server either didn't start or exited immediately.

**Step 3 (path resolution):** The Zig code at `xpc.zig:745-779` resolves the
Chromium server path in three steps:

1. **Bundle path** — Walk up 3 dirs from the executable to the bundle root,
   check
   `Contents/Helpers/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server`.
   Used by installed builds (Helpers exists).
2. **Env var** — `TERMSURF_CHROMIUM_SERVER` override.
3. **Dev fallback** — `$HOME/dev/termsurf/chromium/src/out/Default/...`. Used by
   repo builds (no Helpers directory).

The installed build hits path #1 (bundled Chromium). Repo builds hit path #3
(dev fallback). This is the key difference.

**Step 4 (Chromium log):** A stale log at
`Chromium Profile Server.app/Contents/MacOS/chromium_profile_server.log` showed:
`No --xpc-service specified, idling.` — from an earlier launch, not the current
test. Confirms the binary can start from the bundled location, but something is
wrong with how it's invoked or what resources it can find.

**Step 7 (minimal install):** Copied the bare release build to
`/Applications/TermSurf.app` WITHOUT Chromium helpers or the web binary. Ran
`web` via `cargo run` from the repo. **Pages loaded successfully.** This
confirms:

- The `/Applications/` location is fine.
- The app binary works correctly from `/Applications/`.
- The failure is caused by `install.sh` bundling Chromium helpers, which makes
  the path resolution use the bundled copy instead of the dev fallback.

#### Root cause

`install.sh` copies the Chromium `.app` bundles into `Contents/Helpers/` but the
bundled Chromium server doesn't work from that location. The `xpc.zig` path
resolution prefers the bundle path over the dev fallback, so the installed build
uses the broken bundled copy while repo builds use the working dev copy.

This is a pre-existing `install.sh` issue unrelated to the XPC gateway isolation
changes in this issue. The bundled Chromium likely fails because it's a
component build (`is_component_build = true`) that needs shared libraries and
resource files (`.pak`, `icudtl.dat`, `v8_context_snapshot.bin`) that are in
`chromium/src/out/Default/` but not copied into `Contents/Helpers/`.

#### Next step

Copy all Chromium resources alongside the `.app` bundles so the bundled Chromium
server can actually run.

### Experiment 6: Bundle Chromium resources in install.sh

**Goal:** Make the installed app at `/Applications/TermSurf.app` load pages by
copying all files the Chromium server needs into `Contents/Helpers/`.

#### Background

Chromium resolves resources relative to the directory containing the `.app`
bundle — not inside it. When `Chromium Profile Server.app` lives at
`chromium/src/out/Default/`, it finds `.pak` files, `icudtl.dat`,
`v8_context_snapshot.arm64.bin`, and `.dylib` shared libraries in that same
`out/Default/` directory. When copied to `Contents/Helpers/`, those sibling
files are absent.

The build uses `is_component_build = true`, so Chromium is split into shared
libraries (`.dylib` files) that must be present alongside the `.app` bundles.

#### Changes

**1. Identify required files.** Inspect `chromium/src/out/Default/` and
determine the full set of files needed:

- `*.pak` — Resource packs (at minimum: `chromium_profile_server.pak`,
  `chromium_profile_server_resources.pak`, `content_shell.pak`,
  `shell_resources.pak`)
- `icudtl.dat` — ICU internationalization data
- `v8_context_snapshot.arm64.bin` — V8 JavaScript engine snapshot
- `*.dylib` — Shared libraries (component build). Copy all `.dylib` files from
  `out/Default/`.
- `locales/` — If the Chromium server needs locale pak files, copy the
  `locales/` directory too.

Run the bundled Chromium server manually from `Contents/Helpers/` after copying
each category to find the minimum set needed. Start with just `.pak` +
`icudtl.dat` + `v8_context_snapshot` and add `.dylib` files if it crashes on
missing libraries.

**2. Update `install.sh`.** After copying the `.app` bundles, also copy the
resource files:

```bash
# Bundle Chromium resources (component build needs these alongside .app bundles).
echo "==> Bundling Chromium resources..."
cp "$CHROMIUM"/*.pak "$APP/Contents/Helpers/"
cp "$CHROMIUM/icudtl.dat" "$APP/Contents/Helpers/"
cp "$CHROMIUM"/v8_context_snapshot*.bin "$APP/Contents/Helpers/"
cp "$CHROMIUM"/*.dylib "$APP/Contents/Helpers/"
if [ -d "$CHROMIUM/locales" ]; then
  cp -R "$CHROMIUM/locales" "$APP/Contents/Helpers/"
fi
```

**3. Test incrementally.** After updating `install.sh`:

- Run `install.sh` (with sudo if needed for `/Applications/`)
- Open `/Applications/TermSurf.app`
- Run `web https://google.com`
- If it fails, check what's missing by running the Chromium server directly and
  reading the error output

#### Verification

The installed app at `/Applications/TermSurf.app` loads pages via
`web https://google.com`. Both the installed release and the debug repo build
work simultaneously with separate gateways.

**Result: Pass.**

#### What happened

The initial attempt copied resources into `Contents/Helpers/` as designed, but
the Chromium server crashed on startup with:

```
FATAL:content/chromium_profile_server/app/paths_apple.mm:48
DCHECK failed: "Contents" == path.BaseName().value() (Contents vs. /)
```

The crash was caused by `GetContentsPath()` in `paths_apple.mm`, which detects
helper processes by checking if `/Helpers/` appears in the executable path. When
the main server lives at
`Contents/Helpers/Chromium Profile Server.app/Contents/MacOS/...`, the
`/Helpers/` substring triggers the helper code path, which walks up 9
`DirName()` calls instead of 2 — overshooting past the bundle root to `/`.

**Fix:** Renamed the bundling directory from `Contents/Helpers/` to
`Contents/Chromium/` in both `install.sh` and `xpc.zig:760`. This avoids the
`/Helpers/` substring match while keeping all resources and `.app` bundles in
one place.

#### Changes made

- `install.sh`: Changed `Contents/Helpers` to `Contents/Chromium` for all
  bundled Chromium files. Added copying of `*.pak`, `icudtl.dat`,
  `v8_context_snapshot*.bin`, and `*.dylib` (472 shared libraries, ~595MB for
  the component build).
- `gui/src/apprt/xpc.zig:760`: Updated the bundle path resolution from
  `Contents/Helpers/` to `Contents/Chromium/`.

#### Conclusion

The installed app at `/Applications/TermSurf.app` now loads pages. The two fixes
were: (1) copy all Chromium resource files alongside the `.app` bundles, and (2)
avoid the `Contents/Helpers/` directory name because Chromium's path detection
uses `/Helpers/` as a substring match to identify helper processes.

## Conclusion

XPC gateway isolation is complete. Debug and release builds use separate
gateways (`com.termsurf.debug.xpc-gateway` and `com.termsurf.xpc-gateway`) that
run simultaneously without conflict. All three deployment modes work:

- **Debug build** (`gui/macos/build/Debug/TermSurf-Debug.app`) — uses
  `com.termsurf.debug.xpc-gateway`, pages load.
- **Release build** (`gui/zig-out/TermSurf.app`) — uses
  `com.termsurf.xpc-gateway`, pages load.
- **Installed build** (`/Applications/TermSurf.app`) — uses
  `com.termsurf.xpc-gateway`, pages load.

### What changed

- **Experiment 1:** Made the gateway binary accept its Mach service name as a
  command-line argument instead of hardcoding it. Created separate LaunchAgent
  plists for debug (`com.termsurf.debug.xpc-gateway`) and release
  (`com.termsurf.xpc-gateway`). Set `TERMSURF_XPC_SERVICE` at comptime in
  `xpc.zig` so the TUI connects to the right gateway.
- **Experiment 2:** Switched from manual `launchctl` registration to
  `SMAppService.agent()`. The gateway binary and plist live inside the app
  bundle at `Contents/MacOS/xpc-gateway` and `Contents/Library/LaunchAgents/`.
  The app registers them on launch — no manual plist loading needed.
- **Experiment 4:** Renamed the debug app from `TermSurf Debug.app` to
  `TermSurf-Debug.app`. Spaces in the bundle path broke launchd's
  `BundleProgram` resolution (`EX_CONFIG` exit code 78). Switched SMAppService
  logging from `fputs` to `os_log` `Logger`.
- **Experiment 6:** Made `install.sh` copy all Chromium resources (`.pak`,
  `icudtl.dat`, `v8_context_snapshot*.bin`, 472 `.dylib` shared libraries) into
  the app bundle. Renamed the bundling directory from `Contents/Helpers/` to
  `Contents/Chromium/` because Chromium's `paths_apple.mm` uses `/Helpers/` as a
  substring match to detect helper processes.

### Caveats

The release repo build and the installed build share the same gateway name
(`com.termsurf.xpc-gateway`). SMAppService binds the gateway to whichever app
registered it last. If you switch between them, the old registration points to
the wrong bundle. Run `deregister.sh` to clear stale registrations before
launching the other build.

### Key findings

1. **SMAppService works for XPC gateway auto-registration.** No manual
   `launchctl` commands needed. The gateway starts on demand when a client
   connects to the Mach port.
2. **Spaces in bundle names break `BundleProgram`.** launchd fails with
   `EX_CONFIG` when the parent bundle path contains spaces.
3. **Chromium component builds need all sibling resources.** The 472 shared
   libraries, resource packs, ICU data, and V8 snapshot must all be copied
   alongside the `.app` bundles.
4. **Never use `Contents/Helpers/` as a directory name.** Chromium's path
   detection uses `/Helpers/` as a substring match. The main server gets
   misidentified as a helper process, causing a fatal path resolution error.
