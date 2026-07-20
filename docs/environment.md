# Environment Variables

Canonical taxonomy for Astrohacker process environment variables and packaging
script locals. Agents and humans should add new names only under this scheme.

## Taxonomy

| Prefix | Use for |
| --- | --- |
| `TERMSURF_*` | TermSurf **protocol** and pane/session IPC; protocol-adjacent embedding traces |
| `ASTROHACKER_TERMINAL_*` | Desktop host product packaging, release, smoke, install knobs |
| `ASTROHACKER_SHELL_*` | Shell component (reserved if needed later) |
| `ASTROHACKER_{CHROMIUM,WEBKIT,LADYBIRD}_*` | Engine helper product overrides |

**Do not** introduce process environment variables named `AHT_*`, `AHE_*`,
`AHW_*`, or retired codenames (`ROAMIUM`, `SURFARI`, `GIRLBAT`, `GHOSTBOARD`).

Script-local bash variables may use `AHTERM_*` to match shipped binary
`ahterm` (they are not process env).

## Protocol (keep `TERMSURF_*`)

| Variable | Role |
| --- | --- |
| `TERMSURF_SOCKET` | Host↔client IPC socket |
| `TERMSURF_PANE_ID` | Pane id for `ahweb` |
| `TERMSURF_ENGINE_STARTUP_TRACE` / `_FILE` | Engine warmup/startup traces |
| `TERMSURF_PDF_INPUT_TRACE` / `_FILE` | PDF input traces |
| `TERMSURF_WEBTUI_STATE_TRACE_FILE` | WebTUI debug state |
| `TERMSURF_BROWSER_STARTUP_TRACE` | Host browser-startup trace |
| `TERMSURF_GEOMETRY_TRACE` / `TERMSURF_GEOMETRY_SCENARIO` | Geometry harness |
| `TERMSURF_DEVTOOLS_RESERVATION_TIMEOUT_MS` | DevTools reservation |
| `TERMSURF_GTUI_APP_PATH` / `TERMSURF_DENO_PATH` | GTUI app/runtime discovery |

Generated protobuf symbols `TERMSURF__TERM_SURF_MESSAGE__MSG_*` are protocol
API, not user env.

## Product packaging / release (primary)

| Variable | Role |
| --- | --- |
| `ASTROHACKER_TERMINAL_RELEASE_PUBLISH` | Publish mode for `scripts/release.sh` |
| `ASTROHACKER_TERMINAL_RELEASE_PACKAGE_ONLY` | Package-only mode |
| `ASTROHACKER_TERMINAL_PUBLIC_REPO` | Public source checkout path |
| `ASTROHACKER_TERMINAL_PUBLIC_GITHUB_REPO` | GitHub repo `org/name` |
| `ASTROHACKER_TERMINAL_HOMEBREW_TAP_REPO` | Tap checkout path |
| `ASTROHACKER_TERMINAL_PRIVATE_REPO` | Private monorepo path (sync) |
| `ASTROHACKER_TERMINAL_SYNC_ALLOW_DIRTY` | Allow dirty private tree for sync |
| `ASTROHACKER_TERMINAL_SKIP_POSTFLIGHT_WARMUP` | Skip Homebrew cask postflight browser warmups |
| `HOMEBREW_ASTROHACKER_TERMINAL_SKIP_POSTFLIGHT_WARMUP` | Homebrew-prefixed skip |
| `ASTROHACKER_TERMINAL_SMOKE_VERSION` | Installed smoke expected version |
| `ASTROHACKER_VERSION` | Bundle / product version stamp at build time |

Retired public direct-install vars (not active product policy):
`ASTROHACKER_DIRECT_SKIP_WARMUP`, `ASTROHACKER_DIRECT_WARMUP_TIMEOUT_SEC`,
`ASTROHACKER_DIRECT_WARMUP_TEST`, `ASTROHACKER_DIRECT_TEST_CODESIGN_FAIL`.

Legacy dual-read aliases still accepted by packaging scripts (deprecated for
new docs/scripts):

- `TERMSURF_RELEASE_PUBLISH` / `TERMSURF_RELEASE_PACKAGE_ONLY`
- `TERMSURF_PUBLIC_REPO` / `TERMSURF_PUBLIC_GITHUB_REPO` /
  `TERMSURF_HOMEBREW_TAP_REPO` / `TERMSURF_PRIVATE_REPO` /
  `TERMSURF_SYNC_ALLOW_DIRTY`
- `TERMSURF_VERSION`
- `TERMSURF_SMOKE_VERSION`
- `HOMEBREW_TERMSURF_SKIP_POSTFLIGHT_WARMUP`


## Engine path overrides

Resolution order for each engine family:

1. Primary product env if nonempty absolute path  
2. Legacy codename env if nonempty absolute (deprecated; logged)  
3. Non-debug: legacy installed override if nonempty absolute (deprecated)  
4. Non-debug: installed default under
   `/opt/homebrew/opt/astrohacker-terminal-ah-*`  
5. Debug without valid override: unset/null  

| Primary | Legacy dual-read (deprecated) |
| --- | --- |
| `ASTROHACKER_CHROMIUM_PATH` | `TERMSURF_ROAMIUM_PATH`, `TERMSURF_INSTALLED_ROAMIUM_PATH` |
| `ASTROHACKER_WEBKIT_PATH` | `TERMSURF_SURFARI_PATH`, `TERMSURF_INSTALLED_SURFARI_PATH` |
| `ASTROHACKER_LADYBIRD_PATH` | `TERMSURF_GIRLBAT_PATH`, `TERMSURF_INSTALLED_GIRLBAT_PATH` |
| `ASTROHACKER_GECKO_PATH` | (none; no legacy alias) |

## Rename map (product knobs)

| Old / legacy | New primary |
| --- | --- |
| `TERMSURF_ROAMIUM_PATH` / `TERMSURF_INSTALLED_ROAMIUM_PATH` | `ASTROHACKER_CHROMIUM_PATH` |
| `TERMSURF_SURFARI_PATH` / `TERMSURF_INSTALLED_SURFARI_PATH` | `ASTROHACKER_WEBKIT_PATH` |
| `TERMSURF_GIRLBAT_PATH` / `TERMSURF_INSTALLED_GIRLBAT_PATH` | `ASTROHACKER_LADYBIRD_PATH` |
| `TERMSURF_GHOSTBOARD_APP` / `TERMSURF_RELEASE_GHOSTBOARD_APP` | `ASTROHACKER_TERMINAL_APP` (when harnesses migrate) |
| `TERMSURF_WEB` / `TERMSURF_RELEASE_WEB` | `ASTROHACKER_WEB_PATH` (when harnesses migrate) |
| `TERMSURF_SMOKE_VERSION` | `ASTROHACKER_TERMINAL_SMOKE_VERSION` |
| `TERMSURF_RELEASE_*` / `TERMSURF_PUBLIC_*` / `TERMSURF_HOMEBREW_*` | matching `ASTROHACKER_TERMINAL_*` |

## Packaging script locals (bash, not process env)

| Old | New |
| --- | --- |
| `AHT_APP` | `AHTERM_APP` |
| `AHT_RELEASE_APP` | `AHTERM_RELEASE_APP` |

## Structural check

```sh
scripts/check-env-var-names.sh
```
