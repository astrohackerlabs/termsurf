# Chromium Patches

Astrohacker Terminal uses Chromium through the Roamium engine. The Chromium
working tree is local-only under `forks/chromium/src`; Chromium tooling lives in
`forks/chromium/depot_tools`. This directory tracks the patch archives and
branch notes that are safe to commit.

## Current State

- **Active pin (Issue 26071814115751):** Electron stable Chromium **`150.0.7871.114`**
  (`f405107495a07cb1bfcf687d4af8d91117098db6`) / archive
  `patches/chromium/patches/issue-26071814115751/` (122 format-patches)
- Main build target: `libtermsurf_chromium`
- Working tree: `forks/chromium/src`
- Tooling: `forks/chromium/depot_tools`
- Patch archives: `patches/chromium/patches`
- Release authority: `patches/release-manifest.json` chromium entry

### Issue 26071814115751 / Electron stable Chromium 150.0.7871.114 (current)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.114` / `f405107495a07cb1bfcf687d4af8d91117098db6` |
| Policy | Electron stable Chromium only (`43.1.1` chrome field at pin) |
| Product branch | `150.0.7871.114-issue-26071814115751` |
| Product HEAD | `476c8df1c2de6d65fdf8990d02b31c002d81a10b` (122 commits on base) |
| Product tree | `ad70b28349aac8c2b8083e61127c4f05953c8b50` |
| Archive | `patches/chromium/patches/issue-26071814115751/` (122 format-patches) |
| Archive aggregate SHA-256 | `59ff364e27546dd3692381585b797b8f6dccc5bc274c2999fd169a84924a2997` |
| Reconstruction | **Pass** — clean-base `git am` TREE_MATCH equal to product tree |
| Build status | **Green** (local) — `scripts/build.sh chromium-fork` + `ah-chromiumd` exit 0 on Exp 1 observations |

### Issue 26071420489654 / 0.1.17 restoration (historical)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.47` / `0c3cca15d78645281db2d339b2dc3d6fad4ee90a` |
| Policy | Restore the exact shipped `0.1.17` Chromium product tree |
| Product branch | `150.0.7871.47-issue-26071420489654` |
| Product HEAD | `cd36368f70078014b2b6386fae0999b912b86b30` (119 commits on base) |
| Product tree | `8264590e738a8f4b2f0c1f0b4f46a4431347f073` (equal to historical `0.1.17`) |
| Archive | `patches/chromium/patches/issue-26071420489654/` (119 format-patches) |
| Archive aggregate SHA-256 | `b332e1468f309e78459da164b40656aa848b4caa2e2f0e92a3abab0844f04a8b` |
| Reconstruction | **Pass** — 119 stable patch IDs equal; two clean replays produced the expected tree |
| Build status | Historical — superseded by Issue 26071814115751 pin |

### Issue 26071112000924 / Electron stable Chromium 150 (`0.1.17` historical)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.47` / `0c3cca15d78645281db2d339b2dc3d6fad4ee90a` |
| Policy | Electron stable Chromium only |
| Product branch | `150.0.7871.47-issue-26071112000924` |
| Product HEAD (local) | `ca9329e85c734d8cb1524a9e27328349a72c94de` (119 commits on base) |
| Archive | `patches/chromium/patches/issue-26071112000924/` (119 format-patches; TREE_MATCH) |
| Build status | **Green** — `libtermsurf_chromium` + `ah-chromiumd --termsurf-warmup` |

### Merge-upstream (Chromium)

1. Discover Electron stable Chromium version (see Issue 26071112000924 Exp 1 pattern).
2. Fetch tag; branch `{version}-issue-NNNN` at the tag commit.
3. `gclient sync` / `runhooks` (prefer `managed: False` for src; avoid full
   unshallow stalls).
4. `git am` current archive; resolve conflicts; keep stack ledger.
5. `gn gen out/Default` then `autoninja -C out/Default libtermsurf_chromium`.
6. Build/smoke `ah-chromiumd`; regenerate format-patch archive; update this
   README.

## Branch Strategy

Chromium issue branches use:

```text
{version}-issue-{N}
{version}-issue-{N}-exp{M}
```

When future Astrohacker issues modify Chromium source, create an issue-specific
branch in `forks/chromium/src`, commit there, regenerate the matching patch
archive under `patches/chromium/patches/`, and record the issue/experiment in
the result.

## Applying Patches

For the current fully archived baseline:

```bash
cd forks/chromium/src
git checkout f405107495a07cb1bfcf687d4af8d91117098db6
git checkout -b 150.0.7871.114-issue-26071814115751
git am ../../../patches/chromium/patches/issue-26071814115751/*.patch
```

Historical 901 baseline (pre–Issue 26071112000924):

```bash
cd forks/chromium/src
git checkout 148.0.7778.271
git checkout -b 148.0.7778.271-issue-26070612000901
git am ../../../patches/chromium/patches/issue-26070612000901/*.patch
```

Some historical patch directories after issue 794 are incremental rather than
cumulative. Treat those as branch history records unless a later experiment
regenerates and verifies them as full-stack archives.

## Generating Patches

After committing Chromium changes inside `forks/chromium/src`:

```bash
cd forks/chromium/src
rm -rf ../../../patches/chromium/patches/issue-{N}
git format-patch f405107495a07cb1bfcf687d4af8d91117098db6..HEAD \
  -o ../../../patches/chromium/patches/issue-{N}
```

Then commit the patch archive and the issue experiment result in the
Astrohacker repo.

## Verification

```bash
git -C forks/chromium/src status --short
git -C forks/chromium/src rev-parse --abbrev-ref HEAD
git -C forks/chromium/src rev-parse HEAD
git diff --check
```

When Chromium source changed, also build:

```bash
cd forks/chromium/src
export PATH="$PWD/../depot_tools:$PATH"
autoninja -C out/Default libtermsurf_chromium
```
