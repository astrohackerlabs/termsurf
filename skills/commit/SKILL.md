---
name: commit
description: "Write entertaining commit messages as poetry"
---

# GitPoet

Write commit messages that accurately describe changes while delighting readers
with poetic wit.

## Philosophy

Every commit tells a story. GitPoet transforms mundane diffs into memorable
verses. The goal is to make people smile when they read the commit log on
termsurf.com.

## Format

Each commit message should have two parts:

1. **First line**: A short, accurate summary (50 chars max) - this is the
   "title"
2. **Body**: A short poem (2-8 lines) that humorously describes the change

## Style Guidelines

- **Accuracy first**: The poem must accurately describe what changed
- **Humor over formality**: Prefer wit, wordplay, puns, and absurdity
- **Keep it short**: Poems should be 2-8 lines, not epics
- **Vary the form**: Mix haikus, limericks, couplets, free verse, etc.
- **Stay tasteful**: Funny but professional enough for public viewing

## Examples

### Haiku style

```
Fix null pointer crash

A pointer walked alone,
Into the void it did fall—
Now it checks its path.
```

### Limerick style

```
Add dark mode toggle

A user who coded at night,
Found the screen far too bright.
So we added a switch,
Now it's dark, what a pitch!
Their retinas now feel just right.
```

### Couplet style

```
Refactor auth module

The auth code was a tangled mess,
Now it's clean—we must confess.
```

### Free verse style

```
Update dependencies

The packages grew old and weary,
Their CVEs made security teary.
We bumped the versions, one by one,
Now npm audit says: "Well done!"
```

## Arguments

`/commit <what>` — commit exactly `<what>`. Stage only the files related to
`<what>` and nothing else. Do not include unrelated changes. If no argument is
given, look at all uncommitted changes and commit everything.

## Process

1. **Check the argument.** If the user provided `<what>`, identify exactly which
   files belong to that scope. Stage only those files.
2. **Run git diff --staged** to see what's being committed
3. **Understand the change**: What problem does it solve? What was
   added/removed/fixed?
4. **Write the title**: Accurate, imperative mood, 50 chars max
5. **Compose the poem**: Pick a style that fits the change, make it fun
6. **Stage and commit** using the poetic message

## Commit Message Mechanics

Poem lines must be real newline characters in Git history. Never put literal
`\n` sequences inside a shell string and expect Git to interpret them.

For multi-line commit messages, prefer writing the message to a temporary file
and committing with `git commit -F <file>`:

```text
Title under 50 chars

Poem line one,
Poem line two.
```

Then run:

```bash
git commit -F /path/to/message-file
```

If using `git commit -m`, only use it for separate paragraphs where the shell
arguments already contain the exact intended text. Do not encode poem line
breaks as `\n` inside `-m` arguments.

After committing, verify the rendered message:

```bash
git log -1 --format=%B
```

If literal `\n` appears in the output, amend or rewrite the commit message
immediately before continuing.

## Chromium Fork Workflow

The Chromium fork at `chromium/src/` is a separate git repo (gitignored from the
main TermSurf repo). It doesn't use `main`. Branches are named
`{version}-termsurf` (e.g., `146.0.7650.0-termsurf`), built as a series of
commits on top of the vanilla Chromium version tag. The branch name encodes the
upstream version.

**Git-poet applies to all TermSurf commits — both inside the Chromium fork and
in the main repo.**

**The only exception is `git am` patches.** Patches applied with `git am` (e.g.,
from Electron) keep their original commit messages. We don't rewrite upstream
authorship.

**Typical flow:**

1. Work inside `chromium/src/` on the `{version}-termsurf` branch
2. Commit with git-poet
3. Return to the main repo
4. Update `chromium/README.md` if the branch or version changed
5. Commit with git-poet — this records what TermSurf did at the project level

**When creating a new Chromium branch**, add it to the Branches table in
`chromium/README.md` with a link to the corresponding issue doc.

## When NOT to use GitPoet

- Merge commits (use standard merge messages)
- Reverts (use standard revert messages)
- Version bumps (keep these straightforward)
- Security fixes (be clear, not clever)
- `git am` patches (keep upstream commit messages intact)
