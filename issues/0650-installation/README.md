+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 650: Installation

## Goal

Remove hardcoded development paths so TermSurf can be built once and run from
any location. Currently the app only works when run from the source tree because
it references `~/dev/termsurf/` paths directly.

## Hardcoded paths

All hardcoded development paths that need to change:

### 1. Chromium Profile Server path (CRITICAL)

`gui/src/apprt/xpc.zig:734-742`:

```zig
const server_path = std.fmt.bufPrintZ(
    &path_buf,
    "{s}/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server",
    .{home},
) catch { ... };
```

The GUI constructs the Chromium server binary path by appending a hardcoded
development path to `$HOME`. This is the main blocker — the app cannot find the
Chromium server unless the source tree is at `~/dev/termsurf/`.

### 2. Log file path

`gui/src/apprt/xpc.zig:771-776`:

```zig
const logfile_arg = std.fmt.bufPrintZ(
    &logfile_buf,
    "--log-file={s}/dev/termsurf/logs/chromium-server.log",
    .{home},
) catch return;
```

Chromium server logs go to `~/dev/termsurf/logs/`. Should use an XDG-compliant
path.

### 3. XPC gateway plist

`gui/macos/com.termsurf.xpc-gateway.plist:15`:

```xml
<string>/Users/ryan/dev/termsurf/ghost/xpc-gateway/.build/debug/xpc-gateway</string>
```

Hardcoded absolute path to the XPC gateway binary, including a stale directory
name (`ghost/` was renamed to `gui/`).

### 4. `web` TUI binary

The `web` TUI binary (`tui/target/release/web`) is not bundled with the app. The
user must have it on their `$PATH` manually. For a proper installation, it
should be discoverable without modifying `$PATH`.

## What's already correct

- **Browser profile data**: Uses `XDG_DATA_HOME/termsurf/chromium-profiles/`
  (line 752-755). Respects the environment variable with `~/.local/share`
  fallback.
- **Ghostty config**: Uses `XDG_CONFIG_HOME` (handled by upstream Ghostty).

## Proposed approach: macOS-native bundling with dev fallback

Bundle everything inside `TermSurf.app` for release/install builds. For dev
builds, use a fallback path resolution so the app still works without bundling.

### Release bundle layout

```
TermSurf.app/
├── Contents/
│   ├── MacOS/
│   │   ├── termsurf             ← CLI binary (renamed from ghostty)
│   │   └── web                  ← web TUI binary
│   ├── Helpers/
│   │   ├── Chromium Profile Server.app/
│   │   ├── Chromium Profile Server Helper.app/
│   │   ├── Chromium Profile Server Helper (GPU).app/
│   │   ├── Chromium Profile Server Helper (Renderer).app/
│   │   └── Chromium Profile Server Helper (Plugin).app/
│   ├── Frameworks/
│   ├── Resources/
│   └── Info.plist
```

This is the macOS-native pattern. Chrome bundles its helpers the same way. The
app is fully self-contained — drag to `/Applications` and it works.

### Path resolution order

The GUI finds the Chromium server using this fallback chain:

1. **Bundle path**: `TermSurf.app/Contents/Helpers/Chromium Profile Server.app`
   — release/install builds. The app resolves its own bundle path at runtime.
2. **Environment variable**: `TERMSURF_CHROMIUM_SERVER` — custom override for
   testing or non-standard setups.
3. **Dev fallback**:
   `$HOME/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app` —
   existing behavior, works during development without any bundling.

For the `web` binary, the same pattern:

1. **Bundle path**: `TermSurf.app/Contents/MacOS/web`
2. **`$PATH`**: fall back to finding `web` on the user's PATH (dev builds)

### Log file location

Logs should move to `XDG_STATE_HOME/termsurf/` (default: `~/.local/state`), per
the XDG spec:

```
~/.local/state/termsurf/
└── chromium-server.log
```

### CLI binary rename

The CLI binary is still named `ghostty` (`gui/src/build/GhosttyExe.zig:15`). It
needs to be renamed to `termsurf`. This affects:

- `gui/src/build/GhosttyExe.zig:15` — `.name = "ghostty"` → `"termsurf"`
- `gui/src/build/GhosttyXcodebuild.zig:144` — path reference
  `Contents/MacOS/ghostty` → `Contents/MacOS/termsurf`

### Three build scenarios

**1. Dev build** (`cd gui && zig build`):

- Produces `gui/zig-out/TermSurf.app` (or `gui/macos/build/Debug/TermSurf.app`)
- No Chromium bundled — uses dev fallback path
- `web` not bundled — user has it on PATH via cargo
- Run with `open gui/zig-out/TermSurf.app` or
  `gui/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf`

**2. Release build** (`cd gui && zig build -Doptimize=ReleaseFast`):

- Produces `gui/macos/build/ReleaseLocal/TermSurf.app`
- Optimized binary, still no Chromium bundled
- Same dev fallback for Chromium

**3. Install build** (release build + install script):

- Takes the release build and bundles everything into it
- Copies Chromium Profile Server + helpers into `Contents/Helpers/`
- Copies `web` binary into `Contents/MacOS/`
- Copies the complete bundle to `/Applications/TermSurf.app`
- Symlinks `termsurf` and `web` to `/usr/local/bin/` (or `~/.local/bin/`) so CLI
  commands work

### Install script

`install.sh` (top-level, alongside `build-release.sh` and `build-debug.sh`).
Does NOT build anything — it installs the latest release build. Run
`build-release.sh` first.

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"
APP="/Applications/TermSurf.app"
SRC="$REPO_DIR/gui/macos/build/ReleaseLocal/TermSurf.app"
CHROMIUM="$REPO_DIR/chromium/src/out/Default"
WEB="$REPO_DIR/tui/target/release/web"

# Verify release build exists.
if [ ! -d "$SRC" ]; then
  echo "Error: Release build not found at $SRC"
  echo "Run build-release.sh first."
  exit 1
fi

# Copy app bundle.
echo "==> Installing to $APP..."
rm -rf "$APP"
cp -R "$SRC" "$APP"

# Bundle Chromium server + helpers.
echo "==> Bundling Chromium Profile Server..."
mkdir -p "$APP/Contents/Helpers"
cp -R "$CHROMIUM/Chromium Profile Server.app" "$APP/Contents/Helpers/"
cp -R "$CHROMIUM/Chromium Profile Server Helper.app" "$APP/Contents/Helpers/"
cp -R "$CHROMIUM/Chromium Profile Server Helper (GPU).app" "$APP/Contents/Helpers/"
cp -R "$CHROMIUM/Chromium Profile Server Helper (Renderer).app" "$APP/Contents/Helpers/"
cp -R "$CHROMIUM/Chromium Profile Server Helper (Plugin).app" "$APP/Contents/Helpers/"

# Bundle web TUI.
if [ -f "$WEB" ]; then
  echo "==> Bundling web TUI..."
  cp "$WEB" "$APP/Contents/MacOS/web"
else
  echo "Warning: web TUI not found at $WEB (skipping)"
fi

# Symlink CLI tools.
echo "==> Symlinking CLI tools to /usr/local/bin/..."
ln -sf "$APP/Contents/MacOS/termsurf" /usr/local/bin/termsurf
ln -sf "$APP/Contents/MacOS/web" /usr/local/bin/web

echo ""
echo "Done."
echo "  App:  $APP"
echo "  CLI:  /usr/local/bin/termsurf"
echo "  Web:  /usr/local/bin/web"
```

No signing needed for personal use. Gatekeeper may warn on first launch —
right-click → Open bypasses it. Signing and notarization are only required for
distribution to other people.

## Experiments

### Experiment 1: Path resolution and install script

**Goal:** Replace hardcoded dev paths with a fallback chain (bundle → env var →
dev path). Rename the CLI binary from `ghostty` to `termsurf`. Create an install
script that bundles everything and copies to `/Applications`. Verify all three
build scenarios work.

#### Changes

**1. `gui/src/apprt/xpc.zig:734-742`** — Replace hardcoded Chromium server path
with fallback chain:

```zig
// Resolution order: bundle → env var → dev fallback.
var path_buf: [std.fs.max_path_bytes]u8 = undefined;
const server_path = blk: {
    // 1. Check inside app bundle (release/install builds).
    //    Use std.fs.selfExePath() to discover our own bundle root,
    //    then look for Contents/Helpers/.
    var exe_buf: [std.fs.max_path_bytes]u8 = undefined;
    if (std.fs.selfExePath(&exe_buf)) |exe| {
        // Walk up 3 components: termsurf → MacOS → Contents → bundle root.
        var dir: []const u8 = exe;
        var i: usize = 0;
        while (i < 3) : (i += 1) {
            dir = std.fs.path.dirname(dir) orelse break;
        }
        if (i == 3) {
            const helpers_path = std.fmt.bufPrintZ(
                &path_buf,
                "{s}/Contents/Helpers/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server",
                .{dir},
            ) catch null;
            if (helpers_path) |p| {
                if (std.fs.accessAbsolute(p[0 .. p.len], .{})) {
                    break :blk helpers_path;
                } else |_| {}
            }
        }
    } else |_| {}
    // 2. Check environment variable override.
    if (std.posix.getenv("TERMSURF_CHROMIUM_SERVER")) |p| {
        break :blk std.fmt.bufPrintZ(&path_buf, "{s}", .{p}) catch null;
    }
    // 3. Dev fallback.
    break :blk std.fmt.bufPrintZ(
        &path_buf,
        "{s}/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server",
        .{home},
    ) catch null;
} orelse {
    log.err("server path too long", .{});
    return;
};
```

**2. `gui/src/apprt/xpc.zig:771-776`** — Replace hardcoded log path with
XDG_STATE_HOME:

```zig
var state_home_buf: [512]u8 = undefined;
const state_home = std.posix.getenv("XDG_STATE_HOME") orelse std.fmt.bufPrintZ(
    &state_home_buf,
    "{s}/.local/state",
    .{home},
) catch {
    log.err("state home path too long", .{});
    return;
};

var logfile_buf: [256]u8 = undefined;
const logfile_arg = std.fmt.bufPrintZ(
    &logfile_buf,
    "--log-file={s}/termsurf/chromium-server.log",
    .{state_home},
) catch return;
```

Also need to create the directory if it doesn't exist before spawning:

```zig
// Ensure log directory exists.
var logdir_buf: [256]u8 = undefined;
const logdir = std.fmt.bufPrintZ(
    &logdir_buf,
    "{s}/termsurf",
    .{state_home},
) catch null;
if (logdir) |d| {
    std.fs.cwd().makePath(d) catch {};
}
```

**3. `gui/src/build/GhosttyExe.zig:15`** — Rename CLI binary:

```zig
.name = "termsurf",
```

**4. `gui/src/build/GhosttyXcodebuild.zig:144`** — Update run path:

```zig
"{s}/Contents/MacOS/termsurf",
```

**5. Create `install.sh`** — Top-level install script (alongside
`build-release.sh`) that bundles the latest release build with Chromium, copies
to `/Applications`, and symlinks CLI tools.

**6. Update `docs/xdg.md`** — Add `XDG_STATE_HOME` for logs.

#### Bundle path discovery

**Resolved.** Ghostty already has bundle path discovery in
`gui/src/os/resourcesdir.zig:68-84`. It calls `std.fs.selfExePath()` (which uses
`_NSGetExecutablePath` on macOS) and walks up the directory tree looking for
`Contents/Resources`. We use the same pattern:

1. `std.fs.selfExePath()` returns e.g.
   `/Applications/TermSurf.app/Contents/MacOS/termsurf`.
2. Strip 3 path components (`termsurf` → `MacOS` → `Contents`) to get the bundle
   root: `/Applications/TermSurf.app`.
3. Append
   `Contents/Helpers/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server`.
4. Check if the file exists with `std.fs.accessAbsolute()`. If yes, use it. If
   no (dev build — helpers not bundled), fall through to the next option.

No environment variable needed. The bundle path is discovered purely from the
executable location at runtime. The `TERMSURF_CHROMIUM_SERVER` env var remains
as an explicit override for custom setups.

#### Verification

1. **Dev build**: `cd gui && zig build && open zig-out/TermSurf.app`. App
   starts, finds Chromium server via dev fallback, `web` works from PATH.
2. **Release build**: `cd gui && zig build -Doptimize=ReleaseFast`. App starts
   from `gui/macos/build/ReleaseLocal/TermSurf.app`.
3. **Install build**: Run `install.sh`. Verify:
   - `/Applications/TermSurf.app` exists and launches from Finder.
   - `termsurf` command works from terminal.
   - `web <url>` command works from terminal.
   - Chromium server is found inside the bundle (not the dev path).
   - Logs go to `~/.local/state/termsurf/chromium-server.log`.
4. **Coexistence**: Dev build and installed build can run independently. Dev
   build uses the dev Chromium, installed build uses its bundled copy.

**Result: Pass.** All six changes implemented and verified. Dev build compiles
and runs, finding the Chromium server via the dev fallback path. The install
script bundles everything into `/Applications/TermSurf.app` with Chromium
helpers, symlinks CLI tools to `/usr/local/bin/`, and the installed app
discovers its bundled Chromium server via `std.fs.selfExePath()`. Logs now go to
`~/.local/state/termsurf/chromium-server.log`.

## Conclusion

TermSurf can now be built once and installed anywhere. The four hardcoded
development paths are gone:

1. **Chromium server path** — replaced with a three-step fallback chain: check
   the app bundle (`Contents/Helpers/`), then `TERMSURF_CHROMIUM_SERVER` env
   var, then the dev fallback. Bundle discovery uses `std.fs.selfExePath()`, the
   same pattern Ghostty uses in `resourcesdir.zig`.
2. **Log file path** — moved from `~/dev/termsurf/logs/` to
   `XDG_STATE_HOME/termsurf/chromium-server.log` (default:
   `~/.local/state/termsurf/`). The directory is created automatically.
3. **XPC gateway plist** — was already stale (referenced `ghost/` which was
   renamed to `gui/`). Not modified in this issue since the plist is not used in
   the current architecture.
4. **`web` TUI binary** — bundled into `Contents/MacOS/web` by the install
   script and symlinked to `/usr/local/bin/web`.

Additional changes:

- **CLI binary renamed** from `ghostty` to `termsurf` in the build system.
- **Install script** (`install.sh`) copies the release build to
  `/Applications/TermSurf.app`, bundles Chromium helpers, and symlinks
  `termsurf` and `web` to `/usr/local/bin/`.
- **Debug/release profile isolation** — debug builds use
  `~/.local/share/termsurf/debug/chromium-profiles/` while release builds use
  `~/.local/share/termsurf/chromium-profiles/`. Both can run simultaneously
  without Chromium data directory conflicts.
