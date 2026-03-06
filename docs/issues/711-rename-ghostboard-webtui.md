# Issue 711: Rename GUI to Ghostboard, TUI to webtui

## Goal

Rename the GUI application from "TermSurf" to "TermSurf Ghostboard" and the TUI
from `web` to `webtui`. Fix the build and all documentation to reflect the new
names.

## Background

TermSurf is evolving from a single app into a protocol ecosystem. The current
names are ambiguous:

- **GUI** is called "TermSurf" — but TermSurf is the protocol, not one specific
  terminal. As we add more boards (Wezboard, iBoard2, Babycat, etc.), we need a
  name that identifies this specific board as the Ghostty fork. "TermSurf
  Ghostboard" makes the relationship clear: it's the Ghostty-based board for the
  TermSurf protocol.

- **TUI** is called `web` — a generic name that doesn't convey what it is. As we
  add more TUIs, each needs a distinct identity. `webtui` is more descriptive: a
  TUI for web browsing.

### What changes

- **GUI app name:** TermSurf → TermSurf Ghostboard
- **GUI directory:** `gui/` → `ghostboard/`
- **TUI package name:** `web` → `webtui` (binary stays `web`)
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
   completely separate. No shared state between boards unless explicitly
   desired.

3. **TUIs get their own subdirectory** — `termsurf/webtui/` for config. TUIs are
   lightweight and may not need `data`/`cache`/`state`, but the structure is
   ready if they do.

4. **Future boards just pick a name** — A new board (e.g., Wezboard) uses
   `termsurf/wezboard/` in the same structure. No coordination needed.
   Completely isolated from Ghostboard.

5. **Backwards compatibility** — Current data in flat `termsurf/` paths needs a
   one-time migration into `termsurf/ghostboard/`.

**Environment variables** — The board sets env vars for its children:

- `TERMSURF_CONFIG_DIR` → `$XDG_CONFIG_HOME/termsurf/ghostboard`
- `TERMSURF_DATA_DIR` → `$XDG_DATA_HOME/termsurf/ghostboard`
- `TERMSURF_SOCKET` → `$TMPDIR/termsurf/ghostboard-{pid}.sock`

Browser engine processes inherit these and append their own name (e.g.,
`$TERMSURF_DATA_DIR/roamium` for Chromium's profile directory).

## Experiments

### Experiment 1: Rename `tui/` to `webtui/`

Rename the TUI directory from `tui/` to `webtui/` and update the Cargo package
name from `web` to `webtui`. The binary name stays `web` — users still type
`web google.com`.

#### Changes

**Directory rename:**

- `tui/` → `webtui/`

**`webtui/Cargo.toml`:**

- Package name: `web` → `webtui`
- Binary name stays `web` (the `[[bin]] name = "web"` line is unchanged)

**`webtui/src/main.rs`:**

- Line 169: `#[command(name = "web", ...)]` — unchanged (binary is still `web`)
- Line 350: `"web".to_string()` fallback — unchanged (exe name is still `web`)

**`.gitignore`:**

- `tui/target/` → `webtui/target/`

**`CLAUDE.md`:**

- `tui/` → `webtui/` in Directory Structure section
- `tui/` → `webtui/` in all other references
- Update "The first TUI, `web`" description to mention `webtui/` directory

**`scripts/build-debug.sh`:**

- `cd "$REPO_DIR/tui"` → `cd "$REPO_DIR/webtui"`
- `$REPO_DIR/tui/target/debug/web` → `$REPO_DIR/webtui/target/debug/web`
- Comment `# --- Web TUI (Rust) ---` unchanged (describes what it is, not the
  directory)

**`scripts/build-release.sh`:**

- Same changes as `build-debug.sh` (release paths)

**`scripts/install.sh`:**

- `WEB="$REPO_DIR/tui/target/release/web"` →
  `WEB="$REPO_DIR/webtui/target/release/web"`

**`gui/src/apprt/xpc.zig`:**

- No changes. The `web_fd` field name, `sendModeToWeb()` function, and `.tui`
  enum variant refer to the protocol concept (TUI connection), not the directory
  name. These will be revisited in a later experiment if needed.

**`docs/issues/711-rename-ghostboard-webtui.md`:**

- Already up to date.

**No changes to other issue docs.** Historical issue documents reference `tui/`
as it existed at the time. Rewriting history in 47 docs adds noise without
value.

#### Verification

1. `cd webtui && cargo build` — must compile
2. `ls webtui/target/debug/web` — binary is still named `web`
3. `grep -r 'tui/' .gitignore` — no stale `tui/` references
4. `grep 'tui/' CLAUDE.md` — no stale `tui/` references (except historical
   mentions like "directory rename from ghost/web to gui/tui")

**Result:** Pass

All four checks passed. `cargo build` compiled cleanly with the new package name
`webtui`. The binary is still `web`. No stale `tui/` references remain in
`.gitignore` or `CLAUDE.md`.

#### Conclusion

Directory renamed, package renamed, build scripts updated. The TUI now lives at
`webtui/` with package name `webtui` while the user-facing binary stays `web`.
No code changes were needed in the GUI or TUI source — only paths and the Cargo
package name.
