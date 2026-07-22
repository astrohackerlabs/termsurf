# Nushell Patches

Astrohacker Shell uses a patched Nushell fork derived from Shannon. The fork
working tree is local-only under `forks/nushell`; this directory tracks the
patch archive needed to reconstruct Astrohacker Shell's Nushell changes without
importing Nushell history into the company repo.

## Current State (Issue 26072213251282 Exp 2)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `72b01f3e11a02c1a0abd6284cf97f6f37d96677f`
- Workspace version: `0.114.2`
- Product branch: `issue-26072213251282-exp2-path-union`
- Product HEAD: `6f21c94658801c99c6018ec24f25084198ced1c5`
- Product tree: `ed4c9aa90eee6f4e76ce69289db9e182264e2ea7`
- Local fork working tree: `forks/nushell`
- Issue archives (cumulative):
  - `patches/nushell/patches/issue-26071814115751/` (4 patches)
  - `patches/nushell/patches/issue-26072212103788/` (1 patch)
  - `patches/nushell/patches/issue-26072213251282/` (2 patches: barrier + PATH union)
- Archive aggregate SHA-256:
  `b55825886efa3819fe066600e409c22923d818b05f05be98a45d1956e8cee177`
- Reedline path pin: sibling `forks/reedline` at tip
  `f776f5079e49d075c071660ae0f9b040b3ff909b` (`0.49.0`)

## Patch Contents

- Shannon ModeDispatcher, zsh mode cycle, reedline pin, lock refresh
- Lazy env merge (non-blocking + blocking barrier)
- **Exp 2:** `alt_shell_env` Nu-first PATH union + scalar Nu-wins smart merge

## Apply (clean base)

```sh
BASE=72b01f3e11a02c1a0abd6284cf97f6f37d96677f
git -C forks/nushell worktree add -b issue-26072213251282-exp2-path-union \
  /tmp/astrohacker-nushell-pin "$BASE"
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26071814115751/"*.patch
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26072212103788/"*.patch
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26072213251282/"*.patch
```
