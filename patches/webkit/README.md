# WebKit Patches

Astrohacker Terminal uses WebKit through the Surfari engine. The WebKit working
tree is local-only under `forks/webkit/src`. This directory tracks the patch
archives and branch notes that are safe to commit.

## Current State

- **Active add-on (Issue 26072112084519 Exp 1):** live compositor presentation
- **Upstream pin:** main tip
  `e0ee95bcafc0c470dfce6db7cfd8ce708c6e9e5e`
- **Current branch:** `issue-26072112084519-exp1-live-compositor-presentation`
- **Current HEAD:** `bed48373fbdf1400bfbf4f8ecc2c96fb581455cc`
- **Current tree:** `547986ebaf3970020f4dc86325c20dc2fe5fa756`
- **Archives:** `patches/webkit/patches/issue-26071814115751/` (2 patches) plus
  `patches/webkit/patches/issue-26072112084519/` (1 patch)
- **Issue patch SHA-256:**
  `e078724575900dbdf93aa834f30459876b2a715230eb023bf0d6772c2e60afc6`
- **Archive aggregate SHA-256:**
  `644ecfe100feb5c5449f9ef4a3a205e83422b4dbbdbea810bf79fb1bcf299596`
- **Verification:** Debug `webkit-fork`, `libtermsurf_webkit`, and
  `ah-webkitd` builds; native live-context/visibility smoke; source-built
  Release product manually displays animated glyph rain on `astrohacker.com`.
- Working tree: `forks/webkit/src`
- Release authority: `patches/release-manifest.json` webkit entry

Product series on tip: cursor-change notify + unified-build serializer rule +
stable external `LayerHostingContext` live compositor presentation.
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
