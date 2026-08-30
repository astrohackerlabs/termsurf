# AGENTS.md — fork patches

This is the durable record for ignored product forks. Every intentional source
edit under `forks/` must be represented here; there is no local-only fork
change.

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

Chromium, Ghostty, Nushell, Reedline, and Nexus are active patch/pin workspaces.
WebKit, Gecko, and Ladybird are tombstones. Their historical archives remain
immutable unless a new issue deliberately revives the engine.
