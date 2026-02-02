# TermSurf

A terminal emulator with an integrated web browser. Type `web google.com` in your
terminal and a webpage renders directly in the terminal pane.

Built on [WezTerm](https://wezterm.org/) with [CEF](https://bitbucket.org/chromiumembedded/cef/)
(Chromium Embedded Framework) for browser rendering.

## Features

- **Integrated browser**: Run `web <url>` to open a webpage in the current pane
- **Multiple profiles**: Each browser profile runs in its own process with isolated
  cookies, storage, and cache (like Chrome profiles)
- **Browser navigation**: Cmd+[ (back), Cmd+] (forward), Cmd+R (reload),
  Cmd+Shift+R (hard reload)
- **Two modes**: Browse mode (keys go to webpage) and Control mode (Ctrl+C to toggle)
- **GPU-accelerated**: WebGPU rendering with IOSurface texture sharing

## Current Status

| Feature                       | Status      |
| ----------------------------- | ----------- |
| Single webview per profile    | Working     |
| Multiple browser profiles     | Working     |
| Browser navigation            | Working     |
| Browser refresh               | Working     |
| Profile path isolation        | Working     |
| Dynamic initial pane sizing   | Working     |
| Multi-webview per profile     | Not started |
| Dynamic resize on pane change | Not started |
| Input forwarding (full)       | Not started |

## Build Prerequisites

- macOS (currently macOS-only due to XPC/IOSurface)
- Rust toolchain
- Xcode command line tools

## Building

```bash
# Debug build
./scripts/build-debug.sh [--open] [--clean]

# Release build
./scripts/build-release.sh [--open] [--clean]
```

Flags:
- `--open`: Run the app after building
- `--clean`: Clear build caches first

## Usage

```bash
# Open a webpage in the current pane
web google.com

# Browser shortcuts (in Browse mode)
Cmd+[        # Go back
Cmd+]        # Go forward
Cmd+R        # Reload
Cmd+Shift+R  # Hard reload (bypass cache)

# Mode switching
Ctrl+C       # Toggle between Browse and Control mode
```

## Architecture

```
web command → GUI → XPC Launcher → Profile Server (CEF) → IOSurface → GPU render
```

Each browser profile runs in a separate `termsurf-profile` process. The GUI
communicates with profile servers via XPC, receiving rendered frames as IOSurface
Mach ports for zero-copy GPU compositing.

## Logs

Debug logs are written to `/tmp/`:
- `/tmp/termsurf-gui.log` — GUI process
- `/tmp/termsurf-launcher.log` — XPC launcher
- `/tmp/termsurf-profile-*.log` — Profile servers

## Credits

- [WezTerm](https://wezterm.org/) by [@wez](https://github.com/wez) — Terminal emulator foundation
- [CEF](https://bitbucket.org/chromiumembedded/cef/) — Chromium Embedded Framework
