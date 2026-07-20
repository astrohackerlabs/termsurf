# Reedline Pin Workspace

Read this **and** [`../AGENTS.md`](../AGENTS.md) before changing the Reedline
pin for Astrohacker Shell.

## Contract (MUST)

Obey the hub **Fork change contract** in full when Reedline **source** is
edited:

- Every intentional source edit → tracked `format-patch` under
  `patches/reedline/patches/issue-{ID}/` + monorepo pin update (this fork’s
  `README.md` + `patches/release-manifest.json` when the series changes) +
  record in the **current** issue experiment.
- Branch **must** include issue id and `exp{N}` (see local pattern below).
- Work is **incomplete** until the monorepo archive/pin is updated and
  commit-ready (not merely committed inside ignored `forks/reedline/`).

**Pin-only default:** this workspace is normally a **tip pin only** (no
product source patch). That is allowed only when there is **no** intentional
Astrohacker source edit. Document pin-only state in the issue archive README
when that is the product input. Do not invent no-op commits or empty
`.patch` files for pin-only state. The moment you edit source, the full
contract applies.

## Local details

- Source: `forks/reedline`
- Archives / notes: `patches/reedline/patches/`
- Branch pattern (when source is edited):

  ```text
  issue-{ISSUE_ID}-exp{N}-{short-slug}
  ```

- Archive style: start an issue-scoped archive and regenerate with
  `git format-patch` per the hub contract and this fork’s `README.md`.
- Consumers: `forks/nushell` path dep and `rust/ahsh`.

## Fork-specific hazards

- Do not commit Reedline source or build outputs to the Astrohacker repo.

## Learn more

- Pin identity and verify steps: [`README.md`](./README.md)
- Hub fork-change contract: [`../AGENTS.md`](../AGENTS.md)
- Shared patch policy: [`../README.md`](../README.md)
- Release series authority: [`../release-manifest.json`](../release-manifest.json)
