# Gecko Patches

Astrohacker Terminal uses Gecko/Firefox for the planned `gecko` engine
(`ah-geckod` / `libtermsurf_gecko`). The working tree is local-only under
`forks/gecko`. This directory tracks patch archives and branch notes that are
safe to commit.

## Current State

- **Upstream remote:** `https://github.com/mozilla-firefox/firefox.git`
  (active monorepo; **not** frozen `mozilla/gecko-dev`)
- **Upstream policy:** default branch **`main`** tip
- **Current base:** `0ae9827c4d7bc8b28ccbfa58324ded73b68dccf6`
- **Current branch:** `ffe9a294-issue-26071212001982-exp1`
- **Historical archive:** `patches/gecko/patches/issue-26071112000932/` — **eight** patches:
  Exp4 **windowless** probe stack + Exp7 **visible window** CLI
  (`--astrohacker-visible-window`) + Exp8 HOLD-mode follow-on (counted in
  Issue 26071219393388 inventory; prior README said seven — corrected)
- **Current product archive:**
  `patches/gecko/patches/issue-26071212001982/` — Exp1 IOSurface export + Exp4
  continuous control file + **Exp5 hosted input control**
  (`0003-Exp5-hosted-input-control.patch`: HOLD-mode input control file →
  `sendNativeMouseEvent` + content DOM delivery; fixture-owned page oracles).
- Checkout type: **partial clone** (`blob:none`) with **full commit history**
  (`git rev-parse --is-shallow-repository` → `false`). Not a depth-limited
  shallow clone; blobs are on-demand until a build needs them.
- Working tree: `forks/gecko`
- **Full build:** **Pass** (Issue 26071212001982 Exp1, reconfirming Issue 932
  Exp1) — non-artifact desktop Firefox
  (`--enable-application=browser`), objdir `obj-astrohacker-ff`,
  `COMPILE_ENVIRONMENT=1`, Nightly.app runs via direct-bin launch
- **mozconfig:** local untracked `forks/gecko/mozconfig` (never commit); see
  experiment Results for full text + SHA-256
- **Bootstrap:** `./mach --no-interactive bootstrap --application-choice browser`
  with Python 3.12 pin; `MOZBUILD_STATE_PATH` default `~/.mozbuild`
- **Monorepo scaffold (Exp2):** `rust/gecko`, helper `ah-geckod`, C ABI
  `libtermsurf_gecko` in **stub** mode
- **Exp4 windowless load:** in-tree HiddenFrame + remote `<browser>` CLI probe;
  runner `rust/gecko/libtermsurf_gecko/probes/windowless/run-windowless-probe.sh`
- **Exp7 visible window:** on-screen chrome window (not terminal pane yet);
  runner `rust/gecko/libtermsurf_gecko/probes/visible/run-visible-window.sh`
- **Issue 26071212001982 Exp1 surface seam:** direct P1 WebRender/native-layer
  IOSurface tile export builds and passes the standalone cross-process pixel,
  animation, Retina tiled-resize, acknowledged lifetime, and early-exit runner at
  `rust/ah-geckod/libtermsurf_gecko/probes/surface-seam/run-surface-seam.sh`.
- **Experiments:** [Exp1](../../issues/0932-create-gecko/exp-0001-gecko-fork-build-run.md),
  [Exp2](../../issues/0932-create-gecko/exp-0002-gecko-engine-scaffold.md),
  [Exp3](../../issues/0932-create-gecko/exp-0003-xpcom-embed-probe.md),
  [Exp4 windowless](../../issues/0932-create-gecko/exp-0004-in-tree-windowless.md),
  [Exp7 visible window](../../issues/0932-create-gecko/exp-0007-visible-pane-window.md)

Historical note: an earlier placeholder used
`https://github.com/mozilla/gecko-dev.git` @ `5836a062…` (master frozen ~2025-07).
Issue 26071112000932 retargeted to `mozilla-firefox/firefox` `main`.

## Merge-upstream

1. Discover tip: `git ls-remote --symref
https://github.com/mozilla-firefox/firefox.git HEAD` (expect `main`).
2. Fetch tip; branch `{short8}-issue-NNNN` at tip commit.
3. Apply `patches/gecko/patches/issue-NNNN/*.patch` in numeric order when an
   archive exists (`git am`).
4. Bootstrap if toolchains drift: Python 3.9–3.12 +
   `./mach --no-interactive bootstrap --application-choice browser`.
5. Full build:

   ```bash
   export MOZCONFIG=$PWD/forks/gecko/mozconfig
   export PATH="/opt/homebrew/opt/python@3.12/bin:$PATH"
   cd forks/gecko && python3.12 ./mach build
   ```

6. Run smoke: direct-bin Nightly with disposable profile (see Exp1 verification).
7. Regenerate archive after product commits; update this README Current State.

## Applying Patches

When an archive exists:

```bash
cd forks/gecko
git fetch origin <base-sha>
git switch -C <short8>-issue-NNNN <base-sha>
git am ../../patches/gecko/patches/issue-NNNN/*.patch
```

## Generating Patches

```bash
rm -rf patches/gecko/patches/issue-NNNN
mkdir -p patches/gecko/patches/issue-NNNN
git -C forks/gecko format-patch <base>..HEAD \
  -o "$PWD/patches/gecko/patches/issue-NNNN"
```

## Verification

```bash
git -C forks/gecko remote get-url origin   # mozilla-firefox/firefox
git -C forks/gecko rev-parse HEAD
test -d forks/gecko/obj-astrohacker-ff/dist/Nightly.app
```
