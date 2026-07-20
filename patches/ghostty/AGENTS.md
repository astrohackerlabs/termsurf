# Ghostty Patch Workspace

Read this **and** [`../AGENTS.md`](../AGENTS.md) before modifying Ghostty for
Astrohacker Terminal.

## Contract (MUST)

Obey the hub **Fork change contract** in full:

- Every intentional source edit → tracked `format-patch` under
  `patches/ghostty/patches/issue-{ID}/` + monorepo pin update (this fork’s
  `README.md` + `patches/release-manifest.json` when the series changes) +
  record in the **current** issue experiment.
- Branch **must** include issue id and `exp{N}` (see local pattern below).
- Work is **incomplete** until the monorepo archive/pin is updated and
  commit-ready (not merely committed inside ignored `forks/ghostty/`).

## Local details

- Source: `forks/ghostty`
- Patches: `patches/ghostty/patches/`
- Branch pattern:

  ```text
  issue-{ISSUE_ID}-exp{N}-{short-slug}
  ```

  Example shape: `issue-26071818128343-exp2-split-border-corner-radius`.
  Issue-only names (e.g. `issue-26071818128343-rounded-pane-borders` without
  `exp{N}`) are **not** sufficient for new experiment work.

- Archive style: typically **next ordered** `NNNN-….patch` under
  `patches/ghostty/patches/issue-{ID}/` via `git format-patch -1 HEAD`
  (see this fork’s `README.md`). Always refresh the Active Add-on pin and
  release-manifest when the series changes.

## Fork-specific hazards

- Do not commit Ghostty source or build outputs to the Astrohacker repo.
- Release builds need Zig **0.15.2** (`build.zig.zon` minimum). Prefer
  `/opt/homebrew/opt/zig@0.15/bin` on PATH when system Zig is 0.16+.

## Learn more

- Reconstruction and current archives: [`README.md`](./README.md)
- Hub fork-change contract: [`../AGENTS.md`](../AGENTS.md)
- Shared patch policy: [`../README.md`](../README.md)
- Release series authority: [`../release-manifest.json`](../release-manifest.json)
