# TermSurf

**A terminal that surfs.**

Type `web` and a full Chromium browser opens right in your terminal pane. No
window switching. No context loss. Just web.

```bash
web google.com
```

![TermSurf screenshot showing a browser pane alongside terminal panes](assets/screenshot.png)

## Why TermSurf?

You're deep in a terminal session. You need to check docs, hit an API, or log
into a dashboard. The traditional workflow: Cmd+Tab to browser, lose your place,
Cmd+Tab back. Repeat dozens of times a day.

TermSurf eliminates the context switch. Browser panes live alongside terminal
panes in the same window. You stay in flow.

## Profiles

Like Chrome, TermSurf supports isolated browser profiles. Each profile has its
own cookies, storage, and login sessions.

```bash
web google.com                      # Default profile
web --profile work slack.com        # Work profile (separate login)
web --profile personal github.com   # Personal profile (different account)
```

Run all three in the same terminal window. Each profile is completely isolated —
logging into Google in one profile doesn't affect the others.

## Features

- **Full Chromium** — Not a simplified renderer. Real DevTools, real JavaScript,
  real web. Embedded via the Content API (not CEF).
- **Profile isolation** — Separate cookies, sessions, and storage per profile.
- **60fps rendering** — Hardware-accelerated via Metal. GPU textures composited
  directly into the terminal pane.
- **Keyboard modes** — Browse mode for the web, Control mode for terminal
  keybindings.

## Getting Started

### Prerequisites (macOS)

- [Zig](https://ziglang.org/) (for building the terminal)
- [Rust](https://rustup.rs/) (for building the `web` TUI)

### Build

```bash
# Build the terminal
cd gui && zig build

# Build the web TUI
cargo build -p web
```

### Launch

```bash
open gui/zig-out/TermSurf.app
```

Then in a TermSurf terminal pane:

```bash
cargo run -p web -- https://google.com
```

## Keyboard Modes

The `web` TUI has two modes:

| Mode        | Behavior                                     |
| ----------- | -------------------------------------------- |
| **Browse**  | Keyboard/mouse goes to the browser (default) |
| **Control** | Terminal keybindings active                  |

| Key             | Action                 |
| --------------- | ---------------------- |
| Esc (Browse)    | Switch to Control mode |
| Enter (Control) | Switch to Browse mode  |
| q (Control)     | Quit                   |
| Ctrl+C (any)    | Force quit             |

## Status

TermSurf is in active development. The project has evolved through six
generations (ts1 through ts5, then gui). The current generation (`gui/`) forks
[Ghostty](https://ghostty.org/) as the terminal with all browser integration
logic in Zig.

**What works today:**

- Terminal emulator (full Ghostty, native Metal rendering)
- `web` TUI chrome (URL bar, viewport border, status bar via ratatui)
- Chromium streaming (real webpages render in terminal panes at 60fps)
- IOSurface overlay pipeline (zero-copy GPU texture compositing via Metal)
- Retina resolution and dynamic resize
- Multi-pane, multi-profile server reuse
- Mouse input forwarding (clicks, drag, scroll, cursor changes, text selection)
- Keyboard input forwarding (key events, Cmd+key bypass, clipboard)
- Browser navigation (Cmd+[ back, Cmd+] forward, Cmd+R reload)
- Loading progress indicator (indeterminate pulse during page load)
- XPC communication (Zig ↔ `web` TUI ↔ Chromium server)

**Not yet started:**

- In-process Chromium embedding (currently out-of-process streaming over XPC)

macOS only for now.

## Contributing

See [CLAUDE.md](./CLAUDE.md) for architecture details, build instructions, and
the full development guide.

## License

See individual component licenses in `gui/`, `ts5/`, `ts1/`, `ts3/`, and
`vendor/cef-rs/`.
