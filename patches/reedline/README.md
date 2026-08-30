# Reedline Patches

Astrohacker Shell uses a Reedline checkout under `forks/reedline` that
Nushell and `code/termsurf/rs/ahsh` share via path dependency. Issue 26082214188331
adds a product `AGENTS.md` overlay (one patch on the upstream tip).

## Current State (Issue 26083023000538 Exp 2)

- Upstream repository: `https://github.com/nushell/reedline`
- Upstream base policy: **latest commit on upstream `main`**
- Upstream base: `9230319ae57f88bac5a2a17dc3f9a313cff3330d`
- Product branch: `issue-26083023000538-exp2-refresh-guidance`
- Product parent: `3a5079b3627445327881da20d9aa03b9fa8e5a3a`
- Product HEAD: `06fcc7049f387afc61920b2a119209637b7fefe0`
- Product tree: `73c3e0b5e552d7aa2edacfdb7ca334a6a15d8ab7`
- Version: `0.50.0`
- Local fork working tree: `forks/reedline`
- Issue archives: `patches/reedline/patches/issue-26082214188331/`
  (**1**) + `patches/reedline/patches/issue-26082310413946/` (**1**) +
  `patches/reedline/patches/issue-26083023000538/` (**1** refreshed
  `AGENTS.md`)
- Product commits / patch files: `3` / `3`
- Archive aggregate SHA-256:
  `49861ea2c5e566bc9cccd0a2b217c4b72a22adc04a80fe0bcd97b4d2a182baa3`
- New patch SHA-256:
  `4ab772538038638c247bb0b1e0f5b8ec7786b1264321a571a91e92ef027e42fb`
- New stable patch ID: `fcc2a9292c5a9d04521cd8b2f78ae0482bde24e0`
- Verification: guidance-only direct child; canonical release manifest is the
  shipping authority.

## Prior State (Issue 26082310413946 Exp 2)

- Upstream repository: `https://github.com/nushell/reedline`
- Upstream base policy: **latest commit on upstream `main`**
- Upstream base: `9230319ae57f88bac5a2a17dc3f9a313cff3330d`
- Product branch: `issue-26082310413946-exp2-live-ai-prompt`
- Product HEAD: `3a5079b3627445327881da20d9aa03b9fa8e5a3a`
- Product tree: `6cf0fc1141427db8a5db2cc3f4f439a0f515f847`
- Version: `0.50.0` (`rust-version` 1.95.0; helix is default-on in reedline
  — Nu/ahsh keep `default-features = false` so helix stays off)
- Local fork working tree: `forks/reedline`
- Issue archives: `patches/reedline/patches/issue-26082214188331/` (**1**
  Astrohacker `AGENTS.md` overlay) +
  `patches/reedline/patches/issue-26082310413946/` (**1** live-buffer
  left prompt)
- Product commits / patch files: `2` / `2`
- Archive aggregate SHA-256:
  `c6b6701f23f3287f897c86921e4a4f911803606608071dea42419ae1161681bc`
- Verification: **TREE_MATCH**; paired with Nushell Exp 2 + ahsh

## Prior State (Issue 26082214188331 Exp 1)

- Upstream repository: `https://github.com/nushell/reedline`
- Upstream base policy: **latest commit on upstream `main`**
- Upstream base: `9230319ae57f88bac5a2a17dc3f9a313cff3330d`
- Product branch: `issue-26082214188331-exp1-agents-overlay`
- Product HEAD: `d2e82678e0a63f09e7b94fed54851e02eb612154`
- Product tree: `c2a64ea66361595079357ae3222a3cb0a97c5c29`
- Version: `0.50.0` (`rust-version` 1.95.0; helix is default-on in reedline
  — Nu/ahsh keep `default-features = false` so helix stays off)
- Local fork working tree: `forks/reedline`
- Issue archive: `patches/reedline/patches/issue-26082214188331/` (**1**
  Astrohacker `AGENTS.md` overlay)
- Product commits / patch files: `1` / `1`
- Archive aggregate SHA-256:
  `1c9e91f7595e12fa0efac9d54d970a7d466a52f9f71521cf1c0cfb17aed08561`
- Verification: **TREE_MATCH**; paired with Nushell overlay + ahsh

## Prior State (Issue 26081615463315 Exp 2)

- Upstream repository: `https://github.com/nushell/reedline`
- Upstream base policy: **latest commit on upstream `main`**
- Upstream base / product HEAD: `9230319ae57f88bac5a2a17dc3f9a313cff3330d`
- Product tree: `c0f4a5ff6ff58b9f4756f567d5576da7daeb8177`
- Version: `0.50.0` (`rust-version` 1.95.0; helix is default-on in reedline
  — Nu/ahsh keep `default-features = false` so helix stays off)
- Local fork working tree: `forks/reedline`
- Product branch: `issue-26081615463315-exp2-reedline-main` (tip pin only)
- Product commits / patch files: `0` / `0`
- Empty patch-inventory aggregate SHA-256:
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Verification: **pin at main tip**; paired with Nushell Exp 2 + ahsh Release
- Consumers:
  - `forks/nushell` workspace `reedline = { path = "../reedline", version = "0.50.0", default-features = false }`
  - `code/termsurf/rs/ahsh` `reedline = { version = "0.50.0", path = "../../../../forks/reedline", default-features = false, features = ["sqlite", "bashisms"] }`

## Prior State (Issue 26081311412273 Exp 5)

- Upstream base / product HEAD: `464efb2ed5b8d294f81b787e70418006d48428f3`
- Product tree: `c904d8f3393e8e67ee4d02f8624656ac9804675f`
- Version: `0.49.0`
- Product branch: `issue-26081311412273-exp5-reedline-main` (tip pin only)

## Prior State (Issue 26080213543507 Exp 5)

- Upstream base / product HEAD: `60d9967420b5a56745f6ec250b40bc3b6813092b`
- Product tree: `5e3174ef4c31cb701a7b4f0a9062ac38a21e57c2`
- Version: `0.49.0`
- Product branch: `issue-26080213543507-exp5-reedline-main` (tip pin only)

## Prior State (Issue 26072616587256 Exp 4)

- Upstream base / product HEAD: `7eb9bf219456202052aaa976842e9e790b88ed85`
- Product tree: `67c5aed1ea36ac15c03139a43d916b6ba348451b`
- Product branch: `issue-26072616587256-exp4-reedline-main` (tip pin only)

## Prior State (Issue 26071814115751)

- Pin `f776f5079e49d075c071660ae0f9b040b3ff909b` / tree
  `76093e9dd271aaa3627d27c53a6b9d881c22c88b` (historical).

## Merge-upstream checklist

1. `git ls-remote https://github.com/nushell/reedline.git refs/heads/main`
2. Checkout tip on `issue-NNNN-reedline` (or detached tip).
3. Confirm `Cargo.toml` version; rebuild `ahsh` with Nushell path pin.
4. Re-apply `patches/reedline/patches/issue-26082214188331/` while the
   overlay remains product source; update this README and
   `patches/release-manifest.json`. Do not return to a zero-patch pin
   while `AGENTS.md` is an Astrohacker edit.
