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

#### Step 1: Push local branches to origin

The `termsurf-chromium/src/` repo currently has `origin` pointing to a local
path (`/Users/ryan/dev/termsurf-chromium/src`). Seven local branches have never
been pushed to `github.com/termsurf/termsurf-chromium`.

```bash
cd ts4/termsurf-chromium/src

# Change origin to the GitHub fork
git remote set-url origin git@github.com:termsurf/termsurf-chromium.git

# Add upstream (official Chromium mirror)
git remote add upstream https://github.com/chromium/chromium.git

# Push all local branches to origin
git push origin 146.0.7650.0-termsurf
git push origin 146.0.7650.0-issue-411
git push origin 146.0.7650.0-issue-412
git push origin 146.0.7650.0-issue-413
git push origin 146.0.7650.0-issue-414
git push origin 146.0.7650.0-issue-415
git push origin 146.0.7650.0-issue-416
```

Verify with `git branch -r` that all branches appear on origin.

#### Step 2: Remove the submodule

From the main repo root:

```bash
cd ~/dev/termsurf

# Remove submodule entry from the git index
git rm --cached ts4/termsurf-chromium/src

# Remove submodule config from .git/config
git config --remove-section submodule.ts4/termsurf-chromium/src

# Remove submodule metadata
rm -rf .git/modules/ts4/termsurf-chromium

# Remove the submodule entry from .gitmodules
# (edit .gitmodules to remove the [submodule "ts4/termsurf-chromium/src"] block)
```

The ts2 and ts3 submodules (freetype, harfbuzz, etc.) remain unchanged.

#### Step 3: Convert submodule to standalone repo

The `src/` directory currently has a `.git` file (not a directory) containing a
relative path like `gitdir: ../../../.git/modules/ts4/termsurf-chromium/src`.
After the `git rm --cached` in Step 2, this pointer still exists but will break
when we move the directory. Convert it to a standalone repo before moving:

```bash
# Read the current gitdir path
cat ts4/termsurf-chromium/src/.git
# e.g.: gitdir: ../../../.git/modules/ts4/termsurf-chromium/src

# Replace the .git file with the actual .git directory
rm ts4/termsurf-chromium/src/.git
mv .git/modules/ts4/termsurf-chromium/src ts4/termsurf-chromium/src/.git

# Fix the core.worktree config inside the moved .git directory
# (it may point to the old absolute path)
cd ts4/termsurf-chromium/src
git config --unset core.worktree 2>/dev/null || true
cd ~/dev/termsurf
```

#### Step 4: Move the directory

```bash
mv ts4/termsurf-chromium termsurf-chromium
```

The `.gclient` file inside `termsurf-chromium/` already has the correct URL
(`git@github.com:termsurf/termsurf-chromium.git`) and does not need updating.

#### Step 5: Update `.gitignore`

Remove:

```
ts4/termsurf-chromium/depot_tools/
```

Add:

```
# Chromium fork (managed separately, shallow clone of termsurf/termsurf-chromium)
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

| Remote   | URL                                           |
| -------- | --------------------------------------------- |
| origin   | git@github.com:termsurf/termsurf-chromium.git |
| upstream | https://github.com/chromium/chromium.git      |

## Current State

- Branch: `146.0.7650.0-termsurf`
- Commit: `b2907d660628a`
- Base version: `146.0.7650.0` (tracking Electron's Chromium version)

## Branch Strategy

Track the same Chromium version as Electron. Branches are named
`{version}-termsurf` for the main working branch and `{version}-issue-{N}` for
issue-specific branches.

## Branches

| Branch                   | Issue                                       | Description                  |
| ------------------------ | ------------------------------------------- | ---------------------------- |
| `146.0.7650.0-termsurf`  | —                                           | Main working branch for v146 |
| `146.0.7650.0-electron`  | —                                           | Electron's v146 base         |
| `146.0.7650.0-issue-411` | [Issue 411](issues/411-two-profiles-3.md)   | Two profiles experiment 3    |
| `146.0.7650.0-issue-412` | [Issue 412](issues/412-one-profile.md)      | One profile                  |
| `146.0.7650.0-issue-413` | [Issue 413](issues/413-one-profile-2.md)    | One profile experiment 2     |
| `146.0.7650.0-issue-414` | [Issue 414](issues/414-two-profiles-xpc.md) | Two profiles via XPC         |
| `146.0.7650.0-issue-415` | [Issue 415](issues/415-swift-receiver.md)   | Swift receiver               |
| `146.0.7650.0-issue-416` | [Issue 416](issues/416-rust-receiver.md)    | Rust receiver                |

## Local Setup

The `termsurf-chromium/` directory at the repo root is a shallow clone,
gitignored from the main repo. To set up from scratch:

    git clone --depth 1 --branch 146.0.7650.0-termsurf \
      git@github.com:termsurf/termsurf-chromium.git termsurf-chromium/src
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
