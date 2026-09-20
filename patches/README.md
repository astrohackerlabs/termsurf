# Fork Patches

## Current operator workflow

```nu
use scripts/ah.nu *
ah fork ls
ah fork status
ah patch ls nushell
ah patch check nushell
ah fork prepare nushell --dry-run
ah fork prepare nushell
ah fork check nushell
```

PWD selects the monorepo checkout and its local forks. Linked branches never
borrow main's clones. Listing and archive checks work without clones; prepare
requires an existing clone with its recorded base. It reconstructs a recognized
prefix in isolation, verifies ordered patch IDs/count/tree, then activates the
selected branch. Original branches, ignored caches and pinned nested checkouts
are preserved. Dirty/divergent histories, occupied branches, active Git
operations, locks and path collisions refuse with diagnostics.

Status reports each fork independently; check requires readiness and raises a
catchable error on failures. Exact HEAD equality is reported separately because
replaying the same patches can change commit hashes. None of these checks
establishes runtime behavior or published/installed product acceptance.

On a clean issue/experiment fork branch, preview and export committed changes:

```nu
ah patch export nushell --issue <id> --experiment <n> --from <commit> --dry-run
ah patch export nushell --issue <id> --experiment <n> --from <commit>
```

Use the selected upstream base for a cumulative replacement series, or a tip
proven equivalent to the complete selected series for a delta. The issue must
be open and the experiment unfinished. Export reconstructs the proposed series
in a scratch clone, verifies its tree and ordered identities, and creates a new
`patches/<fork>/patches/issue-<id>/` directory. Existing directories refuse;
manual archive work is still needed to extend an existing issue directory or
change upstream bases. Do not delete historical archives to bypass this limit.

The returned record contains source HEAD/tree, ordered paths and digests, and
a proposed manifest entry. Explicitly update the fork reconstruction README,
the manifest when shipping selection changes, and experiment evidence, then
commit those records and patches together. Export does not stage, commit,
approve a result, build or publish. See [AGENTS.md](AGENTS.md).

## Layout and authority

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
product fork inputs. Each publisher selects its own inputs; a manifest entry
alone does not establish publication. It records ordered active
patch directories, exact counts/digests, base/head/tree identities, and narrow
untracked paths only when they are exact clean nested checkouts with pinned
head/tree identities. The release command never guesses “latest” from issue
folder names. Per-fork READMEs retain reconstruction detail and historical
archives, but a released patch change must also update the manifest.

RedVim's independent source baseline is maintained under
[`red/README.md`](red/README.md). Its pin is in the shared manifest, but it is
not a TermSurf or NuTorch release input. TermSurf preparation explicitly selects
Ghostty, Nushell, Reedline, Nexus and Chromium; NuTorch selects Nushell/Reedline.

Active forks are Chromium, Ghostty, Nushell, Reedline, Nexus and Red. WebKit, Gecko
and Ladybird are tombstones; historical archives are immutable.

Historical per-fork pointers (not current selections; the manifest is authoritative):

- `chromium/` — **Issue 26071814115751:** Electron stable Chromium **150.0.7871.114** /
  archive `issue-26071814115751` (122 patches; release-manifest pin).
- `webkit/` — **tombstone:** not a shipped engine (Issue 26072120115614);
  historical archives under `patches/webkit/patches/` only.
- `ladybird/` — **tombstone:** not a shipped engine (Issue 26072117006454); historical archives under `patches/ladybird/patches/` only.
- `ghostty/` — **Issue 26071814115751:** main tip `f3c9a2b7…` / archive
  `issue-26071814115751` (17 patches; release-manifest pin).
- `gecko/` — retired; this former optional engine has historical archives only.
- `nushell/` — **Issue 26071814115751:** main tip `72b01f3e…` / archive
  `issue-26071814115751` (4 patches).
- `reedline/` — **Issue 26071814115751:** main tip `f776f507…` / zero-patch pin.
  (Helix/editor patch tree retired; recovery only via monorepo history — Issue
  26071716113040.)
- `nexus/` — **Issue 26080119514255 Exp 3:** pin-only
  `9b9b69ca…` / zero-patch pin (`nexus-common` for `ahnexus`). See
  [`nexus/README.md`](./nexus/README.md).

### Merge-upstream (portfolio)

1. Identify targets (Electron stable Chromium; main/master tips for others).
2. Per fork: branch → apply/regenerate patches → build → smoke → update
   per-fork README + issue-scoped archive under `patches/<fork>/patches/`.
3. Build and smoke affected consumers using the current `scripts/build.nu`
   product targets (Chromium engine for ship).
4. Do not publish Homebrew from upgrade issues (use Issue 26071112000925+).
