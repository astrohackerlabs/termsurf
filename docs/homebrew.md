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
- Roamium and Chromium runtime resources to
  `/opt/homebrew/opt/termsurf-roamium/`;
- Surfari and WebKit runtime resources to `/opt/homebrew/opt/termsurf-surfari/`.

The release tarball contains the same top-level package contract:

- `TermSurf.app/`;
- `web`;
- `roamium/`;
- `surfari/`.

## Verification

After install or upgrade, verify that `web` can open a page from inside TermSurf
without passing a repo-local browser path. That proves the installed app,
installed `web` binary, and installed Roamium runtime discovery path are working
together.

For local release validation, the smoke test should record elapsed time and
evidence such as logs or screenshots showing:

- `TermSurf.app` launched from the installed app path;
- the TermSurf socket was created;
- `web` opened a simple page such as `https://example.com`;
- Ghostboard resolved the installed Roamium path;
- the page rendered successfully.
