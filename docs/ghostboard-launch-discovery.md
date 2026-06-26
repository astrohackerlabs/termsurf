# Ghostboard Launch Discovery

Ghostboard has two local launch modes that must stay distinct while TermSurf is
under active development:

- debug runs from this repository; and
- installed distribution runs, which are tracked separately by Issue 819.

Issue 814 defines the debug contract. The goal is to make it obvious which
binary Ghostboard will spawn and to fail clearly instead of silently using an
old installed Roamium.

## Debug App Launch

The debug app binary is:

```bash
ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf
```

The geometry harness launches this binary directly from
`scripts/ghostboard-geometry-matrix.sh`. The app creates its normal terminal
session, listens on a PID-scoped TermSurf socket, and exposes that socket to
child shell commands through `TERMSURF_SOCKET`.

The `web` TUI discovers Ghostboard through `TERMSURF_SOCKET`. A successful debug
launch must show `HelloRequest` in the Ghostboard log before any browser launch
claim is trusted.

## Browser Selection

Ghostboard currently supports these browser selection rules:

| Web command                                   | Browser field received by Ghostboard       | Spawn behavior                                                      |
| --------------------------------------------- | ------------------------------------------ | ------------------------------------------------------------------- |
| `web --browser /absolute/path/to/roamium URL` | absolute path                              | Spawn exactly that path.                                            |
| `web URL`                                     | named/default `roamium`                    | Debug: resolve through `TERMSURF_ROAMIUM_PATH`; release: installed. |
| `web --browser relative-name URL`             | named browser other than supported default | Fail as unsupported.                                                |

In debug builds, the named/default `roamium` path is intentionally explicit:

- `TERMSURF_ROAMIUM_PATH` must be set;
- it must be an absolute path;
- debug harnesses set it to `chromium/src/out/Default/roamium`;
- missing, empty, or relative values fail with a clear
  `SetOverlay: named browser unresolved` log line; and
- Ghostboard must not fall through to `/usr/local/roamium`,
  `/usr/local/bin/roamium`, or `/opt/homebrew/opt/termsurf-roamium` during debug
  testing.

In non-debug builds, named/default `roamium` first accepts an absolute
`TERMSURF_ROAMIUM_PATH` if one is present, then resolves through installed
Roamium discovery. The canonical installed Roamium binary is:

```bash
/opt/homebrew/opt/termsurf-roamium/roamium
```

Release harnesses may set `TERMSURF_INSTALLED_ROAMIUM_PATH` to an absolute
Roamium binary path to test installed discovery without writing to
`/opt/homebrew`. This override is for release/installed discovery tests; it does
not change the debug no-installed-fallback contract.

Ghostboard keeps the pane/server/browser key as the requested browser name
(`roamium`) even when it spawns the executable from `TERMSURF_ROAMIUM_PATH`.
That preserves protocol identity: `BrowserReady` reports `browser=roamium`,
while the process spawn log records the resolved executable path.

## Harness Coverage

`scripts/ghostboard-geometry-matrix.sh launch-discovery-contract` validates the
launch contract without opening the GUI:

- the absolute-path command includes `--browser` with the debug Roamium path;
- the named/default command omits `--browser`;
- the named/default debug environment uses an absolute Roamium path; and
- the invalid-env sentinel is relative.

Runtime coverage is provided by:

- `scripts/ghostboard-geometry-matrix.sh initial-open` for the explicit absolute
  browser path;
- `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch` for
  default/named `roamium` resolving through `TERMSURF_ROAMIUM_PATH`; and
- `scripts/ghostboard-geometry-matrix.sh named-roamium-invalid-env` for clear
  failure without creating a pending `default/roamium` server or spawning a
  browser process; and
- `scripts/ghostboard-geometry-matrix.sh installed-roamium-release-launch` for
  release named/default `roamium` resolving through installed discovery without
  `TERMSURF_ROAMIUM_PATH`.

## Boundary With Issue 819

Issue 814 does not define the final installed distribution path. It defines the
debug contract and prevents accidental installed-binary fallback while the app
is being tested from the repository.

Issue 819 owns packaging identity and normal installed distribution behavior. It
defines the installed Roamium location as
`/opt/homebrew/opt/termsurf-roamium/roamium`, matching the Homebrew cask and
manual install scripts.
