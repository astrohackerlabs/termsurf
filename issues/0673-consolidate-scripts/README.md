+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 673: Consolidate Scripts

Move all shell scripts into a single `scripts/` directory at the repo root.
Currently scripts are split across two locations (`*.sh` at the repo root and
`gui/scripts/*.sh`), making them hard to find.

## Current State

**Repo root (5 scripts):**

- `build-debug.sh`
- `build-release.sh`
- `deregister.sh`
- `install.sh`
- `rename-ghostty.sh`

**gui/scripts/ (2 scripts):**

- `clean-zig.sh`
- `generate-icons.sh`

## Experiment 1: Move all scripts to `scripts/`

### Hypothesis

Moving all 7 scripts into `scripts/` and updating internal path references will
consolidate them without breaking functionality.

### Changes

#### 1. Move top-level scripts

```bash
mkdir -p scripts
git mv build-debug.sh scripts/
git mv build-release.sh scripts/
git mv deregister.sh scripts/
git mv install.sh scripts/
git mv rename-ghostty.sh scripts/
```

#### 2. Move gui/scripts/ scripts

```bash
git mv gui/scripts/clean-zig.sh scripts/
git mv gui/scripts/generate-icons.sh scripts/
```

#### 3. Update path references in scripts

- `clean-zig.sh` — currently resolves `GUI_DIR` from `SCRIPT_DIR`'s parent.
  After moving from `gui/scripts/` to `scripts/`, the parent is the repo root,
  not `gui/`. Fix: set `GUI_DIR="$REPO_ROOT/gui"`.
- `generate-icons.sh` — same issue with `GHOST_DIR`. Fix: set
  `GUI_DIR="$REPO_ROOT/gui"`.
- Usage comments in both scripts need updating.

#### 4. Update documentation references

Search docs for `gui/scripts/` and update to `scripts/`.

#### 5. Remove empty `gui/scripts/` directory

```bash
rmdir gui/scripts/
```

### Test

1. `scripts/generate-icons.sh` — runs without errors, generates icons.
2. `scripts/clean-zig.sh` — runs without errors, clears cache.
3. `scripts/build-debug.sh` — runs without errors.
4. `scripts/rename-ghostty.sh` — runs without errors (dry run if possible).
5. All 7 scripts live in `scripts/`, no scripts at repo root or `gui/scripts/`.

### Result: PASS

All 7 scripts moved to `scripts/`. Path references fixed in 5 scripts
(`clean-zig.sh`, `generate-icons.sh`, `build-debug.sh`, `build-release.sh`,
`install.sh`). Two scripts needed no fixes (`deregister.sh` has no path refs,
`rename-ghostty.sh` uses `git rev-parse`). All scripts pass syntax check. No
scripts remain at the repo root or in `gui/scripts/`.

## Conclusion

All shell scripts consolidated into `scripts/` at the repo root. No more hunting
across two locations.

- 5 scripts moved from repo root → `scripts/`
- 2 scripts moved from `gui/scripts/` → `scripts/`
- Path references updated in 5 scripts to resolve `REPO_DIR` from `SCRIPT_DIR`'s
  parent
