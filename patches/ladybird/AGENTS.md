# Ladybird Patch Workspace

Read this **and** [`../AGENTS.md`](../AGENTS.md) before modifying Ladybird for
Astrohacker Terminal.

## Contract (MUST)

Obey the hub **Fork change contract** in full:

- Every intentional source edit → tracked `format-patch` under
  `patches/ladybird/patches/issue-{ID}/` + monorepo pin update (this fork’s
  `README.md` + `patches/release-manifest.json` when the series changes) +
  record in the **current** issue experiment.
- Branch **must** include issue id and `exp{N}` (see local pattern below).
- Work is **incomplete** until the monorepo archive/pin is updated and
  commit-ready (not merely committed inside ignored `forks/ladybird/`).

## Local details

- Source: `forks/ladybird`
- Patches: `patches/ladybird/patches/`
- Branch pattern:

  ```text
  issue-{ISSUE_ID}-exp{N}-{short-slug}
  ```

- Archive style: follow this fork’s `README.md` (next ordered `NNNN` or
  full regenerate when the README says the archive is cumulative-from-base).

## Fork-specific hazards

- Do not commit Ladybird source or build outputs to the Astrohacker repo.

## Learn more

- Reconstruction and current archives: [`README.md`](./README.md)
- Hub fork-change contract: [`../AGENTS.md`](../AGENTS.md)
- Shared patch policy: [`../README.md`](../README.md)
- Release series authority: [`../release-manifest.json`](../release-manifest.json)
