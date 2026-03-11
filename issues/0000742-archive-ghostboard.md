# Issue 742: Archive Ghostboard

## Goal

Archive the `ghostboard/` directory to reduce maintenance burden. Wezboard is
the sole active GUI during protocol iteration. Ghostboard will be re-created
from a fresh Ghostty fork after the protocol stabilizes, closer to launch.

## Background

TermSurf currently maintains two GUI implementations: Ghostboard (Ghostty fork,
Zig) and Wezboard (WezTerm fork, Rust). Every protocol change requires
implementation in both. The protocol is evolving rapidly — Issue 741 just added
direct TUI↔Browser connections, and many more changes are coming (proto split,
new message types for downloads, dialogs, bookmarks, etc.).

Maintaining two GUIs during this period doubles the implementation work for
every protocol change with no user-facing benefit. Wezboard is the better choice
for iteration because:

- **Cross-platform** — WezTerm works on macOS, Linux, and Windows. Ghostty does
  not support Windows, making Ghostboard unsuitable for the cross-platform
  milestone.
- **Same language as the ecosystem** — Wezboard, Roamium, and the TUI are all
  Rust. Protocol changes often touch all three. Having the GUI in Rust too
  eliminates context-switching between Zig and Rust.
- **Active development** — Wezboard has full protocol support, CALayerHost
  rendering, input forwarding, and the direct TUI↔Browser connection from
  Issue 741. Ghostboard is missing the direct connection (Issue 741 was
  Wezboard-only) and would need porting work just to catch up.

Ghostboard will return. The vision — multiple terminal emulators speaking the
TermSurf protocol — is central to the project. But the right time to fork
Ghostty again is after the protocol stabilizes, when Ghostty itself will have
months of additional development. Re-creating Ghostboard from a fresh fork will
be cleaner than maintaining a stale fork through dozens of protocol changes.

The archived directory will be called "Ghostboard Legacy" to distinguish it from
the future re-creation.

## Analysis

### What to archive

- `ghostboard/` — The entire Ghostty fork directory. This is a git subtree
  import, so all history is preserved in git.

### What to update

- `docs/early-prototypes.md` — Add a "Ghostboard Legacy" entry to the Archive
  Log table with the commit hash, date, and notes.
- `CLAUDE.md` — Remove Ghostboard from the active development sections. Update
  the GUI table to show only Wezboard as active. Remove the Ghostboard source
  layout, build commands, and upstream merge instructions. Keep the mention in
  the vision section (Ghostboard will return).
- `TODO.md` — Update the 1.0 Milestone to reflect that Ghostboard is deferred.
- `scripts/` — Remove or update scripts that reference Ghostboard
  (`build.sh ghostboard`, `install.sh ghostboard`, `rename-ghostty.sh`, etc.).

### What NOT to change

- Issue documents in `issues/` — These are immutable historical records. They
  reference Ghostboard extensively and that's correct — it was active at the
  time.
- `roamium/` — Roamium works with any GUI. No changes needed.
- `webtui/` — The TUI doesn't know which GUI it's connected to. No changes
  needed.

## Experiments

### Experiment 1: Archive ghostboard and update docs

#### Description

Delete the `ghostboard/` directory, update all documentation and scripts to
reflect that Wezboard is the sole active GUI, and add Ghostboard Legacy to the
archive log. This is a single experiment because the changes are all
documentation and file deletion — no code logic to test.

#### Changes

**Delete `ghostboard/`**

```bash
git rm -r ghostboard/
```

The full history is preserved in git. Recovery:
`git checkout <commit>~1 -- ghostboard/`

**`docs/early-prototypes.md`** — Add archive entry

Add a new row to the Archive Log table after the ts5 entry:

```
| `ghostboard/`    | `{hash}` | 2026-03-11 | Ghostboard Legacy (Ghostty fork, Zig). Archived to focus on Wezboard during protocol iteration. Will be re-created from fresh Ghostty fork after protocol stabilizes. |
```

Add a new section after the ts5 documentation: "## Ghostboard Legacy
(ghostboard/) — Archived". Move the Ghostboard-specific content from CLAUDE.md
(architecture, current state, source layout) into this section so the knowledge
is preserved in the archive doc rather than lost.

**`CLAUDE.md`** — Remove Ghostboard from active development

1. **Vision / Multiple GUIs section** — Remove Ghostboard from the active list.
   Keep it mentioned as planned/future. Update the table:

   ```
   | Ghostboard | ghostboard/ | Archived. Will return from fresh Ghostty fork. |
   ```

2. **Directory Structure** — Change the Ghostboard entry:

   ```
   - `ghostboard/` — Archived. See docs/early-prototypes.md.
   ```

3. **Remove "Ghostboard (ghostboard/) — Active Development" section** — Delete
   the entire section including Architecture, Current State subsections. This
   content moves to `docs/early-prototypes.md`.

4. **Remove "Wezboard Current State" from under Ghostboard** — Move this to be
   its own top-level section "Wezboard (wezboard/) — Active Development" since
   it's now the sole GUI.

5. **Source Layout** — Remove the Ghostboard subsection. Keep Wezboard and
   Roamium.

6. **Build & Install** — Update the scripts table to remove `ghostboard` from
   the component lists. Remove the Ghostboard-only iteration note
   (`cd ghostboard && zig build`).

7. **Upstream Merges** — Remove the Ghostboard upstream merge instructions
   entirely. Wezboard has its own rename script already documented.

**`TODO.md`** — Update 1.0 Milestone

Change the terminal emulator line from:

```
- [ ] Ghostty, Wezterm, Kitty, Alacritty, iTerm2
```

To:

```
- [ ] Wezterm (active), Ghostty, Kitty, Alacritty, iTerm2
```

**`scripts/build.sh`** — Remove Ghostboard

1. Delete the `build_ghostboard()` function.
2. Remove `ghostboard)` from the case statement.
3. Remove `build_ghostboard` from the `all)` case.
4. Update usage strings to remove `ghostboard` from component lists.

**`scripts/install.sh`** — Remove Ghostboard

1. Delete the `install_ghostboard()` function.
2. Remove `ghostboard)` from the case statement.
3. Remove `install_ghostboard` from the `all)` case.
4. Update usage strings.
5. Update `install_webtui()` message — it currently says "bundled inside
   Ghostboard". Change to reference Wezboard or just install standalone.

**`scripts/uninstall.sh`** — Remove Ghostboard

1. Delete the `uninstall_ghostboard()` function.
2. Remove `ghostboard)` from the case statement.
3. Remove `uninstall_ghostboard` from the `all)` case.
4. Update usage strings.

**`scripts/clean-zig.sh`** — Delete entirely

This script only cleans Ghostboard's Zig build artifacts. With Ghostboard
archived, it has no purpose.

**`scripts/rename-ghostty.sh`** — Delete entirely

This script renames Ghostty references inside `ghostboard/`. With the directory
archived, it has no purpose. When Ghostboard is re-created from a fresh fork, a
new rename script will be written.

**`scripts/generate-icons.sh`** — Delete entirely

This script generates icons for Ghostboard's macOS app icon assets. With
Ghostboard archived, it has no purpose.

#### Verification

1. `git status` — Confirm `ghostboard/` is staged for deletion, all doc/script
   changes are staged.
2. `grep -r ghostboard scripts/` — No references remain in scripts.
3. `./scripts/build.sh wezboard` — Still builds successfully.
4. `./scripts/build.sh roamium` — Still builds successfully.
5. `./scripts/build.sh webtui` — Still builds successfully.
6. Review `CLAUDE.md` — Ghostboard is mentioned only as archived/future, not as
   active.
7. Review `docs/early-prototypes.md` — Ghostboard Legacy section preserves the
   architecture and current state documentation.

**Result:** Partial

Ghostboard archived, all docs and scripts updated, Ghostboard Legacy preserved
in `docs/early-prototypes.md`. Verification items 1–2 and 6–7 pass. However,
`install_webtui()` now just prints a message instead of actually installing the
`web` binary to `/usr/local/bin/`. The old Ghostboard install bundled webtui
into the app and symlinked it — that path is gone, but no replacement install
was added. `scripts/install.sh webtui` needs to build and copy the binary to
`/usr/local/bin/web`.

#### Conclusion

The archive itself is complete — Ghostboard is removed, docs are updated, and
the Ghostboard Legacy section preserves the knowledge. The webtui install path
is broken: there's no way to install `web` to `/usr/local/bin/` via
`scripts/install.sh webtui`. Next experiment should fix the webtui install.

### Experiment 2: Fix webtui install

#### Description

Replace the placeholder `install_webtui()` with a real install that copies the
release binary to `/usr/local/bin/web`. Also add `install_webtui` to the `all)`
case — Experiment 1 removed it along with `install_ghostboard` but webtui should
still be installed as part of `all`.

#### Changes

**`scripts/install.sh`** — Replace `install_webtui()`

Replace the current stub:

```bash
install_webtui() {
  echo "To install standalone: cargo install --path webtui"
}
```

With:

```bash
install_webtui() {
  local WEB="$REPO_DIR/webtui/target/release/web"

  if [ ! -f "$WEB" ]; then
    echo "Error: Release build not found at $WEB"
    echo "Run: scripts/build.sh webtui --release"
    exit 1
  fi

  echo "==> Installing webtui to /usr/local/bin/web..."
  sudo cp "$WEB" /usr/local/bin/web

  echo "  Bin: /usr/local/bin/web"
}
```

Add `install_webtui` to the `all)` case, after `install_wezboard`:

```bash
  all)
    install_roamium
    install_wezboard
    install_webtui
    echo ""
    echo "Done (all)."
    ;;
```

#### Verification

1. `scripts/build.sh webtui --release` — builds the release binary.
2. `scripts/install.sh webtui` — copies `web` to `/usr/local/bin/web`.
3. `which web` — returns `/usr/local/bin/web`.
4. `web --help` — binary runs.

### Experiment 3: Single sudo for install script

#### Description

The install script calls `sudo` on every individual operation (12 calls across
the three install functions). On machines where sudo doesn't cache credentials,
this means typing the password up to 12 times per install. Fix this by having
the script re-exec itself as root with a single `sudo` at the top, then run all
operations without `sudo`.

#### Changes

**`scripts/install.sh`**

1. Add a root check after the `COMPONENT` validation (after line 15). If not
   root, re-exec with sudo:

   ```bash
   if [ "$(id -u)" -ne 0 ]; then
     exec sudo "$0" "$@"
   fi
   ```

2. Remove every `sudo` prefix from commands inside the three install functions:
   - `install_roamium()`: 7 `sudo` calls (lines 28–29, 32, 35–37, 40–41)
   - `install_wezboard()`: 5 `sudo` calls (lines 59–62, 65)
   - `install_webtui()`: 1 `sudo` call (line 80)

#### Verification

1. `scripts/install.sh webtui` — prompts for password exactly once, then
   installs without further prompts.
2. `ls -la /usr/local/bin/web` — file is owned by root (confirming it ran as
   root).
3. `scripts/install.sh` with no args — still prints usage and exits without
   asking for a password (the sudo re-exec happens after arg validation).

### Experiment 4: Fix uninstall script

#### Description

`scripts/uninstall.sh` has two bugs and one consistency issue:

1. **Missing webtui from `all`** — The `all)` case calls `uninstall_roamium` and
   `uninstall_wezboard` but never `uninstall_webtui`. Running
   `scripts/uninstall.sh all` leaves `/usr/local/bin/web` behind.
2. **Missing sudo on webtui** — `uninstall_webtui()` runs `rm -f` without
   `sudo`, but the file is owned by root (installed by a root-running script).
   This will fail for non-root users.
3. **Inconsistent sudo pattern** — The install script now uses a single sudo
   re-exec at the top (Experiment 3). The uninstall script still sprinkles
   `sudo` on individual commands. Apply the same pattern for consistency.

#### Changes

**`scripts/uninstall.sh`**

1. Add root re-exec after argument validation (after the empty-component check),
   same pattern as install.sh:

   ```bash
   # Re-exec as root so we only prompt for the password once.
   if [ "$(id -u)" -ne 0 ]; then
     exec sudo "$0" "$@"
   fi
   ```

2. Remove `sudo` prefix from all commands in `uninstall_roamium()` and
   `uninstall_wezboard()` (3 `sudo` calls total).

3. Add `uninstall_webtui` to the `all)` case, after `uninstall_wezboard`:

   ```bash
   all)
     uninstall_roamium
     uninstall_wezboard
     uninstall_webtui
     echo ""
     echo "Done (all)."
     ;;
   ```

#### Verification

1. `scripts/uninstall.sh webtui` — prompts for password once, removes
   `/usr/local/bin/web`.
2. `ls /usr/local/bin/web` — file is gone.
3. `scripts/uninstall.sh` with no args — prints usage, no password prompt.
4. Read the `all)` case — confirms `uninstall_webtui` is called.
