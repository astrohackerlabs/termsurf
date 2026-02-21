# Issue 613: Rename ghost/ to gui/ and web/ to tui/

## Goal

Rename the `ghost/` directory to `gui/` and the `web/` directory to `tui/`. All
references across the repo (docs, scripts, configs) are updated to match.

## Background

`ghost/` was named after "Ghost", the working name for our Ghostty fork. But the
directory contains the GUI application — the terminal emulator with integrated
browser. `gui/` is a clearer, shorter name that describes what it is rather than
where it came from.

`web/` was named after the `web` CLI command that users type to open a webpage.
But the directory contains a TUI application (Rust/ratatui) — the browser chrome
rendered in a terminal pane. `tui/` describes what the code actually is.

### Scope

The rename is two `git mv` operations plus a find/replace across documentation
and configuration files. The code inside the directories doesn't change — both
`gui/` and `tui/` are self-contained projects with their own build systems.

### Files to change

**Directory renames:**

- `ghost/` → `gui/`
- `web/` → `tui/`

**Configuration files:**

- `.gitignore` — ~20 lines referencing `ghost/` paths, 1 line referencing
  `web/target/`
- `CLAUDE.md` — ~30+ references to `ghost/` and `web/` paths across build
  commands, directory listings, architecture descriptions, and upstream merge
  instructions

**Documentation:**

- `docs/keybindings.md` — References to `ghost/src/` and `web/src/`
- `docs/issues/600-termsurf-ghost.md` through `docs/issues/612-icon.md` — All
  recent issues contain references to `ghost/` and `web/` paths in code
  examples, build commands, and file inventories

**Scripts:**

- `gui/scripts/generate-icons.sh` (after rename) — References `assets/` relative
  to repo root via `GHOST_DIR`/`REPO_ROOT`, which derive from `$0`. These will
  work automatically after the rename since the script uses its own path to find
  the repo root.

### What does NOT change

- Code inside `gui/` and `tui/` — no source file modifications
- The `ghostty` CLI binary name — unchanged per Issue 611
- Internal Ghostty identifiers (`GhosttyKit`, `Ghostty.*` Swift namespaces,
  `ghostty_*` C API) — unchanged per Issue 611
- Older generation directories (`ts1/` through `ts5/`) — historical, left as-is
- Issue documents for older generations — historical references stay as-is

### Documentation update strategy

Issue documents (600–612) contain hundreds of references to `ghost/` paths.
These are historical records of experiments that were run with those paths at
the time. Two options:

1. **Update all references** — Accurate but tedious, and rewrites history.
2. **Leave historical docs as-is** — The paths were correct when written. Only
   update living documents (CLAUDE.md, .gitignore, keybindings.md).

Option 2 is simpler and preserves the historical record. Issue docs are closed —
they won't be used as instructions for future work.
