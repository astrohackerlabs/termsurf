# Issue 504: Web TUI Chrome

## Background

The `web` command is how users open a browser inside TermSurf. The user types
`web google.com` in their terminal pane, and a webpage renders directly in that
pane. Previous generations (ts1, ts3) treated the browser as a full-pane overlay
that replaced the terminal content entirely. The terminal was invisible while
browsing.

This issue takes a different approach: **the terminal is the browser chrome.**
Instead of hiding the terminal, the `web` command launches a TUI (Terminal User
Interface) that wraps the browser viewport in a terminal-native frame. The TUI
provides the URL bar, status indicators, navigation controls, and keyboard
shortcuts — all rendered as terminal text with colors, box-drawing characters,
and animations. The browser content renders as a GPU texture in the center
region, but the surrounding chrome is pure TUI.

This reinforces TermSurf's identity as a keyboard-first browser controlled from
a terminal. The chrome isn't a traditional window titlebar — it's a terminal
interface with its own personality.

## What `web` Is

`web` is a standalone Rust CLI application at the top level of the repo. It uses
[ratatui](https://ratatui.rs/) for terminal UI rendering. When the user runs
`web <url>`, it:

1. Takes over the terminal pane with an alternate screen buffer.
2. Draws a TUI frame: a control panel (URL bar, status, keyboard hints) and a
   border around a central viewport area.
3. The viewport area is where the browser content will eventually render (via
   GPU texture compositing by TermSurf). For this issue, it remains empty.
4. Listens for keyboard input to handle navigation, URL editing, and TUI
   interactions.
5. On quit (e.g., `q` or `Ctrl+C`), restores the terminal to its previous state.

The `web` binary ships alongside the TermSurf application. It communicates with
the running TermSurf instance via IPC (mechanism TBD in a future issue) to
request browser rendering in the viewport region. For this issue, no IPC or
browser rendering is implemented — the focus is purely on the TUI chrome.

## Architecture

```
Terminal Pane (owned by TermSurf / Ghostty fork)
┌──────────────────────────────────────────────────┐
│  ┌─ URL ──────────────────────────────────────┐  │
│  │ https://google.com                         │  │
│  └────────────────────────────────────────────┘  │
│  ┌─ Viewport ─────────────────────────────────┐  │
│  │                                            │  │
│  │                                            │  │
│  │          (browser content here)            │  │
│  │          (empty for this issue)            │  │
│  │                                            │  │
│  │                                            │  │
│  └────────────────────────────────────────────┘  │
│  [b]ack  [f]orward  [r]eload  [q]uit    60fps    │
└──────────────────────────────────────────────────┘
```

The TUI chrome consists of:

- **URL bar** — Displays the current URL. Editable in a future issue (type a new
  URL and press Enter to navigate).
- **Viewport** — The central area reserved for browser content. Rendered as
  empty space (or a placeholder) in this issue. In a future issue, TermSurf will
  composite the Chromium GPU texture into this region.
- **Status bar** — Shows keyboard shortcuts, connection status, FPS, and other
  indicators. The exact layout will be refined through experiments.

### Technology

- **Language:** Rust
- **TUI framework:** [ratatui](https://ratatui.rs/) with
  [crossterm](https://docs.rs/crossterm/) as the backend
- **Location:** `/web/` at the repo root (a Cargo workspace member)
- **Binary name:** `web`

### Why Rust + ratatui

The `web` TUI needs rich terminal rendering: box-drawing characters, 256/true
color, animations, keyboard handling, and alternate screen management. Zig has
no TUI ecosystem. Rust's ratatui is the most mature terminal UI framework
available — it provides layout, widgets, styling, and event handling out of the
box. Rust also produces a single static binary with fast startup, suitable for a
tool users invoke frequently.

### Relationship to TermSurf

`web` runs **inside** a TermSurf terminal pane. It is a regular terminal
application — it writes escape sequences to stdout and reads keyboard input from
stdin, just like `vim` or `htop`. TermSurf (the Ghostty fork) renders the
terminal output as usual.

The key integration point (for a future issue): TermSurf will detect that `web`
is running and knows the viewport coordinates. It will composite the Chromium
browser texture on top of the viewport region in the GPU render pass. The TUI
chrome (URL bar, status bar, border) remains visible because it's outside the
viewport region. The mechanism for communicating viewport coordinates between
`web` and TermSurf is TBD.

## Goal

Build the TUI chrome for `web` — no browser, no IPC, no Chromium. Just the
frame:

1. `web <url>` launches a full-screen TUI with a URL bar, viewport placeholder,
   and status bar.
2. The URL from the command line is displayed in the URL bar.
3. Keyboard shortcuts are displayed in the status bar.
4. `q` or `Ctrl+C` exits cleanly, restoring the terminal.
5. The viewport area is visually distinct (empty or with a placeholder message)
   so it's clear where browser content will go.
6. The TUI renders correctly at different terminal sizes and handles resize
   events.

## Non-Goals (for this issue)

- Browser rendering (no Chromium, no WebContents, no IOSurface).
- IPC with TermSurf (no socket, no XPC, no viewport coordinate communication).
- URL editing (the URL bar displays the CLI argument but is not yet editable).
- Navigation commands (back, forward, reload are displayed but non-functional).
- Profile selection.
- Any interaction with the Chromium Profile Server.

## Prior Art

- **Issue 209** (`docs/issues/209-web.md`) — Original `web` command design for
  ts2/ts3. Defined `web open`, `web close`, console output streaming. The TUI
  chrome concept did not exist; the browser was a full-pane overlay.
- **ts1 `web.zig`** (`ts1/src/cli/web.zig`) — ts1's CLI web command in Zig. Sent
  IPC to the Ghostty fork to trigger WKWebView. No TUI chrome.
- **ts3 Unix socket approach** — CLI sent JSON over a Unix domain socket to the
  WezTerm GUI. Minimal CLI, no TUI.

## Directory Structure

```
web/
├── Cargo.toml
├── src/
│   └── main.rs
```

## Build

```bash
cd web && cargo build
# or
cargo build -p web
```

## Experiments
