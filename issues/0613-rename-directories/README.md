+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

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
- `docs/issues/0600-termsurf-ghost.md` through `docs/issues/0612-icon.md` — All
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

## Experiments

### Experiment 1: Rename directories and update living documents

#### Goal

`ghost/` is renamed to `gui/`, `web/` is renamed to `tui/`.
`cd gui && zig build` succeeds. All living documents (CLAUDE.md, .gitignore,
keybindings.md, Claude skills) reference the new paths. Historical issue docs
(600–612) are left unchanged.

#### Steps

##### Step 1: Rename directories

```bash
git mv ghost gui
git mv web tui
```

##### Step 2: Update `.gitignore`

Replace all `ghost/` references with `gui/` (~20 lines). Replace `web/target/`
with `tui/target/`. Update the section comment from `# TermSurf Ghost (ghost/)`
to `# TermSurf GUI (gui/)`.

##### Step 3: Update `CLAUDE.md`

Replace references to `ghost/` and `web/` paths with `gui/` and `tui/`. This
includes:

- Directory listing (lines 62–63): `ghost/` → `gui/`, `web/` → `tui/`
- Section header: `## TermSurf Ghost (ghost/)` → `## TermSurf GUI (gui/)`
- Directory structure listing (~10 paths): `ghost/src/`, `ghost/macos/`, etc.
- Build command: `cd ghost && zig build` → `cd gui && zig build`
- Launch command: `open ghost/zig-out/Ghostty.app` →
  `open gui/zig-out/TermSurf.app`
- Upstream merge: `--prefix=ghost` → `--prefix=gui`
- ts5 section: `web/` paths → `tui/` paths, `cargo build -p web` →
  `cargo build -p web` (package name unchanged)

##### Step 4: Update `docs/keybindings.md`

- `web/src/main.rs` → `tui/src/main.rs`
- `ghost/src/Surface.zig` → `gui/src/Surface.zig`

##### Step 5: Update Claude skills

- `.claude/skills/keybindings/SKILL.md` — `web/src/main.rs` → `tui/src/main.rs`
- `.claude/skills/fix-nerd-fonts/SKILL.md` — `web/src/main.rs` →
  `tui/src/main.rs`

##### Step 6: Build and verify

```bash
cd gui && zig build
```

#### Verification

1. **Build succeeds:** `cd gui && zig build` completes without errors
2. **No stale references in living docs:**
   `grep -r 'ghost/' CLAUDE.md .gitignore docs/keybindings.md .claude/skills/`
   returns no matches (excluding references to "Ghostty" the upstream project,
   `ghostty` the binary, and `Ghostty.*` internal identifiers)
3. **Historical docs unchanged:**
   `git diff docs/issues/6{00,01,02,03,04,05,06,07,08,09,10,11,12}*` shows no
   changes

**Result:** Pass

Build succeeds from `gui/`. No stale `ghost/` or `web/` references in living
documents. Historical issue docs unchanged.

#### Conclusion

The rename was straightforward — two `git mv` operations plus find/replace
across 5 living documents (CLAUDE.md, .gitignore, keybindings.md, 2 Claude
skills). No code changes needed inside either directory.

## Conclusion

`ghost/` is now `gui/` and `web/` is now `tui/`. Build commands are
`cd gui && zig build` and `cargo build -p web` (from `tui/`). Historical issue
docs (600–612) retain their original `ghost/` and `web/` paths as a record of
when those names were in use.
