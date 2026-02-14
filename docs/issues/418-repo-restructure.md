# Issue 418: Repo Restructure

**Goal:** Reorganize the TermSurf repo folder structure to reflect the decision
to fork Ghostty (Issue 417), clean up vendored dependencies, and prepare for the
Ghostty merge.

**Context:** With Ghostty selected as the terminal emulator (Issue 417), the
repo will contain Ghostty's source tree inside a `ts5/` directory, with
historical directories (ts1–ts4), vendored code, and documentation alongside it.
Keeping Ghostty in its own subdirectory (rather than at the repo root) avoids
conflicts with our own top-level files (README.md, docs/, etc.) and makes future
upstream merges cleaner. The Chromium fork moves from a submodule inside ts4 to
a top-level gitignored directory with proper origin/upstream configuration.

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

The Chromium repo is so large that even a shallow clone makes `git status`,
`git diff`, and other commands noticeably slow in the parent repo — git scans
submodule state on every invocation. By extracting `termsurf-chromium/` as a
completely separate, gitignored directory, git commands in the main TermSurf
repo return instantly. Git commands inside `termsurf-chromium/` are still slow
(unavoidable with a repo that size), but at least it no longer drags down
day-to-day development in the main repo.

Instead:

- Add `/termsurf-chromium/` to `.gitignore`
- Document the tracked branch/commit in `docs/chromium.md`
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

**Push local branches to origin.** Several local branches exist that have never
been pushed to `github.com/termsurf/termsurf-chromium`. Push them all before the
move:

| Branch                   | Status                      |
| ------------------------ | --------------------------- |
| `146.0.7650.0-termsurf`  | Local only — push to origin |
| `146.0.7650.0-issue-411` | Local only — push to origin |
| `146.0.7650.0-issue-412` | Local only — push to origin |
| `146.0.7650.0-issue-413` | Local only — push to origin |
| `146.0.7650.0-issue-414` | Local only — push to origin |
| `146.0.7650.0-issue-415` | Local only — push to origin |
| `146.0.7650.0-issue-416` | Local only — push to origin |
| `146.0.7650.0-electron`  | Local only — push to origin |
| `main`                   | Already on origin           |

**Verify ts4 test apps still work after the move.** Several ts4 apps and scripts
reference `ts4/termsurf-chromium/` paths (for `content_shell`, build output,
etc.). After moving `termsurf-chromium/` to the top level, update these
references:

- `ts4/scripts/build-phase*.sh` — build scripts that may reference
  `termsurf-chromium/src/out/Default/`
- `ts4/.gitignore` — references `termsurf-chromium/` subdirectories
- `.claude/skills/build-chromium/SKILL.md` — build instructions reference
  `ts4/termsurf-chromium/`
- `CLAUDE.md` — build commands and directory structure reference
  `ts4/termsurf-chromium/`
- Launchd plists for test receivers (Issues 414–416) — the `content_shell`
  sender path in the plist `ProgramArguments` may reference the old location
- Any hardcoded paths in experiment code (two-profiles-receiver,
  two-profiles-swift, two-profiles-rust)

After updating paths, verify that `content_shell` can still be built and that
the test senders (from Issues 414–416) can still connect to their receivers.

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

### 6. Merge Ghostty into `ts5/`

After the restructure is complete, merge the latest Ghostty into a `ts5/`
subdirectory. Keeping Ghostty in its own directory avoids conflicts with
TermSurf's top-level files (README.md, .gitignore, docs/, etc.) and makes future
upstream merges cleaner — Ghostty's files never collide with ours.

```
termsurf/                        (root — TermSurf repo)
│
├── ts5/            (Ghostty fork)
│   ├── src/                     (libghostty — Zig core)
│   ├── macos/                   (Ghostty macOS app — Swift)
│   ├── pkg/                     (Ghostty platform packages)
│   ├── build.zig                (Ghostty build system)
│   ├── build.zig.zon            (Ghostty dependencies)
│   └── include/                 (libghostty C API headers)
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

**Merge strategy:** Add Ghostty as a remote, fetch, then use `git merge` with
`--allow-unrelated-histories` into a temporary branch. Use `git read-tree` or
equivalent to place Ghostty's tree under `ts5/`, preserving Ghostty's full
commit history. This is the same pattern used by projects like git-subtree: the
upstream history is preserved, and future merges from Ghostty can be pulled and
re-prefixed into `ts5/`.

**After the merge:** `cd ts5 && zig build` should work. The macOS app builds
from `ts5/`. From this point forward, TermSurf development means modifying files
inside `ts5/` to add browser pane support, and the ts1–ts4 directories are
historical reference.

## Order of Operations

1. Move vendored/analysis repos to `vendor/`
2. Move `cef-rs/` to `vendor/cef-rs/` (update ts3 paths if needed)
3. Remove `termsurf-chromium` submodule from git tracking
4. Move `termsurf-chromium/` from `ts4/` to top level
5. Update `.gitignore`
6. Update `CLAUDE.md`
7. Commit the restructure
8. Merge Ghostty into `ts5/`
9. Verify build (`cd ts5 && zig build`)
10. Commit the merge

## Implementations

### Change 1: Move `termsurf-chromium` to top level

(To be filled out.)
