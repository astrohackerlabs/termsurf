# Issue 418: Repo Restructure

**Goal:** Reorganize the TermSurf repo folder structure to reflect the decision
to fork Ghostty (Issue 417), clean up vendored dependencies, and prepare for the
Ghostty merge.

**Context:** With Ghostty selected as the terminal emulator (Issue 417), the
repo will become primarily a Ghostty fork. The top level will contain Ghostty's
source tree, with historical directories (ts1–ts4), vendored code, and
documentation alongside it. The Chromium fork moves from a submodule inside ts4
to a top-level gitignored directory with proper origin/upstream configuration.

## Changes

### 1. Move `termsurf-chromium` to top level

Currently at `ts4/termsurf-chromium/`. Move to `/termsurf-chromium/`.

This is a Chromium source tree managed by `gclient`/`depot_tools`. It contains:

```
termsurf-chromium/
├── .cipd/
├── .gclient
├── .gclient_entries
├── .gcs_entries
├── depot_tools/         (gitignored)
├── src/                 (the Chromium source — currently a submodule)
└── _bad_scm/
```

**Submodule → gitignored directory.** The `src/` subdirectory is currently
registered as a git submodule (in `.gitmodules`, pointing to a local path
`/Users/ryan/dev/termsurf-chromium/src`). Remove this submodule registration.
Instead:

- Add `/termsurf-chromium/` to `.gitignore`
- Document the tracked branch/commit in this issue and in CLAUDE.md
- The local clone is a **shallow clone** of origin

**Remote configuration for `termsurf-chromium/src/`:**

| Remote   | URL                                     | Purpose                  |
| -------- | --------------------------------------- | ------------------------ |
| origin   | `github.com/termsurf/termsurf-chromium` | Our fork                 |
| upstream | `github.com/chromium/chromium`          | Official Chromium mirror |

We regularly pull `main` and tags from upstream to origin. Our working branches
follow the pattern `{version}-termsurf` (e.g., `146.0.7650.0-termsurf`).

**Branch strategy:** Track the same Chromium version as Electron. This lets us
reference Electron's patches and solutions even though we use the Content API
directly (not Electron itself). To find the current version:

1. Check Electron's `DEPS` file for `chromium_version`
2. Use that version tag as our base
3. Create `{version}-termsurf` branch on top of the tag

**Current state:**

- Branch: `146.0.7650.0-termsurf`
- Commit: `b2907d660628a` (6 commits ahead of `146.0.7650.0` tag)

### 2. Move vendored/analysis repos into `vendor/`

Several top-level directories are vendored or analysis copies of external
projects. Move them into `vendor/`:

| Current path  | New path            | Notes                                         |
| ------------- | ------------------- | --------------------------------------------- |
| `/wezterm/`   | `vendor/wezterm/`   | WezTerm source (analysis copy, not committed) |
| `/cef/`       | `vendor/cef/`       | CEF source (analysis copy)                    |
| `/cef-rs/`    | `vendor/cef-rs/`    | CEF Rust bindings (used by ts3)               |
| `/alacritty/` | `vendor/alacritty/` | Alacritty source (analysis copy)              |
| `/electron/`  | `vendor/electron/`  | Electron source (analysis/reference)          |
| `/chromium/`  | `vendor/chromium/`  | Chromium source (analysis copy)               |

All of these except `cef-rs/` are already in `.gitignore`. After moving, update
`.gitignore` paths accordingly. `cef-rs/` is committed and used by ts3 — it
moves as-is.

### 3. Update `.gitignore`

Remove:

```
/wezterm/
/electron/
/alacritty/
/cef/
/chromium/
ts4/termsurf-chromium/depot_tools/
```

Add:

```
# Chromium fork (managed separately, shallow clone)
/termsurf-chromium/

# Vendored analysis repos (not committed)
vendor/wezterm/
vendor/electron/
vendor/alacritty/
vendor/cef/
vendor/chromium/
```

### 4. Remove the `termsurf-chromium` submodule

Remove the submodule entry from `.gitmodules`:

```
[submodule "ts4/termsurf-chromium/src"]
    path = ts4/termsurf-chromium/src
    url = /Users/ryan/dev/termsurf-chromium/src
```

Remove the submodule from `.git/config` and `.git/modules/`. Clean up the
submodule tracking in the git index.

### 5. Update `CLAUDE.md`

Update the project overview and directory structure sections to reflect:

- `termsurf-chromium/` at top level (gitignored, shallow clone)
- `vendor/` directory for analysis repos and vendored code
- Chromium remote configuration (origin = termsurf fork, upstream = official)
- Chromium branch strategy (track Electron's version)
- Current tracked version and commit

### 6. Merge Ghostty into top level

After the restructure is complete, merge the latest Ghostty into the top level
of the repo. The repo becomes primarily a Ghostty fork:

```
termsurf/                        (root — Ghostty fork)
├── src/                         (libghostty — Zig core)
├── macos/                       (Ghostty macOS app — Swift)
├── pkg/                         (Ghostty platform packages)
├── build.zig                    (Ghostty build system)
├── build.zig.zon                (Ghostty dependencies)
├── include/                     (libghostty C API headers)
│
├── termsurf-chromium/           (gitignored — Chromium fork, shallow clone)
│
├── ts1/                         (historical — Ghostty + WKWebView)
├── ts2/                         (historical — WezTerm + in-process CEF)
├── ts3/                         (historical — WezTerm + out-of-process CEF)
├── ts4/                         (experiments — Content API PoCs)
│
├── vendor/
│   ├── cef-rs/                  (CEF Rust bindings, used by ts3)
│   ├── wezterm/                 (gitignored — analysis copy)
│   ├── electron/                (gitignored — reference)
│   ├── alacritty/               (gitignored — analysis copy)
│   ├── cef/                     (gitignored — analysis copy)
│   └── chromium/                (gitignored — analysis copy)
│
├── docs/                        (all documentation)
├── assets/                      (branding assets)
├── html/                        (HTML resources)
├── website/                     (termsurf.com)
├── logs/                        (gitignored — debug logs)
│
├── CLAUDE.md                    (AI agent guide)
├── README.md
├── CHANGELOG.md
└── TODO.md
```

**Merge strategy:** Add Ghostty as a remote, fetch, and merge with
`--allow-unrelated-histories`. This preserves both Ghostty's commit history and
TermSurf's commit history. Conflicts (README.md, .gitignore, etc.) are resolved
in favor of TermSurf's versions where appropriate.

**After the merge:** The top level is a buildable Ghostty. `zig build` should
work. The macOS app builds. From this point forward, TermSurf development means
modifying Ghostty's source tree to add browser pane support, and the ts1–ts4
directories are historical reference.

## Order of Operations

1. Move vendored/analysis repos to `vendor/`
2. Move `cef-rs/` to `vendor/cef-rs/` (update ts3 paths if needed)
3. Remove `termsurf-chromium` submodule from git tracking
4. Move `termsurf-chromium/` from `ts4/` to top level
5. Update `.gitignore`
6. Update `CLAUDE.md`
7. Commit the restructure
8. Merge Ghostty into top level
9. Resolve conflicts, verify build
10. Commit the merge
