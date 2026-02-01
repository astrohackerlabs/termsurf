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

## Experiment 2: Rename app bundle

Rename `wezterm-gui.app` to `termsurf-gui.app`.

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
