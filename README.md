# TermSurf

A terminal emulator with integrated browser panes.

## Project Structure

| Version | Directory | Base         | Browser                      | Status                 |
| ------- | --------- | ------------ | ---------------------------- | ---------------------- |
| **3.0** | `ts3/`    | WezTerm fork | CEF (out-of-process via XPC) | **Active development** |
| 2.0     | `ts2/`    | WezTerm fork | CEF (in-process)             | Superseded             |
| 1.x     | `ts1/`    | Ghostty fork | WKWebView                    | Legacy (macOS only)    |

```
termsurf/
├── ts3/           # TermSurf 3.0 (active)
├── ts2/           # TermSurf 2.0 (superseded)
├── ts1/           # TermSurf 1.x (legacy)
├── cef-rs/        # CEF Rust bindings
└── docs/issues/   # Documentation
```

## TermSurf 3.0

Cross-platform terminal emulator with browser panes. Each browser profile runs
in its own CEF process, enabling true session isolation (separate cookies,
storage, logins) like Chrome profiles.

### Prerequisites (macOS)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install build tools
brew install cmake ninja
```

### Quick Start

```bash
cd ts3 && ./scripts/build-debug.sh --open
```

The first build downloads CEF (~300MB) automatically.

Then in the terminal:

```bash
web google.com
```

### Architecture

```
User: web google.com
    │
    ▼
CLI ──Unix socket──► GUI (WezTerm)
                         │
                         ▼
                    XPC Manager
                         │
                         ▼
                Launcher (com.termsurf.launcher)
                         │
                         ▼
                Profile Server (one per profile)
                         │
                         ▼
                CEF off-screen render
                         │
                         ▼
                IOSurface ──Mach port──► GUI ──wgpu──► screen
```

### Implementation Status

| Category        | Feature                                    | Status  |
| --------------- | ------------------------------------------ | ------- |
| **Core**        | Webpage rendering via CEF                  | Working |
|                 | One process per profile                    | Working |
|                 | Multiple webviews per profile              | Working |
|                 | Profile data isolation                     | Working |
|                 | Cross-process texture sharing (Mach ports) | Working |
| **Resize**      | Initial pane sizing                        | Working |
|                 | Dynamic resize with debounce               | Working |
|                 | Half-cell boundary accuracy                | Working |
| **UI**          | Control panel with URL                     | Working |
|                 | Browse/Control mode switching              | Working |
|                 | Visual dimming (HSB from config)           | Working |
|                 | Multi-tab support                          | Working |
| **Keyboard**    | Typing in text fields                      | Working |
|                 | Arrow keys, Tab, Enter, Backspace          | Working |
|                 | Cmd+V (paste via JS injection)             | Working |
|                 | Cmd+C (copy)                               | Working |
|                 | Cmd+X (cut)                                | Working |
|                 | Cmd+A (select all)                         | Working |
|                 | Ctrl+C mode switching                      | Working |
| **Mouse**       | Click (links, buttons, forms)              | Working |
|                 | Double-click (word select)                 | Working |
|                 | Triple-click (line select)                 | Working |
|                 | Drag selection                             | Working |
|                 | Shift-click extend selection               | Working |
|                 | Scroll (trackpad, wheel)                   | Working |
|                 | Hover effects                              | Working |
|                 | Cursor feedback (hand, I-beam, arrow)      | Working |
| **Performance** | 60fps rendering                            | Working |
|                 | Graceful process shutdown                  | Working |
| **Not Started** | Multiple profiles (`--profile`)            | Planned |
|                 | Navigation (back, forward, reload)         | Planned |
|                 | DevTools                                   | Planned |
|                 | Loading indicators                         | Planned |

### Modes

TermSurf webviews have two modes:

- **Browse mode** (default): Browser receives input, control panel shows URL
- **Control mode**: Browser dimmed, control panel shows instructions

| Key              | Action                 |
| ---------------- | ---------------------- |
| Ctrl+C (Browse)  | Switch to Control mode |
| Enter (Control)  | Switch to Browse mode  |
| Ctrl+C (Control) | Close webview          |
| Cmd+C (Control)  | Copy URL to clipboard  |

### Logs

Debug logs written to `/tmp/`:

- `/tmp/termsurf-gui.log` — GUI process
- `/tmp/termsurf-launcher.log` — Launcher service
- `/tmp/termsurf-profile-*.log` — Profile servers

## TermSurf 1.x (Legacy)

macOS terminal with WKWebView browser panes. Still builds but no longer actively
developed.

```bash
cd ts1 && ./scripts/build-debug.sh --open
```

## Documentation

Issue documents in `docs/issues/`:

| Range | Version | Notes              |
| ----- | ------- | ------------------ |
| 3xx   | ts3     | Active development |
| 2xx   | ts2     | Historical         |
| 1xx   | ts1     | Legacy             |

Key documents:

- `docs/issues/301-architecture.md` — ts3 process model
- `docs/issues/303-xpc.md` — XPC and Mach port transfer
- `docs/issues/307-profile.md` — One-process-per-profile implementation
- `docs/issues/317-input.md` — Keyboard input forwarding
- `docs/issues/319-mouse.md` — Mouse input forwarding
- `CLAUDE.md` — Development guide for coding agents

## License

See individual component licenses in `ts1/`, `ts2/`, `ts3/`, and `cef-rs/`.
