# Ladybird Patches

Astrohacker Terminal uses Ladybird through the experimental Girlbat engine. The
Ladybird working tree is local-only under `forks/ladybird`. This directory
tracks patch archives and branch notes that are safe to commit.

## Current State

- **Upstream policy:** default branch **`master`** tip (remote HEAD is
  `refs/heads/master`, not `main`).
- **Active pin (Issue 26071814115751):** master tip
  `5baf8116efdeafc74f883e5c2bf9f12d9d80c608`
- **Current branch:** `5baf8116-issue-26071814115751`
- **Current HEAD:** `63ced9b469a213c161b6b1b9c74d9e9d024b7552`
- **Current tree:** `4470e021f2df4eace2dce992c99f4913ae23e7f5`
- **Archive:** `patches/ladybird/patches/issue-26071814115751/` (22 patches)
- **Archive aggregate SHA-256:**
  `c981e048efbec1140a515369a399100e792d2c45227b2cb8d53f843e8b8ad9e2`
- **Verification:** **TREE_MATCH Pass**; release-mode
  `TERMSURF_LADYBIRD_BACKEND=real scripts/build.sh ladybird --release` green
  (Exp 4 implementer)
- Working tree: `forks/ladybird`
- Release authority: `patches/release-manifest.json` ladybird entry

Product series includes TermSurf C ABI, navigation/refresh, and a tip port of
JS dialog hooks to `Utf16String` / `Optional<Utf16String>` for current
LibWebView.

All prior archives remain under `patches/ladybird/patches/` as historical
records (including Issue `26071420489654` and add-ons on `2a3bc6a3…`).

## Merge-upstream

1. Discover tip: `git ls-remote --symref
   https://github.com/LadybirdBrowser/ladybird.git HEAD`
2. Fetch tip; branch `{short8}-issue-NNNN` at tip commit.
3. Apply current issue archive (`git am`) or rebase product series onto tip.
4. Build real backend:

   ```bash
   TERMSURF_LADYBIRD_BACKEND=real scripts/build.sh ladybird --release
   ```

5. Regenerate archive:

   ```bash
   rm -rf patches/ladybird/patches/issue-NNNN
   mkdir -p patches/ladybird/patches/issue-NNNN
   git -C forks/ladybird format-patch {base}..HEAD \
     -o "$PWD/patches/ladybird/patches/issue-NNNN"
   ```

6. Update this README Current State + `patches/release-manifest.json`.

## Applying Patches

```bash
cd forks/ladybird
git worktree add -b 5baf8116-issue-26071814115751 \
  /tmp/astrohacker-ladybird-pin \
  5baf8116efdeafc74f883e5c2bf9f12d9d80c608
git -C /tmp/astrohacker-ladybird-pin am \
  "$PWD/../../patches/ladybird/patches/issue-26071814115751/"*.patch
```

## Generating Patches

```bash
git -C forks/ladybird format-patch \
  5baf8116efdeafc74f883e5c2bf9f12d9d80c608..HEAD \
  -o "$PWD/patches/ladybird/patches/issue-26071814115751"
```

## Verification

```bash
git -C forks/ladybird status --short
git -C forks/ladybird rev-parse HEAD
TERMSURF_LADYBIRD_BACKEND=real scripts/build.sh ladybird --release
python3 scripts/lib/release_forks.py
```
