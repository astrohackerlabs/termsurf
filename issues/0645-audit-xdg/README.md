+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 645: Audit XDG Support

## Goal

Confirm that TermSurf correctly uses the four XDG base directories — config,
data, state, and cache — with separate paths for each. If any are conflated or
missing, fix them.

| Variable          | Purpose                            | Default          |
| ----------------- | ---------------------------------- | ---------------- |
| `XDG_CONFIG_HOME` | Configuration files                | `~/.config`      |
| `XDG_DATA_HOME`   | Persistent data (state, databases) | `~/.local/share` |
| `XDG_STATE_HOME`  | State data (logs, history)         | `~/.local/state` |
| `XDG_CACHE_HOME`  | Non-essential cached data          | `~/.cache`       |

## Experiments

### Experiment 1: Audit current XDG usage

**Goal:** Document every XDG path usage in the codebase and identify problems.

#### All XDG path usages

| # | File                                  | Line    | XDG variable      | Subdir                                 | Resulting path                                               |
| - | ------------------------------------- | ------- | ----------------- | -------------------------------------- | ------------------------------------------------------------ |
| 1 | `gui/src/config/file_load.zig`        | 12      | `XDG_CONFIG_HOME` | `termsurf/config.ghostty`              | `~/.config/termsurf/config.ghostty`                          |
| 2 | `gui/src/config/file_load.zig`        | 21      | `XDG_CONFIG_HOME` | `termsurf/config`                      | `~/.config/termsurf/config`                                  |
| 3 | `gui/src/config/theme.zig`            | 33      | `XDG_CONFIG_HOME` | `termsurf/themes`                      | `~/.config/termsurf/themes/`                                 |
| 4 | `gui/src/apprt/xpc.zig`               | 707–736 | `XDG_DATA_HOME`   | `termsurf/chromium-profiles/{profile}` | `~/.local/share/termsurf/chromium-profiles/{profile}`        |
| 5 | `gui/src/crash/dir.zig`               | 9       | `XDG_STATE_HOME`  | `ghostty/crash`                        | `~/.local/state/ghostty/crash/`                              |
| 6 | `gui/src/cli/ssh-cache/DiskCache.zig` | 37      | `XDG_STATE_HOME`  | `ghostty/ssh_cache`                    | `~/.local/state/ghostty/ssh_cache`                           |
| 7 | `gui/src/cli/ssh_cache.zig`           | 88      | —                 | —                                      | Calls `DiskCache.defaultPath(alloc, "ghostty")` (same as #6) |
| 8 | `gui/src/crash/sentry.zig`            | 128     | `XDG_CACHE_HOME`  | `ghostty/sentry`                       | `~/.cache/ghostty/sentry/`                                   |

Note: sentry.zig line 120–126 has a macOS-specific path using
`NSCachesDirectory` when `XDG_CACHE_HOME` is not set. The XDG path on line 128
is the non-macOS fallback.

#### Problem 1: `ghostty` app name in state and cache paths

Config and data paths correctly use `termsurf/`:

- `~/.config/termsurf/config.ghostty` (#1)
- `~/.config/termsurf/themes/` (#3)
- `~/.local/share/termsurf/chromium-profiles/` (#4)

State and cache paths still use the upstream fork name `ghostty/`:

- `~/.local/state/ghostty/crash/` (#5) — should be `termsurf/crash`
- `~/.local/state/ghostty/ssh_cache` (#6) — should be `termsurf/ssh_cache`
- `~/.cache/ghostty/sentry/` (#8) — should be `termsurf/sentry`

Files to fix:

- `gui/src/crash/dir.zig:9` — `"ghostty/crash"` → `"termsurf/crash"`
- `gui/src/cli/ssh-cache/DiskCache.zig:37` — subdir comes from `program`
  parameter; callers at `DiskCache.zig:372` and `ssh_cache.zig:88` pass
  `"ghostty"` → should pass `"termsurf"`
- `gui/src/crash/sentry.zig:128–130` — `"ghostty/sentry"` → `"termsurf/sentry"`

#### Directory separation: correct

Each XDG directory is used for its intended purpose:

- **Config** (`XDG_CONFIG_HOME`) — config file, themes. Correct.
- **Data** (`XDG_DATA_HOME`) — Chromium profiles. Correct.
- **State** (`XDG_STATE_HOME`) — crash reports, SSH cache. Correct.
- **Cache** (`XDG_CACHE_HOME`) — Sentry cache. Correct.

No directory is being used for the wrong purpose.

**Result: Pass.** Audit complete. One problem found: three state/cache paths
still use the upstream `ghostty/` name instead of `termsurf/`.

## Conclusion

The audit is complete. TermSurf uses all four XDG base directories correctly —
config, data, state, and cache are each used for their intended purpose with no
conflation.

One problem found: three paths in state and cache still use the upstream
`ghostty/` app name instead of `termsurf/`. These are straightforward string
replacements in `crash/dir.zig`, `crash/sentry.zig`, `DiskCache.zig`, and
`ssh_cache.zig`. The rename can be done in a future issue when the branding
sweep continues.

**Status: Closed.**
