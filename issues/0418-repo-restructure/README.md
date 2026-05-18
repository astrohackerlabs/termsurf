+++
status = "closed"
opened = "2026-02-14"
closed = "2026-02-14"
+++

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
a top-level gitignored directory with upstream configured.

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
- `termsurf-chromium/src/` is the Chromium git repo; `termsurf-chromium/` also
  contains `depot_tools/`, `.gclient`, and other build infrastructure (Chromium
  requires the repo directory to be named `src/`)

**Remote configuration for `termsurf-chromium/src/`:**

| Remote   | URL                                                  | Purpose           |
| -------- | ---------------------------------------------------- | ----------------- |
| upstream | `https://chromium.googlesource.com/chromium/src.git` | Official Chromium |

We regularly pull `main` and tags from upstream. Our working branches follow the
pattern `{version}-termsurf` (e.g., `146.0.7650.0-termsurf`).

**Note:** There is no `origin` remote for now. The only remote is `upstream`
(Google's official Chromium source). Remote hosting for our fork will be decided
later — most likely as a patch set rather than a full fork, since Chromium is
too large for standard GitHub hosting. Anyone wanting to build TermSurf's
Chromium fork would fork Chromium themselves and apply our patches.

**Branch strategy:** Track the same Chromium version as Electron. This lets us
reference Electron's patches and solutions even though we use the Content API
directly (not Electron itself). To find the current version:

1. Check Electron's `DEPS` file for `chromium_version`
2. Use that version tag as our base
3. Create `{version}-termsurf` branch on top of the tag

**Local branches.** The following branches exist locally:

| Branch                   |
| ------------------------ |
| `146.0.7650.0-termsurf`  |
| `146.0.7650.0-issue-411` |
| `146.0.7650.0-issue-412` |
| `146.0.7650.0-issue-413` |
| `146.0.7650.0-issue-414` |
| `146.0.7650.0-issue-415` |
| `146.0.7650.0-issue-416` |
| `146.0.7650.0-electron`  |
| `main`                   |

These are local-only for now. They will be pushed to a remote once hosting is
decided.

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

### 3–5. (Completed)

Changes 3 (`.gitignore`), 4 (remove submodule), and 5 (`CLAUDE.md`) were folded
into the Change 1 and Change 2 implementations.

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
├── termsurf-chromium/           (gitignored — Chromium build workspace)
│   ├── src/                    (Chromium git repo)
│   ├── depot_tools/            (Chromium build tools)
│   └── .gclient                (gclient config)
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

**Merge strategy:** `git subtree add --prefix=ts5 upstream main` for the initial
import. This places Ghostty's tree under `ts5/` and creates a merge commit.
Future upstream merges use `git subtree pull --prefix=ts5 upstream main`. See
Experiments section below for why `git merge -X subtree` doesn't work with this
repo's history.

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

#### Step 1: Update remotes

The `termsurf-chromium/src/` repo currently has `origin` pointing to a local
path (`/Users/ryan/dev/termsurf-chromium/src`). Remove it and add `upstream`
instead.

```bash
cd ts4/termsurf-chromium/src

# Remove old origin (local path that will no longer be used)
git remote remove origin

# Add upstream (official Chromium — Google's canonical source)
git remote add upstream https://chromium.googlesource.com/chromium/src.git
```

Verify with `git remote -v` that only `upstream` exists.

#### Step 2: Convert submodule to standalone repo

The `src/` directory currently has a `.git` file (not a directory) containing a
relative path like `gitdir: ../../../.git/modules/ts4/termsurf-chromium/src`.
This pointer will break when we move the directory. Convert it to a standalone
repo first, before removing submodule tracking or moving anything:

```bash
cd ~/dev/termsurf

# Replace the .git file with the actual .git directory (62 GB of git data)
rm ts4/termsurf-chromium/src/.git
mv .git/modules/ts4/termsurf-chromium/src ts4/termsurf-chromium/src/.git

# Fix the core.worktree config inside the moved .git directory
# (it may point to the old absolute path)
cd ts4/termsurf-chromium/src
git config --unset core.worktree 2>/dev/null || true
cd ~/dev/termsurf
```

Verify with `cd ts4/termsurf-chromium/src && git status` that the repo works as
a standalone git repo.

#### Step 3: Remove the submodule

Now that the git data has been moved out of `.git/modules/`, remove the
submodule tracking from the main repo:

```bash
cd ~/dev/termsurf

# Remove submodule entry from the git index
git rm --cached ts4/termsurf-chromium/src

# Remove submodule config from .git/config
git config --remove-section submodule.ts4/termsurf-chromium/src

# Clean up the now-empty .git/modules directory
rm -rf .git/modules/ts4/termsurf-chromium

# Remove the submodule entry from .gitmodules
# (edit .gitmodules to remove the [submodule "ts4/termsurf-chromium/src"] block)
```

The ts2 and ts3 submodules (freetype, harfbuzz, etc.) remain unchanged.

#### Step 4: Move the directory

```bash
mv ts4/termsurf-chromium termsurf-chromium
```

The `.gclient` file inside `termsurf-chromium/` may need its URL updated since
there is no longer an origin remote. It currently references
`git@github.com:termsurf/termsurf-chromium.git`.

#### Step 5: Update `.gitignore`

Remove:

```
ts4/termsurf-chromium/depot_tools/
```

Add:

```
# Chromium build workspace (gitignored; src/ requires directory named "src")
/termsurf-chromium/
```

#### Step 6: Update `ts4/.gitignore`

Remove all `termsurf-chromium/` entries:

```
termsurf-chromium/.cipd/
termsurf-chromium/.gclient_entries
termsurf-chromium/.gclient_previous_sync_commits
termsurf-chromium/.gcs_entries
termsurf-chromium/_bad_scm/
termsurf-chromium/src/out/
```

These are no longer needed since the entire directory is gitignored at the top
level.

#### Step 7: Update Claude skills

**`.claude/skills/build-chromium/SKILL.md`** — All paths change from
`ts4/termsurf-chromium/` to `termsurf-chromium/`:

| Old path                                 | New path                             |
| ---------------------------------------- | ------------------------------------ |
| `ts4/termsurf-chromium/depot_tools`      | `termsurf-chromium/depot_tools`      |
| `ts4/termsurf-chromium/src`              | `termsurf-chromium/src`              |
| `ts4/termsurf-chromium/src/out/Default/` | `termsurf-chromium/src/out/Default/` |

Add a note that `docs/chromium.md` tracks the current branch, commit, and a
complete list of all branches with links to their issue docs. Any time the build
skill switches branches, updates the Chromium version, or creates a new branch,
it must also update `docs/chromium.md` accordingly.

**`.claude/skills/git-poet/SKILL.md`** — Update the Submodule Workflow section:

- `ts4/termsurf-chromium/src/` → `termsurf-chromium/src/`
- Remove the instruction to `git add ts4/termsurf-chromium/src` (no longer a
  submodule — Chromium is gitignored and tracked separately)
- Update the workflow to reflect that Chromium commits happen in the
  `termsurf-chromium/` repo independently, not as submodule pointer updates
- When committing a branch or version change in `termsurf-chromium/`, the
  corresponding commit in the main repo should update `docs/chromium.md`
- When creating a new Chromium branch, add it to the Branches table in
  `docs/chromium.md` with a link to the corresponding issue doc

#### Step 8: Update `CLAUDE.md`

Update the ts4 directory structure and build commands sections to reference
`termsurf-chromium/` at the top level instead of `ts4/termsurf-chromium/`.

#### Step 9: Create `docs/chromium.md`

Document the tracked branch, commit, remote configuration, and branch strategy
in a dedicated file so it's easy to find:

```markdown
# Chromium Fork

## Repository

| Remote   | URL                                                |
| -------- | -------------------------------------------------- |
| upstream | https://chromium.googlesource.com/chromium/src.git |

No `origin` remote for now. Remote hosting TBD (likely patch set distribution).

## Current State

- Branch: `146.0.7650.0-termsurf`
- Commit: `b2907d660628a`
- Base version: `146.0.7650.0` (tracking Electron's Chromium version)

## Branch Strategy

Track the same Chromium version as Electron. Branches are named
`{version}-termsurf` for the main working branch and `{version}-issue-{N}` for
issue-specific branches.

## Branches

| Branch                   | Issue                                        | Description                  |
| ------------------------ | -------------------------------------------- | ---------------------------- |
| `146.0.7650.0-termsurf`  | —                                            | Main working branch for v146 |
| `146.0.7650.0-electron`  | —                                            | Electron's v146 base         |
| `146.0.7650.0-issue-411` | [Issue 411](issues/0411-two-profiles-3.md)   | Two profiles experiment 3    |
| `146.0.7650.0-issue-412` | [Issue 412](issues/0412-one-profile.md)      | One profile                  |
| `146.0.7650.0-issue-413` | [Issue 413](issues/0413-one-profile-2.md)    | One profile experiment 2     |
| `146.0.7650.0-issue-414` | [Issue 414](issues/0414-two-profiles-xpc.md) | Two profiles via XPC         |
| `146.0.7650.0-issue-415` | [Issue 415](issues/0415-swift-receiver.md)   | Swift receiver               |
| `146.0.7650.0-issue-416` | [Issue 416](issues/0416-rust-receiver.md)    | Rust receiver                |

## Local Setup

The `termsurf-chromium/` directory at the repo root is a Chromium build
workspace, gitignored from the main repo. The `src/` subdirectory is the
Chromium git repo (Chromium requires it to be named `src/`). To set up from
scratch, use `fetch chromium` from depot_tools or clone from upstream and apply
our patches (patch distribution TBD).
```

#### Step 10: Verify test apps

The ts4 test apps (Issues 414–416) do **not** reference `ts4/termsurf-chromium/`
in their own code or plists. The launchd plists reference
`ts4/target/debug/receiver` (the Rust binary), not Chromium paths. The
`content_shell` sender is launched manually from the command line by pointing to
`termsurf-chromium/src/out/Default/Content Shell.app`. After the move, the only
change is the path used when manually launching `content_shell`:

```bash
# Old
ts4/termsurf-chromium/src/out/Default/Content\ Shell.app/Contents/MacOS/Content\ Shell

# New
termsurf-chromium/src/out/Default/Content\ Shell.app/Contents/MacOS/Content\ Shell
```

No code changes needed in the test apps themselves.

#### Step 11: Commit

Stage all changes (`.gitmodules`, `.gitignore`, `ts4/.gitignore`, `CLAUDE.md`,
`.claude/skills/build-chromium/SKILL.md`, `.claude/skills/git-poet/SKILL.md`,
`docs/chromium.md`, removal of submodule index entry) and commit.

### Change 2: Move vendored/analysis repos into `vendor/`

#### Step 1: Create `vendor/` directory

```bash
mkdir -p vendor
```

#### Step 2: Move gitignored directories

These directories are all gitignored (their own `.git` repos, not tracked by the
main repo). Moving them is a filesystem operation only — no git history impact:

```bash
mv wezterm vendor/wezterm
mv cef vendor/cef
mv alacritty vendor/alacritty
mv electron vendor/electron
mv chromium vendor/chromium
```

If any of these directories don't exist on a given machine, skip them.

#### Step 3: Move `cef-rs/`

`cef-rs/` is the only vendored directory that is **committed** (tracked by git).
It must be moved with `git mv` to preserve history:

```bash
git mv cef-rs vendor/cef-rs
```

#### Step 4: Update ts3 paths

ts3 has 13+ references to `../../cef-rs/` in Cargo.toml files and build scripts.
After moving to `vendor/cef-rs/`, the relative path from `ts3/` becomes
`../vendor/cef-rs/`. Files to update:

| File                                    | Old path           | New path                  |
| --------------------------------------- | ------------------ | ------------------------- |
| `ts3/termsurf-web/Cargo.toml`           | `../../cef-rs/cef` | `../../vendor/cef-rs/cef` |
| `ts3/termsurf-profile/Cargo.toml`       | `../../cef-rs/cef` | `../../vendor/cef-rs/cef` |
| `ts3/cef-test-profile/Cargo.toml`       | `../../cef-rs/cef` | `../../vendor/cef-rs/cef` |
| `ts3/cef-test-gui/Cargo.toml`           | `../../cef-rs/cef` | `../../vendor/cef-rs/cef` |
| `ts3/wezterm-gui/Cargo.toml`            | `../../cef-rs/cef` | `../../vendor/cef-rs/cef` |
| `ts3/scripts/build-debug.sh`            | `cef-rs`           | `vendor/cef-rs`           |
| `ts3/scripts/build-release.sh`          | `cef-rs`           | `vendor/cef-rs`           |
| `ts3/cef-test-scripts/build.sh`         | `cef-rs`           | `vendor/cef-rs`           |
| `ts3/cef-test-scripts/build-profile.sh` | `cef-rs`           | `vendor/cef-rs`           |

#### Step 5: Update `.gitignore`

Replace the analysis repos section:

```
# Analysis repos (not part of TermSurf)
/wezterm/
/electron/
/alacritty/
/cef/
/chromium/
```

With:

```
# Vendored analysis repos (not committed)
vendor/wezterm/
vendor/electron/
vendor/alacritty/
vendor/cef/
vendor/chromium/
```

Also update the cef-rs build output line:

```
# Old
cef-rs/target/

# New
vendor/cef-rs/target/
```

#### Step 6: Update `CLAUDE.md`

Update all references to `cef-rs/` to `vendor/cef-rs/`:

- Directory structure listing (`cef-rs/` → `vendor/cef-rs/`)
- Build commands (`cd cef-rs` → `cd vendor/cef-rs`)
- Key files section (all `cef-rs/` paths)

#### Step 7: Commit

Stage all changes (`vendor/cef-rs/` move, ts3 path updates, `.gitignore`,
`CLAUDE.md`) and commit.

## Experiments

### Experiment 1: Subtree merge with `-X subtree=ts5`

**Hypothesis:** Since our repo shares history with Ghostty (it was originally
forked), `git merge -X subtree=ts5 upstream/main` should map upstream's `/` to
`ts5/`. Since `ts5/` doesn't exist on our side, all upstream files should be
clean additions with no conflicts.

**Command:**

```bash
git fetch upstream
git merge -X subtree=ts5 upstream/main --no-commit
```

**Why not copy ts1?** ts5 starts from a clean upstream Ghostty. The ts1
modifications (WKWebView browser panes, `web` CLI command) are specific to ts1's
approach. ts5 will get different modifications (in-process Chromium via Content
API). Starting clean avoids carrying ts1-specific code that would immediately be
deleted.

**How the subtree merge works.** Since our repo shares history with Ghostty, git
can find the common ancestor (the Ghostty commit where we forked). From that
ancestor, our side moved files to `ts1/` and upstream kept modifying files at
`/`. The `-X subtree=ts5` option tells git to map upstream's `/` to our `ts5/`.
Since `ts5/` doesn't exist on our side, all upstream files are clean additions —
no conflicts.

**Submodules.** Ghostty has vendor submodules (glslang, SPIRV-Cross, fontconfig,
etc.). The subtree merge places Ghostty's `.gitmodules` at `ts5/.gitmodules`,
but git only reads the root `.gitmodules`. We must manually add ts5 submodule
entries to the root `.gitmodules` and initialize them. The entries mirror ts1's
but with `ts5/` prefix instead of `ts1/`.

#### Step 1: Fetch upstream and merge

```bash
git fetch upstream
git merge -X subtree=ts5 upstream/main --no-commit
```

This maps upstream's files to `ts5/` and stages them. Review the result before
committing — verify that `ts5/` contains the full Ghostty source tree and that
no files were accidentally placed at the root or in `ts1/`.

```bash
# Verify ts5/ looks right
ls ts5/
# Should see: build.zig, build.zig.zon, src/, macos/, pkg/, etc.

# Verify nothing changed in ts1/
git diff --cached --name-only | grep -v '^ts5/' | head -20
# Should be empty (no files outside ts5/)

git commit -m "Import Ghostty as ts5/"
```

Future upstream merges use the same command:

```bash
git fetch upstream
git merge -X subtree=ts5 upstream/main -m "Merge upstream Ghostty into ts5"
```

#### Step 2: Add ts5 submodule entries

Ghostty's submodules need entries in the root `.gitmodules`. Check what
`ts5/.gitmodules` contains (it comes from upstream) and add corresponding
entries to the root `.gitmodules` with `ts5/` prefix paths.

The current ts1 submodules are:

| Submodule   | ts1 path                 | ts5 path                 |
| ----------- | ------------------------ | ------------------------ |
| mach_defs   | `ts1/vendor/mach_defs`   | `ts5/vendor/mach_defs`   |
| glslang     | `ts1/vendor/glslang`     | `ts5/vendor/glslang`     |
| SPIRV-Cross | `ts1/vendor/SPIRV-Cross` | `ts5/vendor/SPIRV-Cross` |
| fontconfig  | `ts1/vendor/fontconfig`  | `ts5/vendor/fontconfig`  |
| utfcpp      | `ts1/vendor/utfcpp`      | `ts5/vendor/utfcpp`      |
| zig-libcxx  | `ts1/vendor/zig-libcxx`  | `ts5/vendor/zig-libcxx`  |
| zf          | `ts1/vendor/zf`          | `ts5/vendor/zf`          |
| zig-libxml2 | `ts1/vendor/zig-libxml2` | `ts5/vendor/zig-libxml2` |

**Important:** The upstream Ghostty may have added or removed submodules since
ts1 was created. Compare `ts5/.gitmodules` (from the import) with the ts1
entries above. Use whatever upstream has — do not copy ts1's entries blindly.

```bash
# Initialize ts5 submodules
git submodule update --init --recursive ts5/vendor/
```

#### Step 3: Add ts5 `.gitignore` entries

Add build output patterns for ts5 (mirroring ts1's patterns):

```gitignore
# TermSurf 5.0 / Ghostty (ts5/)
ts5/zig-cache/
ts5/.zig-cache/
ts5/zig-out/
ts5/build/
ts5/.flatpak-builder/
ts5/flatpak/builddir/
ts5/flatpak/repo/
ts5/result*
ts5/.nixos-test-history
ts5/example/*.wasm
ts5/test/ghostty
ts5/test/cases/**/*.actual.png
ts5/glad.zip
ts5/Box_test.ppm
ts5/Box_test_diff.ppm
ts5/ghostty.qcow2
ts5/vgcore.*
```

#### Step 4: Update `docs/issues/0002-merge-upstream.md`

Add a ts5 section mirroring the ts1 section. The merge command is:

```bash
git merge -X subtree=ts5 upstream/main -m "Merge upstream Ghostty into ts5"
```

ts5 starts with no TermSurf modifications, so the conflict guide is initially
empty. As we add browser pane support, document the modified files and conflict
resolution strategies.

Also update the Overview table to add the ts5 row and mark ts1 as historical.

#### Step 5: Update `.claude/skills/merge-upstream/SKILL.md`

Add `ts5` as a merge target for the `ghostty` argument. The skill should merge
into ts5 (active development), not ts1 (historical). Update:

- The usage section to note `ghostty` merges into `ts5/`
- The Upstream Repositories table
- The merge command
- The verify/test commands (`cd ts5 && zig build`)

#### Step 6: Update `CLAUDE.md`

Add a "TermSurf 5.0 (ts5/)" section as the active development target. This
section documents:

- Architecture (Ghostty fork + in-process Chromium)
- Directory structure (`ts5/src/`, `ts5/macos/`, `ts5/build.zig`, etc.)
- Build commands (`cd ts5 && zig build`)
- That ts5 starts as unmodified Ghostty, with browser pane integration TBD

Mark ts1 as "Legacy (superseded by ts5)".

#### Step 7: Verify build

```bash
cd ts5 && zig build
```

If this fails, debug and fix before committing. Common issues:

- Missing submodules (Step 3)
- Zig version mismatch (check ts5's `.zigversion` or `build.zig.zon`)

#### Step 8: Commit

Stage all changes (`.gitmodules`, `.gitignore`, `CLAUDE.md`,
`docs/issues/0002-merge-upstream.md`, `.claude/skills/merge-upstream/SKILL.md`)
and commit. The Ghostty import itself (Step 1) is already committed separately.

#### Result

Massive conflicts. Git's rename detection found the ts1/ renames (from when we
moved Ghostty's files into `ts1/`) and tried to merge upstream changes into
`ts1/` instead of `ts5/`. Specific problems:

1. **modify/delete conflicts** — `.github/workflows/*.yml` files were deleted on
   our side (moved to `ts1/`) but modified upstream. Git placed them at the root
   instead of `ts5/`.
2. **file location conflicts** — Upstream added new files in directories that
   git detected as renamed to `ts1/`. Git suggested moving them to `ts1/`, not
   `ts5/`.
3. **content conflicts in ts1/** — Files like `ts1/src/config/CApi.zig` got
   upstream changes merged into them. The subtree option was overridden by
   rename detection.
4. **rename/delete conflicts** — Files renamed to `ts1/` on our side but deleted
   upstream.

#### Conclusion

`-X subtree=ts5` does not override git's rename detection. When git detects that
`/foo` was renamed to `ts1/foo` on our side, it merges upstream's changes to
`/foo` into `ts1/foo` regardless of the subtree option. The subtree strategy
only works when there is no competing rename history.

### Experiment 2: Subtree merge with rename detection disabled

**Hypothesis:** Experiment 1 failed because rename detection found the
`/ → ts1/` renames and overrode the subtree mapping. The `-X no-renames`
strategy option disables rename detection entirely. Without rename detection,
git won't find the ts1/ renames and the `-X subtree=ts5` mapping should work
cleanly — placing all upstream files into `ts5/` as new additions.

**Command:**

```bash
git fetch upstream
git merge -X subtree=ts5 -X no-renames upstream/main --no-commit
```

The plan (Steps 1–8 from Experiment 1) remains the same — only the merge command
in Step 1 changes.

#### Result

Complete failure. No `ts5/` directory was created. All files landed at the root
as modify/delete conflicts. The `-X subtree=ts5` mapping was entirely ignored.

Without rename detection, git sees our side as having _deleted_ every Ghostty
file (the move to `ts1/` looks like a delete when renames aren't tracked). So
the three-way merge becomes:

- **Base:** files at `/`
- **Ours:** files deleted
- **Theirs:** files modified at `/`

Git reports modify/delete conflicts for every file upstream touched. The subtree
option doesn't apply to conflicted files — they stay at their upstream paths
(root level, not `ts5/`).

#### Conclusion

Disabling rename detection made things worse. In Experiment 1, at least some
files were placed (incorrectly into `ts1/`). Here, nothing was placed anywhere
useful. The subtree merge strategy fundamentally cannot handle this repo's
history — whether renames are detected or not, the `/ → ts1/` move poisons the
merge.

### Experiment 3: `git subtree add`

**Hypothesis:** `git merge -X subtree` fails because the three-way merge uses
the original fork point as the common ancestor, where the `/ → ts1/` move
poisons the result. `git subtree add` avoids this entirely — it doesn't do a
three-way merge against the fork point. Instead it creates a synthetic merge
commit that maps upstream's tree under the prefix. Future merges use
`git subtree pull`, which tracks changes since the last subtree operation
independently of our repo's rename history.

**Command:**

```bash
git fetch upstream
git subtree add --prefix=ts5 upstream main
```

Future upstream merges:

```bash
git fetch upstream
git subtree pull --prefix=ts5 upstream main
```

Steps 2–8 from Experiment 1 remain the same (submodules, `.gitignore`, docs,
build verification, commit).

#### Result

Success. `ts5/` contains the full Ghostty source tree (`build.zig`, `src/`,
`macos/`, `pkg/`, etc.). No files were placed outside `ts5/`. The command
created a merge commit with the upstream history mapped under the prefix.

#### Conclusion

`git subtree add` works. It bypasses the three-way merge against the fork point
entirely, avoiding the rename detection issues that broke Experiments 1 and 2.

### Experiment 4: Post-import configuration

Experiment 3 imported Ghostty into `ts5/`. This experiment completes the
remaining configuration: submodules, `.gitignore`, documentation, skills, and
build verification.

**Note:** Upstream Ghostty no longer uses submodules. The `ts5/vendor/`
directories (`glad`, `nerd-fonts`) are regular committed trees. No submodule
setup is needed for ts5 — skip Step 2 from Experiment 1.

#### Step 1: Update Change 6 description

The "Merge strategy" paragraph in Change 6 (line ~190) references
`git merge --allow-unrelated-histories` and `git read-tree`. Update it to
reflect the actual approach: `git subtree add --prefix=ts5 upstream main` for
the initial import, `git subtree pull --prefix=ts5 upstream main` for future
merges.

#### Step 2: Add ts5 `.gitignore` entries

Add build output patterns for ts5 (mirroring ts1's patterns):

```gitignore
# TermSurf 5.0 / Ghostty (ts5/)
ts5/zig-cache/
ts5/.zig-cache/
ts5/zig-out/
ts5/build/
ts5/.flatpak-builder/
ts5/flatpak/builddir/
ts5/flatpak/repo/
ts5/result*
ts5/.nixos-test-history
ts5/example/*.wasm
ts5/test/ghostty
ts5/test/cases/**/*.actual.png
ts5/glad.zip
ts5/Box_test.ppm
ts5/Box_test_diff.ppm
ts5/ghostty.qcow2
ts5/vgcore.*
```

#### Step 3: Update `docs/issues/0002-merge-upstream.md`

Add a ts5 section. Key differences from ts1:

- **Merge command:** `git subtree pull --prefix=ts5 upstream main` (not
  `git merge -X subtree`)
- **No submodules** — upstream Ghostty dropped them
- **No TermSurf modifications yet** — conflict guide is initially empty
- Update the Overview table to add the ts5 row
- Note that ts1 uses `git merge -X subtree=ts1` (historical) while ts5 uses
  `git subtree pull` (they use different merge mechanisms)

#### Step 4: Update `.claude/skills/merge-upstream/SKILL.md`

Add `ts5` as a merge target for the `ghostty` argument. The skill should merge
into ts5 (active development), not ts1 (historical). Update:

- The usage section to note `ghostty` merges into `ts5/`
- The Upstream Repositories table
- The merge command: `git subtree pull --prefix=ts5 upstream main`
- The verify/test commands (`cd ts5 && zig build`)

#### Step 5: Update `CLAUDE.md`

Add a "TermSurf 5.0 (ts5/)" section as the active development target:

- Architecture (Ghostty fork + in-process Chromium)
- Directory structure (`ts5/src/`, `ts5/macos/`, `ts5/build.zig`, etc.)
- Build commands (`cd ts5 && zig build`)
- That ts5 starts as unmodified Ghostty, with browser pane integration TBD
- Future upstream merges: `git subtree pull --prefix=ts5 upstream main`

Mark ts1 as "Legacy (superseded by ts5)".

#### Step 6: Verify build

```bash
cd ts5 && zig build
```

If this fails, debug and fix before committing. Common issues:

- Zig version mismatch (check ts5's `.zigversion` or `build.zig.zon`)

#### Step 7: Commit

#### Result

All steps completed. `.gitignore` updated with ts5 build patterns, `CLAUDE.md`
updated with ts5 as the active generation and ts4 marked superseded,
merge-upstream skill rewritten for `git subtree pull`, and `cd ts5 && zig build`
produces `ts5/zig-out/Ghostty.app` successfully.

## Conclusion

The repo restructure is complete. Three changes were made:

1. **Chromium moved to top level.** `termsurf-chromium/` is now a gitignored
   directory at the repo root, no longer a submodule inside ts4. Tracked in
   `docs/chromium.md`.

2. **Vendored repos moved to `vendor/`.** Analysis copies (wezterm, cef,
   alacritty, electron, chromium) and `cef-rs` now live under `vendor/`.

3. **Ghostty imported as ts5.** Three experiments were needed to find a working
   merge strategy. `git merge -X subtree` failed in both configurations
   (Experiments 1–2) because git's rename detection found the original
   `/ → ts1/` move and misrouted upstream changes. `git subtree add` bypassed
   this entirely (Experiment 3). Post-import configuration completed in
   Experiment 4.

The upstream merge strategy is documented in `docs/ghostty.md` and the
`/merge-upstream` skill. Future Ghostty merges use
`git subtree pull --prefix=ts5 upstream main`.
