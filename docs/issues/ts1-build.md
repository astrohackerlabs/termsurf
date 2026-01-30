# Build Guide (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x (Ghostty + WKWebView).
> TermSurf 2.0 build instructions will be documented separately once development begins.
> For 2.0, see the cef-rs build commands in [CLAUDE.md](../CLAUDE.md#cef-rs).

## Build Commands

TermSurf requires building two components: libghostty (Zig) and the macOS app
(Swift).

### Debug Build

```bash
# 1. Build libghostty (debug)
zig build

# 2. Build TermSurf.app (debug)
cd termsurf-macos && xcodebuild -project TermSurf.xcodeproj -scheme TermSurf -configuration Debug build
```

### Release Build

```bash
# 1. Build libghostty (release)
zig build -Doptimize=ReleaseFast

# 2. Build TermSurf.app (release)
cd termsurf-macos && xcodebuild -project TermSurf.xcodeproj -scheme TermSurf -configuration Release build
```

Or use the convenience script:

```bash
./scripts/build-release.sh         # Build release
./scripts/build-release.sh --clean # Clean build
./scripts/build-release.sh --open  # Build and open app
```

## Clean Build

To ensure a completely fresh build with no cached artifacts (both Zig and
Swift):

```bash
# 1. Clear SPM cache for Sparkle dependency
rm -rf ~/Library/Caches/org.swift.swiftpm/artifacts/*Sparkle*

# 2. Clear local build directory (release script output)
rm -rf build

# 3. Clear Xcode DerivedData (debug builds)
rm -rf ~/Library/Developer/Xcode/DerivedData/TermSurf-*

# 4. Clear Zig build cache
rm -rf zig-out zig-cache .zig-cache

# 5. Clear SPM package resolution in project
rm -rf termsurf-macos/.build
rm -rf termsurf-macos/Package.resolved
```

Then build:

```bash
./scripts/build-release.sh --open
```

## Nuclear Option

If you still have dependency issues, clear the entire SPM cache:

```bash
rm -rf ~/Library/Caches/org.swift.swiftpm
```

## What Each Cache Contains

| Cache           | Location                                           | Contents                                 |
| --------------- | -------------------------------------------------- | ---------------------------------------- |
| Zig build       | `zig-out`, `zig-cache`, `.zig-cache`               | Compiled Zig objects, libghostty         |
| Release build   | `build/`                                           | Release script output (predictable path) |
| DerivedData     | `~/Library/Developer/Xcode/DerivedData/TermSurf-*` | Debug builds from Xcode                  |
| SPM artifacts   | `~/Library/Caches/org.swift.swiftpm/artifacts/`    | Downloaded binary dependencies (Sparkle) |
| SPM packages    | `termsurf-macos/.build`                            | Resolved package versions                |

## CLI Access

The TermSurf app binary doubles as a CLI tool. You can run commands like
`termsurf +help`, `termsurf +list-fonts`, etc.

### Binary Locations

After building, the app bundle contains two executables:

- `termsurf` - Main binary
- `web` - Symlink to termsurf (multi-call binary for web commands)

| Build Type              | Location                                                                                                     |
| ----------------------- | ------------------------------------------------------------------------------------------------------------ |
| Debug (Xcode cmd+r)     | `~/Library/Developer/Xcode/DerivedData/TermSurf-*/Build/Products/Debug/TermSurf.app/Contents/MacOS/termsurf` |
| Release (build script)  | `<project>/build/Build/Products/Release/TermSurf.app/Contents/MacOS/termsurf`                                |
| Installed               | `/Applications/TermSurf.app/Contents/MacOS/termsurf`                                                         |

The release build script uses a predictable output path (`build/`) so you can
add it to your PATH.

### Adding to PATH

Add the release binary directory to your PATH in `~/.zshrc`:

```bash
path=(
  $HOME/dev/termsurf/build/Build/Products/Release/TermSurf.app/Contents/MacOS
  $path
)
```

### Shell Aliases (Alternative)

Or use aliases in your shell config (`~/.zshrc` or `~/.bashrc`):

```bash
# For development (Debug build via Xcode)
alias termsurf-dev='~/Library/Developer/Xcode/DerivedData/TermSurf-*/Build/Products/Debug/TermSurf.app/Contents/MacOS/termsurf'

# For release (predictable path)
alias termsurf='~/dev/termsurf/build/Build/Products/Release/TermSurf.app/Contents/MacOS/termsurf'
```

### Example Commands

```bash
termsurf +help           # Show available CLI actions
termsurf +version        # Show version info
termsurf +list-fonts     # List available fonts
termsurf +list-themes    # Browse themes interactively
termsurf +show-config    # Show current configuration

# Web commands (requires running inside TermSurf)
termsurf +web open https://example.com   # Open URL in browser pane
web open https://example.com             # Same, via multi-call binary
web ping                                 # Test connectivity to app
web bookmark list                        # List saved bookmarks
```
