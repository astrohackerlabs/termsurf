# Issue 711: Rename GUI to Ghostboard, TUI to webtui

## Goal

Rename the GUI application from "TermSurf" to "TermSurf Ghostboard" and the TUI
from `web` to `webtui`. Fix the build and all documentation to reflect the new
names.

## Background

TermSurf is evolving from a single app into a protocol ecosystem. The current
names are ambiguous:

- **GUI** is called "TermSurf" — but TermSurf is the protocol, not one specific
  terminal. As we add more boards (Wezboard, Kittyboard, etc.), we need a name
  that identifies this specific board as the Ghostty fork. "TermSurf Ghostboard"
  makes the relationship clear: it's the Ghostty-based board for the TermSurf
  protocol.

- **TUI** is called `web` — a generic name that doesn't convey what it is. As we
  add more TUIs, each needs a distinct identity. `webtui` is more descriptive: a
  TUI for web browsing.

### What changes

- **GUI app name:** TermSurf → TermSurf Ghostboard
- **GUI directory:** `gui/` → `ghostboard/`
- **TUI binary name:** `web` → `webtui`
- **TUI directory:** `tui/` → `webtui/`
- **Documentation:** CLAUDE.md, README files, issue docs, code comments
- **Build system:** Binary names, bundle names, build targets
- **Code:** String literals, log messages, error messages referencing the old
  names

### What stays the same

- **Protocol:** `termsurf.proto` — unchanged

### Socket path change

The board socket path changes from `$TMPDIR/termsurf/gui-{pid}.sock` to
`$TMPDIR/termsurf/ghostboard-{pid}.sock`. This aligns the socket name with the
board name and allows multiple different boards to run simultaneously without
path collisions (e.g., `ghostboard-{pid}.sock` vs `wezboard-{pid}.sock`).

### XDG directory restructure

The XDG directory structure changes from a flat `termsurf/` layout to a
hierarchical one where each component gets its own subdirectory. The top-level
`termsurf/` namespace is the protocol's — individual apps live underneath.

**Board owns browser data.** Each board gets its own subdirectory, and browser
engine data lives under the board that manages it. This means two boards can run
simultaneously without browser profile lock conflicts.

```
~/.config/termsurf/                    # XDG_CONFIG_HOME/termsurf
├── ghostboard/                        # Board config
│   ├── config                         # Ghostboard config (or config.ghostty)
│   ├── roamium/                       # Chromium browser config for this board
│   ├── surfari/                       # WebKit browser config for this board
│   ├── waterwolf/                     # Gecko browser config for this board
│   └── girlbat/                       # Ladybird browser config for this board
├── webtui/                            # TUI config (future)
│   └── config
├── wezboard/                          # Future board
│   ├── config
│   └── roamium/                       # Same browser, isolated from ghostboard
└── ...

~/.local/share/termsurf/               # XDG_DATA_HOME/termsurf
├── ghostboard/
│   ├── roamium/                       # Chromium profiles, cookies, storage
│   ├── surfari/
│   └── ...
├── wezboard/
│   └── roamium/                       # Separate data from ghostboard's roamium
└── webtui/

~/.cache/termsurf/                     # XDG_CACHE_HOME/termsurf
├── ghostboard/
│   └── roamium/
└── ...

~/.local/state/termsurf/               # XDG_STATE_HOME/termsurf
├── ghostboard/
│   └── roamium/
└── ...
```

**Design principles:**

1. **`termsurf/` is the namespace root** — all XDG base dirs (`config`, `data`,
   `cache`, `state`) use `termsurf/` as the top-level folder. This is the
   protocol's namespace, not any single app's.

2. **Board owns browser data** — browser data lives under the board that manages
   it. `termsurf/ghostboard/roamium/` and `termsurf/wezboard/roamium/` are
   completely separate. No shared state between boards unless explicitly desired.

3. **TUIs get their own subdirectory** — `termsurf/webtui/` for config. TUIs are
   lightweight and may not need `data`/`cache`/`state`, but the structure is
   ready if they do.

4. **Future boards just pick a name** — A new board (e.g., Wezboard) uses
   `termsurf/wezboard/` in the same structure. No coordination needed. Completely
   isolated from Ghostboard.

5. **Backwards compatibility** — Current data in flat `termsurf/` paths needs a
   one-time migration into `termsurf/ghostboard/`.

**Environment variables** — The board sets env vars for its children:

- `TERMSURF_CONFIG_DIR` → `$XDG_CONFIG_HOME/termsurf/ghostboard`
- `TERMSURF_DATA_DIR` → `$XDG_DATA_HOME/termsurf/ghostboard`
- `TERMSURF_SOCKET` → `$TMPDIR/termsurf/ghostboard-{pid}.sock`

Browser engine processes inherit these and append their own name (e.g.,
`$TERMSURF_DATA_DIR/roamium` for Chromium's profile directory).
