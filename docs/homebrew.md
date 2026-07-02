# Homebrew

TermSurf ships to macOS through the `termsurf/termsurf` Homebrew tap. The cask
currently targets Apple silicon macOS.

## Install

```bash
brew tap termsurf/termsurf
brew trust termsurf/termsurf
brew install --cask termsurf
```

To upgrade an existing install:

```bash
brew update
brew upgrade --cask termsurf
```

## Installed Layout

The cask installs:

- `TermSurf.app` to `/Applications/TermSurf.app`;
- `web` to the Homebrew binary path;
- `termsurf` to the Homebrew binary path;
- Roamium and Chromium runtime resources to
  `/opt/homebrew/opt/termsurf-roamium/`;
- Surfari and WebKit runtime resources to `/opt/homebrew/opt/termsurf-surfari/`;
- Girlbat prototype and Ladybird runtime resources to
  `/opt/homebrew/opt/termsurf-girlbat/`;
- GTUI Deno app assets to `/opt/homebrew/opt/termsurf-gtui/`.

The release tarball contains the same top-level package contract:

- `TermSurf.app/`;
- `web`;
- `termsurf`;
- `gtui/`;
- `roamium/`;
- `surfari/`;
- `girlbat/`.

## Verification

After install or upgrade, verify that `web` can open a page from inside TermSurf
without passing a repo-local browser path or setting browser path environment
variables. That proves the installed app, installed `web` binary, and installed
browser runtime discovery paths are working together.

For local release validation, the smoke test should record elapsed time and
evidence such as logs or screenshots showing:

- `TermSurf.app` launched from the installed app path;
- the TermSurf socket was created;
- `web --browser roamium https://example.com` opened and rendered the page;
- Ghostboard resolved the installed Roamium path:
  `/opt/homebrew/opt/termsurf-roamium/roamium`;
- `web --browser surfari https://example.com` opened and rendered the page;
- Ghostboard resolved the installed Surfari path:
  `/opt/homebrew/opt/termsurf-surfari/surfari`;
- `web --browser girlbat http://127.0.0.1:<fixture>/` opened the local fixture
  through the installed Girlbat prototype;
- Ghostboard resolved the installed Girlbat path:
  `/opt/homebrew/opt/termsurf-girlbat/bin/girlbat`;
- the installed Girlbat prototype loaded its resources from
  `/opt/homebrew/opt/termsurf-girlbat/Resources` and all non-system dylib
  dependencies resolved from `/opt/homebrew/opt/termsurf-girlbat/lib`;
- no smoke required `TERMSURF_ROAMIUM_PATH`, `TERMSURF_SURFARI_PATH`,
  `TERMSURF_GIRLBAT_PATH`, `TERMSURF_INSTALLED_ROAMIUM_PATH`,
  `TERMSURF_INSTALLED_SURFARI_PATH`, or `TERMSURF_INSTALLED_GIRLBAT_PATH`.

Girlbat is included as a prototype only. Its Homebrew presence proves installed
runtime packaging and gives us a testable Ladybird-backed engine, but it does
not imply browser parity, PDF parity, visual screenshot parity, or production
readiness.
