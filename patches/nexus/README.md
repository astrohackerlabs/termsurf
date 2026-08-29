# Nexus Patches

Astrohacker TermSurf chat (`ahnexus`) uses a **product fork** of upstream
Nexus under `forks/nexus`. The primary consumer is the `nexus-common` crate
(protocol framing, types, TLS helpers). The iced `nexus-client` GUI is **not**
an Astrohacker product dependency.

## Current State (Issue 26082214188331 Exp 1)

- Upstream repository: `https://github.com/zquestz/nexus`
- SSH: `git@github.com:zquestz/nexus.git`
- Upstream base: `85c9edab73733f412f6855800fecdd8da3e76d14`
- Product branch: `issue-26082214188331-exp1-agents-overlay`
- Product HEAD: `57243d2337ea4a0fc351b3ad632d330a3659a484`
- Product tree: `4699720ef4b6480430a936df06cf3da076b6bbed`
- `nexus-common` version / `PROTOCOL_VERSION`: `0.9.10`
- Local fork working tree: `forks/nexus`
- Issue archive: `patches/nexus/patches/issue-26082214188331/` (**1**
  Astrohacker `AGENTS.md` overlay)
- Product commits / patch files: `1` / `1`
- Archive aggregate SHA-256:
  `fab23ef20e14cf3d51c91789613f72d7a74d10451a60eaaf48afc4526308f9f3`
- Verification: **TREE_MATCH**; `scripts/build.sh ahnexus --release`

## Prior State (Issue 26081311412273 Exp 6)

- Upstream repository: `https://github.com/zquestz/nexus`
- SSH: `git@github.com:zquestz/nexus.git`
- Upstream base policy: **latest commit on upstream `main`** (pin-only until first product edit)
- Upstream base / product HEAD: `85c9edab73733f412f6855800fecdd8da3e76d14`
- Product tree: `b92461717cc9eae13989579f471910e58c4cb7c1`
- `nexus-common` version / `PROTOCOL_VERSION`: `0.9.10`
- Local fork working tree: `forks/nexus`
- Product branch: `issue-26081311412273-exp6-nexus-main` (tip pin only)
- Product commits / patch files: `0` / `0`
- Empty patch-inventory aggregate SHA-256:
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Issue pin note: `patches/nexus/patches/issue-26081311412273/`
- Verification: **pin at upstream main tip**; `scripts/build.sh ahnexus --release`
- Consumers:
  - `code/termsurf/rs/ahnexus`: `nexus-common = { path = "../../../../forks/nexus/nexus-common" }`

## Prior State (Issue 26080119514255 Exp 3)

- Upstream base / product HEAD: `9b9b69ca90c3a7d93d70069c9e8ec085e31bf508`
- Product tree: `ea616288dd97bc656c04c8688739962757b46d95`
- `nexus-common` version / `PROTOCOL_VERSION`: `0.9.7`
- Product branch: `issue-26080119514255-exp3-nexus-official-fork` (tip pin only)
- Issue pin note: `patches/nexus/patches/issue-26080119514255/`

## Patch contents

Issue 26082214188331 Exp 1: Astrohacker `AGENTS.md` overlay (one patch
on upstream base `85c9edab73733f412f6855800fecdd8da3e76d14`). See Current
State.

## Archive style

When patches appear: **issue-scoped** directory; prefer regenerating a
**cumulative-from-base** series (`base..HEAD`) into that issue folder unless an
experiment documents ordered multi-dir apply. Pin-only state uses
`patch_directories: []` and `patch_count: 0` in the release manifest.

## Apply (clean base)

```sh
BASE=85c9edab73733f412f6855800fecdd8da3e76d14
git clone https://github.com/zquestz/nexus.git forks/nexus
git -C forks/nexus checkout -B issue-26082214188331-exp1-agents-overlay "$BASE"
git -C forks/nexus am "$PWD/patches/nexus/patches/issue-26082214188331/"*.patch
```

## Generate (after product commits on the issue branch)

```sh
BASE=85c9edab73733f412f6855800fecdd8da3e76d14   # or current documented base
git -C forks/nexus format-patch -o patches/nexus/patches/issue-{ID}/ "${BASE}..HEAD"
# then update this README pin + release-manifest.json
```

## Branch naming

```text
issue-{ISSUE_ID}-exp{N}-{short-slug}
```

## Merge-upstream checklist

1. `git ls-remote https://github.com/zquestz/nexus.git refs/heads/main`
2. Fetch; rebase or re-pin product branch; re-apply patches if any.
3. `git rev-parse HEAD` / `HEAD^{tree}`; update README + release-manifest.
4. Rebuild consumers (`cargo check -p ahnexus` when path-dep is wired).
5. Overlay is product source: keep the issue-scoped archive and update
   expected_head / expected_tree. Do not drop to a zero-patch pin while
   `AGENTS.md` remains an Astrohacker edit.

## Scope notes

- Product protocol crate: **`nexus-common`**
- Optional local smoke: build/run `nexus-server` / `nexusd` from the same tree
- Do **not** ship iced `nexus-client` as the TermSurf UI
