# Gecko Patch Workspace

Read this **and** [`../AGENTS.md`](../AGENTS.md) before modifying Gecko/Firefox
for Astrohacker Terminal.

## Contract (MUST)

Obey the hub **Fork change contract** in full:

- Every intentional source edit → tracked `format-patch` under
  `patches/gecko/patches/issue-{ID}/` + monorepo pin update (this fork’s
  `README.md` + `patches/release-manifest.json` when the series changes) +
  record in the **current** issue experiment.
- Branch **must** include issue id and `exp{N}` (see local pattern below).
- Work is **incomplete** until the monorepo archive/pin is updated and
  commit-ready (not merely committed inside ignored `forks/gecko/`).

## Local details

- Source: `forks/gecko`
- Objdir convention: `forks/gecko/obj-astrohacker-ff`
- Toolchains: `$MOZBUILD_STATE_PATH` (default `~/.mozbuild`)
- Patches: `patches/gecko/patches/` (created when product patches exist)
- Branch pattern:

  ```text
  {short8}-issue-{ISSUE_ID}-exp{N}-{short-slug}
  ```

  Example shape: `ffe9a294-issue-26071212001982-exp1-iosurface`.
  `{short8}-issue-{ID}` without `exp{N}` is **not** sufficient for new
  experiment work.

- Archive style: follow this fork’s `README.md` after committing intentional
  Astrohacker changes on the experiment branch.

## Fork-specific hazards

- Do not commit Firefox source, `obj-*` build trees, `mozconfig`, or
  `~/.mozbuild` toolchains to the Astrohacker monorepo.
- Upstream remote (Issue 26071112000932+):
  `https://github.com/mozilla-firefox/firefox.git`, default branch **`main`**.
  Historical `mozilla/gecko-dev` master is frozen and must not be used as the
  product tip.
- Prefer full (non-artifact) desktop builds for embedding work:
  `ac_add_options --enable-application=browser` without
  `--enable-artifact-builds`.
- Bootstrap and build with a Python **3.9–3.12** interpreter (pin via
  Homebrew `python@3.12` when the system Python is newer).
- Product names: selector `gecko`, helper `ah-geckod`, C ABI
  `libtermsurf_gecko`. Do not use historical codenames as user-facing names.
- Gecko / `ah-geckod` is **not** in the Homebrew ship set unless an issue
  explicitly changes that.

## Learn more

- Reconstruction and notes: [`README.md`](./README.md)
- Hub fork-change contract: [`../AGENTS.md`](../AGENTS.md)
- Shared patch policy: [`../README.md`](../README.md)
- Release series authority: [`../release-manifest.json`](../release-manifest.json)
