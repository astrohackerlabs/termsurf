+++
status = "closed"
opened = "2026-02-25"
closed = "2026-02-25"
+++

# Issue 640: Project Cleanup

## Goal

Archive the five superseded prototype directories (`ts1/`–`ts5/`) and update all
documentation to reflect the current state of the project. Move historical
prototype documentation into `docs/early-prototypes.md` so the main docs stay
focused on the active codebase (`gui/`, `tui/`, `chromium/`).

## Background

TermSurf has evolved through six generations (ts1–ts5 plus the current `gui/`).
Each generation left behind a source directory and extensive documentation in
`CLAUDE.md`. The prototype directories total ~160 GB of build artifacts, source
trees, and vendor dependencies. They are no longer needed for active development
but contain valuable architectural history.

The active codebase is:

- `gui/` — TermSurf GUI (Ghostty fork, Zig-first)
- `tui/` — `web` TUI (Rust/ratatui)
- `chromium/` — Chromium fork

Everything else is historical.

## Current state

### Already archived

| What             | Commit    | Date       | Description                  |
| ---------------- | --------- | ---------- | ---------------------------- |
| `vendor/cef-rs/` | `2c7c5d7` | 2026-02-21 | CEF Rust bindings (ts2, ts3) |

### Directories to archive

| Directory | Generation | Size   | Description                            |
| --------- | ---------- | ------ | -------------------------------------- |
| `ts1/`    | 1.x        | 10 GB  | Ghostty + WKWebView                    |
| `ts2/`    | 2.0        | 49 GB  | WezTerm + in-process CEF               |
| `ts3/`    | 3.0        | 33 GB  | WezTerm + out-of-process CEF via XPC   |
| `ts4/`    | 4.0        | 1.4 GB | Chromium Content API experiments       |
| `ts5/`    | 5.0        | 68 GB  | Ghostty fork + out-of-process Chromium |

### Documentation to update

- **`CLAUDE.md`** — 94 references to `ts1`–`ts5`. Contains full sections for
  each generation including build commands, directory structures, and
  architectural decisions. The active sections (`gui/`, `tui/`, `chromium/`) are
  buried under pages of historical content.
- **`docs/chromium.md`** — References ts4 build commands and history. Mostly
  current.
- **`docs/vendor.md`** — May reference ts3 CEF usage.
- **`docs/keybindings.md`** — Should be current (gui/tui only).
- **`docs/xdg.md`** — Should be current.
- **`docs/ghostty.md`** — May need review.

### Issue docs

The `docs/issues/` directory contains issue docs for all generations (100–639).
These stay in place — they are the project's experiment history and are
referenced by `docs/chromium.md` branch table.

## Plan

### Stage 1: Archive `ts*` directories

Delete `ts1/`, `ts2/`, `ts3/`, `ts4/`, and `ts5/` from the working tree. They
are preserved in git history and can be recovered with `git checkout` if needed.

### Stage 2: Create `docs/early-prototypes.md`

Move the historical generation sections from `CLAUDE.md` into a new file
`docs/early-prototypes.md`. This includes:

- TermSurf 1.x (ts1) section
- TermSurf 2.0 (ts2) section
- TermSurf 3.0 (ts3) section
- TermSurf 4.0 (ts4) section
- TermSurf 5.0 (ts5) section
- cef-rs section
- Documentation index entries for ts1–ts5 issue docs

Include an **Archive Log** table recording when each prototype directory and
dependency was removed from the working tree, with commit hashes for recovery:

| What             | Commit    | Date       | Notes                             |
| ---------------- | --------- | ---------- | --------------------------------- |
| `vendor/cef-rs/` | `2c7c5d7` | 2026-02-21 | CEF Rust bindings (ts2, ts3)      |
| `ts1/`           | TBD       | TBD        | Ghostty + WKWebView               |
| `ts2/`           | TBD       | TBD        | WezTerm + in-process CEF          |
| `ts3/`           | TBD       | TBD        | WezTerm + out-of-process CEF      |
| `ts4/`           | TBD       | TBD        | Chromium Content API PoC          |
| `ts5/`           | TBD       | TBD        | Ghostty + out-of-process Chromium |

Keep a brief summary of each generation in `CLAUDE.md` (2–3 lines each) with a
pointer to `docs/early-prototypes.md` for details.

### Stage 3: Update `CLAUDE.md`

- Remove the full prototype sections (moved to `docs/early-prototypes.md`)
- Add a short "History" section summarizing the six generations
- Ensure the Project Overview, GUI, TUI, and Chromium sections are accurate
- Remove references to directories that no longer exist
- Update the Documentation index to reflect the new file structure

### Stage 4: Review remaining docs

- `docs/chromium.md` — Remove ts4 references if outdated
- `docs/vendor.md` — Remove ts3 CEF references if outdated
- `docs/ghostty.md` — Review for accuracy
- `docs/keybindings.md` — Verify current
- `docs/xdg.md` — Verify current

## Result

All four stages completed.

- **Stage 1** (`0bdf837`): Deleted `ts1/`–`ts5/` — 7,053 files, 3.6M lines,
  ~161 GB freed.
- **Stage 2** (`9554829`): Created `docs/early-prototypes.md` with full
  prototype documentation, archive log, and issue doc index.
- **Stage 3** (`b9f41f7`): Slimmed `CLAUDE.md` from 805 to 236 lines. Replaced
  full prototype sections with a brief History section linking to
  `docs/early-prototypes.md`.
- **Stage 4** (`0779208`): Reviewed all remaining docs. Updated
  `docs/ghostty.md` to remove archived ts1/ts5 references. `docs/chromium.md`,
  `docs/vendor.md`, `docs/keybindings.md`, and `docs/xdg.md` were already
  current — no changes needed.
