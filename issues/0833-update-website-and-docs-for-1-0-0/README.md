+++
status = "open"
opened = "2026-06-20"
+++

# Issue 833: Update Website and Docs for 1.0.0

## Goal

Update the website and any other out-of-date documentation so the public docs
accurately describe the current TermSurf `1.0.0` release, Homebrew installation
flow, Ghostboard app name, and Roamium install layout.

## Background

Issue 832 published TermSurf `1.0.0` to GitHub and the
`termsurf/homebrew-termsurf` tap. The verified Homebrew install layout is:

- `/Applications/TermSurf.app`
- `/opt/homebrew/bin/web`
- `/opt/homebrew/opt/termsurf-roamium/roamium`
- Chromium dylibs and resources under `/opt/homebrew/opt/termsurf-roamium/`

The website and repo docs still have some stale references from earlier release
and packaging states. Known stale or incomplete items include:

- `website/src/pages/docs/getting-started.astro` documents Homebrew install
  without the `brew trust termsurf/termsurf` step used in the README install
  flow.
- `website/src/pages/docs/components/roamium.astro` still says Roamium installs
  to `/usr/local/roamium/`.
- `README.md` contains an old debug app output path using
  `TermSurf Ghostboard.app`; the current app bundle is `TermSurf.app`.

This issue should audit website and documentation content rather than assume
these are the only stale references.

## Scope

Docs and website content only. Do not make app, protocol, Chromium, Homebrew
cask, or release-script changes unless a documentation audit reveals that a doc
claim cannot be made true without a separate code issue.

Likely files to audit:

- `website/src/pages/**/*.astro`
- `website/src/components/**/*.astro`
- `README.md`
- `docs/**/*.md`
- release and install documentation in `issues/0829-*` and `issues/0832-*` for
  historical reference only, without modifying closed issues.

Closed issues are historical records and must not be edited.

## Acceptance Criteria

- Website Homebrew install instructions include the current trusted-tap flow:

  ```bash
  brew tap termsurf/termsurf
  brew trust termsurf/termsurf
  brew install --cask termsurf
  ```

- Website and repo docs describe the current installed paths:
  - `/Applications/TermSurf.app`
  - `/opt/homebrew/bin/web`
  - `/opt/homebrew/opt/termsurf-roamium/`
- Stale `/usr/local/roamium` install-path claims are removed or clearly marked
  historical.
- Stale `TermSurf Ghostboard.app` app-bundle references are updated or clearly
  marked historical.
- Public-facing docs mention that Homebrew currently installs TermSurf `1.0.0`,
  or otherwise avoid stale version claims.
- Any Wezboard references in public docs are accurate: archived/historical, not
  part of the current product install.
- The website builds successfully after docs changes.
- The issue records the audit commands used to search for stale release,
  install, app-name, and archived-GUI references.

## Experiments

- [Experiment 1: Audit and refresh public docs](01-audit-and-refresh-public-docs.md)
  — **Pass**
