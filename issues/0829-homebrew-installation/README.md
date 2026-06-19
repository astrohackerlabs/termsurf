+++
status = "open"
opened = "2026-06-19"
+++

# Issue 829: Homebrew Installation

## Goal

Make TermSurf installable through Homebrew without requiring users to build
Chromium, Ghostboard, Roamium, or Web TUI from source.

The intended user path is a tap plus cask install that downloads prebuilt
Apple-silicon macOS artifacts, with an arm64 Tahoe package as the first
supported target.

## Background

TermSurf's current source build is too large for normal users because Roamium
depends on Chromium and a full Chromium build is expensive in disk, time, and
tooling. A public install path should therefore distribute prebuilt artifacts.

NuTorch solved the analogous distribution problem with a separate public
Homebrew tap and pinned GitHub Release assets. TermSurf should use the same
release discipline, but the packaging shape is different: TermSurf is an app
bundle plus binaries and Chromium resources, so a Homebrew cask is the better
primary install surface than a source-building formula.

The current frontend is Ghostboard, installed as `TermSurf.app`. Wezboard is
archived and must not be part of the new install path.

## Analysis

The Homebrew install should be based on a prebuilt release tarball, not a source
build fallback. A source fallback would imply building Chromium, which is not a
reasonable default for users.

The release artifact should contain the complete runtime closure needed for a
working TermSurf install:

- `TermSurf.app` from Ghostboard.
- The `web` CLI.
- Roamium.
- Chromium dylibs, resources, and any generated Roamium resources required at
  runtime.
- Any supporting files needed by the cask install layout.

The cask should install these artifacts into stable locations, likely:

- `TermSurf.app` into `/Applications`.
- `web` into Homebrew's bin path.
- Roamium and Chromium runtime resources into an opt directory such as
  `/opt/homebrew/opt/termsurf-roamium`.

The first target should be Apple silicon on Tahoe, matching the local release
environment. The issue should explicitly record the platform tag and the exact
artifact layout that the cask expects.

Homebrew 6.0+ may require third-party tap trust, so the documented user flow
should include the trust step if it applies:

```bash
brew tap termsurf/termsurf
brew trust termsurf/termsurf
brew install --cask termsurf
```

The tap should be a separate repository, `termsurf/homebrew-termsurf`, and the
TermSurf repo should keep the cask source or release-generation source of truth
in a predictable location.

## Acceptance Criteria

- A public Homebrew tap exists for TermSurf installation.
- The documented install command installs TermSurf from prebuilt artifacts,
  without building Chromium.
- The cask installs `TermSurf.app`, `web`, Roamium, and Chromium runtime
  resources into the expected locations.
- A cold install on a supported arm64 Tahoe system is verified from the public
  tap and release artifact.
- The installed app and CLI launch the repo-built runtime, not stale Wezboard or
  older installed artifacts.
- Release artifacts are pinned by sha256 in the cask.
- The issue records the release artifact names, platform target, install
  locations, and verification output.
- Current docs explain the Homebrew install path and the source-build fallback
  separately.

## Notes

Do not create experiments upfront. Design Experiment 1 after this issue is open.
