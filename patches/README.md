# Fork Patches

This directory tracks Astrohacker patch sets for large upstream projects whose
working trees live outside git under `forks/`.

Policy:

- `forks/` contains local upstream working trees and build state. It is ignored.
- `patches/` contains the tracked documentation and patch archives needed to
  reconstruct Astrohacker fork branches from recorded upstream commits.
- Each fork should record its upstream base, branch naming convention, patch
  generation command, and patch application command.
- Branch names should map to Astrohacker issues and experiments whenever fork
  source changes are made.

`release-manifest.json` is the machine-readable authority for the cumulative
fork inputs shipped by the next Homebrew release. It records ordered active
patch directories, exact counts/digests, base/head/tree identities, and narrow
untracked paths only when they are exact clean nested checkouts with pinned
head/tree identities. The release command never guesses “latest” from issue
folder names. Per-fork READMEs retain reconstruction detail and historical
archives, but a released patch change must also update the manifest.

Historical per-fork pointers (the release manifest is authoritative):

- `chromium/` — **Issue 26071814115751:** Electron stable Chromium **150.0.7871.114** /
  archive `issue-26071814115751` (122 patches; release-manifest pin).
- `webkit/` — **Issue 26071814115751:** main tip `e0ee95bc…` / archive
  `issue-26071814115751` (2 patches; release-manifest pin).
- `ladybird/` — **Issue 26071814115751:** master tip `5baf8116…` / archive
  `issue-26071814115751` (22 patches; release-manifest pin).
- `ghostty/` — **Issue 26071814115751:** main tip `f3c9a2b7…` / archive
  `issue-26071814115751` (17 patches; release-manifest pin).
- `gecko/` — optional; no product patch set required for 924.
- `nushell/` — **Issue 26071814115751:** main tip `72b01f3e…` / archive
  `issue-26071814115751` (4 patches).
- `reedline/` — **Issue 26071814115751:** main tip `f776f507…` / zero-patch pin.
  (Helix/editor patch tree retired; recovery only via monorepo history — Issue
  26071716113040.)

### Merge-upstream (portfolio)

1. Identify targets (Electron stable Chromium; main/master tips for others).
2. Per fork: branch → apply/regenerate patches → build → smoke → update
   per-fork README + issue-scoped archive under `patches/<fork>/patches/`.
3. Integrated: `TERMSURF_LADYBIRD_BACKEND=real scripts/build.sh all --release`.
4. Do not publish Homebrew from upgrade issues (use Issue 26071112000925+).
