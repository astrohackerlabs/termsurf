# WebKit Patches

Astrohacker Terminal uses WebKit through the Surfari engine. The WebKit working
tree is local-only under `forks/webkit/src`. This directory tracks the patch
archives and branch notes that are safe to commit.

## Current State

- **Active pin (Issue 26071814115751):** main tip
  `e0ee95bcafc0c470dfce6db7cfd8ce708c6e9e5e`
- **Current branch:** `webkit-e0ee95bc-issue-26071814115751`
- **Current HEAD:** `6d219d5057124a5c432e9cd331c6b1fcd4ff5a78`
- **Current tree:** `b3957cdcb5dda6a99e3547e19608a10419d19a62`
- **Archive:** `patches/webkit/patches/issue-26071814115751/` (2 patches)
- **Archive aggregate SHA-256:**
  `87edc26d71c61fd5595c2fbe56a6850a2d4906516f2d125c68cdc0817c89b8c4`
- **Verification:** **TREE_MATCH Pass**; release-mode
  `webkit-fork` + `ah-webkitd` builds green (Exp 3 implementer)
- Working tree: `forks/webkit/src`
- Release authority: `patches/release-manifest.json` webkit entry

Product series on tip: cursor-change notify + unified-build serializer rule.
The prior sandbox EagerLinking exclusion rename is already on upstream main
(`Work around 109484516`) and was not re-exported as a product commit.

All prior archives remain under `patches/webkit/patches/` as historical
records (including Issue `26071420489654` restoration on `f1a2d7cc…`).

## Merge-upstream

1. `git ls-remote https://github.com/WebKit/WebKit.git refs/heads/main`
2. Fetch tip; branch `webkit-{short8}-issue-NNNN` at tip.
3. `git am` current issue archive (or rebase product series onto tip).
4. `scripts/build.sh webkit-fork --release` then `webkit --release`.
5. Smoke: `ah-webkitd --termsurf-warmup` (+ wrapper smoke when fixed).
6. Regenerate format-patch archive; update this README + release-manifest.

## Applying Patches

```bash
cd forks/webkit/src
git worktree add -b webkit-e0ee95bc-issue-26071814115751 \
  /tmp/astrohacker-webkit-pin \
  e0ee95bcafc0c470dfce6db7cfd8ce708c6e9e5e
git -C /tmp/astrohacker-webkit-pin am \
  "$PWD/../../../patches/webkit/patches/issue-26071814115751/"*.patch
```

## Generating Patches

```bash
mkdir -p patches/webkit/patches/issue-26071814115751
git -C forks/webkit/src format-patch \
  e0ee95bcafc0c470dfce6db7cfd8ce708c6e9e5e..HEAD \
  -o "$PWD/patches/webkit/patches/issue-26071814115751"
```

## Verification

```bash
git -C forks/webkit/src status --short
git -C forks/webkit/src rev-parse HEAD
scripts/build.sh webkit-fork --release
scripts/build.sh webkit --release
python3 scripts/lib/release_forks.py
```
