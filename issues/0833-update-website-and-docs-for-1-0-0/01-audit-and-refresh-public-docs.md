# Experiment 1: Audit and Refresh Public Docs

## Description

Audit the public website and repo documentation for stale release, install,
Roamium path, app-name, and archived-GUI claims, then update the active docs to
match the `1.0.0` Homebrew release.

Closed issues and historical prototype docs should remain historical records. If
a stale-looking reference is intentionally historical, leave it alone or make
the surrounding active docs clear instead of rewriting history.

## Changes

Planned audit:

```bash
rg -n "0\\.1\\.|1\\.0\\.0|Homebrew|brew trust|brew install|/usr/local/roamium|/usr/local/bin/roamium|/usr/local/lib/roamium|TermSurf Ghostboard|TermSurf Ghostboard\\.app|Ghostboard\\.app|TermSurf\\.app|Wezboard|wezboard|Roamium installs|/opt/homebrew/opt/termsurf-roamium|/opt/homebrew/bin/web" website README.md docs -g'*.md' -g'*.astro' -g'*.tsx' -g'*.ts' -g'*.js'
```

Initial audit findings to fix:

1. `website/src/pages/docs/getting-started.astro`

   - Add `brew trust termsurf/termsurf` to the Homebrew install block.
   - Mention that the Homebrew cask currently installs TermSurf `1.0.0`.
   - Ensure Homebrew install locations are explicit:
     - `/Applications/TermSurf.app`
     - `/opt/homebrew/bin/web`
     - `/opt/homebrew/opt/termsurf-roamium/`
   - Keep source-build install documentation separate from Homebrew install
     documentation, because `scripts/install.sh webtui` currently installs `web`
     to `/usr/local/bin/web`.

2. `website/src/pages/docs/components/roamium.astro`

   - Replace the stale `/usr/local/roamium/` installed path with
     `/opt/homebrew/opt/termsurf-roamium/`.
   - Explain that the Homebrew install root contains the `roamium` binary,
     Chromium dylibs, `.pak` resources, ICU data, V8 snapshots, and generated
     resources under `gen/`.
   - Keep the source-build command block, but make it clear that
     `scripts/install.sh roamium` installs to the same current Roamium install
     root by default.

3. `README.md`

   - Update the stale debug app output path
     `ghostboard/macos/build/Debug/TermSurf Ghostboard.app` to
     `ghostboard/macos/build/Debug/TermSurf.app`.
   - Check nearby build/run text for any stale app executable names.

4. `docs/ghostboard-launch-discovery.md`

   - Update the stale debug binary example
     `TermSurf Ghostboard.app/Contents/MacOS/ghostboard` to the current debug
     app bundle and executable path.
   - Keep the `/usr/local/roamium` references in the debug no-fallback list if
     they are explicitly described as old/stale paths that Ghostboard must not
     use.

5. `website/src/pages/docs/architecture.astro`

   - Confirm the Wezboard wording says archived/historical and does not imply
     current install support. Update only if needed.

6. `docs/xdg.md`

   - Classify `TermSurf Ghostboard` references. Keep them only if they describe
     the current Ghostboard frontend/config identity; update them if they imply
     the old debug app bundle name or a stale installed app name.

Non-goals:

- Do not modify closed issue documents.
- Do not change Homebrew cask, release scripts, app code, Chromium, or packaging
  behavior.
- Do not deploy the website in this experiment. This experiment updates and
  verifies source content; deployment can be a separate explicit step if needed.

## Verification

Static verification:

```bash
prettier --write --prose-wrap always --print-width 80 README.md docs/ghostboard-launch-discovery.md issues/0833-update-website-and-docs-for-1-0-0/README.md issues/0833-update-website-and-docs-for-1-0-0/01-audit-and-refresh-public-docs.md
git diff --check
```

Website build verification:

```bash
cd website
bun run build
```

Post-change audit:

```bash
rg -n "/usr/local/roamium|/usr/local/bin/roamium|/usr/local/lib/roamium|TermSurf Ghostboard|TermSurf Ghostboard\\.app|Ghostboard\\.app|brew tap termsurf/termsurf|brew install --cask termsurf|brew trust termsurf/termsurf|0\\.1\\." website README.md docs -g'*.md' -g'*.astro' -g'*.tsx' -g'*.ts' -g'*.js'
```

Pass criteria:

- Active website Homebrew install instructions include
  `brew trust termsurf/termsurf`.
- Active website and repo docs use `/Applications/TermSurf.app`,
  `/opt/homebrew/bin/web`, and `/opt/homebrew/opt/termsurf-roamium/` for the
  current Homebrew install layout.
- Active docs no longer claim Roamium installs to `/usr/local/roamium/`.
- Active docs no longer describe the current debug app bundle as
  `TermSurf Ghostboard.app`.
- Wezboard is described only as archived/historical.
- `bun run build` succeeds in `website/`.
- Any remaining `/usr/local/roamium`, `/usr/local/bin/roamium`, or
  `TermSurf Ghostboard.app` matches are either gone or explicitly historical /
  negative examples.
- Any remaining `TermSurf Ghostboard` or `Ghostboard.app` matches refer to the
  current frontend/config identity, or are explicitly historical / negative
  examples, and do not describe the current app bundle name.

## Design Review

Adversarial review, fresh-context Codex subagent:

**Verdict:** Changes required.

Finding:

- Required: the planned audit searched for `TermSurf Ghostboard.app` but missed
  plain `TermSurf Ghostboard`, while active docs include that phrase in
  `docs/xdg.md`.

Fix:

- Broadened the planned and post-change audit regexes to include
  `TermSurf Ghostboard` and `Ghostboard.app`.
- Added `docs/xdg.md` to the planned audit list with explicit classification
  criteria.
- Added a pass criterion for any remaining `TermSurf Ghostboard` or
  `Ghostboard.app` matches.

Re-review:

**Verdict:** Approved.

The fresh-context reviewer verified that the planned audit, post-change audit,
`docs/xdg.md` classification, pass criteria, and README experiment link now
cover the prior finding. No new required findings remain.

## Result

**Result:** Pass.

Implemented the public documentation refresh:

- `website/src/pages/docs/getting-started.astro` now includes
  `brew trust termsurf/termsurf`, states that Homebrew currently installs
  TermSurf `1.0.0`, and names the current Homebrew paths:
  `/Applications/TermSurf.app`, `/opt/homebrew/bin/web`, and
  `/opt/homebrew/opt/termsurf-roamium/`.
- `website/src/pages/docs/components/roamium.astro` now documents the current
  Roamium install root and its required colocated Chromium resources.
- `README.md` now names the debug app bundle as `TermSurf.app`.
- `docs/ghostboard-launch-discovery.md` now names the debug app executable as
  `TermSurf.app/Contents/MacOS/termsurf`.
- `docs/xdg.md` now uses `TermSurf/Ghostboard` wording for config identity
  instead of the ambiguous `TermSurf Ghostboard` phrase.

Verification run:

```bash
prettier --write --prose-wrap always --print-width 80 README.md docs/ghostboard-launch-discovery.md docs/xdg.md issues/0833-update-website-and-docs-for-1-0-0/README.md issues/0833-update-website-and-docs-for-1-0-0/01-audit-and-refresh-public-docs.md
git diff --check
cd website
bun install --frozen-lockfile
bun run build
rg -n "/usr/local/roamium|/usr/local/bin/roamium|/usr/local/lib/roamium|TermSurf Ghostboard|TermSurf Ghostboard\\.app|Ghostboard\\.app|brew tap termsurf/termsurf|brew install --cask termsurf|brew trust termsurf/termsurf|0\\.1\\." website README.md docs -g'*.md' -g'*.astro' -g'*.tsx' -g'*.ts' -g'*.js'
rg -n "0\\.1\\.|1\\.0\\.0|Homebrew|brew trust|brew install|/usr/local/roamium|/usr/local/bin/roamium|/usr/local/lib/roamium|TermSurf Ghostboard|TermSurf Ghostboard\\.app|Ghostboard\\.app|TermSurf\\.app|Wezboard|wezboard|Roamium installs|/opt/homebrew/opt/termsurf-roamium|/opt/homebrew/bin/web" website README.md docs -g'*.md' -g'*.astro' -g'*.tsx' -g'*.ts' -g'*.js'
```

Notes:

- Root Prettier does not have an Astro parser installed, so Astro pages were
  validated by the successful website build instead of Prettier formatting.
- `bun install --frozen-lockfile` was required because `website/node_modules`
  was absent.
- `bun run build` succeeded and generated all 10 website pages.
- The post-change audit has expected remaining matches only:
  - current README Homebrew install commands;
  - explicit negative debug no-fallback examples for `/usr/local/roamium` and
    `/usr/local/bin/roamium` in `docs/ghostboard-launch-discovery.md`.
- The broad audit confirms the website uses the current Homebrew paths,
  `docs/early-prototypes.md` contains only historical prototype `TermSurf.app`
  references, and Wezboard is described as archived in git history.

## Conclusion

The active website and repo docs now describe the current `1.0.0` Homebrew
install flow and layout. No follow-up experiment is needed for the scoped docs
refresh unless the website should be deployed as a separate explicit action.

## Completion Review

Adversarial review, fresh-context Codex subagent:

**Verdict:** Approved.

The reviewer found no issues. Read-only checks passed for `git diff --check`,
targeted stale-reference audits, result commit absence, and the Roamium source
install default path in `scripts/install.sh`. The reviewer did not rerun
`bun run build` because it can generate `website/dist`; the implementation pass
had already run the build successfully.
