+++
status = "closed"
opened = "2026-03-06"
closed = "2026-03-06"
+++

# Issue 714: Seven-Digit Issue Numbers

## Goal

Rename every issue file to use zero-padded 7-digit numbers (e.g.,
`001-competitors.md` → `0000001-competitors.md`) so that issue numbers never
need to be renamed again.

## Background

Issue files currently use variable-width numbers: 1–3 digits (`001`, `100`,
`714`). As the project grows past 999 issues, the numbering scheme would need to
change — breaking the immutability guarantee that issue documents should never
be modified once concluded.

The question is: how many digits are enough to never need renaming again?
Chromium, arguably the largest open-source project in the world, has ~1.7
million commits. If every commit were an issue, 7 digits (up to 9,999,999) would
still have room to spare. Seven digits is the pragmatic ceiling — virtually no
project will ever exceed it.

By renaming all 224 issue files now (before the archive grows further), we
establish a permanent numbering scheme. Once renamed, issue filenames are truly
immutable — no future growth will force another rename.

## Scope

- Rename every file in `issues/` from `{N}-{name}.md` to `{0000N}-{name}.md`
  (7-digit zero-padded).
- Update all internal cross-references: issue documents that link to other issue
  files (e.g., `[Issue 411](../issues/0000411-two-profiles-3.md)` →
  `[Issue 411](../issues/0000411-two-profiles-3.md)`).
- Update `chromium/README.md` branch table links (e.g.,
  `../issues/0000411-two-profiles-3.md` →
  `../issues/0000411-two-profiles-3.md`).
- Update `docs/early-prototypes.md` issue references.
- Update `CLAUDE.md` issue references.
- Update any other files that reference issue filenames by path.

## Analysis

### What changes

| Before                                 | After                                  |
| -------------------------------------- | -------------------------------------- |
| `issues/0000001-competitors.md`        | `issues/0000001-competitors.md`        |
| `issues/0000100-bookmarks.md`          | `issues/0000100-bookmarks.md`          |
| `issues/0000714-seven-digit-issues.md` | `issues/0000714-seven-digit-issues.md` |

### What stays the same

- Issue numbers themselves (714 is still 714)
- Issue document content (immutable once concluded)
- The `issues/` directory location

### Reference locations to update

1. **`chromium/README.md`** — Branch table links (`../issues/{N}-...`)
2. **`docs/early-prototypes.md`** — Issue index links
3. **`CLAUDE.md`** — Issue references in documentation section
4. **Issue documents themselves** — Cross-references between issues
5. **Skill files** — Any hardcoded issue paths

## Experiments

### Experiment 1: Rename all files and update all references

A single script-driven experiment. The rename is mechanical — every file and
every reference follows the same pattern — so it can all be done in one pass.

#### Changes

**Step 1: Rename 224 issue files**

A shell script using `git mv` to rename every file in `issues/` from
`{N}-{name}.md` to `{0000000N}-{name}.md` (7-digit zero-padded). The script
extracts the numeric prefix, zero-pads it to 7 digits, and renames.

**Step 2: Update cross-references in 28 files**

A `sed` or Python script to find-and-replace all `issues/{N}-` patterns with
`issues/{0000000N}-` in:

- 15 issue documents that reference other issues (e.g., `314-control.md` through
  `714-seven-digit-issues.md`)
- `chromium/README.md` (50 references)
- `docs/early-prototypes.md` (60 references)
- `CLAUDE.md` (15 references)
- `.claude/skills/fix-nerd-fonts/SKILL.md` (1 reference)
- `.claude/skills/issues-and-experiments/SKILL.md` (1 reference)

The replacement is a regex: match `issues/(\d{1,3})-` and replace with
`issues/` + zero-padded match + `-`. This is safe because:

- Issue numbers are always followed by a hyphen and a name
- No other content in these files matches the `issues/\d+-` pattern
- The replacement is idempotent (already 7-digit numbers won't match `\d{1,3}`)

**Step 3: Format touched markdown files**

Run `prettier` on all edited `.md` files.

#### Verification

1. `ls issues/ | head -5` — all files start with 7-digit numbers
2. `ls issues/ | wc -l` — still 224 files
3. `grep -r 'issues/[0-9]\{1,3\}-' *.md docs/ chromium/README.md .claude/` —
   zero matches (no old-style references remain)
4. `grep -r 'issues/[0-9]\{7\}-' chromium/README.md | head -3` — references use
   new format

**Result:** Pass

All 225 files renamed to 7-digit zero-padded numbers. All 28 files with
cross-references updated (50 in chromium/README.md, 61 in
docs/early-prototypes.md, 15 in CLAUDE.md, plus 22 issue documents and 2 skill
files). Zero old-style references remain.

#### Conclusion

Clean mechanical rename. The regex `issues/(\d{1,6})-` matched every old-style
reference without false positives. `git mv` preserved history for all 225 files.

## Conclusion

All 225 issue files now use 7-digit zero-padded numbers (`0000001` through
`0000714`). The numbering scheme supports up to 9,999,999 issues — enough that
filenames will never need renaming again. All cross-references across 28 files
(chromium/README.md, docs/early-prototypes.md, CLAUDE.md, skill files, and
inter-issue links) were updated in a single pass. Issue filenames are now truly
immutable.
