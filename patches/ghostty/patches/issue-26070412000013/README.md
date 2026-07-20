# Ghostty Issue 26070412000013 Patch Archive

This directory contains the first Astrohacker Terminal patch archive for the
Ghostty fork.

Archive:

- Patch: `0001-current-astrohacker-terminal-ghostty.patch`
- Upstream project: `ghostty-org/ghostty`
- Local ignored fork clone: `forks/ghostty`
- Base commit: `2c62d182cec246764ff725096a70b9ef44996f7f`
- Base summary: `gtk: fix context menu hiding quick-terminal (#12843)`
- Source input: current ignored fork checkout at `forks/ghostty`
- Temporary verification worktree:
  `/tmp/astrohacker-ghostty-issue-26070412000013-verify`
- Generated for Astrohacker issue:
  `issues/0013-astrohacker-terminal-monorepo-migration/`

This is a current-state archive only. It does not reconstruct historical
Ghostty/Ghostboard experiments.

Experiment 9 regenerated the archive after rebuild verification found that the
fork patch still referred to `../render-channel`. In the monorepo layout that
owned code lives at `rust/render-channel`, so the archive now points Ghostty's
build to `../../rust/render-channel`.

Issue 26070612000900 regenerated the archive after moving Astrohacker Terminal-owned XDG
paths from the shared `astrohacker` root to the product-scoped
`astrohacker/terminal` namespace.

Generation method:

```sh
git -C forks/ghostty rev-parse HEAD
# Must print 2c62d182cec246764ff725096a70b9ef44996f7f.

git -C forks/ghostty diff --binary \
  > patches/ghostty/patches/issue-26070412000013/0001-current-astrohacker-terminal-ghostty.patch
```

The fork checkout is ignored by the main repository. The patch archive is the
committed source of truth for the Ghostty fork delta.

Generated diff summary:

```text
220 files changed, 32374 insertions(+), 5723 deletions(-)
```

To verify application:

```sh
git -C forks/ghostty worktree prune
rm -rf /tmp/astrohacker-ghostty-issue-26070412000013-verify
git -C forks/ghostty worktree add /tmp/astrohacker-ghostty-issue-26070412000013-verify \
  2c62d182cec246764ff725096a70b9ef44996f7f
git -C /tmp/astrohacker-ghostty-issue-26070412000013-verify apply --check \
  "$PWD/patches/ghostty/patches/issue-26070412000013/0001-current-astrohacker-terminal-ghostty.patch"
git -C forks/ghostty worktree remove /tmp/astrohacker-ghostty-issue-26070412000013-verify --force
```
