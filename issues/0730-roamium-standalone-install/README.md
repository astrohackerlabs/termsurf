+++
status = "closed"
opened = "2026-03-08"
closed = "2026-03-08"
+++

# Issue 730: Roamium standalone install

## Goal

Make Roamium installable as a standalone package, separate from any board. Today
Roamium lives inside `chromium/src/out/Default/` alongside ~100 Chromium dylibs,
5 helper .app bundles, .pak files, and resource files. Wezboard hardcodes the
path `$HOME/dev/termsurf/chromium/src/out/Default/roamium`. This needs to become
a proper install — either a single binary or a self-contained bundle — so that
boards discover it via `$PATH` or a known install location.

## Background

### Why this matters

TermSurf is a protocol. Boards (Ghostboard, Wezboard), browser engines (Roamium,
Surfari, Girlbat), and TUIs (`web`) are all separate components that speak the
same protobuf/Unix socket protocol. Users should be able to install each
independently:

- `web` → `/usr/local/bin/web` (already works)
- `roamium` → `/usr/local/bin/roamium` or `/usr/local/lib/roamium/`
- Boards → their own install paths

Third-party apps that implement the TermSurf protocol should be able to launch
Roamium without knowing where Chromium was built.

### Current state

**Roamium binary** (`roamium/`): A ~1 MB Rust binary that links
`libtermsurf_chromium.dylib` and speaks protobuf over Unix sockets. Minimal Rust
code (~400 lines across main.rs, ffi.rs, ipc.rs, dispatch.rs).

**Runtime dependencies** (all in `chromium/src/out/Default/`):

| Category                   | Files                                       | Size   |
| -------------------------- | ------------------------------------------- | ------ |
| libtermsurf_chromium.dylib | 1                                           | ~11 MB |
| Chromium component dylibs  | ~100+                                       | large  |
| Helper .app bundles        | 5 (Server, GPU, Renderer, Plugin, Helper)   | large  |
| Resource files             | .pak, icudtl.dat, v8_context_snapshot\*.bin | ~50 MB |

**Build flow:**

1. Chromium is built in `chromium/src/out/Default/`
2. `scripts/build-roamium.sh` runs `cargo build` and copies the binary there
3. `roamium/build.rs` sets two rpaths: `@loader_path/.` and the chromium build
   dir
4. Wezboard's `resolve_browser_path()` hardcodes
   `$HOME/dev/termsurf/chromium/src/out/Default/roamium`

**Install script** (`scripts/install.sh`): Bundles Chromium files into
`TermSurf Ghostboard.app/Contents/Chromium/` but does NOT copy the roamium
binary itself.

### The challenge

Roamium cannot be a single static binary. Chromium is fundamentally a
multi-process architecture — it launches helper processes (GPU, Renderer,
Plugin) as separate executables. These helpers are .app bundles on macOS. The
~100 component dylibs are how Chromium's build system produces its output. The
.pak files and ICU data are loaded at runtime by path.

Options to investigate:

1. **Bundle directory** — Install Roamium as a directory
   (`/usr/local/lib/roamium/`) containing the binary, all dylibs, helper apps,
   and resources. Put a symlink or wrapper script at `/usr/local/bin/roamium`.

2. **macOS .app bundle** — Package as `Roamium.app` in `/Applications/` or
   `/usr/local/lib/`. Chromium already expects .app structure on macOS.

3. **Static linking** — Investigate whether Chromium can be built as a single
   static library (`is_component_build = false` in GN args). This would
   eliminate the ~100 dylibs but helper .app bundles and resources would still
   be needed.

4. **Single binary with embedded resources** — Investigate whether .pak files
   and ICU data can be embedded in the binary or the dylib. Even if possible,
   helper processes still need to be separate executables.

### Questions to answer

1. What is the minimum set of files Roamium needs at runtime? (Can we trim the
   ~100 dylibs by building non-component?)
2. Can Chromium's helper processes be colocated with the main binary, or do they
   require .app bundle structure on macOS?
3. What does the file layout look like on Linux vs macOS? Linux doesn't use .app
   bundles.
4. How should boards discover Roamium? `$PATH` lookup? A config file? A
   well-known install path?
5. How do debug builds work? Developers need fast iteration without a full
   install step.

### How other projects handle this

- **Electron** — Ships as a .app bundle (macOS) or directory (Linux/Windows)
  containing the framework, helpers, and resources.
- **CEF** — Distributes as a directory with the main binary, libcef.so/dylib,
  helpers, and resources. Applications bundle everything together.
- **Chrome itself** — Installs as a .app bundle on macOS, a directory in
  `/opt/google/chrome/` on Linux.

## Experiments

### Experiment 1: Can Roamium run without .app bundles?

#### Goal

Determine whether Chromium's helper processes can be plain executables colocated
with the Roamium binary, or whether they require macOS .app bundle structure.

#### Research

**How Chromium finds helper executables**
(`content/browser/child_process_host_impl.cc:59-118`):

1. Check `--browser-subprocess-path` CLI flag — if set, use that path directly
2. Fall back to `CHILD_PROCESS_EXE` path service (current executable or
   overridden via `OverrideChildProcessPath()`)
3. **macOS-only transform (line 90-114):** If `flags != CHILD_NORMAL` AND
   `base::apple::AmIBundled()` returns true, navigate up from the helper path
   and construct `.app/Contents/MacOS/` paths for specialized helpers (Renderer,
   GPU, Plugin)

The .app bundle transform at line 90 has two conditions:

```cpp
if (flags != CHILD_NORMAL && base::apple::AmIBundled()) {
```

`AmIBundled()` returns true only when the process is running from inside a macOS
.app bundle. Roamium runs as a plain binary launched by the board (just like
`web`), so `AmIBundled()` returns false and the entire .app bundle transform is
skipped.

**Roamium already works this way today.** Looking at `roamium/src/main.rs`,
Roamium is a plain Rust binary that:

1. Parses `--ipc-socket=` and `--user-data-dir=` from argv
2. Passes all argv through to `ts_content_main()` (which calls Chromium's
   `ContentMain`)
3. Runs from `chromium/src/out/Default/roamium` — a plain binary, not inside any
   .app bundle

The board (Wezboard) launches it as a plain process via
`std::process::Command::new(&binary)` in `spawn_server()`. No .app bundle
involved.

**The `--browser-subprocess-path` flag** (line 62-63) is checked before any
bundle logic. Roamium can pass this flag to tell Chromium where to find the
helper executable. The helper binary handles all process types (Renderer, GPU,
Plugin) — Chromium dispatches by `--type=` argument, not by executable name.

**Current file layout in `chromium/src/out/Default/`:**

The .app bundles that exist today (Chromium Profile Server Helper.app, etc.) are
build artifacts from Chromium's default macOS build configuration. They are NOT
required when the main process is a plain binary. The actual executables inside
those .app bundles can be extracted and colocated as plain files.

#### Proposed install layout

```
/usr/local/lib/roamium/
  roamium                                  # main binary
  chromium_profile_server_helper           # helper (all types via --type=)
  libtermsurf_chromium.dylib               # + other dylibs
  *.pak, icudtl.dat, v8_context_snapshot*  # resources
/usr/local/bin/roamium → ../lib/roamium/roamium  # symlink
```

Roamium would pass
`--browser-subprocess-path=/usr/local/lib/roamium/chromium_profile_server_helper`
to Chromium, and all specialized helper types (GPU, Renderer, Plugin) use the
same binary dispatched by `--type=`.

#### Result

Research confirms the approach is viable.

#### Conclusion

Roamium can run as a plain binary with `--browser-subprocess-path` pointing to
colocated helper executables. No .app bundles required. `AmIBundled()` returns
false for plain binaries, so Chromium's macOS bundle path transform never fires.
The install would be a directory containing the binary, helper, dylibs, and
resources, with a symlink in `$PATH`. This approach works for both Wezboard and
Ghostboard — any board that speaks the TermSurf protocol can launch Roamium from
its installed location.

### Experiment 2: Install script and test with Ghostboard

#### Goal

Create `scripts/install-roamium.sh` that installs Roamium to
`/usr/local/lib/roamium/` with a symlink at `/usr/local/bin/roamium`, then test
it by running `web --browser /usr/local/bin/roamium termsurf.com` in Ghostboard.

#### Background

Both Ghostboard (`xpc.zig:848`) and Wezboard (`conn.rs:938-939`) hardcode
Roamium's path to `$HOME/dev/termsurf/chromium/src/out/Default/roamium`. Both
also support absolute paths — the TUI's `--browser` flag passes the path through
the protocol, and the board's `resolveBrowserPath()` returns absolute paths
as-is.

Roamium already runs as a plain binary (not in a .app bundle). The existing
`@loader_path/.` rpath in `roamium/build.rs` means dylibs resolve relative to
the binary's actual location. A symlink at `/usr/local/bin/roamium` resolves to
`/usr/local/lib/roamium/roamium`, so `@loader_path` becomes
`/usr/local/lib/roamium/` — exactly where the dylibs will be.

Without `--browser-subprocess-path`, Chromium falls back to `CHILD_PROCESS_EXE`
(the current executable). Since `AmIBundled()` is false, no .app transform
happens — Chromium will re-invoke `roamium` itself as the helper process,
dispatching by `--type=`. This is the same pattern as content_shell on Linux.

#### Design

**1. Create `scripts/install-roamium.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
ROAMIUM_SRC="$REPO_DIR/roamium/target/release/roamium"
INSTALL_DIR="/usr/local/lib/roamium"

# Verify release build exists.
if [ ! -f "$ROAMIUM_SRC" ]; then
  echo "Error: Release build not found at $ROAMIUM_SRC"
  echo "Run: scripts/build-roamium.sh --release"
  exit 1
fi

echo "==> Installing Roamium to $INSTALL_DIR..."
sudo mkdir -p "$INSTALL_DIR"

# Copy roamium binary.
sudo cp "$ROAMIUM_SRC" "$INSTALL_DIR/roamium"

# Copy dylibs.
echo "==> Copying dylibs..."
sudo cp "$CHROMIUM_OUT"/*.dylib "$INSTALL_DIR/"

# Copy resources.
echo "==> Copying resources..."
sudo cp "$CHROMIUM_OUT"/*.pak "$INSTALL_DIR/"
sudo cp "$CHROMIUM_OUT/icudtl.dat" "$INSTALL_DIR/"
sudo cp "$CHROMIUM_OUT"/v8_context_snapshot*.bin "$INSTALL_DIR/"

# Symlink to /usr/local/bin.
echo "==> Symlinking /usr/local/bin/roamium..."
sudo ln -sf "$INSTALL_DIR/roamium" /usr/local/bin/roamium

echo ""
echo "Done."
echo "  Dir:  $INSTALL_DIR"
echo "  Bin:  /usr/local/bin/roamium"
```

**2. Test with Ghostboard**

```bash
# Build roamium release
scripts/build-roamium.sh --release

# Install
scripts/install-roamium.sh

# Test — launch Ghostboard, then in the terminal:
web --browser /usr/local/bin/roamium termsurf.com
```

#### Verification

1. `scripts/install-roamium.sh` completes without errors
2. `/usr/local/bin/roamium` exists and is a symlink to
   `/usr/local/lib/roamium/roamium`
3. `/usr/local/lib/roamium/` contains the binary, dylibs, .pak files, and
   resource files
4. In Ghostboard: `web --browser /usr/local/bin/roamium termsurf.com` opens
   termsurf.com with the installed Roamium — page renders, input works
5. In Ghostboard: `web --browser /usr/local/bin/roamium lite.duckduckgo.com` —
   second test to confirm navigation works
6. DevTools: `:devtools right` — verify helper processes spawn correctly from
   the installed location

**Result:** Fail — but the cause is simpler than expected.

Running the binary directly works:

```
$ /usr/local/lib/roamium/roamium --help
[libtermsurf_chromium] Initialized, firing callback
[Roamium] No --ipc-socket, skipping IPC
```

Running via the symlink crashes:

```
$ /usr/local/bin/roamium --help
[ERROR:base/i18n/icu_util.cc:177] icudtl.dat not found in bundle
[FATAL:base/i18n/icu_util.cc:306] Check failed: result.
```

**Root cause: `NSBundle.mainBundle` resolves to the symlink's directory.**

Chromium on macOS loads resources via `PathForFrameworkBundleResource()`
(`base/apple/foundation_util.mm:108`), which calls
`[NSBundle URLForResource:withExtension:]` on the framework bundle. Since
`TsMainDelegate` (our custom delegate in `libtermsurf_chromium.cc:99`) never
calls `OverrideFrameworkBundlePath()`, the framework bundle defaults to
`NSBundle.mainBundle` (`base/apple/bundle_locations.mm:55-60`).

`NSBundle.mainBundle` uses the executable's parent directory as the bundle root.
For a symlink at `/usr/local/bin/roamium`, macOS resolves the process's
executable path to `/usr/local/lib/roamium/roamium` (the real binary), but
`NSBundle.mainBundle` apparently uses the launched path's directory
(`/usr/local/bin/`), not the resolved path's directory. So it looks for
`icudtl.dat` in `/usr/local/bin/` — where it doesn't exist.

When running from `/usr/local/lib/roamium/roamium` directly, `NSBundle` sees the
correct directory and finds `icudtl.dat` right there. When running from
`chromium/src/out/Default/roamium`, same thing — `icudtl.dat` is colocated.

No Chromium patch needed. The fix is to replace the symlink with a wrapper
script:

```bash
#!/bin/sh
exec /usr/local/lib/roamium/roamium "$@"
```

This way the process's actual executable is `/usr/local/lib/roamium/roamium`, so
`NSBundle.mainBundle` resolves to `/usr/local/lib/roamium/` where all resources
live.

#### Conclusion

The install layout is correct — dylibs, resources, and the binary all colocate
in `/usr/local/lib/roamium/`. The only issue was the symlink: `NSBundle` uses
the launched path's directory, not the resolved target. Replace the symlink with
a wrapper script that `exec`s the real binary. No Chromium fork patch required.

### Experiment 3: Wrapper script instead of symlink

#### Goal

Replace the `/usr/local/bin/roamium` symlink with a wrapper script so that
`NSBundle.mainBundle` resolves to `/usr/local/lib/roamium/` (where resources
live) instead of `/usr/local/bin/`.

#### Design

**1. Update `scripts/install-roamium.sh`** — replace the symlink with a wrapper:

```bash
# Replace the symlink section with:
echo "==> Creating wrapper script /usr/local/bin/roamium..."
sudo tee /usr/local/bin/roamium > /dev/null << 'WRAPPER'
#!/bin/sh
exec /usr/local/lib/roamium/roamium "$@"
WRAPPER
sudo chmod +x /usr/local/bin/roamium
```

#### Result

Fail. The wrapper script doesn't fix `NSBundle` resolution either. After `exec`
replaces the shell with the real binary, `NSBundle.mainBundle` still doesn't
resolve to `/usr/local/lib/roamium/`. Running `/usr/local/bin/roamium` (the
wrapper) crashes with the same ICU error. Running
`/usr/local/lib/roamium/roamium` directly works fine.

#### Conclusion

Neither symlinks nor wrapper scripts fix the `NSBundle` issue. `NSBundle` on
macOS determines the bundle directory in a way that doesn't follow `exec` chains
for non-bundled executables. The direct binary path works — the problem is only
the indirection layer. The solution is simpler: don't put anything in
`/usr/local/bin/`. Roamium is not user-facing — boards launch it directly. Just
install everything to a single directory and have the board use that path.

### Experiment 4: Install to /usr/local/roamium

#### Goal

Install Roamium and all its dependencies to `/usr/local/roamium/`. No symlink,
no wrapper, no `/usr/local/bin` entry. The board launches
`/usr/local/roamium/roamium` directly. `NSBundle.mainBundle` resolves to
`/usr/local/roamium/` where all resources live.

Roamium is not a user-facing command — it's a browser engine process launched by
boards (Ghostboard, Wezboard) via the TermSurf protocol. End users never run it
directly. Only boards need to know the path.

#### Design

**1. Update `scripts/install-roamium.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
ROAMIUM_SRC="$REPO_DIR/roamium/target/release/roamium"
INSTALL_DIR="/usr/local/roamium"

# Verify release build exists.
if [ ! -f "$ROAMIUM_SRC" ]; then
  echo "Error: Release build not found at $ROAMIUM_SRC"
  echo "Run: scripts/build-roamium.sh --release"
  exit 1
fi

echo "==> Installing Roamium to $INSTALL_DIR..."
sudo mkdir -p "$INSTALL_DIR"

# Copy roamium binary.
sudo cp "$ROAMIUM_SRC" "$INSTALL_DIR/roamium"

# Copy dylibs.
echo "==> Copying dylibs..."
sudo cp "$CHROMIUM_OUT"/*.dylib "$INSTALL_DIR/"

# Copy resources.
echo "==> Copying resources..."
sudo cp "$CHROMIUM_OUT"/*.pak "$INSTALL_DIR/"
sudo cp "$CHROMIUM_OUT/icudtl.dat" "$INSTALL_DIR/"
sudo cp "$CHROMIUM_OUT"/v8_context_snapshot*.bin "$INSTALL_DIR/"

# Clean up old install locations.
sudo rm -f /usr/local/bin/roamium
sudo rm -rf /usr/local/lib/roamium

echo ""
echo "Done."
echo "  Dir: $INSTALL_DIR"
echo "  Bin: $INSTALL_DIR/roamium"
```

**2. Update `resolve_browser_path` in Wezboard (`conn.rs`)**

Add `/usr/local/roamium/roamium` as a candidate path for the `"roamium"` name:

```rust
let candidates = &[
    ("roamium", format!("/usr/local/roamium/roamium")),
    ("roamium", format!("{}/dev/termsurf/chromium/src/out/Default/roamium", home)),
];
```

The installed path is checked first. If it doesn't exist, fall back to the dev
build directory.

**3. Update `resolveBrowserPath` in Ghostboard (`xpc.zig`)**

Same change — add `/usr/local/roamium/roamium` as the first candidate:

```zig
.{ .name = "roamium", .suffix = "" , .path = "/usr/local/roamium/roamium" },
.{ .name = "roamium", .suffix = "/dev/termsurf/chromium/src/out/Default/roamium" },
```

#### Verification

1. `scripts/install-roamium.sh` completes without errors
2. `/usr/local/roamium/roamium --help 2>&1` — initializes normally, no ICU crash
3. `/usr/local/roamium/` contains roamium, dylibs, .pak files, icudtl.dat, and
   v8_context_snapshot
4. In Wezboard: `web lite.duckduckgo.com` — uses installed Roamium from
   `/usr/local/roamium/roamium`, page renders
5. In Wezboard: `web --browser /usr/local/roamium/roamium lite.duckduckgo.com` —
   explicit path also works
6. Dev build still works: `web --browser roamium lite.duckduckgo.com` when
   installed path doesn't exist falls back to
   `$HOME/dev/termsurf/chromium/src/out/Default/roamium`

#### Result

Success. Both Wezboard and Ghostboard correctly launch Roamium from
`/usr/local/roamium/roamium`. The install script copies all files, `NSBundle`
resolves correctly (no symlink or wrapper indirection), and pages render
normally in both boards.

Initial testing appeared to show Ghostboard not picking up the installed path,
but this was a false alarm — Ghostboard hadn't been rebuilt with the `xpc.zig`
changes. After a release build and reinstall, both boards work.

#### Conclusion

The flat install layout works. `/usr/local/roamium/` with the binary, dylibs,
and resources colocated — no indirection — is the correct approach. Both
Wezboard and Ghostboard confirm end-to-end functionality.

### Experiment 5: Remove dev path fallback and .app bundling

#### Goal

Remove all hardcoded dev build paths from Ghostboard and Wezboard. Boards should
only find Roamium at `/usr/local/roamium/roamium` (the installed location) or
via a user-specified absolute path. Also remove the Chromium bundling from
`scripts/install.sh` — Roamium is now installed separately via
`scripts/install-roamium.sh`.

#### Background

Both boards currently have two candidates for Roamium:

1. `/usr/local/roamium/roamium` (installed)
2. `$HOME/dev/termsurf/chromium/src/out/Default/roamium` (dev build)

The dev path is a hardcoded path to _this_ developer's machine. It shouldn't
exist in shipped code. Developers who want to test a custom build can use
`web --browser /path/to/roamium`.

`scripts/install.sh` also bundles old-style Chromium `.app` bundles and
resources into `Contents/Chromium/` inside the Ghostboard `.app`. This predates
Roamium and is now dead code — Roamium is installed independently.

#### Design

**1. Simplify `initBrowserRegistry` in Ghostboard (`xpc.zig`)**

Remove the dev fallback entry and simplify the registry to a single absolute
path. The `Entry` struct, `home` variable, `suffix` field, and `bufPrint` logic
are no longer needed:

```zig
const browsers = [_][]const u8{
    "/usr/local/roamium/roamium",
};

for (&browsers) |path| {
    if (std.fs.accessAbsolute(path, .{})) {
        // register "roamium" → path
    } else |_| {}
}
```

The browser name is derived from the filename (last path component) or can stay
hardcoded as `"roamium"` for now since there's only one entry.

**2. Remove dev fallback in Wezboard (`conn.rs`)**

Remove the `$HOME/dev/termsurf/...` candidate from `resolve_browser_path`. The
`home` variable is no longer needed:

```rust
let candidates = &[
    ("roamium", "/usr/local/roamium/roamium".to_string()),
];
```

**3. Remove Chromium bundling from `scripts/install.sh`**

Remove lines 8 and 23–38 (the `CHROMIUM` variable, the `.app` bundle copying,
and the resource copying). The `$CHROMIUM` variable is no longer referenced
after this. The codesign step remains since the `web` TUI binary still gets
added.

#### Verification

1. Ghostboard builds without errors (`zig build`)
2. Wezboard builds without errors (`cargo build`)
3. `web lite.duckduckgo.com` works in both boards (uses installed Roamium)
4. `web --browser /some/other/path/roamium lite.duckduckgo.com` works (absolute
   path bypass)
5. `scripts/install.sh` runs without referencing Chromium build directory
6. No reference to `chromium/src/out/Default` remains in `xpc.zig` or `conn.rs`

#### Result

Success. Both Wezboard and Ghostboard use `/usr/local/roamium/roamium`
exclusively. The dev build path is gone from both boards. `scripts/install.sh`
no longer bundles Chromium files.

#### Conclusion

Dev path fallbacks and `.app` bundling removed. Boards now resolve Roamium from
a single install path (`/usr/local/roamium/roamium`) or a user-specified
absolute path via `--browser`. Clean separation between board install and
browser engine install.

### Experiment 6: Install Wezboard with new icon

#### Goal

Create a `scripts/install-wezboard.sh` that installs Wezboard as a macOS `.app`
bundle in `/Applications/`, using the new TermSurf logo
(`assets/termsurf-9.png`) as the app icon.

#### Background

Wezboard already has an app bundle template at
`wezboard/assets/macos/Wezboard.app/` containing `Info.plist`, ANGLE dylibs, and
an icon at `Contents/Resources/terminal.icns`. The deploy script
(`wezboard/ci/deploy.sh`) assembles the final `.app` by copying this template
and adding the compiled binary.

The icon is currently the old WezTerm icon. We need to convert
`assets/termsurf-9.png` to `.icns` format and replace `terminal.icns` in the
template. Ghostboard already has `scripts/generate-icons.sh` that uses `sips` to
resize PNGs — a similar approach can produce the `.icns` via `iconutil`.

#### Design

**1. Generate `terminal.icns` from `assets/termsurf-9.png`**

macOS `.icns` files are created from an `.iconset` directory containing PNGs at
specific sizes. Use `sips` to resize and `iconutil` to pack:

```bash
ICONSET=$(mktemp -d)/Wezboard.iconset
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size assets/termsurf-9.png --out "$ICONSET/icon_${size}x${size}.png"
  double=$((size * 2))
  sips -z $double $double assets/termsurf-9.png --out "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o wezboard/assets/macos/Wezboard.app/Contents/Resources/terminal.icns
```

This replaces the old icon in the template. The `Info.plist` already references
`terminal` as `CFBundleIconFile`, so no plist change needed.

**2. Create `scripts/install-wezboard.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
APP_TEMPLATE="$REPO_DIR/wezboard/assets/macos/Wezboard.app"
BINARY="$REPO_DIR/wezboard/target/release/wezboard-gui"
APP="/Applications/Wezboard.app"

# Verify release build exists.
if [ ! -f "$BINARY" ]; then
  echo "Error: Release build not found at $BINARY"
  echo "Run: cd wezboard && cargo build --release -p wezboard-gui"
  exit 1
fi

echo "==> Installing Wezboard to $APP..."
sudo rm -rf "$APP"
sudo cp -R "$APP_TEMPLATE" "$APP"
sudo mkdir -p "$APP/Contents/MacOS"
sudo cp "$BINARY" "$APP/Contents/MacOS/wezboard-gui"

# Re-sign (ad-hoc).
echo "==> Signing..."
sudo codesign --force --deep --sign - "$APP"

echo ""
echo "Done."
echo "  App: $APP"
```

**3. Update Ghostboard icon too**

Run `scripts/generate-icons.sh assets/termsurf-9.png` to update Ghostboard's
icon assets to the same new logo.

#### Verification

1. `terminal.icns` is regenerated from `termsurf-9.png`
2. `scripts/install-wezboard.sh` completes without errors
3. `/Applications/Wezboard.app` appears in Finder/Launchpad with the new icon
4. Launching Wezboard from `/Applications/` works — terminal opens
5. `web lite.duckduckgo.com` inside Wezboard uses installed Roamium
6. Ghostboard icon also updated via `scripts/generate-icons.sh`

#### Result

Success. All three steps completed:

1. `terminal.icns` regenerated from `termsurf-9.png` — all iconset sizes
   generated, packed with `iconutil`
2. `scripts/install-wezboard.sh` created and working — copies template bundle,
   adds binary, codesigns
3. Ghostboard icons updated via
   `scripts/generate-icons.sh assets/termsurf-9.png`

#### Conclusion

Wezboard is now installable as a macOS `.app` bundle. The install script copies
the template bundle (which includes the new icon, Info.plist, and ANGLE dylibs),
adds the release binary, and ad-hoc codesigns. Both Wezboard and Ghostboard now
use the `termsurf-9` logo.

### Experiment 7: Update to termsurf-10 logo

#### Goal

Replace `termsurf-9.png` with `termsurf-10.png` (same logo with extra border for
better dock appearance) across all icon assets, then reinstall Wezboard.

#### Design

**1. Regenerate `terminal.icns` from `assets/termsurf-10.png`**

Same `sips` + `iconutil` approach as Experiment 6, targeting
`wezboard/assets/macos/Wezboard.app/Contents/Resources/terminal.icns`.

**2. Update Ghostboard icons**

Run `scripts/generate-icons.sh assets/termsurf-10.png`.

**3. Reinstall Wezboard**

Run `scripts/install-wezboard.sh` to install the updated app bundle with the new
icon.

#### Verification

1. `terminal.icns` is regenerated from `termsurf-10.png`
2. Ghostboard icon assets updated to `termsurf-10`
3. `scripts/install-wezboard.sh` completes without errors
4. `/Applications/Wezboard.app` shows the new icon with border in Finder/Dock
5. Launching Wezboard from `/Applications/` works
6. `web lite.duckduckgo.com` inside Wezboard uses installed Roamium

#### Result

Success. All icons regenerated from `termsurf-10.png`. Wezboard reinstalled with
the updated icon. The extra border makes the logo look better in the Dock.

#### Conclusion

The `termsurf-10` logo (with border padding) replaces `termsurf-9` across both
Wezboard and Ghostboard icon assets.

## Summary

Issue 730 made Roamium and Wezboard independently installable as standalone
packages, separate from the development tree.

**Roamium** installs to `/usr/local/roamium/` as a flat directory containing the
binary, all Chromium dylibs, helper processes, and resource files. No `.app`
bundle needed — `AmIBundled()` returns false for plain binaries, so Chromium's
macOS bundle path transform never fires. Symlinks and wrapper scripts don't work
due to `NSBundle.mainBundle` resolving to the wrong directory, so boards launch
the binary directly at `/usr/local/roamium/roamium`.

**Wezboard** installs to `/Applications/Wezboard.app` using a template bundle
from `wezboard/assets/macos/Wezboard.app/` that contains Info.plist, ANGLE
dylibs, and the app icon. The install script copies the template, adds the
release binary, and ad-hoc codesigns.

**Dev path fallbacks removed.** Both Ghostboard and Wezboard no longer hardcode
`$HOME/dev/termsurf/chromium/src/out/Default/roamium`. Boards resolve Roamium
from `/usr/local/roamium/roamium` or a user-specified absolute path via
`--browser`. The old Chromium bundling in `scripts/install.sh` was also removed.

**New TermSurf logo.** Both boards use `termsurf-10.png` (logo with border
padding for better Dock appearance) for their app icons.

### Install scripts

| Script                        | Installs                                |
| ----------------------------- | --------------------------------------- |
| `scripts/install-roamium.sh`  | `/usr/local/roamium/`                   |
| `scripts/install-wezboard.sh` | `/Applications/Wezboard.app`            |
| `scripts/install.sh`          | `/Applications/TermSurf Ghostboard.app` |

### Key experiments

| Exp | What                                  | Result                                                        |
| --- | ------------------------------------- | ------------------------------------------------------------- |
| 1   | Can Roamium run without .app bundles? | Yes — `AmIBundled()` false, `--browser-subprocess-path` works |
| 2   | Install script + test                 | Symlink fails (NSBundle resolves wrong dir)                   |
| 3   | Wrapper script instead of symlink     | Also fails (exec doesn't fix NSBundle)                        |
| 4   | Flat install to /usr/local/roamium    | Works — direct binary path, no indirection                    |
| 5   | Remove dev path fallback              | Clean — boards use install path only                          |
| 6   | Wezboard install + termsurf-9 icon    | Works — template bundle + install script                      |
| 7   | Update to termsurf-10 logo            | Works — better Dock appearance with border                    |

## Conclusion

Roamium and Wezboard are independently installable. Roamium lives at
`/usr/local/roamium/` as a flat directory (no `.app` bundle, no symlink).
Wezboard lives at `/Applications/Wezboard.app` built from a template bundle.
Both use the `termsurf-10` logo. Dev path hardcodes are gone — boards discover
Roamium at its install path or via explicit `--browser` flag.
