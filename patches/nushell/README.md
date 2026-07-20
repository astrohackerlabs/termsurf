# Nushell Patches

Astrohacker Shell uses a patched Nushell fork derived from Shannon. The fork
working tree is local-only under `forks/nushell`; this directory tracks the
patch archive needed to reconstruct Astrohacker Shell's Nushell changes without
importing Nushell history into the company repo.

## Current State (Issue 26071814115751)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `72b01f3e11a02c1a0abd6284cf97f6f37d96677f`
- Workspace version: `0.114.2`
- Product branch: `issue-26071814115751-nushell`
- Product HEAD: `7c2654d3fe952e3b2b5a47686a5ae95174f0a2e1`
- Product tree: `b90d455983a99948c2b442f19f96cc1f74bcf0f2`
- Local fork working tree: `forks/nushell`
- Issue archive: `patches/nushell/patches/issue-26071814115751/` (4 patches)
- Archive aggregate SHA-256:
  `922c4dd881dfd7b42b2f5ed3b893d6414db0d2143207081c0d3ac5cdcb19001e`
- Reedline path pin: sibling `forks/reedline` at tip
  `f776f5079e49d075c071660ae0f9b040b3ff909b` (`0.49.0`)
- Verification: **TREE_MATCH Pass**; `scripts/build.sh ahsh --release` green
  (Exp 5 implementer)

## Patch Contents

Bounded Shannon/Astrohacker deltas on tip:

- `shannon-nu-cli` / `shannon-nu-lsp` package naming
- path pin of `reedline` to sibling `forks/reedline` (workspace + `[patch.crates-io]`)
- `ModeDispatcher` support, bash/zsh highlighting, REPL mode-dispatch hooks
- cycle traditional mode as zsh not bash
- Cargo.lock refresh for tip + path reedline

## Apply (clean base)

```sh
BASE=72b01f3e11a02c1a0abd6284cf97f6f37d96677f
# Reedline tip must exist at forks/reedline (path dep)
git -C forks/nushell worktree add -b issue-26071814115751-nushell \
  /tmp/astrohacker-nushell-pin "$BASE"
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26071814115751/"*.patch
```

## Generating Patches

```sh
git -C forks/nushell format-patch \
  72b01f3e11a02c1a0abd6284cf97f6f37d96677f..HEAD \
  -o "$PWD/patches/nushell/patches/issue-26071814115751"
```
