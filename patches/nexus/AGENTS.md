# Nexus Pin / Patch Workspace

Read this **and** [`../AGENTS.md`](../AGENTS.md) before changing the Nexus fork
for Astrohacker TermSurf (`ahnexus`).

## Contract (MUST)

Obey the hub **Fork change contract** in full when Nexus **source** is edited:

- Every intentional source edit → tracked `format-patch` under
  `patches/nexus/patches/issue-{ID}/` + monorepo pin update (this fork’s
  `README.md` + `patches/release-manifest.json` when the series changes) +
  record in the **current** issue experiment.
- Branch **must** include issue id and `exp{N}` (see local pattern below).
- Work is **incomplete** until the monorepo archive/pin is updated and
  commit-ready (not merely committed inside ignored `forks/nexus/`).

**Pin-only default (Exp 3):** tip pin only — no product source patch. Allowed
only when there is **no** intentional Astrohacker source edit. Document
pin-only state in the issue archive README. Do not invent no-op commits or
empty `.patch` files. The moment you edit source, the full contract applies.

## Local details

- Source: `forks/nexus`
- Archives / notes: `patches/nexus/patches/`
- Branch pattern (when source is edited or for pin branches):

  ```text
  issue-{ISSUE_ID}-exp{N}-{short-slug}
  ```

- Primary crate for `ahnexus`: `forks/nexus/nexus-common`
- Archive style: see this fork’s `README.md`

## Fork-specific hazards

- Do not commit Nexus source, `target/`, or build outputs to the Astrohacker
  repo (`forks/nexus/` is fully gitignored; this file is the tracked hygiene).
- Do not depend on iced `nexus-client` as a library for TermSurf UI.
- Prefer upstream PRs for protocol fixes when possible; still archive any
  Astrohacker-local delta under `patches/nexus/` until/unless dropped.

## Learn more

- Pin identity and verify steps: [`README.md`](./README.md)
- Hub fork-change contract: [`../AGENTS.md`](../AGENTS.md)
- Shared patch policy: [`../README.md`](../README.md)
- Release series authority: [`../release-manifest.json`](../release-manifest.json)
