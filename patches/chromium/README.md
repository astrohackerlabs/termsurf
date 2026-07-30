# Chromium Patches

Astrohacker Terminal uses Chromium through the Roamium engine. The Chromium
working tree is local-only under `forks/chromium/src`; Chromium tooling lives in
`forks/chromium/depot_tools`. This directory tracks the patch archives and
branch notes that are safe to commit.

## Current State

- **Active pin (Issue 26073010145867 Exp 3):** Electron Chromium
  **`150.0.7871.129`** + **127** + **5** file-chooser + **3** drag-freeze
  (`issue-26073010145867/`) = **135** total — see
  `patches/release-manifest.json`
- Product branch:
  `150.0.7871.129-issue-26073010145867-exp3-buildable-drag-suppress`
- Product HEAD: `c9bcf6210561d152816c4f940be2b46ba53c5273`
- Product tree: `f4daaeaddac5b9a3e4ae90ecfe4dca44ab54f3e8`
- Main build target: `libtermsurf_chromium`
- Working tree: `forks/chromium/src`
- Tooling: `forks/chromium/depot_tools`
- Patch archives: `patches/chromium/patches`
- Release authority: `patches/release-manifest.json` chromium entry

### Issue 26073010145867 Exp 3 / buildable drag suppress (current tip)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.129` / `e69b30bba288603e514cffb4c79c359cac68e923` |
| Parent tip | `f05c80951f` (Exp 2 unbuildable suppress) |
| Product branch | `150.0.7871.129-issue-26073010145867-exp3-buildable-drag-suppress` |
| Product HEAD | `c9bcf6210561d152816c4f940be2b46ba53c5273` |
| Product tree | `f4daaeaddac5b9a3e4ae90ecfe4dca44ab54f3e8` |
| Archives | `issue-26072616587256/` (127) + `issue-26072810194224/` (5) + `issue-26073010145867/` (3) |
| Patch count | **135** |
| Archive aggregate SHA-256 | `2ad62ca665375d4e966a4e35130dbef5b5b325fa521bdec6df4ee647e239502b` |
| Scope | Suppress-only `StartDragging` (no dead code); `chromium-fork` **green** |
| Build | **Pass** — `scripts/build.sh chromium-fork` + `ah-chromiumd --release` (2026-07-30) |

### Issue 26073010145867 Exp 2 / unconditional drag suppress (Fail — unbuildable)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.129` / `e69b30bba288603e514cffb4c79c359cac68e923` |
| Parent tip | `182ebbac73` (Issue 26073010145867 Exp 1 always-hidden) |
| Product branch | `150.0.7871.129-issue-26073010145867-exp2-unconditional-drag-suppress` |
| Product HEAD | `f05c80951f94cd0bfa0e8d1216f91048a3f93b3e` |
| Product tree | `96c2a7ff258fbcc4d1d107f1927f1c90f5b535dc` |
| Archives | `issue-26072616587256/` (127) + `issue-26072810194224/` (5) + `issue-26073010145867/` (2) |
| Patch count | **134** |
| Archive aggregate SHA-256 | `67a6c4352d30365ac3a0da8da2b6e32b8f8409945e392b899aae7ae521bc0eea` |
| Scope | Unconditional `SystemDragEnded` + return (left dead stock body) |
| Status | **Fail** — `-Werror,-Wunreachable-code`; superseded by Exp 3 |

### Issue 26073010145867 Exp 1 / always-hidden product engine (parent tip)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.129` / `e69b30bba288603e514cffb4c79c359cac68e923` |
| Parent tip | `8bb8dfd999` (Issue 26072810194224 Exp 5 file chooser) |
| Product branch | `150.0.7871.129-issue-26073010145867-exp1-always-hidden-product-engine` |
| Product HEAD | `182ebbac7331d7bf84889f71f64c297cfdbf591b` |
| Product tree | `fb6c4d4b8898a3bc2c2a4d555864cde221fa33c2` |
| Archives | `issue-26072616587256/` (127) + `issue-26072810194224/` (5) + `issue-26073010145867/` (1) |
| Patch count | **133** |
| Archive aggregate SHA-256 | `16e66907ba151286ff47d03529e5e42c86c1a0081647f04fcfedd34835d61217` |
| Scope | Unconditional embed-hidden Content Shell mac; remove `switches::kHidden` |
| Status | Parent of Exp 2 |

### Issue 26072810194224 Exp 5 / host file chooser (parent tip)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.129` / `e69b30bba288603e514cffb4c79c359cac68e923` |
| Parent tip | `7a0d9de374` (Issue 26072616587256 Exp 1 Electron pin) |
| Product branch | `150.0.7871.129-issue-26072810194224-exp5-off-ui-folder-enumerate` |
| Product HEAD | `8bb8dfd999b096a91d708e0488303ce6b1c0f1f1` |
| Product tree | `b365c026e8a5dcedc2cbd9f8a3c5703b69d939a8` |
| Archives | `issue-26072616587256/` (127) + `issue-26072810194224/` (5) |
| Patch count | **132** |
| Archive aggregate SHA-256 | `84c02c4dce3f9bc11161769139c7d1466a1e8b5c2127f4ee2ef86615b0f1343a` |
| Reconstruction | **Pass** — `ensure_fork` already-applied TREE_MATCH (2026-07-28) |
| Build status | **Green** — `chromium-fork` + `ah-chromiumd` (Exp 5) |
| Scope | Host-mediated open/multi/folder; off-UI MayBlock enumerate; not drag/save download |
| Status | Parent of active pin (Issue 26073010145867 Exp 1) |

### Issue 26072616587256 Exp 1 / Electron pin 150.0.7871.129 (parent series)

| Field | Value |
| --- | --- |
| Target base | `150.0.7871.129` / `e69b30bba288603e514cffb4c79c359cac68e923` |
| Policy | Electron **v43.2.0** stable Chromium (chrome field) |
| Product branch | `150.0.7871.129-issue-26072616587256-exp1-electron-pin` |
| Product HEAD | `7a0d9de374a26dfa3bfda534443e96c4ab707e67` |
| Product tree | `9c6ae32ff888da73fe5c29027403b6b8788acc51` |
| Archive | `patches/chromium/patches/issue-26072616587256/` (127 format-patches) |
| Archive aggregate SHA-256 | `299a342ffb462d5c395f0ad265c9d5cd0be96f93c40ef525dcb789e7535f747c` |
| Status | **Parent of active pin** (still first of two `patch_directories`) |

### Issue 26072209562907 Exp 1 / hard refresh BYPASSING_CACHE (historical tip on .114)

| Field | Value |
| --- | --- |
| Product branch | `150.0.7871.114-issue-26072209562907-exp1-hard-refresh` |
| Product HEAD | `c12eb5f541f2cb639417eb2c58bcaed74a039833` |
| Product tree | `0734f3149013f235ef73a9c2b3b8e628ced449de` |
| Add-on archive | `patches/chromium/patches/issue-26072209562907/` |
| Patches | `0001` `refresh_ignore_cache` → `ReloadType::BYPASSING_CACHE` + LOG oracle |
| Patch SHA-256 | 0001 `c3d170fb4dbd68e24e6154e0f4482c48dc7bdef02209ca475d07a3099badb45d` |
| Scope | Soft `refresh` still `NORMAL` + `reload_type=normal`; hard only at `bypassing_cache` |
| Status | Superseded as **active pin** by Issue 26072616587256 Exp 1 (`.129`) |

### Issue 26072214390772 Exp 3 / Mac wheel phase route

| Field | Value |
| --- | --- |
| Product branch | `issue-26072214390772-exp1-chrome-parity-wheel` |
| Product HEAD | `9732d253fc23538c05a339da0b8451d5ce218130` |
| Product tree | `dc6774dde18c3022c9c81c79e664f9f7c5f1ecd8` |
| Add-on archive | `patches/chromium/patches/issue-26072214390772/` |
| Patches | `0001` Exp 1 field fill; `0002` Exp 3 `RouteOrProcessWheelEvent` via `ts_wheel_route_mac.mm` |
| Scope | `ForwardScrollEvent` → Mac RWHV phase-handler route (delayed phase-end) |

### Issue 26072214390772 Exp 1 / Chrome-parity wheel

| Field | Value |
| --- | --- |
| Product branch | `issue-26072214390772-exp1-chrome-parity-wheel` |
| Product HEAD (Exp 1 tip) | `a63469659677975a503baedd5741725d79b8d519` |
| Add-on archive | `patches/chromium/patches/issue-26072214390772/` |
| Scope | `ForwardScrollEvent`: wheel_ticks, event_action, kNoButton, AppKit/Blink phase bitmasks |

### Issue 26072110403572 Exp 2 / Space co-location

| Field | Value |
| --- | --- |
| Parent | `476c8df1c2de6d65fdf8990d02b31c002d81a10b` (122-patch series tip) |
| Product branch | `issue-26072110403572-exp2-helper-space-colocation` |
| Product HEAD | `180beaea2255171081b14ef28d77b4404a165230` |
| Product tree | `8417d71be8c7febb95feb03b79711f557a730dd8` |
| Add-on archive | `patches/chromium/patches/issue-26072110403572/` |
| Patches | `0001` Join all Spaces + FullScreenAuxiliary; `0002` clear FullScreenPrimary/None before Auxiliary |
| Patch SHA-256 | 0001 `b75a3fd9…`; 0002 `2cd114f9…` |
| Scope | `ApplyTermSurfSpaceCollectionBehavior` on configure + move; exclusive FS roles cleared |

### Issue 26071814115751 / Electron stable Chromium 150.0.7871.114 (base series)

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
