+++
status = "closed"
opened = "2026-03-06"
closed = "2026-03-06"
+++

# Issue 713: Rename `homepage/` back to `website/`

## Goal

Rename the website directory from `homepage/` back to `website/`. The original
name was clear and simple — the renaming in Issue 711 was unnecessary.

## Background

Issue 711 renamed `website/` to `homepage/` (via an intermediate `termsurf.com/`
step). In hindsight, `website/` was the right name all along. It's the most
natural, obvious name for the directory that contains the project website.

## Analysis

Same pattern as every other rename. The changes are:

1. Directory rename: `homepage/` → `website/`
2. `homepage/package.json` name field: `"termsurf-homepage"` →
   `"termsurf-website"`
3. Root `.prettierignore`: `homepage/.next` → `website/.next`
4. `ghostboard/.prettierignore`: `homepage/.next` → `website/.next`
5. `ghostboard/.gitattributes`: `homepage/**` → `website/**`

Historical issue docs are immutable — no changes to concluded issues.

## Experiments

### Experiment 1: Rename `homepage/` to `website/`

#### Changes

**1. Directory rename:**

- `homepage/` → `website/`

**2. `homepage/package.json`** (becomes `website/package.json`):

- `"name": "termsurf-homepage"` → `"name": "termsurf-website"`

**3. `.prettierignore`** (root):

- `homepage/.next` → `website/.next`

**4. `ghostboard/.prettierignore`:**

- `homepage/.next` → `website/.next`

**5. `ghostboard/.gitattributes`:**

- `homepage/** linguist-documentation` → `website/** linguist-documentation`

#### Verification

1. `ls website/package.json` — directory exists
2. `grep -r 'homepage/' .prettierignore` — no stale references in root
3. `grep -r 'homepage/' ghostboard/.prettierignore ghostboard/.gitattributes` —
   no stale references

**Result:** Pass

All verifications passed. Git detected all files as renames (100% match).

#### Conclusion

Clean rename back to `website/`.

## Conclusion

The directory is back to `website/` where it started. Issue 711 renamed it
through `termsurf.com/` and `homepage/`, but the original name was the best fit
all along.
