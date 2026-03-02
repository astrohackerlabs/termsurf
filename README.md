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

## Features

### Browser Integration

- **Full Chromium** — Not a simplified renderer. Real DevTools, real JavaScript,
  real web. Embedded via the Content API (not CEF).
- **Zero-copy compositing** — CALayerHost lets Window Server composite directly
  from GPU VRAM. No per-frame IPC, no texture copies.
- **60fps Metal rendering** — Hardware-accelerated at Retina resolution.
- **Dynamic resize** — Browser pane resizes with window and splits.
- **Multi-pane** — Multiple browser panes in one window.
- **Profile isolation** — Separate cookies, sessions, and storage per profile.
- **Dark mode** — System color scheme forwarded to Chromium. Override with
  `:colorscheme dark|light|system`.
- **Chrome DevTools** — Open in a split pane with
  `:devtools right|left|up|down`.

### Mouse Input

- Click, drag, and scroll forwarded to browser
- Cursor changes (pointer, text, crosshair, etc.)
- Text selection
- Click-to-focus — clicking an unfocused pane activates it without passing the
  click through (macOS-style)

### Keyboard Input

- Full keyboard forwarding to Chromium in browse mode
- Cmd+key bypass — Cmd+C/V/A/X/Z go to browser, not terminal
- Clipboard integration

### Navigation

- URL bar with vim-style editing (edtui widget)
- Smart URL resolution — `web google.com`, `web ./file.html`, `web :3000`,
  `web devtools` all resolve correctly
- URL normalization — bare domains get `https://` prefix automatically
- `file://` support — `web file <path>` or `web ./path`
- Browser navigation: Cmd+[ (back), Cmd+] (forward), Cmd+R (reload)
- Loading progress indicator
- Page title display in viewport border
- Links open in same tab (no popups)
- Configurable homepage — `web` without args opens default page

### Vim-Style Modes

| Mode        | Behavior                                          |
| ----------- | ------------------------------------------------- |
| **Control** | Terminal keybindings active (default on startup)  |
| **Browse**  | Keyboard/mouse goes to the browser                |
| **Edit**    | Vim-style URL editing with Normal/Insert submodes |
| **Command** | `:` prefix for commands                           |

| Key    | Mode    | Action             |
| ------ | ------- | ------------------ |
| Esc    | Browse  | Switch to Control  |
| Enter  | Control | Switch to Browse   |
| i      | Control | Edit URL (insert)  |
| e      | Control | Edit URL (normal)  |
| :      | Control | Enter Command mode |
| q      | Control | Quit               |
| Ctrl+C | Any     | Force quit         |

Context-sensitive Esc exits the current mode appropriately. Per-mode color
indicators follow the LazyVim Tokyo Night palette.

### Commands

| Command                            | Action                      |
| ---------------------------------- | --------------------------- |
| `:q` / `:quit`                     | Quit                        |
| `:qa` / `:quitall`                 | Quit all panes              |
| `:devtools [direction]`            | Open DevTools in split pane |
| `:colorscheme dark\|light\|system` | Set color scheme            |

Vim-style subsequence matching — `:cs dark` works for `:colorscheme dark`.

### UI

- Active pane indicator with colored borders and background desaturation
- Inner padding so borders don't cover content
- Purple border in Edit mode
- Tight title spacing

### Terminal

Based on [Ghostty](https://ghostty.org/). All Ghostty features, configuration,
and keybindings work out of the box. TermSurf adds browser integration on top.

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

## Getting Started

### Prerequisites (macOS)

- [Zig](https://ziglang.org/) (for building the terminal)
- [Rust](https://rustup.rs/) (for building the `web` TUI)

### Build

```bash
# Build the terminal
cd gui && zig build

# Build the web TUI
cd tui && cargo build
```

### Launch

```bash
open gui/zig-out/TermSurf.app
```

Then in a TermSurf terminal pane:

```bash
cd tui && cargo run -- google.com
```

macOS only for now.

## Contributing

See [CLAUDE.md](./CLAUDE.md) for architecture details, build instructions, and
the full development guide.

## License

[MIT](./LICENSE). See [TRADEMARKS.md](./TRADEMARKS.md) for trademark policy.
