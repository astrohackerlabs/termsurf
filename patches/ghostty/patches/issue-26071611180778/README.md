# Issue 26071611180778 Ghostty Patch Series

This archive preserves the observation-only split-tree and focus traces from
Experiment 1 and the AppKit overlay-lifetime correction from Experiment 2 of
Issue `26071611180778`. The correction keeps active or deferred TermSurf state
across a transient `window == nil` detach while leaving explicit browser,
pane-close, bridge, and deinitialization cleanup authoritative. It does not
change split construction, focus scheduling, browser engines, or the TermSurf
protocol.

- Parent product commit: `328d150826cb17be0f0eaa15fada9549fe2c60a1`
- Issue branch: `issue-26071611180778-split-webview-disappearance`
- Diagnostic commit: `4132f4a44d7f6ca32dd159a6a32191519d36864b`
- Correction commit: `58d5855ccfc1b2d5d788af87d708f8c1b9b15c98`
- Issue tree: `c49e204f49636262be90e23c0fd90e5b7c4f0a4e`
- Patch 1: `0001-Trace-the-split-tree-s-eclipse.patch`
- Patch 1 SHA-256:
  `cdb2689c7318dcd157f7d1dd9fccdbd730861960c009fc0dc595459502bdc9b8`
- Patch 2: `0002-Keep-the-browser-layer-alive.patch`
- Patch 2 SHA-256:
  `cd0853a66cf00339a8090ed155f846e877193948b729f9220d7a3982361f5f80`
- Replay verification: **Pass**; applying the archive to the parent commit
  produces tree `c49e204f49636262be90e23c0fd90e5b7c4f0a4e`, exactly matching the issue
  branch. Both patches are byte-identical to fresh `format-patch` output.
- Focused test verification: **Pass** via
  `macos/build.nu --action test --only-testing GhosttyTests/SurfaceViewAppKitTests`.
- Source build and corrected Chromium product verification: **Pass** via
  `scripts/build.sh ahterm` and the two-trial Experiment 2 preservation gate.

Apply and verify from the repository root:

```sh
BASE=328d150826cb17be0f0eaa15fada9549fe2c60a1
WORKTREE=/tmp/astrohacker-ghostty-26071611180778
git -C forks/ghostty worktree add --detach "$WORKTREE" "$BASE"
git -C "$WORKTREE" am \
  "$PWD/patches/ghostty/patches/issue-26071611180778/0001-Trace-the-split-tree-s-eclipse.patch" \
  "$PWD/patches/ghostty/patches/issue-26071611180778/0002-Keep-the-browser-layer-alive.patch"
git -C "$WORKTREE" rev-parse HEAD^{tree}
```
