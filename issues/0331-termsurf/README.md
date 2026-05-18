+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# Issue 331: Rename WezTerm to TermSurf

Rename user-facing elements from "WezTerm" to "TermSurf". Internal crate names
and Lua API (`require('wezterm')`) remain unchanged.

## Items to Rename

| Item                      | Current                        | Target                           | Location                                                     |
| ------------------------- | ------------------------------ | -------------------------------- | ------------------------------------------------------------ |
| CLI binary                | `wezterm`                      | `termsurf`                       | `ts3/wezterm/Cargo.toml`                                     |
| GUI binary                | `wezterm-gui`                  | `termsurf-gui`                   | `ts3/wezterm-gui/Cargo.toml`                                 |
| macOS app bundle          | `wezterm-gui.app`              | `termsurf-gui.app`               | `ts3/scripts/build-debug.sh`, `ts3/scripts/build-release.sh` |
| macOS menu bar            | "WezTerm"                      | "TermSurf"                       | `ts3/wezterm-gui/src/commands.rs`                            |
| Config directory          | `~/.config/wezterm/`           | `~/.config/termsurf/`            | `ts3/config/src/lib.rs`                                      |
| Config file               | `wezterm.lua` / `.wezterm.lua` | `termsurf.lua` / `.termsurf.lua` | `ts3/config/src/config.rs`                                   |
| CEF helper apps           | "WezTerm Helper"               | "TermSurf Helper"                | `ts3/scripts/build-debug.sh`, `ts3/scripts/build-release.sh` |
| CEF helper path (profile) | "WezTerm Helper"               | "TermSurf Helper"                | `ts3/termsurf-profile/src/main.rs`                           |
| CEF helper path (web)     | "WezTerm Helper"               | "TermSurf Helper"                | `ts3/termsurf-web/src/main.rs`                               |
| Bundle identifier         | `org.wezfurlong.wezterm`       | `com.termsurf.termsurf`          | `ts3/scripts/build-debug.sh`, `ts3/scripts/build-release.sh` |
| Bundle name               | "WezTerm"                      | "TermSurf"                       | `ts3/assets/macos/TermSurf.app/Contents/Info.plist`          |
| Bundle executable         | `wezterm-gui`                  | `termsurf-gui`                   | `ts3/scripts/build-debug.sh`, `ts3/scripts/build-release.sh` |

## Not Renaming

- Lua API: `require('wezterm')` stays as-is
- Internal crate names: `wezterm-term`, `wezterm-font`, `wezterm-client`, etc.
- GitHub issue URLs in comments
- Author attribution

---

## Experiment 1: Rename GUI binary ✓

Rename `wezterm-gui` to `termsurf-gui`.

**Status: Success**

### Changes

| File                           | Change                                           |
| ------------------------------ | ------------------------------------------------ |
| `ts3/wezterm-gui/Cargo.toml`   | `name = "wezterm-gui"` → `name = "termsurf-gui"` |
| `ts3/scripts/build-debug.sh`   | `cp .../wezterm-gui` → `cp .../termsurf-gui`     |
| `ts3/scripts/build-release.sh` | `cp .../wezterm-gui` → `cp .../termsurf-gui`     |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
# App should launch and function normally
```

---

## Experiment 2: Rename app bundle ✓

Rename `wezterm-gui.app` to `termsurf-gui.app`.

**Status: Success**

### Changes

| File                           | Change                                                  |
| ------------------------------ | ------------------------------------------------------- |
| `ts3/scripts/build-debug.sh`   | `APP_BUNDLE=.../wezterm-gui.app` → `.../termsurf-gui.app` |
| `ts3/scripts/build-release.sh` | `APP_BUNDLE=.../wezterm-gui.app` → `.../termsurf-gui.app` |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
# App should launch from termsurf-gui.app bundle
```

---

## Experiment 3: Rename menu bar items ✓

Rename "WezTerm" to "TermSurf" in macOS menu bar.

**Status: Success**

### Changes

| File                              | Change                            |
| --------------------------------- | --------------------------------- |
| `ts3/wezterm-gui/src/commands.rs` | "WezTerm" → "TermSurf" (8 places) |

### Locations

- Line 423: Menu order array
- Line 443: Menu bar check
- Line 447: Version string format
- Line 748: menubar array
- Line 1271: menubar array
- Line 1275: "Quit WezTerm" brief
- Line 1276: "Quits WezTerm" doc
- Line 1279: menubar array

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
# Check menu bar shows "TermSurf" instead of "WezTerm"
```

---

## Experiment 4: Rename bundle name ✓

Rename `CFBundleName` from "WezTerm" to "TermSurf" so the main menu bar shows "TermSurf".

**Status: Success**

### Changes

| File                           | Change                                      |
| ------------------------------ | ------------------------------------------- |
| `ts3/scripts/build-debug.sh`   | `<string>WezTerm</string>` → `<string>TermSurf</string>` |
| `ts3/scripts/build-release.sh` | `<string>WezTerm</string>` → `<string>TermSurf</string>` |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
# Main menu bar item should show "TermSurf" (bold, next to Apple logo)
```

---

## Experiment 5: Rename config directory and file ✓

Rename `~/.config/wezterm/wezterm.lua` to `~/.config/termsurf/termsurf.lua`.

**Status: Success** - Works when `WEZTERM_CONFIG_FILE` env var is not set. (When launching from WezTerm, this var is inherited and overrides the path.)

### Changes

| File                       | Line | Change                                  |
| -------------------------- | ---- | --------------------------------------- |
| `ts3/config/src/lib.rs`    | 386  | `"wezterm"` → `"termsurf"` (XDG path)   |
| `ts3/config/src/lib.rs`    | 388  | `"wezterm"` → `"termsurf"` (home path)  |
| `ts3/config/src/lib.rs`    | 398  | `"wezterm"` → `"termsurf"` (split path) |
| `ts3/config/src/config.rs` | 1009 | `".wezterm.lua"` → `".termsurf.lua"`    |
| `ts3/config/src/config.rs` | 1011 | `"wezterm.lua"` → `"termsurf.lua"`      |
| `ts3/config/src/config.rs` | 1025 | `"wezterm.lua"` → `"termsurf.lua"`      |
| `ts3/config/src/lua.rs`    | 230  | `".wezterm"` → `".termsurf"` (Lua path) |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
# Create ~/.config/termsurf/termsurf.lua with: return {}
# App should load the new config location
```

---

## Experiment 6: Rename config env vars ✓

Rename `WEZTERM_CONFIG_FILE` → `TERMSURF_CONFIG_FILE` and `WEZTERM_CONFIG_DIR` → `TERMSURF_CONFIG_DIR`.

**Status: Success**

### Changes

| File | Line | Change |
|------|------|--------|
| `ts3/config/src/config.rs` | 1029 | GET: `"WEZTERM_CONFIG_FILE"` → `"TERMSURF_CONFIG_FILE"` |
| `ts3/config/src/config.rs` | 1030 | log message update |
| `ts3/config/src/config.rs` | 1061 | REMOVE: `"WEZTERM_CONFIG_FILE"` → `"TERMSURF_CONFIG_FILE"` |
| `ts3/config/src/config.rs` | 1062 | REMOVE: `"WEZTERM_CONFIG_DIR"` → `"TERMSURF_CONFIG_DIR"` |
| `ts3/config/src/config.rs` | 1133 | SET: `"WEZTERM_CONFIG_FILE"` → `"TERMSURF_CONFIG_FILE"` |
| `ts3/config/src/config.rs` | 1135 | SET: `"WEZTERM_CONFIG_DIR"` → `"TERMSURF_CONFIG_DIR"` |
| `ts3/wezterm-mux-server-impl/src/sessionhandler.rs` | 862 | GET: `"WEZTERM_CONFIG_FILE"` → `"TERMSURF_CONFIG_FILE"` |
| `ts3/wezterm-gui/src/main.rs` | 564 | GET: `"WEZTERM_CONFIG_FILE"` → `"TERMSURF_CONFIG_FILE"` |
| `ts3/env-bootstrap/src/lib.rs` | 89 | REMOVE: `"WEZTERM_CONFIG_FILE"` → `"TERMSURF_CONFIG_FILE"` |
| `ts3/env-bootstrap/src/lib.rs` | 90 | REMOVE: `"WEZTERM_CONFIG_DIR"` → `"TERMSURF_CONFIG_DIR"` |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
# Set TERMSURF_CONFIG_FILE=/path/to/custom.lua before launch
# App should use the custom config path
```

---

## Experiment 7: Rename CEF helper apps ✓

Rename "WezTerm Helper" to "TermSurf Helper" in build scripts and source code.

**Status: Success**

### Changes

| File | Line | Change |
|------|------|--------|
| `ts3/scripts/build-debug.sh` | 90 | `WezTerm Helper` → `TermSurf Helper` (dst path) |
| `ts3/scripts/build-debug.sh` | 93 | Comment: `WezTerm` → `TermSurf` |
| `ts3/scripts/build-debug.sh` | 94 | sed: `WezTerm` → `TermSurf` |
| `ts3/scripts/build-debug.sh` | 96 | `WezTerm Helper` → `TermSurf Helper` (mv) |
| `ts3/scripts/build-release.sh` | 90 | `WezTerm Helper` → `TermSurf Helper` (dst path) |
| `ts3/scripts/build-release.sh` | 93 | Comment: `WezTerm` → `TermSurf` |
| `ts3/scripts/build-release.sh` | 94 | sed: `WezTerm` → `TermSurf` |
| `ts3/scripts/build-release.sh` | 96 | `WezTerm Helper` → `TermSurf Helper` (mv) |
| `ts3/termsurf-profile/src/main.rs` | 203 | `WezTerm Helper.app` → `TermSurf Helper.app` |
| `ts3/termsurf-profile/src/main.rs` | 204 | `WezTerm Helper` → `TermSurf Helper` |
| `ts3/termsurf-web/src/main.rs` | 298 | `WezTerm Helper.app` → `TermSurf Helper.app` |
| `ts3/termsurf-web/src/main.rs` | 301 | `WezTerm Helper` → `TermSurf Helper` |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com
# Webview should render (CEF helper path must match)
```
