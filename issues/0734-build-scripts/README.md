+++
status = "closed"
opened = "2026-03-10"
closed = "2026-03-10"
+++

# Issue 734: Consistent build and install scripts

## Goal

Replace the inconsistent collection of build and install scripts with a uniform
CLI that can build, install, and uninstall each component independently (debug
or release), or all together.

## Background

The current `scripts/` directory has grown organically. Each component got its
own script at the time it was added, with no shared conventions:

| Script                | Builds | Installs | Uninstalls | Debug/Release |
| --------------------- | ------ | -------- | ---------- | ------------- |
| `build-debug.sh`      | All    | —        | —          | Debug only    |
| `build-release.sh`    | All    | —        | —          | Release only  |
| `build-roamium.sh`    | Roam   | —        | —          | Either        |
| `install.sh`          | —      | Ghost    | —          | Release only  |
| `install-roamium.sh`  | —      | Roam     | —          | Release only  |
| `install-wezboard.sh` | —      | Wez      | —          | Release only  |

### Problems

1. **No per-component builds.** `build-debug.sh` and `build-release.sh` build
   everything — Ghostboard, Chromium, webtui, and Roamium — with no way to build
   just one. `build-roamium.sh` exists as a one-off exception.
2. **No uninstall.** There is no way to remove installed components. Install
   scripts overwrite previous installs, but leave symlinks, directories, and
   Launch Services registrations behind.
3. **Duplicate logic.** `build-debug.sh` and `build-release.sh` are nearly
   identical (97 lines each), differing only in optimization flags and output
   paths.
4. **Inconsistent naming.** `install.sh` installs Ghostboard but the name
   doesn't say so. `build-roamium.sh` exists but `build-ghostboard.sh` doesn't.
5. **No Wezboard build script.** `install-wezboard.sh` exists but there's no
   corresponding build script.
6. **Mixed concerns.** The monolithic build scripts handle Chromium, Zig, and
   Rust builds in one file, making it impossible to iterate on one component.

### Desired interface

Individual component scripts with a consistent pattern:

```
scripts/build.sh <component> [--release] [--clean] [--open]
scripts/install.sh <component>
scripts/uninstall.sh <component>
```

Where `<component>` is one of: `ghostboard`, `wezboard`, `roamium`, `webtui`,
`chromium`, or `all`.

- `build.sh ghostboard` — debug build of Ghostboard
- `build.sh ghostboard --release` — release build of Ghostboard
- `build.sh all --release --clean` — clean release build of everything
- `install.sh roamium` — install Roamium to system location
- `install.sh all` — install all components
- `uninstall.sh ghostboard` — remove Ghostboard from system
- `uninstall.sh all` — remove all installed components

### Install locations

| Component  | Install location                        | Symlinks                  |
| ---------- | --------------------------------------- | ------------------------- |
| Ghostboard | `/Applications/TermSurf Ghostboard.app` | `/usr/local/bin/termsurf` |
| Wezboard   | `/Applications/Wezboard.app`            | —                         |
| Roamium    | `/usr/local/roamium/`                   | —                         |
| webtui     | Bundled inside Ghostboard app           | `/usr/local/bin/web`      |
| Chromium   | Not installed separately                | —                         |

### Scripts to keep unchanged

These scripts are unrelated to build/install and should not be touched:

- `clean-zig.sh` — Zig-specific cache cleanup
- `generate-icons.sh` — icon asset generation
- `nerd-font-test.sh` — font verification
- `rename-ghostty.sh` — upstream merge rename
- `rename-wezterm.sh` — upstream merge rename

### Scripts to replace

These scripts will be replaced by the new `build.sh`, `install.sh`, and
`uninstall.sh`:

- `build-debug.sh`
- `build-release.sh`
- `build-roamium.sh`
- `install.sh` (current Ghostboard-only installer)
- `install-roamium.sh`
- `install-wezboard.sh`

## Experiments

### Experiment 1: Unified build.sh, install.sh, and uninstall.sh

#### Description

Create three new scripts that replace the six existing ones. Each takes a
component name as the first argument. `build.sh` accepts `--release`, `--clean`,
and `--open` flags. Delete the old scripts after the new ones are verified.

#### Changes

**1. `scripts/build.sh`** — new file

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_SRC="$REPO_DIR/chromium/src"
CHROMIUM_OUT="$CHROMIUM_SRC/out/Default"
CHROMIUM_PROTOC="$CHROMIUM_OUT/protoc"

RELEASE=false
CLEAN=false
OPEN=false
COMPONENT=""

for arg in "$@"; do
  case "$arg" in
    --release) RELEASE=true ;;
    --clean)   CLEAN=true ;;
    --open)    OPEN=true ;;
    -*)
      echo "Unknown flag: $arg"
      echo "Usage: $0 <component> [--release] [--clean] [--open]"
      echo "Components: ghostboard, wezboard, roamium, webtui, chromium, all"
      exit 1
      ;;
    *)
      if [ -z "$COMPONENT" ]; then
        COMPONENT="$arg"
      else
        echo "Error: multiple components specified"
        exit 1
      fi
      ;;
  esac
done

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component> [--release] [--clean] [--open]"
  echo "Components: ghostboard, wezboard, roamium, webtui, chromium, all"
  exit 1
fi

# Export PROTOC from Chromium if available (needed by prost_build).
if [ -x "$CHROMIUM_PROTOC" ]; then
  export PROTOC="$CHROMIUM_PROTOC"
fi

build_ghostboard() {
  cd "$REPO_DIR/ghostboard"
  if $CLEAN; then
    echo "==> Cleaning Ghostboard..."
    rm -rf zig-out zig-cache macos/build/Debug macos/build/ReleaseLocal
  fi
  if $RELEASE; then
    echo "==> Building Ghostboard (release)..."
    zig build -Doptimize=ReleaseFast
    APP="$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app"
  else
    echo "==> Building Ghostboard (debug)..."
    zig build
    APP="$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard Debug.app"
  fi
  echo "  Ghostboard: $APP"
  if $OPEN; then
    echo "==> Opening $APP..."
    open "$APP"
  fi
}

build_chromium() {
  if [ ! -d "$CHROMIUM_SRC" ]; then
    echo "==> Skipping Chromium (chromium/src not found)"
    return
  fi
  export PATH="$REPO_DIR/chromium/depot_tools:$PATH"
  cd "$CHROMIUM_SRC"
  if $CLEAN; then
    echo "==> Cleaning Chromium..."
    gn clean out/Default
  fi
  echo "==> Building Chromium..."
  autoninja -C out/Default libtermsurf_chromium
  echo "  Chromium: $CHROMIUM_OUT"
}

build_webtui() {
  cd "$REPO_DIR/webtui"
  if $CLEAN; then
    echo "==> Cleaning webtui..."
    cargo clean
  fi
  if $RELEASE; then
    echo "==> Building webtui (release)..."
    cargo build --release
    echo "  webtui: $REPO_DIR/webtui/target/release/web"
  else
    echo "==> Building webtui (debug)..."
    cargo build
    echo "  webtui: $REPO_DIR/webtui/target/debug/web"
  fi
}

build_roamium() {
  cd "$REPO_DIR/roamium"
  if $CLEAN; then
    echo "==> Cleaning Roamium..."
    cargo clean
  fi
  if $RELEASE; then
    echo "==> Building Roamium (release)..."
    cargo build --release
    cp "$REPO_DIR/roamium/target/release/roamium" "$CHROMIUM_OUT/roamium"
  else
    echo "==> Building Roamium (debug)..."
    cargo build
    cp "$REPO_DIR/roamium/target/debug/roamium" "$CHROMIUM_OUT/roamium"
  fi
  echo "  Roamium: $CHROMIUM_OUT/roamium"
}

build_wezboard() {
  cd "$REPO_DIR/wezboard"
  if $CLEAN; then
    echo "==> Cleaning Wezboard..."
    cargo clean
  fi
  if $RELEASE; then
    echo "==> Building Wezboard (release)..."
    cargo build --release -p wezboard-gui
    echo "  Wezboard: $REPO_DIR/wezboard/target/release/wezboard-gui"
  else
    echo "==> Building Wezboard (debug)..."
    cargo build -p wezboard-gui
    echo "  Wezboard: $REPO_DIR/wezboard/target/debug/wezboard-gui"
  fi
}

case "$COMPONENT" in
  ghostboard) build_ghostboard ;;
  chromium)   build_chromium ;;
  webtui)     build_webtui ;;
  roamium)    build_roamium ;;
  wezboard)   build_wezboard ;;
  all)
    build_ghostboard
    build_chromium
    build_webtui
    build_roamium
    build_wezboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: ghostboard, wezboard, roamium, webtui, chromium, all"
    exit 1
    ;;
esac
```

**2. `scripts/install.sh`** — new file (replaces current `install.sh`,
`install-roamium.sh`, `install-wezboard.sh`)

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
CHROMIUM_OUT="$REPO_DIR/chromium/src/out/Default"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: ghostboard, wezboard, roamium, webtui, all"
  exit 1
fi

install_ghostboard() {
  local APP="/Applications/TermSurf Ghostboard.app"
  local SRC="$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app"
  local WEB="$REPO_DIR/webtui/target/release/web"

  if [ ! -d "$SRC" ]; then
    echo "Error: Release build not found at $SRC"
    echo "Run: scripts/build.sh ghostboard --release"
    exit 1
  fi

  echo "==> Installing Ghostboard to $APP..."
  rm -rf "$APP"
  cp -R "$SRC" "$APP"

  # Bundle web TUI.
  if [ -f "$WEB" ]; then
    echo "==> Bundling web TUI..."
    cp "$WEB" "$APP/Contents/MacOS/web"
  else
    echo "Warning: web TUI not found at $WEB (skipping)"
  fi

  # Re-sign.
  echo "==> Codesigning..."
  codesign --force --deep --sign - "$APP"

  # Unregister build tree copies from Launch Services.
  echo "==> Unregistering build tree copies..."
  "$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard Debug.app" 2>/dev/null || true
  "$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/Debug/TermSurf Ghostboard.app" 2>/dev/null || true
  "$LSREGISTER" -u "$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf Ghostboard.app" 2>/dev/null || true

  # Symlinks.
  echo "==> Symlinking CLI tools..."
  ln -sf "$APP/Contents/MacOS/termsurf" /usr/local/bin/termsurf
  ln -sf "$APP/Contents/MacOS/web" /usr/local/bin/web

  echo "  App:  $APP"
  echo "  CLI:  /usr/local/bin/termsurf"
  echo "  Web:  /usr/local/bin/web"
}

install_roamium() {
  local ROAMIUM_SRC="$REPO_DIR/roamium/target/release/roamium"
  local INSTALL_DIR="/usr/local/roamium"

  if [ ! -f "$ROAMIUM_SRC" ]; then
    echo "Error: Release build not found at $ROAMIUM_SRC"
    echo "Run: scripts/build.sh roamium --release"
    exit 1
  fi

  echo "==> Installing Roamium to $INSTALL_DIR..."
  sudo mkdir -p "$INSTALL_DIR"
  sudo cp "$ROAMIUM_SRC" "$INSTALL_DIR/roamium"

  echo "==> Copying dylibs..."
  sudo cp "$CHROMIUM_OUT"/*.dylib "$INSTALL_DIR/"

  echo "==> Copying resources..."
  sudo cp "$CHROMIUM_OUT"/*.pak "$INSTALL_DIR/"
  sudo cp "$CHROMIUM_OUT/icudtl.dat" "$INSTALL_DIR/"
  sudo cp "$CHROMIUM_OUT"/v8_context_snapshot*.bin "$INSTALL_DIR/"

  # Clean up old install locations.
  sudo rm -f /usr/local/bin/roamium
  sudo rm -rf /usr/local/lib/roamium

  echo "  Dir: $INSTALL_DIR"
  echo "  Bin: $INSTALL_DIR/roamium"
}

install_wezboard() {
  local BINARY="$REPO_DIR/wezboard/target/release/wezboard-gui"
  local TEMPLATE="$REPO_DIR/wezboard/assets/macos/Wezboard.app"
  local APP="/Applications/Wezboard.app"

  if [ ! -f "$BINARY" ]; then
    echo "Error: Release build not found at $BINARY"
    echo "Run: scripts/build.sh wezboard --release"
    exit 1
  fi

  echo "==> Installing Wezboard to $APP..."
  sudo rm -rf "$APP"
  sudo cp -R "$TEMPLATE" "$APP"
  sudo mkdir -p "$APP/Contents/MacOS"
  sudo cp "$BINARY" "$APP/Contents/MacOS/wezboard-gui"

  echo "==> Codesigning..."
  sudo codesign --force --deep --sign - "$APP"

  echo "  App: $APP"
}

install_webtui() {
  echo "webtui is bundled inside Ghostboard during 'install.sh ghostboard'."
  echo "To install standalone, run: scripts/install.sh ghostboard"
}

case "$COMPONENT" in
  ghostboard) install_ghostboard ;;
  roamium)    install_roamium ;;
  wezboard)   install_wezboard ;;
  webtui)     install_webtui ;;
  all)
    install_ghostboard
    install_roamium
    install_wezboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: ghostboard, wezboard, roamium, webtui, all"
    exit 1
    ;;
esac
```

**3. `scripts/uninstall.sh`** — new file

```bash
#!/usr/bin/env bash
set -euo pipefail

COMPONENT="${1:-}"

if [ -z "$COMPONENT" ]; then
  echo "Usage: $0 <component>"
  echo "Components: ghostboard, wezboard, roamium, webtui, all"
  exit 1
fi

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"

uninstall_ghostboard() {
  local APP="/Applications/TermSurf Ghostboard.app"

  echo "==> Uninstalling Ghostboard..."
  rm -rf "$APP"
  rm -f /usr/local/bin/termsurf
  rm -f /usr/local/bin/web
  "$LSREGISTER" -u "$APP" 2>/dev/null || true

  echo "  Removed: $APP"
  echo "  Removed: /usr/local/bin/termsurf"
  echo "  Removed: /usr/local/bin/web"
}

uninstall_roamium() {
  echo "==> Uninstalling Roamium..."
  sudo rm -rf /usr/local/roamium
  sudo rm -f /usr/local/bin/roamium
  sudo rm -rf /usr/local/lib/roamium

  echo "  Removed: /usr/local/roamium"
}

uninstall_wezboard() {
  local APP="/Applications/Wezboard.app"

  echo "==> Uninstalling Wezboard..."
  sudo rm -rf "$APP"

  echo "  Removed: $APP"
}

uninstall_webtui() {
  echo "==> Uninstalling webtui..."
  rm -f /usr/local/bin/web

  echo "  Removed: /usr/local/bin/web"
}

case "$COMPONENT" in
  ghostboard) uninstall_ghostboard ;;
  roamium)    uninstall_roamium ;;
  wezboard)   uninstall_wezboard ;;
  webtui)     uninstall_webtui ;;
  all)
    uninstall_ghostboard
    uninstall_roamium
    uninstall_wezboard
    echo ""
    echo "Done (all)."
    ;;
  *)
    echo "Unknown component: $COMPONENT"
    echo "Components: ghostboard, wezboard, roamium, webtui, all"
    exit 1
    ;;
esac
```

**4. Delete old scripts**

- `scripts/build-debug.sh`
- `scripts/build-release.sh`
- `scripts/build-roamium.sh`
- `scripts/install.sh` (old Ghostboard-only)
- `scripts/install-roamium.sh`
- `scripts/install-wezboard.sh`

**5. `CLAUDE.md`** — update the scripts table

Replace the current scripts table with:

| Script                                                   | Purpose                                                                              |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `scripts/build.sh <comp> [--release] [--clean] [--open]` | Build a component. Components: ghostboard, wezboard, roamium, webtui, chromium, all. |
| `scripts/install.sh <comp>`                              | Install a component. Components: ghostboard, wezboard, roamium, webtui, all.         |
| `scripts/uninstall.sh <comp>`                            | Uninstall a component. Components: ghostboard, wezboard, roamium, webtui, all.       |
| `scripts/clean-zig.sh`                                   | Clean Zig build artifacts + Xcode DerivedData. Preserves Chromium cache.             |
| `scripts/rename-ghostty.sh [dir]`                        | Rename all Ghostty references to TermSurf in `ghostboard/`. Re-runnable.             |
| `scripts/rename-wezterm.sh [dir]`                        | Rename all WezTerm references to Wezboard in `wezboard/`. Re-runnable.               |
| `scripts/generate-icons.sh [image]`                      | Generate app icon assets from a source image.                                        |
| `scripts/nerd-font-test.sh`                              | Print Nerd Font test glyphs for visual verification.                                 |

#### Verification

1. `scripts/build.sh ghostboard` — builds Ghostboard in debug mode
2. `scripts/build.sh roamium --release` — builds Roamium in release mode
3. `scripts/build.sh webtui` — builds webtui in debug mode
4. `scripts/build.sh all --release` — builds everything in release mode
5. Old scripts are deleted and no longer referenced

**Result:** Pass

All three scripts print correct usage when called with no args. Old scripts
(`build-debug.sh`, `build-release.sh`, `build-roamium.sh`, `install-roamium.sh`,
`install-wezboard.sh`) are deleted. The new `build.sh`, `install.sh`, and
`uninstall.sh` each take a component name as the first argument with consistent
dispatch. CLAUDE.md scripts table updated to reflect the new interface.

#### Conclusion

The six inconsistent scripts are replaced by three unified scripts with a
consistent `<script> <component> [flags]` interface. Each component can now be
built, installed, or uninstalled independently.

## Conclusion

The build and install script interface is now uniform. Three scripts —
`build.sh`, `install.sh`, `uninstall.sh` — replace six ad-hoc scripts. Every
component (ghostboard, wezboard, roamium, webtui, chromium) can be targeted
individually or together with `all`. Debug vs release is a single `--release`
flag. Uninstall support is new — previously there was no way to cleanly remove
installed components.
