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

### Experiment 1: Scaffold and basic chrome

#### Goal

Create the Rust project, add ratatui + crossterm dependencies, and render the
basic TUI chrome layout: URL bar at the top, empty viewport in the middle,
status bar at the bottom. The TUI runs in the alternate screen buffer and exits
cleanly on `q` or `Ctrl+C`.

No animations, no colors beyond basic styling, no interactivity beyond quit.
Just prove the layout works and responds to terminal resize.

#### Setup

Create `web/` at the repo root:

```
web/
├── Cargo.toml
├── src/
│   └── main.rs
```

**`Cargo.toml` dependencies:**

- `ratatui` — TUI framework (widgets, layout, rendering)
- `crossterm` — terminal backend (raw mode, alternate screen, event polling)

**CLI:** `web <url>` takes a single positional argument (the URL). No flags, no
subcommands. Use `std::env::args` — no need for `clap` yet.

#### Layout

Three vertical sections using ratatui's `Layout::vertical` with constraints:

```
┌─────────────────────────────────────────────┐
│  https://google.com                         │  <- URL bar (1 line + border)
├─────────────────────────────────────────────┤
│                                             │
│                                             │
│              (viewport)                     │  <- Min(fill remaining space)
│                                             │
│                                             │
├─────────────────────────────────────────────┤
│  [q] quit                                   │  <- Status bar (1 line)
└─────────────────────────────────────────────┘
```

- **URL bar:** A `Block` with a border, containing a `Paragraph` with the URL
  from the CLI argument. Title: `" URL "`.
- **Viewport:** A `Block` with a border, containing a centered `Paragraph` with
  placeholder text like `"waiting for browser..."` in dark gray. Title:
  `" Viewport "`. Takes all remaining vertical space.
- **Status bar:** A single-line `Paragraph` with `"[q] quit"` in a muted style.
  No border.

#### Event Loop

Use crossterm's `event::poll` with a 250ms timeout for a simple blocking loop:

1. Enter raw mode, enable alternate screen.
2. Loop:
   - Poll for events (250ms timeout).
   - On `KeyCode::Char('q')` or `Ctrl+C`: break.
   - On `Resize`: redraw (ratatui handles this automatically on next `draw`).
   - Draw the frame.
3. On exit: leave alternate screen, disable raw mode.

No tick-based rendering needed — ratatui only redraws when `draw()` is called.

#### Build and Run

```bash
cd web && cargo run -- https://google.com
```

#### Pass Criteria

1. `cargo build` succeeds with no warnings.
2. `web https://google.com` enters alternate screen and shows the three-section
   layout.
3. The URL bar displays `https://google.com`.
4. The viewport shows placeholder text centered in the available space.
5. The status bar shows `[q] quit`.
6. Pressing `q` exits cleanly, restoring the terminal.
7. `Ctrl+C` also exits cleanly.
8. Resizing the terminal redraws the layout correctly.

#### Result

All pass criteria met. Layout renders correctly with URL bar, viewport
placeholder, and status bar. Resize works. `q` and `Ctrl+C` both exit cleanly.

### Experiment 2: Brighten the chrome

#### Goal

The URL bar border/title, viewport border/title, and status bar text all use
`Color::DarkGray` or the terminal default, making them too dim to read
comfortably. Increase the brightness of the chrome elements so they're clearly
visible while still being visually subordinate to the viewport area (which will
eventually contain browser content).

#### Changes

##### `web/src/main.rs`

**URL bar:** Set the border and title to `Color::Gray`. The URL text itself
stays at the default (white/foreground) — it's the most important piece of
information in the chrome.

**Viewport:** Keep the placeholder text at `Color::DarkGray` — this is temporary
and should be dim. Set the border and title to `Color::Gray` to match the URL
bar frame.

**Status bar:** Change from `Color::DarkGray` to `Color::Gray`. The key hints
(`[q]`) should be legible at a glance.

#### Pass Criteria

1. Chrome borders and titles are visibly brighter than before.
2. Status bar text is clearly readable.
3. Viewport placeholder text remains dim (it's temporary).
4. URL text is the brightest element in the chrome (default foreground).

#### Result

Builds with no warnings. Ready for visual inspection.

### Experiment 3: Control mode and browse mode

#### Goal

Add two input modes — **browse mode** and **control mode** — so the TUI can
distinguish between keypresses intended for the browser and keypresses intended
for the chrome. The current mode is displayed in the bottom-right of the status
bar.

This is the foundation for all future keyboard handling: in browse mode, keys go
to the browser; in control mode, keys control the TUI (quit, navigate, edit URL,
etc.).

#### Modes

**Browse mode** (default on launch):

- All keypresses will eventually be forwarded to the browser (not implemented in
  this experiment — there is no browser yet). For now, keypresses are simply
  ignored.
- `Esc` exits browse mode and enters control mode. In a future issue, `Esc` will
  be sent to the browser first; only if the browser doesn't consume it will it
  propagate to `web`. For this experiment, `Esc` always switches modes.
- `Ctrl+Esc` always exits browse mode, unconditionally. The browser never sees
  `Ctrl+Esc`. This is the guaranteed escape hatch for pages that trap `Esc`.

**Control mode:**

- `q` quits the application (same as before).
- `Ctrl+C` quits the application (same as before).
- `Enter` enters browse mode.
- Other keys are ignored for now (future experiments will add navigation, URL
  editing, etc.).

#### Changes

##### `web/src/main.rs`

**Add mode enum:**

```rust
enum Mode {
    Browse,
    Control,
}
```

**App state:** Track the current mode. Start in `Mode::Browse`.

**Event handling:** Branch on the current mode:

- In `Mode::Browse`:
  - `Esc` or `Ctrl+Esc` → switch to `Mode::Control`.
  - All other keys → ignore (future: forward to browser).
- In `Mode::Control`:
  - `q` → quit.
  - `Ctrl+C` → quit (works in both modes for safety).
  - `Enter` → switch to `Mode::Browse`.
  - All other keys → ignore.

Note: `Ctrl+C` should quit from **both** modes. It's the universal emergency
exit. A user pressing `Ctrl+C` always expects the program to stop.

**Status bar:** Split into left and right sections using a horizontal layout.
Left side shows key hints (mode-dependent). Right side shows the current mode
label.

- Browse mode left: `[esc] control mode`
- Browse mode right: `BROWSE`
- Control mode left: `[q] quit  [enter] browse`
- Control mode right: `CONTROL`

The mode label uses the same `Color::Gray` as the rest of the status bar.

#### Pass Criteria

1. Launches in browse mode. Status bar shows `BROWSE` on the right.
2. Pressing `q` in browse mode does NOT quit.
3. `Esc` switches to control mode. Status bar shows `CONTROL` on the right.
4. `Ctrl+Esc` also switches to control mode from browse mode.
5. In control mode, `q` quits cleanly.
6. In control mode, `Enter` switches back to browse mode.
7. `Ctrl+C` quits from either mode.
8. Status bar left side shows mode-appropriate key hints.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 4: Show Ctrl+Esc hint in browse mode

#### Goal

The browse mode status bar currently shows `[esc] control mode`, but `Ctrl+Esc`
is the guaranteed escape hatch — the one that always works, even when a webpage
traps `Esc`. Users need to know about it. Add `Ctrl+Esc` to the browse mode hint
so both options are visible.

#### Changes

##### `web/src/main.rs`

Change the browse mode hint from:

```
[esc] control mode
```

to:

```
[esc] control mode  [ctrl+esc] force exit browse mode
```

No other changes. Control mode hints remain `[q] quit  [enter] browse`.

#### Pass Criteria

1. Browse mode status bar shows
   `[esc] control mode  [ctrl+esc] force exit browse mode` on the left.
2. Control mode status bar is unchanged.
3. Both `Esc` and `Ctrl+Esc` still switch to control mode (no behavior change).

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 5: Mode icons

#### Goal

Add Nerd Font icons to the mode labels in the status bar to make them more
visually distinctive. Browse mode gets `nf-md-web` (󰖟), control mode gets
`nf-fa-keyboard_o` (). These render consistently in terminal emulators with Nerd
Fonts and are easier to see than emoji.

#### Changes

##### `web/src/main.rs`

Change the mode labels:

- `"BROWSE"` → `"󰖟 BROWSE"` (U+F059F, nf-md-web)
- `"CONTROL"` → `" CONTROL"` (U+F11C, nf-fa-keyboard_o)

Increase the `Constraint::Length` for the mode label from `10` to `12` to
accommodate the icon + space.

#### Pass Criteria

1. Browse mode shows `󰖟 BROWSE` on the right of the status bar.
2. Control mode shows `CONTROL` on the right of the status bar.
3. Labels are not clipped — icon and text are fully visible.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 6: Simplify browse mode hints

#### Goal

The browse mode status bar currently shows
`[esc] control mode  [ctrl+esc] force exit browse mode`. The `[esc]` hint is
unnecessary — users press Esc instinctively, and it will work most of the time.
Only the escape hatch needs to be documented. Remove `[esc] control mode` and
keep only `[ctrl+esc] force exit browse mode`.

#### Changes

##### `web/src/main.rs`

Change the browse mode hint from:

```
[esc] control mode  [ctrl+esc] force exit browse mode
```

to:

```
[ctrl+esc] force exit browse mode
```

#### Pass Criteria

1. Browse mode status bar shows only `[ctrl+esc] force exit browse mode`.
2. `Esc` still switches to control mode (behavior unchanged).
3. Control mode hints are unchanged.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 7: Display viewport dimensions

#### Goal

Replace the "waiting for browser..." placeholder with the viewport's actual
inner coordinates and dimensions (in rows and columns). This proves `web` knows
exactly where the browser content region is, which is the foundation for telling
TermSurf where to composite the Chromium texture.

#### Changes

##### `web/src/main.rs`

After computing `layout[1]` (the viewport rect), calculate the inner area by
subtracting the border. The inner rect is the area inside `Borders::ALL` — 1
row/col inset on each side.

Use ratatui's `Block::inner()` to get the inner `Rect`, then format the
coordinates and dimensions as the viewport paragraph text:

```
origin: (col, row)
size: cols x rows
```

Both lines centered in the viewport. Keep the `Color::DarkGray` style — this is
debug/informational text that will eventually be replaced by browser content.

Update on every draw, so resizing the terminal immediately shows the new
dimensions.

#### Pass Criteria

1. Viewport displays its inner origin and size (e.g., `origin: (1, 3)` and
   `size: 78 x 20`).
2. Resizing the terminal updates the displayed dimensions immediately.
3. The displayed dimensions can be verified by counting — the inner width
   matches the number of columns available inside the border, and the inner
   height matches the number of rows.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 8: Mode-dependent border highlighting

#### Goal

Use border brightness to show which region is active. In browse mode, the
viewport is the focus — its border should be highlighted. In control mode, the
URL bar is the focus (future: editable URL) — its border should be highlighted.
The inactive region's border dims. This gives an immediate visual cue for which
mode you're in.

#### Changes

##### `web/src/main.rs`

Pass `mode` into the border style logic for both the URL bar and viewport
blocks:

- **Browse mode:** Viewport border/title → `Color::White`. URL bar border/title
  → `Color::DarkGray`.
- **Control mode:** URL bar border/title → `Color::White`. Viewport border/title
  → `Color::DarkGray`.

The current `Color::Gray` is replaced — borders are now either bright (active)
or dim (inactive), never in between.

#### Pass Criteria

1. In browse mode, the viewport border is bright and the URL bar border is dim.
2. In control mode, the URL bar border is bright and the viewport border is dim.
3. Switching modes with `Esc`/`Enter` immediately updates the border brightness.
4. The active border is clearly distinguishable from the inactive border.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 9: Color-based border highlighting

#### Goal

Replace brightness-based border highlighting with color-based highlighting.
Instead of `DarkGray` (dim) vs `White` (bright), use `Color::Reset` (default
terminal foreground) for inactive borders and `Color::Cyan` for active borders.
This works in both light and dark terminal themes and gives a cleaner visual cue
— the active region pops with color while inactive regions blend in naturally.

#### Changes

##### `web/src/main.rs`

Change the mode-dependent border tuple:

- **Active border:** `Color::Cyan` (was `Color::White`).
- **Inactive border:** `Color::Reset` (was `Color::DarkGray`).

So:

- **Browse mode:** Viewport border → `Color::Cyan`. URL bar border →
  `Color::Reset`.
- **Control mode:** URL bar border → `Color::Cyan`. Viewport border →
  `Color::Reset`.

#### Pass Criteria

1. Active border is cyan — clearly distinguishable by color, not just
   brightness.
2. Inactive border uses the terminal's default foreground — visible and
   theme-appropriate.
3. Works in both dark and light terminal themes.
4. Switching modes immediately updates the border colors.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 10: Undim viewport text

#### Goal

The viewport coordinate/dimension text still uses `Color::DarkGray` from when it
was a "waiting for browser..." placeholder. Now that it displays useful debug
information (origin and size), it should be readable. Change it to
`Color::Reset` to use the terminal's default foreground.

#### Changes

##### `web/src/main.rs`

Change the viewport `Paragraph` style from `Color::DarkGray` to `Color::Reset`.

#### Pass Criteria

1. Viewport origin and size text is clearly readable.
2. Text uses the terminal's default foreground color.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 11: Tokyo Night theme

#### Goal

Replace all `Color::Reset` and ANSI named colors with explicit RGB values from
the [Tokyo Night](https://github.com/enkia/tokyo-night-vscode-theme) palette.
This gives `web` a guaranteed visual identity regardless of the user's terminal
theme. Every element — background, foreground, borders, text — is explicitly
colored.

Future experiments can add theme switching (Tokyo Night Light, Catppuccin,
etc.), but for now we hardcode a single dark theme.

#### Palette

Tokyo Night (dark variant) colors used:

| Role          | Color   | Hex       |
| ------------- | ------- | --------- |
| Background    | bg      | `#1a1b26` |
| Foreground    | fg      | `#c0caf5` |
| Comment/muted | comment | `#565f89` |
| Active accent | cyan    | `#7dcfff` |
| Border subtle | border  | `#3b4261` |

#### Changes

##### `web/src/main.rs`

**Add color constants** at the top of the file:

```rust
const BG: Color = Color::Rgb(0x1a, 0x1b, 0x26);
const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);
const COMMENT: Color = Color::Rgb(0x56, 0x5f, 0x89);
const CYAN: Color = Color::Rgb(0x7d, 0xcf, 0xff);
const BORDER: Color = Color::Rgb(0x3b, 0x42, 0x61);
```

**Paint the full background:** Before rendering any widgets, render a `Block`
with `BG` background over the entire `frame.area()`. This ensures no terminal
default colors bleed through.

**URL bar:**

- Text: `FG`.
- Active border/title (control mode): `CYAN`.
- Inactive border/title (browse mode): `BORDER`.
- Block background: `BG`.

**Viewport:**

- Coordinate text: `COMMENT` (debug info, visually subordinate).
- Active border/title (browse mode): `CYAN`.
- Inactive border/title (control mode): `BORDER`.
- Block background: `BG`.

**Status bar:**

- Key hints: `COMMENT`.
- Mode label: `FG`.

#### Pass Criteria

1. The entire TUI has a dark blue-gray background (`#1a1b26`), regardless of the
   terminal's own theme.
2. Active borders are cyan (`#7dcfff`).
3. Inactive borders are a subtle gray-blue (`#3b4261`).
4. URL text is bright (`#c0caf5`).
5. Status bar hints are muted (`#565f89`), mode label is bright (`#c0caf5`).
6. Viewport coordinates use the muted comment color.
7. No terminal default colors bleed through anywhere.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 12: Brighten muted colors

#### Goal

The inactive borders (`#3b4261`) and muted text (`#565f89`) are too dim against
the Tokyo Night background. Shift both one step brighter using colors from the
Tokyo Night palette, keeping the visual hierarchy intact.

#### Palette Update

| Role            | Before    | After     |
| --------------- | --------- | --------- |
| Inactive border | `#3b4261` | `#565f89` |
| Muted text      | `#565f89` | `#737aa2` |

Foreground (`#c0caf5`) and active accent (`#7dcfff`) are unchanged.

#### Changes

##### `web/src/main.rs`

- Rename `BORDER` to `COMMENT` value: `Color::Rgb(0x56, 0x5f, 0x89)` (was
  `0x3b, 0x42, 0x61`).
- Rename `COMMENT` to `MUTED` and change to `Color::Rgb(0x73, 0x7a, 0xa2)` (was
  `0x56, 0x5f, 0x89`).

Actually, simpler: just update the two hex values:

- `BORDER`: `Color::Rgb(0x56, 0x5f, 0x89)` (was `0x3b, 0x42, 0x61`)
- `COMMENT`: `Color::Rgb(0x73, 0x7a, 0xa2)` (was `0x56, 0x5f, 0x89`)

No renaming, no logic changes — just two color values bumped up.

#### Pass Criteria

1. Inactive borders are visibly brighter than before but still subordinate to
   cyan active borders.
2. Status bar hints and viewport coordinates are brighter but still subordinate
   to foreground text.
3. The visual hierarchy is preserved: active accent > foreground > muted text >
   inactive border > background.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 13: Profile indicator

#### Goal

Display the current browser profile name in the top-right corner of the URL bar,
with a Nerd Font person icon to its left. This highlights one of TermSurf's key
differentiators: side-by-side browsing with different profiles in the same
window. The profile defaults to `"default"` and can be set with
`--profile <name>`.

#### Layout

The profile name appears as a right-aligned title in the URL bar's border:

```
┌─ URL ──────────────────────────  default ─┐
│ https://google.com                         │
└────────────────────────────────────────────┘
```

The `` is `nf-fa-user` (U+F007). The profile name uses `FG` color to stand out,
while the icon uses `COMMENT` for subtle visual separation.

#### Changes

##### `web/src/main.rs`

**CLI parsing:** After extracting the URL from args, scan for `--profile` and
take the next argument as the profile name. Default to `"default"` if not
provided. No need for `clap` — a simple manual parse is sufficient.

```
web <url>                    → profile = "default"
web <url> --profile work     → profile = "work"
web --profile work <url>     → profile = "work"
```

**URL bar block:** Add a second right-aligned title to the URL bar block using
ratatui's `.title_bottom()` or `.title()` with `Position::Top` and
`Alignment::Right`. The title text is `" {icon} {profile} "` with appropriate
styling.

**Pass profile to `ui()`:** Add a `profile: &str` parameter.

#### Pass Criteria

1. `web https://google.com` shows ` default` in the top-right of the URL bar
   border.
2. `web https://google.com --profile work` shows ` work` instead.
3. The profile name uses `FG` color and the icon uses `COMMENT`.
4. The profile indicator doesn't overlap with the URL text or `URL` title.

#### Result

Builds with no warnings. Ready for interactive testing.

### Experiment 14: Brighten Viewport Text and Status Hints

The viewport dimensions and the bottom-left key hints both use `COMMENT`
(#737aa2), making them hard to read against the dark background. These are
useful information, not decorative — they should use the normal text color.

#### Changes

##### `web/src/main.rs`

1. **Viewport text:** Change `.fg(COMMENT)` to `.fg(FG)` on the viewport
   `Paragraph` style (the `origin:` / `size:` lines).
2. **Status hints:** Change `.fg(COMMENT)` to `.fg(FG)` on the `hints_widget`
   `Paragraph` style.

#### Pass Criteria

1. Viewport dimensions render in `FG` (#c0caf5) — clearly readable.
2. Key hints in the status bar render in `FG` — clearly readable.
3. Mode label (right side of status bar) remains `FG` (unchanged).
4. Profile icon in URL bar remains `COMMENT` (unchanged — it's decorative).

#### Result

Builds with no warnings. Ready for interactive testing.
