# Reedline Patches

Astrohacker Shell uses a **path pin** of upstream Reedline under
`forks/reedline`. There is **no product source patch** — only an exact tip pin
that Nushell and `rust/ahsh` share via path dependency.

## Current State (Issue 26080213543507 Exp 5)

- Upstream repository: `https://github.com/nushell/reedline`
- Upstream base policy: **latest commit on upstream `main`**
- Upstream base / product HEAD: `60d9967420b5a56745f6ec250b40bc3b6813092b`
- Product tree: `5e3174ef4c31cb701a7b4f0a9062ac38a21e57c2`
- Version: `0.49.0`
- Local fork working tree: `forks/reedline`
- Product branch: `issue-26080213543507-exp5-reedline-main` (tip pin only)
- Product commits / patch files: `0` / `0`
- Empty patch-inventory aggregate SHA-256:
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Verification: **pin at main tip**; paired with Nushell Exp 5 + ahsh Release
- Consumers:
  - `forks/nushell` workspace `reedline = { path = "../reedline", version = "0.49.0" }`
  - `rust/ahsh` `reedline = { version = "0.49.0", path = "../../forks/reedline", … }`

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
4. If product edits appear, start an issue-scoped patch archive; until then
   keep zero-patch release-manifest pin (base == expected_head).
