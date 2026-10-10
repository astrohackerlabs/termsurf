# AGENTS.md — fork patches

This is the durable record for ignored product forks. Every intentional source
edit under `forks/` must be represented here; there is no local-only fork
change. Never `git add` a path under `forks/`. Checkout `AGENTS.md` files are
ignored with the clone; write operator guidance in this tree
(`patches/<fork>/AGENTS.md`).

Before editing, create or switch to a branch containing both issue id and
experiment number:

```text
issue-{ISSUE_ID}-exp{N}-{short-slug}
```

Per-fork guidance may add a version prefix. After every intentional fork commit,
complete the same monorepo work unit:

1. generate the ordered `format-patch` archive under
   `patches/<fork>/patches/issue-{ISSUE_ID}/`;
2. update that fork's reconstruction README pin;
3. update `release-manifest.json` whenever the shipped series changes; and
4. record branch, base, HEAD/tree, patch paths, and required digests in the
   current experiment.

Work is incomplete while the branch name lacks `exp{N}`, a commit has no
tracked patch, the README/manifest pin differs from the fork tip, or the archive
is not committed in the monorepo. Release builds apply only the manifest.

A zero-patch tip pin is allowed only when no intentional Astrohacker source edit
exists; never create no-op patches. Per-fork reconstruction and archive style
live in each `README.md`; local hazards live in its `AGENTS.md`.

Chromium, Ghostty, Nushell, Reedline, Nexus, and Red are active patch/pin workspaces.
Red supplies the independent RedVim source baseline; TermSurf and NuTorch
release preparation exclude it.
WebKit, Gecko, and Ladybird are tombstones. Their historical archives remain
immutable unless a new issue deliberately revives the engine.

## Ah workflow

From the monorepo checkout, import `use scripts/ah.nu *`. Use `ah fork ls`,
`ah fork status [name]`, `ah fork check [name]`, `ah patch ls <name>` and
`ah patch check <name>` for discovery and verification. `ah fork outdated [name]`
reports which pinned bases are behind the upstream latest named by each
entry's `update` policy (`branch`, `tag` or `electron-stable`); set that policy
when adding a fork, and keep it matching how the base is actually chosen.
The manifest defines the active ordered series; archive date or issue number
does not select it. Patch checks need no clone. Status distinguishes actual
commit identity from equivalent replayed patches and trees; readiness also
requires cleanliness, the selected branch and pinned nested checkouts.

Preview with `ah fork prepare <name> --dry-run`, then run
`ah fork prepare <name>` to reconstruct and activate the selected series.
A missing checkout whose manifest entry has `upstream` is created with a full
`git clone` of that URL, then the series is applied. Chromium's `gclient`
provision shallow-fetches that base, installs depot_tools, fetches the nested
pins, applies the series, and syncs dependencies. An existing checkout is not
fetched, replaced, or synced. Preparation preserves old branches and caches and refuses
unsafe/divergent state. PWD selects this checkout's forks, including on linked
branches. No build, install, publication, or fallback to main is implicit.

For committed changes, use `ah patch export <name> --issue <id> --experiment <n>
--from <commit> --dry-run`, then omit `--dry-run` to export. Supply the selected
base for a cumulative archive, or a tip verified equivalent to the full selected
series for a delta. The issue must be open, the experiment unfinished, and the
clean fork branch must contain their IDs. Export verifies reconstruction and
returns ordered hashes, HEAD/tree and a proposed manifest entry.

By default export creates a new `patches/<name>/patches/issue-<id>/` directory
and refuses an existing one. A later experiment in the same issue uses
`--append`: a delta whose new files are numbered after the last one, allowed only
when that directory (or the issue's rebased `issue-<id>-exp<n>/`) is the last
selected archive directory. A series rebased
onto a new upstream base uses `--base <commit>` with `--from` equal to it. That
writes `issue-<id>/`, or `issue-<id>-exp<n>/` when the issue directory exists,
and proposes the new base. Every mode replays the series in a scratch clone,
and none rewrites existing archive bytes. Hand-built archives are recovery only;
never delete historical patches to make export run.
After export, explicitly update the reconstruction README, the manifest when
the shipped series changes, and experiment evidence, then commit the archive
with those records. Generated evidence is not approval or product qualification.
