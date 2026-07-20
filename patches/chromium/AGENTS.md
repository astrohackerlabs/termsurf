# Chromium Patch Workspace

Read this **and** [`../AGENTS.md`](../AGENTS.md) before modifying Chromium for
Astrohacker Terminal.

## Contract (MUST)

Obey the hub **Fork change contract** in full:

- Every intentional source edit → tracked `format-patch` under
  `patches/chromium/patches/issue-{ID}/` + monorepo pin update (this fork’s
  `README.md` + `patches/release-manifest.json` when the series changes) +
  record in the **current** issue experiment.
- Branch **must** include issue id and `exp{N}` (see local pattern below).
- Work is **incomplete** until the monorepo archive/pin is updated and
  commit-ready (not merely committed inside ignored `forks/chromium/`).

## Local details

- Source: `forks/chromium/src`
- Tools: `forks/chromium/depot_tools`
- Patches: `patches/chromium/patches/`
- Branch pattern:

  ```text
  {version}-issue-{ISSUE_ID}-exp{N}-{short-slug}
  ```

  Example shape: `150.0.7871.114-issue-26071814115751-exp1-warmup`.
  Issue-only `{version}-issue-{ID}` names are **not** sufficient for new
  experiment work.

- Archive style: **cumulative from base** — regenerate
  `base..HEAD` into `patches/chromium/patches/issue-{ID}/` per this fork’s
  `README.md` (Generating Patches), then commit the archive with the
  experiment result in the monorepo.

## Fork-specific hazards

- Do not commit Chromium source, `depot_tools`, gclient state, or build
  outputs to the Astrohacker repo.
- Never run `ninja` directly in Chromium’s build output; use `autoninja`.

## Learn more

- Reconstruction and current archives: [`README.md`](./README.md)
- Hub fork-change contract: [`../AGENTS.md`](../AGENTS.md)
- Shared patch policy: [`../README.md`](../README.md)
- Release series authority: [`../release-manifest.json`](../release-manifest.json)
