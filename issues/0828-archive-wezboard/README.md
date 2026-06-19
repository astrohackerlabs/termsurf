+++
status = "closed"
opened = "2026-06-19"
closed = "2026-06-19"
+++

# Issue 828: Archive Wezboard

## Goal

Archive Wezboard by deleting `wezboard/` from the TermSurf repository and
recording the exact commit hash where the deletion lands.

When solved, Ghostboard remains the primary TermSurf front-end, Wezboard source
code is no longer present in the repo, and the issue documents the deletion
commit hash so the archived implementation can be recovered from git history.

## Background

Wezboard was the active TermSurf GUI while Ghostboard was archived and later
rebuilt. Ghostboard has now returned as the primary front-end. Keeping Wezboard
in the repo adds size, build surface, and maintenance burden for a deprecated
implementation.

The archive mechanism for this issue is intentionally simple: delete the
`wezboard/` directory from the repo. Do not move it to another folder inside the
same repo. Git history is the archive.

## Analysis

The deletion commit is the key artifact. A commit cannot contain its own hash,
so the deletion experiment should land first, then a follow-up issue-document
commit should record the exact deletion commit hash in the experiment result and
final conclusion.

The implementation should also update active documentation and scripts that
still claim Wezboard is an installable or buildable current component. Those
updates should be scoped to avoiding broken references after `wezboard/` is
deleted. Historical issue documents remain immutable and should not be changed.

Initial active surfaces to audit include:

```text
AGENTS.md
README.md
docs/
homebrew/
scripts/
test-html/
webtui/
website/
```

## Acceptance Criteria

- `wezboard/` is deleted from the repository.
- The exact deletion commit hash is recorded in this issue.
- Active build, install, uninstall, release, and documentation surfaces no
  longer advertise Wezboard as a current component.
- Historical closed issues are not rewritten.
- Ghostboard remains documented as the primary TermSurf front-end.
- The issue conclusion records how to recover Wezboard from git history.

## Notes

Do not create experiments upfront. Design Experiment 1 after this issue is open.

## Experiments

- [Experiment 1: Delete Wezboard and remove active references](01-delete-wezboard-and-active-references.md)
  — **Pass**

## Conclusion

Wezboard was archived by deleting `wezboard/` from the repository in commit
`d92ba788058d2569497789f6deabbffe7f2508b9`.

That commit also removed active Wezboard build/install/release surfaces, deleted
Wezboard-only helper scripts and active website pages, updated the Homebrew cask
through submodule commit `3f15e14`, and updated current documentation to treat
Ghostboard/TermSurf as the active frontend.

To recover the archived Wezboard source, check out or inspect any commit before
`d92ba788058d2569497789f6deabbffe7f2508b9`, for example:

```bash
git checkout d92ba788058d2569497789f6deabbffe7f2508b9^ -- wezboard
```
