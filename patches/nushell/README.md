# Nushell Patches

Astrohacker Shell uses a patched Nushell fork derived from Shannon. The fork
working tree is local-only under `forks/nushell`; this directory tracks the
patch archive needed to reconstruct Astrohacker Shell's Nushell changes without
importing Nushell history into the company repo.

## Current State (Issue 26072212103788)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `72b01f3e11a02c1a0abd6284cf97f6f37d96677f`
- Workspace version: `0.114.2`
- Product branch: `issue-26072212103788-exp1-lazy-zsh-startup`
- Product HEAD: `81696a9224d4c958b7798782b267e8325ac8e6cf`
- Product tree: `3dd6e20580a70a258f820300e73dc8805c24fea6`
- Local fork working tree: `forks/nushell`
- Issue archives (cumulative):
  - `patches/nushell/patches/issue-26071814115751/` (4 patches)
  - `patches/nushell/patches/issue-26072212103788/` (1 patch — pending env merge)
- Archive aggregate SHA-256:
  `f030a7d2a66e9b64dc3048fc884b0014fc6b6b28b42c7d4654fb4d8fec84f4ba`
- Reedline path pin: sibling `forks/reedline` at tip
  `f776f5079e49d075c071660ae0f9b040b3ff909b` (`0.49.0`)

## Patch Contents

Bounded Shannon/Astrohacker deltas on tip:

- `shannon-nu-cli` / `shannon-nu-lsp` package naming
- path pin of `reedline` to sibling `forks/reedline` (workspace + `[patch.crates-io]`)
- `ModeDispatcher` support, bash/zsh highlighting, REPL mode-dispatch hooks
- cycle traditional mode as zsh not bash
- Cargo.lock refresh for tip + path reedline
- **Issue 26072212103788:** defaulted `ModeDispatcher::take_pending_env_merge`
  + REPL one-shot stack apply (lazy zsh env inject without blocking first prompt)

## Apply (clean base)

```sh
BASE=72b01f3e11a02c1a0abd6284cf97f6f37d96677f
# Reedline tip must exist at forks/reedline (path dep)
git -C forks/nushell worktree add -b issue-26072212103788-exp1-lazy-zsh-startup \
  /tmp/astrohacker-nushell-pin "$BASE"
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26071814115751/"*.patch
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26072212103788/"*.patch
```

## Generating Patches

Cumulative series from base:

```sh
git -C forks/nushell format-patch \
  72b01f3e11a02c1a0abd6284cf97f6f37d96677f..HEAD \
  -o /tmp/nushell-all-patches
```

Or incremental for this issue only (on top of prior product tip):

```sh
git -C forks/nushell format-patch -1 HEAD \
  -o "$PWD/patches/nushell/patches/issue-26072212103788"
```
