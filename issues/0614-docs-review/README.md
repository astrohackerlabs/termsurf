+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

# Issue 614: Review documentation for accuracy and conciseness

## Goal

All living documentation is accurate, concise, and reflects the current state of
the project after the renames in Issues 611–613.

## Background

Issues 611 (rename Ghostty → TermSurf), 612 (app icon), and 613 (rename ghost/ →
gui/, web/ → tui/) changed names, paths, and branding across the project. The
living documents were updated for path references, but haven't been reviewed
holistically for accuracy, stale content, or unnecessary verbosity.

### Files to review

**Top-level:**

- `AGENTS.md`
- `CHANGELOG.md`
- `CLAUDE.md`
- `README.md`
- `TODO.md`

**docs/:**

- `docs/chromium.md`
- `docs/ghostty.md`
- `docs/keybindings.md`
- `docs/vsync.md`

**TUI:**

- `tui/` has no README. One may need to be created.

### What to look for

- Stale references to old names (`ghost/`, `web/`, `Ghostty.app`,
  `com.mitchellh.ghostty`)
- Outdated architecture descriptions that don't match current state
- Unnecessary verbosity — documentation that could be shorter without losing
  information
- Missing information about recent changes (icon, rename, directory structure)
- Accuracy of build commands, launch commands, and file paths

### Out of scope

- Historical issue docs (`docs/issues/`) — left as-is per Issue 613
- `gui/` internal docs — owned by upstream Ghostty

## Experiments

### Experiment 1: Update all living documents

#### Goal

Every living document accurately reflects the current state of the project. No
stale "Ghost" naming, no ts5 references where gui/ is meant, no missing issue
docs, no broken links.

#### Findings

Reviewed all 9 files plus confirmed tui/ has no README. Here is what needs to
change, organized by file.

##### CLAUDE.md (and AGENTS.md — identical copy)

AGENTS.md and CLAUDE.md have byte-identical content. Every change to CLAUDE.md
must be mirrored in AGENTS.md.

1. **"Ghost" generation naming (line 55):** The generation list says
   `**ghost** (Ghostty fork, Zig-first)`. The directory is now `gui/` and the
   section header (line 72) already says "TermSurf GUI (gui/)". Rename the
   generation entry to `**gui**` and update the description to match the section
   header.

2. **"Ghost" references in gui/ section (lines 76, 95):** "Ghost forks Ghostty"
   → "TermSurf GUI forks Ghostty". "Ghost is a clean Ghostty fork with no
   TermSurf modifications yet" → update to reflect that modifications have been
   made (Issues 601–612: XPC, IOSurface overlay, Chromium streaming, two-pane
   multi-profile, mouse input, keyboard input, icon, rename).

3. **Stale "Current State" (lines 93–96):** "Ghost is a clean Ghostty fork with
   no TermSurf modifications yet. Browser integration will be built
   incrementally across Issues 601+." — This is now wrong. Multiple issues have
   landed. Rewrite to describe the actual current state.

4. **ts5 "Superseded by Ghost" (line 131):** Change to "Superseded by TermSurf
   GUI".

5. **Documentation section header (line 640):** "TermSurf Ghost (active)" →
   "TermSurf GUI (active)".

6. **Missing issue docs (lines 640+):** The Documentation section is missing:
   - ts5 issues: 513 (Ctrl+Esc), 514 (mouse), 515 (drag)
   - gui issues: 601 (Zig XPC), 602 (pink texture), 603 (box demo), 604 (two
     panes), 605 (two profiles), 606 (mouse input), 607 (keyboard input), 608
     (search input), 609 (keyboard input 2), 610 (app icon — blocked), 611
     (rename), 612 (icon), 613 (rename directories), 614 (docs review)

##### README.md

1. **Build commands (lines 57–65):** Reference `ts5/xpc-gateway` and `cd ts5`.
   The active generation is gui/. Update to `cd gui && zig build`. The
   xpc-gateway build step is ts5-specific and not needed for gui/.

2. **Launch command (lines 70–71):** `open ts5/zig-out/TermSurf.app` →
   `open
   gui/zig-out/TermSurf.app`.

3. **Status section (lines 99–123):** Says "five generations (ts1 through ts5).
   The current generation (ts5)..." — gui/ is the sixth generation and the
   current one. The "what works today" and "not yet started" lists describe
   ts5's state. Update to describe gui/'s current state.

4. **License (lines 132–133):** References `ts5/` but not `gui/`.

##### CHANGELOG.md

1. **Broken links (lines 167–169):** References `docs/ts1-webview.md` which does
   not exist. Fix or remove the links.

2. **Scope clarity:** The entire changelog covers ts1 (WKWebView generation).
   Add a note at the top clarifying this is the ts1 changelog, since the project
   has moved through multiple generations since.

##### TODO.md

1. **Entirely stale.** All items are from ts1/ts2/ts3. The CEF Integration
   section lists open items for a technology that was abandoned in ts4. The UX
   Refinements section is from ts1. Nothing reflects current gui/ work.

2. **Rewrite** with current gui/ priorities, or clearly mark as historical.

##### docs/ghostty.md

1. **Table (line 15):** Says ts5 is "Active". ts5 is superseded by gui/. Add a
   gui/ row as Active, mark ts5 as Superseded.

2. **Prose (lines 21–22):** "ts5 is active development" → gui/ is active.

3. **Merge instructions (lines 59–86):** Only cover ts5. Add gui/ merge
   instructions (same pattern: `git subtree pull --prefix=gui`).

##### docs/keybindings.md

1. **"Ghost's Zig core" (line 20):** Update to "TermSurf's Zig core" or "the
   GUI's Zig core".

##### docs/chromium.md

1. **"Box demo in Ghost" (line 42):** Update branch description to "Box demo in
   GUI" or "Box demo".

2. **Otherwise accurate.** Branch table, build commands, and recovery
   instructions are current.

##### docs/vsync.md

1. **No changes needed.** Accurate and concise.

##### tui/README.md

1. **Does not exist.** Create a minimal README explaining what the tui/
   directory contains: a Rust/ratatui TUI that draws browser chrome (URL bar,
   status bar) in the terminal pane. Include build command
   (`cargo build -p web`).

#### Steps

##### Step 1: Update CLAUDE.md

Apply all 6 changes listed above. Focus on accuracy — update the generation
name, current state, and documentation index.

##### Step 2: Sync AGENTS.md

Copy the updated CLAUDE.md content to AGENTS.md (they must stay identical).

##### Step 3: Update README.md

Update build/launch commands to reference gui/. Rewrite the Status section to
reflect the current generation and its state.

##### Step 4: Update CHANGELOG.md

Add a header note clarifying this is the ts1 changelog. Fix the broken
`docs/ts1-webview.md` links.

##### Step 5: Update TODO.md

Rewrite with current gui/ priorities. Remove stale ts1/ts2/ts3 items.

##### Step 6: Update docs/ghostty.md

Add gui/ to the table, mark ts5 as superseded, add gui/ merge instructions.

##### Step 7: Update docs/keybindings.md

Replace "Ghost's Zig core" with "TermSurf's Zig core".

##### Step 8: Update docs/chromium.md

Update the one "Ghost" reference in the branch table.

##### Step 9: Create tui/README.md

Minimal README: what it is, how to build, how it fits into the project.

#### Verification

1. **No stale "Ghost" naming in living docs:**
   `grep -rw 'Ghost' CLAUDE.md AGENTS.md README.md TODO.md CHANGELOG.md docs/chromium.md docs/ghostty.md docs/keybindings.md docs/vsync.md`
   returns no matches (excluding "Ghostty" the upstream project name).

2. **No stale ts5 references where gui/ is meant:** Spot-check that build
   commands, launch commands, and status descriptions reference gui/, not ts5/.

3. **No broken links:** Verify any markdown links point to files that exist.

4. **AGENTS.md and CLAUDE.md are identical:** `diff AGENTS.md CLAUDE.md` returns
   no output.

**Result:** Pass

All four verification checks passed. The one "Ghost" match in docs/ghostty.md is
an intentional historical reference ("It was originally named `ghost/` (after the
working name 'Ghost')"), not stale naming. AGENTS.md is a symlink to CLAUDE.md,
so they stay in sync automatically.

#### Conclusion

Seven files updated, one created, one deleted (CHANGELOG.md, removed before the
experiment). The "Ghost" generation name is gone from all living docs, replaced
by "TermSurf GUI" or "gui". README build/launch commands now point to gui/.
TODO.md is rewritten for current priorities. docs/ghostty.md tracks all three
Ghostty copies with gui/ as active. The tui/ directory has a README.

## Conclusion

All living documentation is accurate and reflects the current state of the
project. The renames from Issues 611–613 are fully propagated. No stale "Ghost"
naming, no ts5 references where gui/ is meant, no broken links. CHANGELOG.md
(ts1-era, obsolete) was removed. TODO.md was rewritten for current gui/
priorities. The documentation index in CLAUDE.md now lists all 18 missing issue
docs (513–515, 601–614).
