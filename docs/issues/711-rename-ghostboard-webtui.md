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
- **Config directories:** XDG paths using `termsurf` — unchanged

### Socket path change

The board socket path changes from `$TMPDIR/termsurf/gui-{pid}.sock` to
`$TMPDIR/termsurf/ghostboard-{pid}.sock`. This aligns the socket name with the
board name and allows multiple different boards to run simultaneously without
path collisions (e.g., `ghostboard-{pid}.sock` vs `wezboard-{pid}.sock`).
