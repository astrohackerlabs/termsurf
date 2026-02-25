# Issue 641: Chromium Patch Archive

## Goal

Store `git format-patch` output for every TermSurf Chromium branch in the main
repo at `chromium/patches/`. This makes the Chromium modifications portable,
reviewable, and recoverable without access to the full Chromium git history.

## Background

The Chromium fork at `chromium/src/` is a separate git repo, gitignored from the
main TermSurf repo. It contains 35 branches with TermSurf modifications on top
of the `146.0.7650.0` base tag. These branches are only accessible locally — if
the Chromium checkout is lost, all modifications are lost.

`git format-patch` converts commits into `.patch` files that preserve commit
messages, authorship, and diffs. These files are small (a few KB each) and can
be re-applied with `git am` to reconstruct any branch from a fresh Chromium
checkout.

## Current State

### Gitignore

The entire `chromium/` directory is gitignored via `/chromium/` in the root
`.gitignore`. This needs to change so that `chromium/patches/` and
`chromium/README.md` are tracked while everything else remains ignored.

Contents of `chromium/`:

| Path                             | Size     | What                                             | Track?  |
| -------------------------------- | -------- | ------------------------------------------------ | ------- |
| `src/`                           | ~100 GB+ | Chromium source tree (the git repo we modify)    | No      |
| `depot_tools/`                   | 647 MB   | Chromium build tools (gclient, gn, autoninja)    | No      |
| `_bad_scm/`                      | 402 MB   | Quarantined third_party from failed gclient sync | No      |
| `.cipd/`                         | 3.4 MB   | CIPD package manager cache (used by depot_tools) | No      |
| `.gclient`                       | 172 B    | gclient config (which repo to sync)              | No      |
| `.gclient_entries`               | 38 KB    | gclient dependency map                           | No      |
| `.gclient_previous_sync_commits` | 2 B      | gclient sync state                               | No      |
| `.gcs_entries`                   | 11 KB    | Google Cloud Storage dependency state            | No      |
| `patches/`                       | TBD      | Patch archive (this issue)                       | **Yes** |
| `README.md`                      | TBD      | Patch archive documentation (this issue)         | **Yes** |

Strategy: ignore everything in `chromium/`, then un-ignore what we want:

```gitignore
/chromium/*
!/chromium/patches/
!/chromium/README.md
```

This is safer than listing individual paths — any future gclient artifacts are
automatically ignored.

### Branches

35 branches exist locally. 33 are logged in `docs/chromium.md`. Two are missing:

| Branch                   | Issue doc                                           | Notes                            |
| ------------------------ | --------------------------------------------------- | -------------------------------- |
| `146.0.7650.0-issue-507` | None (`docs/issues/507-chromium.md` does not exist) | First Chromium streaming attempt |
| `146.0.7650.0-issue-635` | None (`docs/issues/635-*.md` does not exist)        | Persistent compositor work       |

### Branch types

Not all branches need patches:

| Branch                  | Type           | Patches? | Notes                                     |
| ----------------------- | -------------- | -------- | ----------------------------------------- |
| `146.0.7650.0`          | Upstream tag   | No       | Vanilla Chromium, the base for everything |
| `146.0.7650.0-electron` | Electron base  | No       | Electron's unmodified Chromium            |
| `146.0.7650.0-termsurf` | TermSurf base  | Yes      | 5 foundational commits on top of tag      |
| `146.0.7650.0-issue-*`  | Issue branches | Yes      | Each adds commits for a specific issue    |

### Patch generation

For each branch, generate patches relative to the upstream tag:

```bash
cd chromium/src
git format-patch 146.0.7650.0..<branch> -o ../../chromium/patches/<name>/
```

This produces one `.patch` file per commit, numbered sequentially
(`0001-*.patch`, `0002-*.patch`, etc.). Each patch folder contains the complete
set of modifications needed to reconstruct that branch from the vanilla tag.

## Plan

### Step 1: Update `.gitignore`

Replace `/chromium/` with a wildcard-and-exclude pattern:

```gitignore
/chromium/*
!/chromium/patches/
!/chromium/README.md
```

This ignores everything in `chromium/` (src, depot_tools, gclient files,
\_bad_scm, .cipd) while tracking only the patch archive and its README.

### Step 2: Add missing branches to `docs/chromium.md`

Add `146.0.7650.0-issue-507` and `146.0.7650.0-issue-635` to the Branches table.

### Step 3: Generate patches

For each TermSurf branch, run `git format-patch` and store output in
`chromium/patches/<name>/`:

- `chromium/patches/termsurf/` — 5 patches (base modifications)
- `chromium/patches/issue-411/` — patches for Issue 411
- `chromium/patches/issue-412/` — patches for Issue 412
- ... (one folder per issue branch)
- `chromium/patches/issue-639/` — patches for Issue 639

### Step 4: Create `chromium/README.md`

Document the patch archive: what it is, how to apply patches, how to generate
new patches, and the relationship between patches and branches.

### Step 5: Update `docs/chromium.md`

Add a Patches section explaining the archive and how to use it.
