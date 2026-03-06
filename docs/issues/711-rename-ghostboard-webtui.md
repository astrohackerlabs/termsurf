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

- **GUI app name:** TermSurf → TermSurf Ghostboard (filename: TermSurf
  Ghostboard.app)
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
`$TMPDIR/termsurf/termsurf-ghostboard-{pid}.sock`. This aligns the socket name
with the board name and allows multiple different boards to run simultaneously
without path collisions (e.g., `termsurf-ghostboard-{pid}.sock` vs
`termsurf-wezboard-{pid}.sock`).

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
- `TERMSURF_SOCKET` → `$TMPDIR/termsurf/termsurf-ghostboard-{pid}.sock`

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

### Experiment 2: Rename `gui/` to `ghostboard/`

Rename the GUI directory from `gui/` to `ghostboard/`, rename the app from
"TermSurf" to "TermSurf Ghostboard", update XDG paths from `termsurf/` to
`termsurf/ghostboard/`, and change the socket name from `gui-{pid}.sock` to
`termsurf-ghostboard-{pid}.sock`.

This is a large rename touching the directory, build system, Xcode project, menu
bar, About page, XDG paths, socket path, scripts, gitignore, and CLAUDE.md.

#### Changes

**1. Directory rename:**

- `gui/` → `ghostboard/`

**2. Socket path** (`ghostboard/src/apprt/xpc.zig` line 1552):

- `"gui-{d}.sock"` → `"termsurf-ghostboard-{d}.sock"`

**3. XDG paths** — add `ghostboard/` subdirectory under `termsurf/`:

All XDG paths currently use `termsurf/` as the subdirectory. They need to become
`termsurf/ghostboard/` so that each board gets its own namespace.

- `ghostboard/src/config/file_load.zig`:
  - `"termsurf/config.ghostty"` → `"termsurf/ghostboard/config.ghostty"`
  - `"termsurf/config"` → `"termsurf/ghostboard/config"`
- `ghostboard/src/crash/dir.zig`:
  - `"termsurf/crash"` → `"termsurf/ghostboard/crash"`
- `ghostboard/src/crash/sentry.zig`:
  - `"termsurf/sentry"` → `"termsurf/ghostboard/sentry"`
- `ghostboard/src/cli/ssh-cache/DiskCache.zig`:
  - `"termsurf"` → `"termsurf/ghostboard"` (ssh cache path)
- `ghostboard/src/cli/ssh_cache.zig`:
  - `"termsurf"` → `"termsurf/ghostboard"` (ssh cache path)

**4. App name in UI** — "TermSurf" → "TermSurf Ghostboard":

- `ghostboard/macos/Sources/Features/About/AboutView.swift` line 51:
  - `Text("TermSurf")` → `Text("TermSurf Ghostboard")`
  - Line 54: Update description text to mention Ghostboard
- `ghostboard/macos/Sources/App/macOS/MainMenu.xib`:
  - `title="TermSurf"` → `title="TermSurf Ghostboard"` (menu bar app name)
  - `title="About TermSurf"` → `title="About TermSurf Ghostboard"`
  - `title="Hide TermSurf"` → `title="Hide TermSurf Ghostboard"`
  - `title="Quit TermSurf"` → `title="Quit TermSurf Ghostboard"`
- `ghostboard/macos/Sources/App/iOS/iOSApp.swift` line 45:
  - `Text("TermSurf")` → `Text("TermSurf Ghostboard")`

**5. Xcode project** (`ghostboard/macos/TermSurf.xcodeproj/project.pbxproj`):

- All `INFOPLIST_KEY_CFBundleDisplayName = TermSurf` → `"TermSurf Ghostboard"`
  (5 occurrences)
- `INFOPLIST_KEY_CFBundleDisplayName = "TermSurf[DEBUG]"` →
  `"TermSurf Ghostboard[DEBUG]"`
- `PRODUCT_NAME = TermSurf` → `"TermSurf-Ghostboard"` (2 occurrences for release
  configs — these control the `.app` bundle name)
- `PRODUCT_NAME = "TermSurf-Debug"` → `"TermSurf-Ghostboard-Debug"`

**6. Zig build** (`ghostboard/src/build/TermSurfXcodebuild.zig` line 52):

- `"TermSurf-Debug"` → `"TermSurf-Ghostboard-Debug"`
- `"TermSurf"` → `"TermSurf-Ghostboard"`

**7. Zig help text** — references to `TermSurf.app`:

- `ghostboard/src/cli/help.zig` line 56:
  - `"TermSurf.app"` → `"TermSurf-Ghostboard.app"`
- `ghostboard/src/main_termsurf.zig` lines 80, 83, 84:
  - `"TermSurf.app"` → `"TermSurf-Ghostboard.app"`
- `ghostboard/src/cli/list_themes.zig` line 90:
  - `"TermSurf.app"` → `"TermSurf-Ghostboard.app"`
- `ghostboard/src/config/Config.zig` line 558:
  - `"TermSurf.app"` → `"TermSurf-Ghostboard.app"`

**8. `.gitignore`:**

- All `gui/` prefixes → `ghostboard/`

**9. Build scripts:**

- `scripts/build-debug.sh`:
  - `cd "$REPO_DIR/gui"` → `cd "$REPO_DIR/ghostboard"`
  - `$REPO_DIR/gui/macos/build/Debug/TermSurf-Debug.app` →
    `$REPO_DIR/ghostboard/macos/build/Debug/TermSurf-Ghostboard-Debug.app`
- `scripts/build-release.sh`:
  - `cd "$REPO_DIR/gui"` → `cd "$REPO_DIR/ghostboard"`
  - `$REPO_DIR/gui/macos/build/ReleaseLocal/TermSurf.app` →
    `$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf-Ghostboard.app`
- `scripts/install.sh`:
  - `APP="/Applications/TermSurf.app"` →
    `APP="/Applications/TermSurf-Ghostboard.app"`
  - `SRC="$REPO_DIR/gui/macos/build/ReleaseLocal/TermSurf.app"` →
    `SRC="$REPO_DIR/ghostboard/macos/build/ReleaseLocal/TermSurf-Ghostboard.app"`
  - Update all `lsregister -u` paths
- `scripts/clean-zig.sh`:
  - `GUI_DIR="$REPO_ROOT/gui"` → `GUI_DIR="$REPO_ROOT/ghostboard"`
- `scripts/rename-ghostty.sh`:
  - `GUI_DIR="${1:-gui}"` → `GUI_DIR="${1:-ghostboard}"`
  - Update comment and echo strings
- `scripts/generate-icons.sh`:
  - `GUI_DIR="$REPO_ROOT/gui"` → `GUI_DIR="$REPO_ROOT/ghostboard"`

**10. `CLAUDE.md`:**

- All `gui/` directory references → `ghostboard/`
- Update "GUI (gui/)" section heading to "Ghostboard (ghostboard/)"
- Update description to use "Ghostboard" terminology

**11. Not changed:**

- XDG `"termsurf"` references in build system files (`TermSurfResources.zig`,
  `TermSurfLib.zig`, etc.) — these refer to the installed resource directory
  name, not the config/data XDG path
- `TERM_PROGRAM` = `"termsurf"` — this is the protocol identity, not the board
- Historical issue docs — same rationale as Experiment 1
- The Xcode project directory remains `TermSurf.xcodeproj` — renaming it is
  fragile (hundreds of internal path references) and unnecessary since the
  display name is what users see

#### Verification

1. `cd ghostboard && zig build` — must compile
2. App bundle is named `TermSurf-Ghostboard.app` or
   `TermSurf-Ghostboard-Debug.app`
3. Launch the app — dock shows "TermSurf Ghostboard", menu bar shows "TermSurf
   Ghostboard", About page shows "TermSurf Ghostboard"
4. Socket path is `$TMPDIR/termsurf/termsurf-ghostboard-{pid}.sock`
5. Config file loads from `~/.config/termsurf/ghostboard/config.ghostty`
6. `grep -r 'gui/' .gitignore` — no stale `gui/` references
7. `grep '"gui/' CLAUDE.md` — no stale `gui/` references

**Result:** Fail

The Zig build compiled and the directory rename, socket path, XDG paths,
scripts, `.gitignore`, and `CLAUDE.md` all updated correctly. However, the app
name approach — using `PRODUCT_NAME = "TermSurf-Ghostboard"` for the filename
and `CFBundleDisplayName = "TermSurf Ghostboard"` for the display name — failed.

macOS ignores `CFBundleDisplayName` for the menu bar and Dock. Instead, it
derives these from `CFBundleName`, which Xcode auto-generates from
`PRODUCT_NAME`. So the dash from the filename leaked into every user-visible
surface: the menu bar said "TermSurf-Ghostboard", the Dock said
"TermSurf-Ghostboard", and `/Applications` showed "TermSurf-Ghostboard".

We attempted to fix this by adding an explicit `CFBundleName` key in
`TermSurf-Info.plist` set to "TermSurf Ghostboard" (with space), hoping it would
override the `PRODUCT_NAME`-derived value. This did not work — the dash still
appeared everywhere.

**Root cause:** macOS uses `PRODUCT_NAME` as the definitive source for the app's
display identity in the menu bar and Dock. `CFBundleDisplayName` is only used
for App Store listings and localized display names, not for the running app's
chrome. `CFBundleName` set in the plist may be overridden by Xcode's
`GENERATE_INFOPLIST_FILE` merging behavior. The only reliable way to control
what the user sees is to make `PRODUCT_NAME` itself contain the desired display
string — which means it cannot contain dashes if we don't want dashes visible.

**Conclusion:** The filename-has-dash / display-name-has-space split is not
viable on macOS. The next experiment must use a `PRODUCT_NAME` without dashes
(e.g., "Ghostboard") so that the `.app` filename, menu bar, and Dock all show
the same dash-free name. This means either:

1. Use a single-word `PRODUCT_NAME` like `Ghostboard` (app becomes
   `Ghostboard.app`, display name "Ghostboard")
2. Use a space in `PRODUCT_NAME` like `TermSurf Ghostboard` (app becomes
   `TermSurf Ghostboard.app`, display name "TermSurf Ghostboard") — spaces in
   `.app` filenames are common on macOS (e.g., "Visual Studio Code.app", "Google
   Chrome.app")

Option 2 is preferred — spaces in macOS app names are standard practice, and the
earlier concern about spaces was likely about other contexts (binary names,
socket paths) rather than `.app` bundles specifically.

### Experiment 3: Fix app name — use space instead of dash

Experiment 2 correctly renamed the directory, socket path, XDG paths, UI
strings, scripts, and docs. The only thing that failed was the dash in
`PRODUCT_NAME`. This experiment fixes that single issue: replace every
`TermSurf-Ghostboard` with `TermSurf Ghostboard` (space) so macOS shows the
correct name everywhere.

Spaces in `.app` filenames are standard on macOS: "Google Chrome.app", "Visual
Studio Code.app", "Firefox Nightly.app". The earlier no-spaces rule was about
binary names and socket paths, not app bundles.

#### Changes

**1. Xcode project** (`ghostboard/macos/TermSurf.xcodeproj/project.pbxproj`):

- `PRODUCT_NAME = "TermSurf-Ghostboard"` → `"TermSurf Ghostboard"` (2
  occurrences — ReleaseLocal and Release configs)
- `PRODUCT_NAME = "TermSurf-Ghostboard-Debug"` → `"TermSurf Ghostboard Debug"`

**2. Zig build** (`ghostboard/src/build/TermSurfXcodebuild.zig` line 52):

- `"TermSurf-Ghostboard-Debug"` → `"TermSurf Ghostboard Debug"`
- `"TermSurf-Ghostboard"` → `"TermSurf Ghostboard"`

**3. Zig help text** — `.app` references need quoted paths since they contain
spaces:

- `ghostboard/src/cli/help.zig` line 56–57:
  - `TermSurf-Ghostboard.app` → `"TermSurf Ghostboard.app"` (add quotes for
    shell usage in help text)
- `ghostboard/src/main_termsurf.zig` lines 80, 83, 84:
  - `TermSurf-Ghostboard.app` → `"TermSurf Ghostboard.app"`
- `ghostboard/src/cli/list_themes.zig` line 90:
  - `TermSurf-Ghostboard.app` → `TermSurf Ghostboard.app` (doc comment, no
    quotes needed)
- `ghostboard/src/config/Config.zig` line 558:
  - `TermSurf-Ghostboard.app` → `TermSurf Ghostboard.app` (doc comment, no
    quotes needed)

**4. Build scripts:**

- `scripts/build-debug.sh` line 33:
  - `TermSurf-Ghostboard-Debug.app` → `"TermSurf Ghostboard Debug.app"` (quote
    for shell)
- `scripts/build-release.sh` line 33:
  - `TermSurf-Ghostboard.app` → `"TermSurf Ghostboard.app"` (quote for shell)
- `scripts/install.sh` lines 6–7, 57–59:
  - `TermSurf-Ghostboard.app` → `"TermSurf Ghostboard.app"`
  - `TermSurf-Ghostboard-Debug.app` → `"TermSurf Ghostboard Debug.app"`

**5. `CLAUDE.md`:**

- Update install script description: `/Applications/TermSurf-Ghostboard.app` →
  `/Applications/TermSurf Ghostboard.app`

**6. `TermSurf-Info.plist`:**

- Remove the `CFBundleName` key added in the failed fix attempt — it's no longer
  needed since `PRODUCT_NAME` itself will be "TermSurf Ghostboard"

**7. Not changed:**

- `CFBundleDisplayName` values — already correct ("TermSurf Ghostboard" /
  "TermSurf Ghostboard[DEBUG]")
- Menu bar titles — already correct ("TermSurf Ghostboard")
- AboutView.swift — already correct ("TermSurf Ghostboard")
- Socket path — already correct (`termsurf-ghostboard-{pid}.sock`, no spaces)
- XDG paths — already correct (`termsurf/ghostboard/`)
- `.gitignore` — already correct (`ghostboard/`)
- Directory name — already correct (`ghostboard/`)

#### Verification

1. `cd ghostboard && zig build` — must compile
2. Build the full app — `scripts/build-debug.sh`
3. App bundle is named `TermSurf Ghostboard Debug.app` (space, no dash)
4. Launch the app — Dock shows "TermSurf Ghostboard", menu bar shows "TermSurf
   Ghostboard", About page shows "TermSurf Ghostboard"
5. `/Applications/` path in `install.sh` uses quoted `"TermSurf Ghostboard.app"`

**Result:** Pass

All five checks passed. The Zig build compiled cleanly. The app bundle is named
`TermSurf Ghostboard Debug.app` (space, no dash). The Dock shows "TermSurf
Ghostboard", the menu bar shows "TermSurf Ghostboard", and the About page shows
"TermSurf Ghostboard". No dashes visible anywhere in the UI.

#### Conclusion

Using a space in `PRODUCT_NAME` is the correct approach on macOS. The earlier
assumption that `.app` filenames cannot have spaces was wrong — spaces are
standard (Google Chrome.app, Visual Studio Code.app). macOS derives the menu bar
and Dock name directly from `PRODUCT_NAME`, so it must contain the exact string
you want users to see. `CFBundleDisplayName` and `CFBundleName` cannot override
this when `GENERATE_INFOPLIST_FILE = YES`.

Combined with Experiment 2's directory rename, socket path, XDG paths, and UI
string changes, the full rename from `gui/` → `ghostboard/` and "TermSurf" →
"TermSurf Ghostboard" is now complete.

### Experiment 4: Rename `website/` to `termsurf.com/`

Rename the website directory from `website/` to `termsurf.com/`. The directory
name matches the domain it serves, making the relationship immediately clear.
This follows the same pattern as `ghostboard/` and `webtui/` — each component
gets a descriptive directory name.

#### Changes

**1. Directory rename:**

- `website/` → `termsurf.com/`

**2. `website/package.json`** (becomes `termsurf.com/package.json`):

- `"name": "termsurf-website"` → `"name": "termsurf.com"`

**3. `.prettierignore`** (root):

- `website/.next` → `termsurf.com/.next`

**4. `ghostboard/.prettierignore`:**

- `website/.next` → `termsurf.com/.next`

**5. `ghostboard/.gitattributes`:**

- `website/** linguist-documentation` → `termsurf.com/** linguist-documentation`

**6. `docs/issues/003-website.md`:**

- Update directory references from `website/` to `termsurf.com/` in the
  Directory Structure section and any other path references

**7. Not changed:**

- `ghostboard/src/apprt/gtk/class/window.zig` — the `"website"` string there is
  a GTK action name for opening the project website URL, not a directory
  reference
- `CLAUDE.md` line 227 — historical reference ("website deps and linting")
  describes what Issue 677–678 did, not a directory path
- `CLAUDE.md` line 316 — already says "termsurf.com website", not `website/`

#### Verification

1. `ls termsurf.com/package.json` — directory exists
2. `cd termsurf.com && bun install && bun run build` — builds successfully
3. `grep -r 'website/' .prettierignore` — no stale references in root
4. `grep -r 'website/' ghostboard/.prettierignore ghostboard/.gitattributes` —
   no stale references
