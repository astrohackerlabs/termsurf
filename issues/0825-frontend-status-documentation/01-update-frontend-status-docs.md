# Experiment 1: Update Frontend Status Docs

## Description

Audit current mutable documentation for stale frontend status language, then
update the docs that guide ordinary TermSurf development so they clearly say:
Ghostboard is the primary front-end, Wezboard is deprecated, and Roastty is a
proof-of-concept.

This experiment is documentation-only. It must not modify code or closed issue
records.

## Changes

Planned files:

- `AGENTS.md`
  - update the Multiple GUIs section so Ghostboard is the primary front-end;
  - mark Wezboard as deprecated/reference;
  - add Roastty as proof-of-concept/reference if mentioned in the frontend
    status list;
  - update the process topology diagram and socket example away from Wezboard;
  - update directory structure and build/run guidance so current frontend work
    centers Ghostboard;
  - preserve Wezboard instructions only if explicitly labeled deprecated or
    reference.
- `CLAUDE.md`
  - apply the same current frontend status and build/run guidance updates as
    `AGENTS.md`, because `README.md` points contributors there and it carries
    the same stale Wezboard-active language.
- `README.md`
  - update the Terminal, install, build, and run sections so they no longer tell
    users that TermSurf is based on Wezboard or should be launched through
    Wezboard;
  - describe Ghostboard as the primary terminal front-end;
  - avoid making unverified packaging claims beyond what current docs already
    support.
- `docs/early-prototypes.md`
  - clarify that the archived Ghostboard Legacy entry is historical reference
    and that the current `ghostboard/` tree is the recreated primary front-end;
  - do not rewrite prototype history.
- `docs/vendor.md`
  - clarify the Roastty dependency-source section as proof-of-concept/reference
    material rather than the production frontend direction.
- Open issue documentation that describes current frontend direction
  - audit open issue README files for stale Wezboard-active or future-Ghostboard
    language;
  - update any open issue docs that describe current frontend architecture, such
    as `issues/0756-surfari/README.md`, so they refer to the current primary
    front-end as Ghostboard;
  - list any remaining open-issue matches as historical or otherwise justified.
- `issues/0825-frontend-status-documentation/README.md`
  - update Experiment 1 status after the result.
- `issues/0825-frontend-status-documentation/01-update-frontend-status-docs.md`
  - record design review, result, completion review, and conclusion.

Explicit non-changes:

- Do not modify closed issue documents.
- Do not modify source code, build scripts, packaging scripts, or generated
  code.
- Do not remove historical references to Wezboard, Roastty, or Ghostboard Legacy
  when they are clearly historical.
- Do not claim Wezboard has been deleted; it is deprecated/reference code.

## Verification

Pass criteria:

- `AGENTS.md` identifies Ghostboard as the primary TermSurf front-end.
- `AGENTS.md` identifies Wezboard as deprecated/reference, not active.
- `AGENTS.md` identifies Roastty as proof-of-concept/reference if Roastty is
  mentioned in current frontend status.
- `CLAUDE.md` matches the same frontend status as `AGENTS.md`.
- `README.md` does not direct users to launch or build Wezboard as the current
  primary TermSurf frontend.
- `docs/early-prototypes.md` keeps Ghostboard Legacy historical and archived,
  while avoiding stale claims that Ghostboard will only return in the future.
- `docs/vendor.md` does not present Roastty dependency work as the production
  frontend path.
- Open issue docs do not describe Wezboard as the current active GUI or
  Ghostboard as only future work.
- No closed issue file is changed:

  ```bash
  git diff --name-only -- 'issues/[0-9][0-9][0-9][0-9]-*/**' |
    while read -r path; do
      rel="${path#issues/}"
      issue_dir="issues/${rel%%/*}"
      if rg -q '^status = "closed"$' "$issue_dir/README.md"; then
        echo "$path"
      fi
    done
  ```

  The command must print nothing.

- Stale current-status grep has no unqualified active-Wezboard or
  future-Ghostboard claims in mutable current docs:

  ```bash
  rg -n \
    "Active GUI|Active Development|currently ships as a WezTerm fork|Ghostboard.*Archived|Will return|Wezboard.*Active|launch Wezboard|Build.*Wezboard|Roastty.*production|Roastty.*primary" \
    AGENTS.md CLAUDE.md README.md docs
  ```

  Any remaining matches must be explicitly listed and justified as historical or
  deprecated/reference.

- Open issue stale-status grep has no unqualified active-Wezboard or
  future-Ghostboard claims:

  ```bash
  for readme in issues/[0-9][0-9][0-9][0-9]-*/README.md; do
    if rg -q '^status = "open"$' "$readme"; then
      rg -n \
        "Wezboard \\(GUI\\)|Wezboard.*creates|Wezboard.*composit|Wezboard.*code path|Ghostboard.*Archived|Will return|Active GUI|Active Development" \
        "$readme" || true
    fi
  done
  ```

  Any remaining matches must be explicitly listed and justified.

- Markdown files are formatted:

  ```bash
  git diff --name-only -- '*.md' |
    xargs prettier --write --prose-wrap always --print-width 80
  ```

- Markdown formatting check passes:

  ```bash
  git diff --name-only -- '*.md' |
    xargs prettier --check
  ```

- `git diff --check` passes.
- Design review is recorded and approved before implementation.
- The Experiment 1 plan commit exists before non-issue documentation edits
  begin.
- Completion review approves before the result commit.

Fail criteria:

- Current docs still describe Wezboard as the active frontend.
- Current docs still describe Ghostboard as only archived or future work.
- Current docs imply Roastty is the primary production frontend.
- A closed issue document is modified.
- The experiment changes runtime code or build scripts.

## Design Review

Fresh-context adversarial design review initially returned **CHANGES REQUIRED**
with two required findings:

- the closed-issue diff guard derived `issues` instead of the changed issue
  directory, so it would miss closed issue modifications;
- the design omitted open issue docs from the audit scope even though Issue 825
  covers current mutable documentation and `issues/0756-surfari/README.md`
  contained stale Wezboard GUI language.

The reviewer also raised one optional finding:

- the formatting verification used only `prettier --write`; a non-mutating
  `prettier --check` should also be present.

The design was updated to fix the closed-issue guard, add open issue docs to the
scope and stale-status grep, and add `prettier --check`. Re-review found those
issues resolved but raised one new required finding:

- the formatting commands listed specific files and omitted newly in-scope open
  issue docs.

The formatting commands were changed to use `git diff --name-only -- '*.md'` so
all edited markdown files are covered dynamically. Final re-review returned
**APPROVED** with no required findings.
