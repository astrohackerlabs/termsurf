+++
status = "closed"
opened = "2026-02-21"
closed = "2026-02-21"
+++

# Issue 615: Use XDG directories correctly

## Goal

Chromium browser profile data is stored in `XDG_DATA_HOME`
(`~/.local/share/
termsurf/chromium-profiles/`), not `XDG_CONFIG_HOME`
(`~/.config/termsurf/`). The XDG directory pattern is documented in
`docs/xdg.md`.

## Background

The XDG Base Directory Specification defines three standard directories for user
data:

| Variable          | Default          | Purpose                                                    |
| ----------------- | ---------------- | ---------------------------------------------------------- |
| `XDG_CONFIG_HOME` | `~/.config`      | User configuration (settings, keybindings)                 |
| `XDG_DATA_HOME`   | `~/.local/share` | Persistent application data (databases, profiles, history) |
| `XDG_CACHE_HOME`  | `~/.cache`       | Regenerable cached data (thumbnails, compiled assets)      |

TermSurf currently stores everything under `~/.config/termsurf/`, including
Chromium browser profile data (cookies, localStorage, browsing history, cached
assets). This is wrong — browser profiles are application data, not user
configuration. A user should be able to delete `~/.config/termsurf/` and lose
their settings, not their browsing sessions.

### Current state

The active codebase (gui/) has one reference to the profile path:

- `gui/src/apprt/xpc.zig:638` — passes `--user-data-dir` to the Chromium Profile
  Server with path `~/.config/termsurf/chromium-profiles/<profile>`

Superseded generations (ts5, ts3, ts2) have similar references in their own code
but are not changed here — they are frozen.

CLAUDE.md documents the old paths in the ts4 and ts3 sections (PoC profile
paths, CEF profile paths). These are accurate for those generations and will be
left as-is.

### Target state

| Directory                                              | Contents                                          |
| ------------------------------------------------------ | ------------------------------------------------- |
| `~/.config/termsurf/`                                  | Ghostty configuration, TermSurf settings (future) |
| `~/.local/share/termsurf/chromium-profiles/<profile>/` | Chromium browser profile data                     |

The app should respect `XDG_DATA_HOME` if set, falling back to `~/.local/share`
when unset. Similarly, `XDG_CONFIG_HOME` is already respected by Ghostty for its
config file.

### What changes

1. **`gui/src/apprt/xpc.zig`** — Change `--user-data-dir` path from
   `$HOME/.config/termsurf/chromium-profiles/` to
   `$XDG_DATA_HOME/termsurf/chromium-profiles/` (falling back to
   `$HOME/.local/share/termsurf/chromium-profiles/`).

2. **`docs/xdg.md`** (new) — Document the XDG pattern: which directories
   TermSurf uses, what goes where, and how environment variables are respected.

3. **`CLAUDE.md`** — Add `docs/xdg.md` to the documentation index.

### What does NOT change

- Ghostty's own config path (`~/.config/ghostty/`) — owned by upstream
- Superseded generation code (ts5, ts3, ts2, ts1) — frozen
- Historical issue docs — left as-is
- `XDG_CACHE_HOME` — not needed yet; Chromium manages its own cache within the
  profile data directory

### Migration

Users who have existing Chromium profile data at
`~/.config/termsurf/chromium-profiles/` will need to move it manually. This is
acceptable — the project is pre-release and the number of affected users is
small. The old directory can be documented in the experiment conclusion.

## Experiments

### Experiment 1: Move Chromium profiles to XDG_DATA_HOME

#### Goal

Chromium Profile Server launches with `--user-data-dir` pointing to
`$XDG_DATA_HOME/termsurf/chromium-profiles/<profile>` (defaulting to
`~/.local/share/termsurf/chromium-profiles/<profile>`). A new `docs/xdg.md`
documents the pattern.

#### Steps

##### Step 1: Update `gui/src/apprt/xpc.zig`

In `spawnServerProcess`, read `XDG_DATA_HOME` with fallback to
`$HOME/.local/share`:

```zig
const data_home = std.posix.getenv("XDG_DATA_HOME") orelse blk: {
    var buf: [512]u8 = undefined;
    break :blk std.fmt.bufPrintZ(&buf, "{s}/.local/share", .{home}) catch {
        log.err("data home path too long", .{});
        return;
    };
};
```

Then change the `--user-data-dir` format string from:

```
--user-data-dir={s}/.config/termsurf/chromium-profiles/{s}
```

to:

```
--user-data-dir={s}/termsurf/chromium-profiles/{s}
```

passing `data_home` instead of `home`.

##### Step 2: Create `docs/xdg.md`

Document the XDG directory pattern:

- Which XDG variables TermSurf uses (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`)
- What goes in each directory
- How environment variables are respected with fallbacks
- The `termsurf` folder name convention

##### Step 3: Update `CLAUDE.md`

Add `docs/xdg.md` to the documentation index under the "TermSurf GUI (active)"
section.

##### Step 4: Build and verify

```bash
cd gui && zig build
```

Launch the app, run `web`, and confirm the Chromium Profile Server receives the
correct `--user-data-dir` path in the process arguments.

#### Verification

1. **Build succeeds:** `cd gui && zig build` completes without errors.
2. **Correct path in process args:** After running `web https://example.com`,
   `ps aux | grep 'Chromium Profile Server'` shows
   `--user-data-dir=.../.local/share/termsurf/chromium-profiles/default` (or the
   `XDG_DATA_HOME` value if set).
3. **Profile data created in new location:**
   `ls ~/.local/share/termsurf/chromium-profiles/` shows the profile directory.
4. **No data in old location:** No new files created under
   `~/.config/termsurf/chromium-profiles/`.

**Result:** Pass

Build succeeds. Chromium Profile Server launches with `--user-data-dir` pointing
to `~/.local/share/termsurf/chromium-profiles/<profile>`. Profile data is
created in the new location.

#### Conclusion

One line changed in xpc.zig — read `XDG_DATA_HOME` (falling back to
`$HOME/.local/share`) and use it for the `--user-data-dir` path. Users with
existing profile data at `~/.config/termsurf/chromium-profiles/` can move it to
`~/.local/share/termsurf/chromium-profiles/` to preserve their sessions.

## Conclusion

Chromium browser profile data now lives in
`XDG_DATA_HOME/termsurf/
chromium-profiles/` where it belongs. Configuration
stays in `XDG_CONFIG_HOME`. The pattern is documented in `docs/xdg.md`.
