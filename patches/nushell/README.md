# Nushell Patches

Astrohacker Shell uses a patched Nushell fork derived from Shannon. The fork
working tree is local-only under `forks/nushell`; this directory tracks the
patch archive needed to reconstruct Astrohacker Shell's Nushell changes without
importing Nushell history into the company repo.

## Current State (Issue 26072616587256 Exp 4)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `bcadaea5c8b19d9fd3bea4089c40449a3802c1e2`
- Workspace version: `0.114.2`
- Product branch: `issue-26072616587256-exp4-nushell-main`
- Product HEAD: `1c21baed491ef31588fb693eef9bf6f0b903135b`
- Product tree: `ff13c12bdaab31a58998d135c5f01da33dc4f0d5`
- Local fork working tree: `forks/nushell`
- Issue archive (cumulative, release authority):
  - `patches/nushell/patches/issue-26072616587256/` (**7** patches)
- Archive aggregate SHA-256:
  `1cd6f8358ca9f30aa5ae49b8a322e3890c44529b7c0dca162da68a4d80ed0479`
- Reedline path pin: sibling `forks/reedline` at tip
  `7eb9bf219456202052aaa976842e9e790b88ed85` (`0.49.0`)
- Verification: **TREE_MATCH**; `scripts/build.sh ahsh --release`

## Patch Contents

- Shannon ModeDispatcher, zsh mode cycle, reedline path pin, lock refresh
- Lazy env merge (non-blocking + blocking barrier)
- Nu-first PATH union for alt-shell env merge

## Apply (clean base)

```sh
BASE=bcadaea5c8b19d9fd3bea4089c40449a3802c1e2
git -C forks/nushell worktree add /tmp/astrohacker-nushell-pin "$BASE"
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26072616587256/"*.patch
```

## Prior State (Issue 26072213251282 Exp 2)

- Base `72b01f3e11a02c1a0abd6284cf97f6f37d96677f`; HEAD
  `6f21c94658801c99c6018ec24f25084198ced1c5`; multi-dir archives under
  `issue-26071814115751`, `issue-26072212103788`, `issue-26072213251282`
  (historical; release authority moved to cumulative Exp 4 archive).
