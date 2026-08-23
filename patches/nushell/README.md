# Nushell Patches

Astrohacker Shell uses a patched Nushell fork derived from Shannon. The fork
working tree is local-only under `forks/nushell`; this directory tracks the
patch archive needed to reconstruct Astrohacker Shell's Nushell changes without
importing Nushell history into the company repo.

## Current State (Issue 26082310413946 Exp 3)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `8e03210652f3c48c4521cec982d96e4cb6c67181`
- Workspace version: `0.115.1`
- Product branch: `issue-26082310413946-exp3-ai-line-foreground`
- Product HEAD: `effe0d79d9940f7c7fed41b5d7d0172c20577860`
- Product tree: `3cbc84522f3c4734019a9ca455d606391dbb24a7`
- Local fork working tree: `forks/nushell`
- Issue archives (release authority):
  `patches/nushell/patches/issue-26081615463315/` (**8**) +
  `patches/nushell/patches/issue-26082214188331/` (**1** overlay
  `AGENTS.md`) +
  `patches/nushell/patches/issue-26082310413946/` (**4** AI dispatch,
  live `[ai]` overlay, default-fg AI lines) from `base..HEAD`
- Total patch count: **13**
- Archive aggregate SHA-256:
  `cf2d74d5de860f247600e551434bb587215c1fbc5ec2834cb33d9d82b309f589`

## Prior State (Issue 26082310413946 Exp 2)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `8e03210652f3c48c4521cec982d96e4cb6c67181`
- Workspace version: `0.115.1`
- Product branch: `issue-26082310413946-exp2-live-ai-prompt`
- Product HEAD: `d23eebba278fa56636fd45572040ac6ae62d8aa6`
- Product tree: `6ba4075689db8119db578f7e5285976d61064e64`
- Local fork working tree: `forks/nushell`
- Issue archives (release authority):
  `patches/nushell/patches/issue-26081615463315/` (**8**) +
  `patches/nushell/patches/issue-26082214188331/` (**1** overlay
  `AGENTS.md`) +
  `patches/nushell/patches/issue-26082310413946/` (**3** full-line
  `#` AI dispatch + live `[ai]` prompt overlay) from `base..HEAD`
- Total patch count: **12**
- Archive aggregate SHA-256:
  `8d9217798d372fdadada3b82557af79f4e6abbb5a617f7fae41af3007a9e3d3a`

## Prior State (Issue 26082310413946 Exp 1)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `8e03210652f3c48c4521cec982d96e4cb6c67181`
- Workspace version: `0.115.1`
- Product branch: `issue-26082310413946-exp1-column0-hash-stub`
- Product HEAD: `c880a7aab5db00a7cc4340b05370508c10a94b1f`
- Product tree: `24606b6a7d96a04e68a49f7abf97bf64b0f25a5c`
- Local fork working tree: `forks/nushell`
- Issue archives (release authority):
  `patches/nushell/patches/issue-26081615463315/` (**8**) +
  `patches/nushell/patches/issue-26082214188331/` (**1** overlay
  `AGENTS.md`) +
  `patches/nushell/patches/issue-26082310413946/` (**2** full-line
  `#` AI dispatch, left-trim) from `base..HEAD`
- Total patch count: **11**
- Archive aggregate SHA-256:
  `7163f29c24c6db54c67a3fe4bd433705bfe18bb979ed714c9355a04e9d221725`

## Prior State (Issue 26082214188331 Exp 1)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `8e03210652f3c48c4521cec982d96e4cb6c67181`
- Workspace version: `0.115.1`
- Product branch: `issue-26082214188331-exp1-agents-overlay`
- Product HEAD: `aa82105a4b55670ade99617043cd1b74a436aac8`
- Product tree: `f2273e1cf5842a4256da9486233f22c93d80e0f2`
- Local fork working tree: `forks/nushell`
- Issue archives (release authority):
  `patches/nushell/patches/issue-26081615463315/` (**8**) +
  `patches/nushell/patches/issue-26082214188331/` (**1** overlay
  `AGENTS.md`) from `base..HEAD`
- Total patch count: **9**
- Archive aggregate SHA-256:
  `12ab3a767f83d60d27ae5ab89de470cf9857403680b2758e3216393bbed1a682`

## Prior State (Issue 26081615463315 Exp 2)

- Upstream repository: `https://github.com/nushell/nushell`
- Upstream base commit (main tip): `8e03210652f3c48c4521cec982d96e4cb6c67181`
- Workspace version: `0.115.1`
- Product branch: `issue-26081615463315-exp2-nushell-main`
- Product HEAD: `789490788161dd8d5ab5770e27b13b160d04a496`
- Product tree: `b88140e054dc825c0e3b61797bd482ff7e27ebcc`
- Local fork working tree: `forks/nushell`
- Issue archive (release authority):
  `patches/nushell/patches/issue-26081615463315/` (**8** from `base..HEAD`;
  prior `issue-26081311412273` stays historical)
- Total patch count: **8**
- Archive aggregate SHA-256:
  `a2e90a980fafb55c6b041f1d1d191e92af1e97d81fb16228a038828595ddb5cf`
- Reedline path pin: sibling `forks/reedline` at tip
  `9230319ae57f88bac5a2a17dc3f9a313cff3330d` (`0.50.0`, helix default off
  via `default-features = false`)
- Verification: **TREE_MATCH**; `scripts/build.sh ahsh --release`

## Prior State (Issue 26081311412273 Exp 5)

- Upstream base commit (main tip): `c68420afd55f8dd3a3ec09e14f779ff48aebc8e5`
- Workspace version: `0.114.2`
- Product branch: `issue-26081311412273-exp5-nushell-main`
- Product HEAD: `07823d55131289ead112821171c118c87825adbf`
- Product tree: `9f58c51bd9c3efb85a86a29a9f5736497308a0dd`
- Issue archive: `patches/nushell/patches/issue-26081311412273/` (**8**)
- Archive SHA-256:
  `25a8e0a3efd0fe0d9a6779fecaa4482810bf04a8c3fb641be499f0c442c3d464`
- Reedline path pin: `464efb2ed5b8d294f81b787e70418006d48428f3` (`0.49.0`)

## Prior State (Issue 26080510416061 Exp 1)

- Upstream base: `e08c27a42405c05de629ac50077ce7f759d82a64`
- Product branch: `issue-26080510416061-exp1-unbind-ctrl-l`
- Product HEAD: `211b92c2130e4bcdffb70c23b6a979ff6791d1a1`
- Product tree: `ec00517618c8989d46200603fc026fab2c08ab26`
- Archives: `issue-26080213543507/` (**9**) + `issue-26080510416061/` (**1**)
- Archive SHA-256:
  `8e2c8bda7a344da524901b83aab05a122f06258ede67ade7fd46682d5f86935f`
- Reedline path pin: `60d9967420b5a56745f6ec250b40bc3b6813092b` (`0.49.0`)

## Prior State (Issue 26080213543507 Exp 5)

- Product branch: `issue-26080213543507-exp5-nushell-main`
- Product HEAD: `b9e066a52c28cfaf142a0462fd0f60b3e7f98cbe`
- Product tree: `d711069b84f67a781b4d09572a30e9e1b83aa77b`
- Archive: `patches/nushell/patches/issue-26080213543507/` (**9** patches)
- Archive SHA-256:
  `cb5be5a6139803d3f55627a0a106be1a02fa2434b05693981d40c1c6cc9c4d7f`

## Prior State (Issue 26072616587256 Exp 4)

- Upstream base: `bcadaea5c8b19d9fd3bea4089c40449a3802c1e2`
- Product branch: `issue-26072616587256-exp4-nushell-main`
- Product HEAD: `1c21baed491ef31588fb693eef9bf6f0b903135b`
- Product tree: `ff13c12bdaab31a58998d135c5f01da33dc4f0d5`
- Archive: `patches/nushell/patches/issue-26072616587256/` (**7** patches)
- Archive SHA-256:
  `1cd6f8358ca9f30aa5ae49b8a322e3890c44529b7c0dca162da68a4d80ed0479`

## Patch Contents

- Shannon ModeDispatcher, zsh mode cycle, reedline path pin, lock refresh
- Lazy env merge (non-blocking + blocking barrier)
- Nu-first PATH union for alt-shell env merge
- Port `nu-cli` to reedline tip (`CompletionOrigin` / `CutSelection` granularity
  / new edit-command discriminants)
- Product unbind Ctrl+L ClearScreen (ahsh / host pane pass-through)
- Column-0 `#` submitted lines dispatch to mode `"ai"` when a
  `ModeDispatcher` is present (Issue 26082310413946)
- Live left-prompt `[ai]` overlay while the buffer is a full-line `#`
  comment (Issue 26082310413946 Exp 2)
- Full-line `#` input painted as default foreground, not bash comment
  gray (Issue 26082310413946 Exp 3)

## Apply (clean base)

```sh
BASE=c68420afd55f8dd3a3ec09e14f779ff48aebc8e5
git -C forks/nushell worktree add /tmp/astrohacker-nushell-pin "$BASE"
git -C /tmp/astrohacker-nushell-pin am \
  "$PWD/patches/nushell/patches/issue-26081311412273/"*.patch
```

## Prior State (Issue 26072213251282 Exp 2)

- Base `72b01f3e11a02c1a0abd6284cf97f6f37d96677f`; HEAD
  `6f21c94658801c99c6018ec24f25084198ced1c5`; multi-dir archives under
  `issue-26071814115751`, `issue-26072212103788`, `issue-26072213251282`
  (historical; release authority moved to cumulative Exp 4 archive).
