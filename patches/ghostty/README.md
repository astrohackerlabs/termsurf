# Ghostty Patches

## Active Add-on (Issue 26072112084519 Exp 1)

- Parent product commit: `f713728dc20e3c382cd8ad14b11eccf60a96fe21`
  (prior tip on `issue-26072110403572-exp2-helper-space-colocation`)
- Product branch: `issue-26072112084519-exp1-live-compositor-presentation`
- Product HEAD: `3c9ffede08df58661668d4c5dd8c7f5d0f5965d5`
- Product tree: `1c92cf2162a6afff251629a95e18044518510158`
- Issue archive: `patches/ghostty/patches/issue-26072112084519/`
- Patch SHA-256:
  `63dbbf946ef9e50b2fb6a9fe0ba55c41f8f930d55da1a32236d9e72e6dc412c9`
- Aggregate series: 38 patches; archive SHA-256
  `6588a63c2dfe7db7441d2e12acda4cde182f375bf813ed3c6a2f7f0efc9f3181`
- Scope: derive actual hosted-pane presentation visibility in AppKit and route
  edge-triggered `SetPresentationVisible` state through the TermSurf protocol,
  including state remembered before `TabReady`.
- Verification: focused Zig routing test, incremental Debug `ahterm` build,
  and source-built Release product manual live-animation acceptance.

## Active Add-on (Issue 26072110403572 Exp 2)

- Parent product commit: `2b78cbf340afbb53a5717e9c981c27216b9bd708`
  (prior tip on `issue-26072110403572-exp1-disable-occlusion`)
- Product branch: `issue-26072110403572-exp2-helper-space-colocation`
- Product HEAD: `f713728dc20e3c382cd8ad14b11eccf60a96fe21`
- Product tree: `f2df63b34bc0ce2d36995808d3cfd1629e4eb7e3`
- Issue archive: `patches/ghostty/patches/issue-26072110403572/`
- Patches: `0001` Exp 1 occlusion flag probe; `0002` Exp 2 remove flag
- Patch SHA-256:
  - 0001: `1404b34f817d9965a72b8395b67074a3ae49826d659dedca931119c72638fa3b`
  - 0002: `319f6afa0f286111955cee1b57e3e27b51ea428beaaf37c39ad8b98152867f5a`
- Scope: **remove** chromium
  `--disable-backgrounding-occluded-windows` from spawn (Space co-location is
  the product fix in Chromium/WebKit shells). Argv tests assert flag absence.
- Verification: **source + 37-patch series pin**;
  `zig build test -Dtest-filter="Girlbat spawn argv"`.

## Prior Add-on (Issue 26072110403572 Exp 1)

- Parent product commit: `2bbe90f5997860ef182e57d809fce4e099c0cd1a`
  (prior tip on `issue-26072016457563-exp1-replace-ghost-title`)
- Product branch: `issue-26072110403572-exp1-disable-occlusion`
- Product HEAD: `2b78cbf340afbb53a5717e9c981c27216b9bd708`
- Product tree: `6b6e61e93be57280774cb1d8158c0be1eda5f042`
- Issue archive: `patches/ghostty/patches/issue-26072110403572/`
- Patches: `0001` Chromium `--disable-backgrounding-occluded-windows` spawn argv
- Patch SHA-256:
  - 0001: `1404b34f817d9965a72b8395b67074a3ae49826d659dedca931119c72638fa3b`
- Scope: diagnostic spawn flag (superseded as product policy by Exp 2).
- Verification: **source + 36-patch series pin**.

## Prior Add-on (Issue 26072016457563 Exp 1)

- Parent product commit: `a6b9b7b83235039df287ce1d0e056e8eaf2f25d8`
  (prior tip on `issue-26072016122202-exp1-secondary-axis-before-mru`)
- Product branch: `issue-26072016457563-exp1-replace-ghost-title`
- Product HEAD: `2bbe90f5997860ef182e57d809fce4e099c0cd1a`
- Product tree: `b86a2d2807d8301858e8635591155fb55283cd89`
- Issue archive: `patches/ghostty/patches/issue-26072016457563/`
- Patches: `0001` Replace ghost title defaults with surfer
- Patch SHA-256:
  - 0001: `173981e4b77792349982dec266426e3886bfc3cad14ad21c10896322516588d4`
- Scope: empty/fallback surface and window titles use **🏄** / **🏄 TermSurf**
  instead of **👻** / **👻 Ghostty**; theme preview CLI title aligned; xibs
  updated. Dock AppIcon unchanged.
- Verification: **source + 35-patch series pin**;
  `bash scripts/test-surfer-title-defaults.sh`.

## Prior Add-on (Issue 26072016122202 Exp 1)

- Parent product commit: `5b55fdcd84c50a181bba830cdb637c9364fba521`
  (prior tip on `issue-26072015221509-exp1-restore-monogram-dock`)
- Product branch: `issue-26072016122202-exp1-secondary-axis-before-mru`
- Product HEAD: `a6b9b7b83235039df287ce1d0e056e8eaf2f25d8`
- Product tree: `30de45b1df245f40040e6c029c54e48056bb64b0`
- Issue archive: `patches/ghostty/patches/issue-26072016122202/`
- Patches: `0001` Secondary-axis strip before MRU on spatial focus
- Patch SHA-256:
  - 0001: `b39e436cd939717e638bd34b11cb52e4235fda8362aa725bd952e72cd5a80424`
- Scope: spatial `focusTarget` uses primary band → **positive** secondary
  overlap (else nearest secondary) → MRU → strip geometry; fixes 2×2
  sideways jumps while keeping nested `L|(TR/BR)` BR restore.
- Verification: **source + 34-patch series pin**;
  `xcodebuild test -scheme Ghostty -only-testing:GhosttyTests/SplitTreeTests`.

## Prior Add-on (Issue 26072015221509 Exp 1)

- Parent product commit: `87ca338679438debc7a0a4c60173a5cd5f897ae5`
  (prior tip on `issue-26072011262273-exp4-half-dock-padding`)
- Product branch: `issue-26072015221509-exp1-restore-monogram-dock`
- Product HEAD: `5b55fdcd84c50a181bba830cdb637c9364fba521`
- Product tree: `234c33cd356b00014e36de2d4cba7b82e73e4e30`
- Issue archive: `patches/ghostty/patches/issue-26072015221509/`
- Patches: `0001` Restore monogram host AppIcon ladder
- Patch SHA-256:
  - 0001: `3910cdde93eef1c6065c30cb124979d4f861c7d10267e0183da49ae34e6552cd`
- Scope: `TermSurf.appiconset` + `AppIconImage` pixels from monogram factory
  dock master (cyan `#1BFEFF` on opaque navy `#07203A`); catalog AppIcon name
  remains `TermSurf`. Default host Dock mark is monogram, not TermSurf wave.
- Verification: **source + 33-patch series pin**;
  `python3 scripts/sync-termsurf-appicon.py --check-only` (default monogram);
  `bash scripts/test-sync-host-appicon.sh`.

## Prior Add-on (Issue 26072011262273 Exp 4)

- Parent product commit: `f58675fa9d88f51c551d157546f080e9379bc09f`
  (prior tip on `issue-26072011262273-exp3-dock-padding`)
- Product branch: `issue-26072011262273-exp4-half-dock-padding`
- Product HEAD: `87ca338679438debc7a0a4c60173a5cd5f897ae5`
- Product tree: `e4266784dbfce565109999462cf0b38b159aef95`
- Issue archive: `patches/ghostty/patches/issue-26072011262273/`
- Patches: `0001`–`0004` as prior; `0005` half dock side pad (scale 0.92)
- Patch SHA-256:
  - 0001: `f7ddb0cb0c54abef3388b354a400907a71dfaadc0cf6bcb5fe7a8bc723d2404a`
  - 0002: `a7e699bc39d9161401c0800d4550ac4d99beb32cc7c0ab045f9e9041d94b91f7`
  - 0003: `e5b458d52b4dd2b8654c21b9f57fdda1188b0fb224bcdd7c90fdb8a1c6530116`
  - 0004: `cf0c8de67830e2e76f726733b369f9e88f21402bd6ad6f8146cba0374c453dcc`
  - 0005: `e26d00f58823b46a9f62b6901362fc7714189e304c1bc94663610ce006aaacc2`
- Scope: AppIcon ladder from factory dock with `dock_content_scale=0.92`
  (~82.5% mark width, ~8.75% L/R pad — half Exp 3 final pad).
- Verification: **source + 32-patch series pin**;
  `python3 scripts/sync-termsurf-appicon.py --check-only`; Release
  `scripts/build.sh ahterm --release` (Zig 0.15.2).

## Prior Add-on (Issue 26072011262273 Exp 3)

- Parent product commit: `cf30906f6786eb610290aab75bcc853789d63aa6`
  (prior tip on `issue-26072011262273-exp2-dock-icon`)
- Product branch: `issue-26072011262273-exp3-dock-padding`
- Product HEAD: `f58675fa9d88f51c551d157546f080e9379bc09f`
- Product tree: `04ceeda3187022099526a074b77a53ea25d87c4c`
- Issue archive: `patches/ghostty/patches/issue-26072011262273/`
- Patches: `0001` Rename + TermSurf icon; `0002` Opaque factory dock AppIcon;
  `0003` dock padding ~10% sides (scale 0.89); `0004` stronger pad scale 0.72
- Patch SHA-256:
  - 0001: `f7ddb0cb0c54abef3388b354a400907a71dfaadc0cf6bcb5fe7a8bc723d2404a`
  - 0002: `a7e699bc39d9161401c0800d4550ac4d99beb32cc7c0ab045f9e9041d94b91f7`
  - 0003: `e5b458d52b4dd2b8654c21b9f57fdda1188b0fb224bcdd7c90fdb8a1c6530116`
  - 0004: `cf0c8de67830e2e76f726733b369f9e88f21402bd6ad6f8146cba0374c453dcc`
- Scope: AppIcon ladder from factory dock with `dock_content_scale=0.72`
  (~65% mark width, ~17% L/R pad) after operator rejected 0.89/10% sides.
  Colors/opaque corners unchanged. Exp 3 **Fail** (too much pad on Dock).
- Verification: **source + 31-patch series pin**;
  `python3 scripts/sync-termsurf-appicon.py --check-only`; Release
  `scripts/build.sh ahterm --release` (Zig 0.15.2).

## Prior Add-on (Issue 26072011262273 Exp 2)

- Parent product commit: `ced9b930f6483e0fd9c0f6e2791e3e9b6f8263ae`
  (prior tip on `issue-26072011262273-exp1-rename-relogo`)
- Product branch: `issue-26072011262273-exp2-dock-icon`
- Product HEAD: `cf30906f6786eb610290aab75bcc853789d63aa6`
- Product tree: `9b9764f8fd9cf0541d9c1ae72932c0f570b0dc35`
- Issue archive: `patches/ghostty/patches/issue-26072011262273/`
- Patches: `0001` Rename app to Astrohacker TermSurf + TermSurf icon assets;
  `0002` Opaque factory dock TermSurf AppIcon ladder
- Patch SHA-256:
  - 0001: `f7ddb0cb0c54abef3388b354a400907a71dfaadc0cf6bcb5fe7a8bc723d2404a`
  - 0002: `a7e699bc39d9161401c0800d4550ac4d99beb32cc7c0ab045f9e9041d94b91f7`
- Scope: regenerate `TermSurf.appiconset` + `AppIconImage` from brand factory
  dock master (cyan `#1BFEFF` on opaque navy `#07203A`); corners α=255.
  Dock authority remains `ASSETCATALOG_COMPILER_APPICON_NAME = TermSurf`.
- Verification: **source + 29-patch series pin** (see
  `patches/release-manifest.json`); `python3 scripts/sync-termsurf-appicon.py
  --check-only`; Release `scripts/build.sh ahterm --release` (Zig 0.15.2).

## Prior Add-on (Issue 26072011262273 Exp 1)

- Parent product commit: `4f000871a51141c37d03f07addb5ad78cf0fc11e`
  (prior tip on `issue-26071914254256-exp7-browse-chrome-keys`)
- Product branch: `issue-26072011262273-exp1-rename-relogo`
- Product HEAD: `ced9b930f6483e0fd9c0f6e2791e3e9b6f8263ae`
- Product tree: `47da67a08cc68ddfaaee19f4bcd64c3333d3939e`
- Issue archive: `patches/ghostty/patches/issue-26072011262273/`
- Patches: `0001` Rename app to Astrohacker TermSurf + TermSurf icon assets
- Patch SHA-256:
  - 0001: `f7ddb0cb0c54abef3388b354a400907a71dfaadc0cf6bcb5fe7a8bc723d2404a`
- Scope: product display/bundle name **Astrohacker TermSurf**; keep
  `EXECUTABLE_NAME=ahterm`; `+version` identity; TermSurf mark as primary
  icon (Ghostty.icon / icons ladder); user-facing menus/help strings.
- Verification: **source + 28-patch series pin** (see
  `patches/release-manifest.json`); Release `scripts/build.sh ahterm --release`
  (Zig 0.15.2).

## Prior Add-on (Issue 26071914254256 Exp 7)

- Parent product commit: `05f6a4d599ea42bf598d031bbeae02b2dc61e7a4`
  (prior tip on `issue-26071913243342-exp1-spatial-mru-focus`)
- Product branch: `issue-26071914254256-exp7-browse-chrome-keys`
- Product HEAD: `4f000871a51141c37d03f07addb5ad78cf0fc11e`
- Product tree: `8fde8bbf717ddcf62a068ecdbd7a8e4e66b7571b`
- Issue archive: `patches/ghostty/patches/issue-26071914254256/`
- Patches: `0001` Browse chrome key allowlist for TermSurf overlays
- Patch SHA-256:
  - 0001: `f5f2dd60feca97b992d38e4ea24dc798a713a2a6c3729580aef854df7954e57f`
- Scope: when browse-forwardable, host-steal only chrome actions
  (splits/tabs/zoom/new_tab/close_tab/fullscreen/quit); else bulk-forward
  webview. Zig classify + unit tests; AppKit preflight before forward.
- Verification: **source + 27-patch series pin** (see
  `patches/release-manifest.json`); `zig build test -Dtest-filter=…` for
  browse_chrome units.

## Prior Add-on (Issue 26071913243342)

- Parent product commit: `3328348e9030fad8a234bb76017418005d3bfc23`
  (prior tip on `issue-26071821572313-exp3-divider-matches-bg`)
- Product branch: `issue-26071913243342-exp1-spatial-mru-focus`
- Product HEAD: `05f6a4d599ea42bf598d031bbeae02b2dc61e7a4`
- Product tree: `87e2e30995a617b7a28b95ea09f6c278c4f7c418`
- Issue archive: `patches/ghostty/patches/issue-26071913243342/`
- Patches: `0001` nearest primary-axis cohort + MRU spatial `goto_split`
- Patch SHA-256:
  - 0001: `3b9cadc4bf6fc0f8f5992a78259f0c42149844a0f0d41f0064eeab38ed7dfc95`
- Scope: spatial left/right/up/down restores last focused leaf in nearest
  band; previous/next tree-order unchanged; window MRU of surface IDs.
- Verification: **source + 26-patch series pin** (archive
  `c1bc34bec200ac65a8199c78fd6e320df642686a473cb3634e1dc7f4f0d89101`);
  SplitTree unit tests + `scripts/build.sh aht --release` (Zig 0.15.2).


## Prior Add-on (Issue 26071821572313)

- Parent product commit: `95b4c3555df1a301a6585200f7c362463acf0b42`
  (prior tip on `issue-26071819414418-exp1-progress-bar-fit`)
- Product branch: `issue-26071821572313-exp3-divider-matches-bg`
- Product HEAD: `3328348e9030fad8a234bb76017418005d3bfc23`
- Product tree: `463a9ce05d3dd4eed01b20d5ed54aee0076fcd68`
- Issue archive: `patches/ghostty/patches/issue-26071821572313/`
- Patches: `0001` gap layout (default 2), `0002` gap default **4**, `0003`
  unset divider color = theme `background` (no darken)
- Patch SHA-256:
  - 0001: `c070ab5767be3ac2e6f2386eb71f87bfa4659860f9abb1e4928fa5ec29c50da4`
  - 0002: `34ef4a476b5bcea0ad57509d5f197852aa161cd45787609e5ba0f0d19b8e2e31`
  - 0003: `85c3a2ba99d5ebe84fc76959bc0528cfb25a4bfcec2de9f4acbea866adb5837b`
- Scope: empty inter-pane gap (`split-pane-gap`, default 4); 1 pt hairline;
  unset `split-divider-color` matches theme background (no darken).
- Verification: **source + 25-patch series pin** (archive
  `0bbad8d0c6bd578e4feba85b68d6c41337e9ca8f8143680bc32792f04a78e1a9`);
  operator Nu visual gate for Exp 3.


## Prior Add-on (Issue 26071819414418)

- Parent product commit: `79f6b04703ea537507599c7ba9116ac97e3ce2ca`
  (prior tip on `issue-26071818128343-exp2-split-border-corner-radius`)
- Product branch: `issue-26071819414418-exp1-progress-bar-fit`
- Product HEAD: `95b4c3555df1a301a6585200f7c362463acf0b42`
- Product tree: `1104fe6b37327bc2156147d1ef1fc2cb2e6b7388`
- Issue archive: `patches/ghostty/patches/issue-26071819414418/`
- Patches: `0001` theme palette colors + initial clip, `0002` concentric
  inset-box clip (R−w on content rect, not full pane)
- Patch SHA-256:
  - 0001: `4ed31e658edd20464025d75ec92b5f0a72d8eac9fa691bbfc1cb623584a756e2`
  - 0002: `0f596616c692f35f355a36cca7cfac017c3e861ba19bd542ea9de0bb2d69ffda`
- Scope: OSC surface progress bar uses theme palette 6/1/3; laid out in the
  inset content box and clipShape'd with continuous radius max(0, R−w) so
  corners are concentric with the split border stroke.
- Verification: **source + 22-patch series pin** (archive
  `a65ae7df61032eadb3f92ae6252cdc3fdb8cecaebeb6f5979168ea03c67d06fa`);
  agent Release `aht` preflight; operator Nu visual gate open (Exp 1).


## Prior Add-on (Issue 26071818128343)

- Parent product commit: `fc25ec02822f9449914e6a95aeefb5bae2e9b28f`
  (prior tip on `issue-26071814115751-ghostty`)
- Product branch: `issue-26071818128343-exp2-split-border-corner-radius`
  (historical tip also reachable as `issue-26071818128343-rounded-pane-borders`)
- Product HEAD: `79f6b04703ea537507599c7ba9116ac97e3ce2ca`
- Product tree: `b39f2254f26f2b460bc75dcb0d22cf77723babf2`
- Issue archive: `patches/ghostty/patches/issue-26071818128343/`
- Patches: `0001` Exp1 outer rounding, `0002` Exp1 all four corners,
  `0003` Exp2 `split-border-corner-radius` (`auto`|`0`|N)
- Patch SHA-256:
  - 0001: `4f77f419990195742bbea578e6b6177144f2346de6cf07e572e67027856c935d`
  - 0002: `21c130a0d46a0ca12299803c16413137873d717d6755cc838b61ffe0d266271d`
  - 0003: `17bf47409c739f11c1458d9741b99ef5bc4702bece68b5cb0b724fc11a1bc77c`
- Scope: every corner of each split pane border uses effective radius
  from `split-border-corner-radius` (`auto` → window `_cornerRadius`
  fallback 10; `0` square; positive fixed). Content/dim clip at
  max(0, R_eff - w); internal T-junctions included.
- Verification: **source + 20-patch series pin** (archive
  `3ae22c5b21160b1b18d61740e586bb89b4d0ba35031815658db22f18969456fd`);
  agent Release `aht` green; operator visual Pass (Exp 3).

## Prior Add-on (Issue 26071814115751)


Ghostty fork work is tracked here as patch archives against the ignored local
clone at `forks/ghostty`.

## Current State (Issue 26071814115751)

- **Upstream policy:** latest commit on **`main`**
- **Upstream base:** `f3c9a2b7262a989ba7e9408d00471fda8f788d16`
- **Product branch:** `issue-26071814115751-ghostty`
- **Product HEAD:** `fc25ec02822f9449914e6a95aeefb5bae2e9b28f`
- **Product tree:** `7f1a24c180d9e935537b08106c0fb093020c8520`
- **Archive:** `patches/ghostty/patches/issue-26071814115751/` (17 patches)
- **Archive aggregate SHA-256:**
  `9467410e92c14a96cb30fb0592f7b2bf839d69551b549e49768e742aa96d45c8`
- **Verification:** **TREE_MATCH Pass**; `scripts/build.sh ahterm --release`
  green with Zig 0.15.2 (Exp 6 implementer)
- **Release authority:** `patches/release-manifest.json` ghostty entry

Build note: tip requires Zig **0.15.2** (`build.zig.zon` minimum). Prefer
`/opt/homebrew/opt/zig@0.15/bin` on PATH when system Zig is 0.16+.

## Prior Active Add-on (Issue 26071813061732)

- Parent product commit: `ee241e83f206288bfa7bd6177a197fcd4b73afd7`
  (prior tip on `issue-26071811041780-welcome-homepage-url`)
- Product branch: `issue-26071813061732-remove-ahapp-poc`
- Product HEAD: `7093f54e7d0e86c558d86dea36cd04b560488d3e`
- Product tree: `5f58f5236712fbc2fd05ba86752fa08c318fe7c4`
- Issue archive: `patches/ghostty/patches/issue-26071813061732/`
- Patches: 0001 app removal, 0002 compile fix, 0003 ignore zig-pkg
- Patch SHA-256:
  - 0001: `850a9d92c2972099b48061b40bd17aa768fd42a74c6d4fe21912d4e40072a1e4`
  - 0002: `cfc2ed8012fca56de057c80746516bcfa04cdeae883310a39ba546c264f087a4`
  - 0003: `0279a6422627c9f4b7701c50ae0062eb373adf3e594d5d68cecef1274b93a3f4`
- Scope: remove TermSurf app host path; fix compile residuals; ignore
  `zig-pkg/` so release_forks clean check passes after local Zig builds.
- Verification: **source + 17-patch release series pin**.

## Prior Add-on (Issue 26071811041780)

- Parent product commit: `ed063b7b49135907b45d32a715bb92d6ba28eb50`
  (prior tip on `issue-26071721129990-shell-xdg-defaults`)
- Product branch: `issue-26071811041780-welcome-homepage-url`
- Product HEAD: `ee241e83f206288bfa7bd6177a197fcd4b73afd7`
- Product tree: `95fa220c4c20cfdf139c6d76775182170aed6d3c`
- Issue archive: `patches/ghostty/patches/issue-26071811041780/`
- Patch: `0001-Default-homepage-to-astrohacker.com-welcome.patch`
- Patch SHA-256:
  `6ec86883ad5afb252690ee8209902f5beb30bcb94a11fdbc453b64125c101b09`
- Scope: product default homepage URL
  `https://astrohacker.com/welcome` (Config, HelloReply fallback, Swift
  bridge) instead of `termsurf.com/welcome`.
- Verification: **source + 14-patch release series pin**; issue closed Pass.

## Prior Add-on (Issue 26071721129990)

- Parent product commit: `1a3ab12fc8619b81d46e61a1be66ef697ae4962e`
  (prior tip on `issue-26071720442142-font-keybind-defaults`)
- Product branch: `issue-26071721129990-shell-xdg-defaults`
- Product HEAD: `ed063b7b49135907b45d32a715bb92d6ba28eb50`
- Product tree: `56354bbfd58ce56f06cfdd5c9175979717acf88e`
- Issue archive: `patches/ghostty/patches/issue-26071721129990/`
- Patch: `0001-Default-shell-to-ahsh-and-XDG_CONFIG_HOME.patch`
- Patch SHA-256:
  `bc54a03efedfd69f89fad9a49a5b047cf26fff5cb2db06f954b9519d04ae62a2`
- Scope: default shell to packaged ahsh absolute paths with system-shell
  fallback; inject `XDG_CONFIG_HOME=$HOME/.config` when unset.
- Verification: **source + 13-patch release series pin**; operator release
  visual gate open (prior tip).

## Prior Add-on (Issue 26071720442142)

- Parent product commit: `56ff57e016c29c670b09867a1722f1d9854c6c9a`
  (prior tip on `issue-26071720300520-unfocused-opacity-default`)
- Product branch: `issue-26071720442142-font-keybind-defaults`
- Product HEAD: `1a3ab12fc8619b81d46e61a1be66ef697ae4962e`
- Product tree: `3e2ec116aae1667819d13f2744d15785bfdca024`
- Issue archive: `patches/ghostty/patches/issue-26071720442142/`
- Patches: `0001-…product-keybinds.patch`, `0002-…Allocator.Error-set.patch`
- Scope: default `font-family = JetBrainsMono Nerd Font`, `font-size = 12`,
  and Astrohacker split/tab product keybinds on macOS.
- Verification: **source + series pin Pass; issue closed Pass**.

## Prior Add-on (Issue 26071720300520)

- Parent product commit: `2aa4373bd65e685ea29d800a28af809cc30a3848`
  (prior tip on `issue-26071720189508-tokyonight-default`)
- Product branch: `issue-26071720300520-unfocused-opacity-default`
- Product HEAD: `56ff57e016c29c670b09867a1722f1d9854c6c9a`
- Product tree: `91ad8b4dc398298ffb9089a8c44495cf2460d64e`
- Issue archive: `patches/ghostty/patches/issue-26071720300520/`
- Patch: `0001-Default-unfocused-split-opacity-to-1.patch`
- Patch SHA-256:
  `e656a2ac7fc0763fe2c12f09251a5a5e3a6fa2a939243685f3196f9d4f028ece`
- Scope: product default `unfocused-split-opacity = 1` (no inactive-pane
  dimming); borders mark focus.
- Verification: **source + series pin Pass; issue closed Pass**.

## Prior Add-on (Issue 26071720189508)

- Parent product commit: `25004fc64cdc3577bccd58238aacef18397f272b`
  (prior tip on `issue-26071719409451-border-theme-defaults`)
- Product branch: `issue-26071720189508-tokyonight-default`
- Product HEAD: `2aa4373bd65e685ea29d800a28af809cc30a3848`
- Product tree: `8d87c4521aa89d7a4b74e3b399fb7c69cd3b1108`
- Issue archive: `patches/ghostty/patches/issue-26071720189508/`
- Patch: `0001-Default-theme-to-TokyoNight.patch`
- Patch SHA-256:
  `d50a411b7e4ac6fc53cebf7d54e447b88fd714ed6ffbf3e56dc2ed3942e0c81c`
- Scope: product default `theme = TokyoNight` (exact resource name) for light
  and dark when unset.
- Verification: **source + series pin Pass; issue closed Pass**.

## Prior Add-on (Issue 26071719409451)

- Parent product commit: `2cc105acaaf8eb8fa82cb3344067d5b4b2468d68`
  (prior tip on `issue-26071611180778-split-webview-disappearance`)
- Product branch: `issue-26071719409451-border-theme-defaults`
- Product HEAD: `25004fc64cdc3577bccd58238aacef18397f272b`
- Product tree: `aa1192bf00dc4359a35d79aba27ed7897b4494e5`
- Issue archive: `patches/ghostty/patches/issue-26071719409451/`
- Patch:
  `0001-Default-split-borders-to-theme-palette-colors.patch`
- Patch SHA-256:
  `985744ab2a9b3b0abecb7fa586440e235a341f6198dacd1973236b17e52cd007`
- Scope: default `split-border-width = 2`; unset focused/unfocused border
  colors fall back to theme `palette[6]` / `palette[8]` in the macOS Swift
  config bridge.
- Verification: **source build Pass; issue closed Pass**.

## Prior Add-on (Issue 26071611180778)

- Parent product commit: `328d150826cb17be0f0eaa15fada9549fe2c60a1`
- Product branch: `issue-26071611180778-split-webview-disappearance`
- Product HEAD: `58d5855ccfc1b2d5d788af87d708f8c1b9b15c98`
- Product tree: `c49e204f49636262be90e23c0fd90e5b7c4f0a4e`
- Issue archive: `patches/ghostty/patches/issue-26071611180778/`
- Scope: split-tree/focus diagnostics plus AppKit overlay-lifetime preservation
  across transient window detachment.
- Verification: **focused tests, source build, corrected Chromium product gate,
  and two-patch archive replay Pass**; Experiment 2 result review approved.

## Current State (Issue 26071420489654)

- Upstream repository: `https://github.com/ghostty-org/ghostty`
- Upstream base commit: `53bd14fecfd68c6c0ab64d37b5943247299e2b40`
- Local fork working tree: `forks/ghostty`
- Product branch: `issue-26071420489654-ghostty-restoration`
- Product HEAD (base + product commit):
  `e380e7211d12c0da2ad7f8a1796d5793e12f11fc`
- Product tree: `362ce2b98d3700ab1a00c231614388d53dff5786`
- Issue archive: `patches/ghostty/patches/issue-26071420489654/`
- Patch:
  `patches/ghostty/patches/issue-26071420489654/0001-astrohacker-Terminal-ghostty-product-patch-on-tip-is.patch`
- Patch SHA-256:
  `e620a06511f57372488dd640459db4700d99cd0a3c5601936b515faada6b9387`
- Archive aggregate SHA-256:
  `1b81bd9875d152221b8d7329883217f590a080f14f828743c0c705bacc4314dc`
- Verification: **archive replay Pass; not built**

## Historical Archives

- Issue `26071112000924`: `patches/ghostty/patches/issue-26071112000924/`
  on base `53bd14fecfd68c6c0ab64d37b5943247299e2b40`, product HEAD
  `ad9768db5138df928b3c307754e7dae0f7945af9`.
- Issue `26070412000013`: `patches/ghostty/patches/issue-26070412000013/`
  on base `2c62d182cec246764ff725096a70b9ef44996f7f`.

Executable product name: **`ahterm`** inside
`Astrohacker Terminal.app`.

## Apply (clean base)

```sh
BASE=53bd14fecfd68c6c0ab64d37b5943247299e2b40
git -C forks/ghostty worktree add /tmp/astrohacker-ghostty-restoration "$BASE"
git -C /tmp/astrohacker-ghostty-restoration am \
  "$PWD/patches/ghostty/patches/issue-26071420489654/0001-astrohacker-Terminal-ghostty-product-patch-on-tip-is.patch"
```

## Generate

```sh
git -C forks/ghostty format-patch -1 HEAD \
  -o patches/ghostty/patches/issue-26071420489654/
```

## Build / verify

```sh
scripts/build.sh ahterm --release
# identity
"./forks/ghostty/macos/build/Release/Astrohacker Terminal.app/Contents/MacOS/ahterm" +version
# host TermSurf browser-resolution unit test
cd forks/ghostty && zig build test \
  -Dtest-filter="termsurf server register matches profile and browser"
```

## Merge-upstream checklist

1. Discover tip: `git ls-remote https://github.com/ghostty-org/ghostty.git refs/heads/main`
2. Fetch; create `issue-NNNN-ghostty-upstream` from the tip SHA.
3. `git am` current archive (or re-apply prior product commit); resolve conflicts.
4. Build `ahterm` Release; run `+version` and TermSurf unit filters.
5. `git format-patch -1` into `patches/ghostty/patches/issue-NNNN/`.
6. Update this README Current State (base SHA, branch, archive path, date).

Do not commit `forks/ghostty` or temporary worktrees to the Astrohacker repo.
