---
name: merge-upstream
description: "Merge upstream changes from Ghostty, WezTerm, or cef-rs into TermSurf"
arguments:
  - name: repo
    description: "Which upstream to merge: ghostty, wezterm, or cef-rs"
    required: true
---

# Merge Upstream

Merge changes from one of our upstream repositories into TermSurf.

## Usage

```
/merge-upstream <repo>
```

Where `<repo>` is one of:
- `ghostty` - Merge from ghostty-org/ghostty into ts5/ (active development)
- `wezterm` - Merge from wez/wezterm into ts2/ and root
- `cef-rs` - Merge from tauri-apps/cef-rs into vendor/cef-rs/

## Upstream Repositories

| Repo | Directory | Remote | Upstream URL | Branch | Merge method |
|------|-----------|--------|--------------|--------|--------------|
| Ghostty | `ts5/` | `upstream` | github.com/ghostty-org/ghostty | main | `git subtree pull` |
| WezTerm | `ts2/` + root | `wezterm-upstream` | github.com/wez/wezterm | main | `git merge -X subtree` |
| cef-rs | `vendor/cef-rs/` | `cef-rs-upstream` | github.com/tauri-apps/cef-rs | dev | `git merge -X subtree` |

## Steps

1. **Read the documentation** - For ghostty, read `docs/ghostty.md`. For wezterm and cef-rs, read `docs/issues/002-merge-upstream.md`.

2. **Pre-merge checklist**
   - Ensure working tree is clean (`git status`)
   - All changes committed
   - Note current HEAD: `git rev-parse HEAD`

3. **Fetch and review upstream**
   ```bash
   # For ghostty:
   git fetch upstream
   git log --oneline upstream/main ^$(git log --all --grep="git-subtree-dir: ts5" --format=%H | head -1) | head -20

   # For wezterm:
   git fetch wezterm-upstream
   git rev-list --count HEAD..wezterm-upstream/main

   # For cef-rs:
   git fetch cef-rs-upstream
   git rev-list --count HEAD..cef-rs-upstream/dev -- vendor/cef-rs/
   ```

4. **Merge upstream**
   ```bash
   # For ghostty (uses git subtree):
   git subtree pull --prefix=ts5 upstream main -m "Merge upstream Ghostty into ts5"

   # For wezterm (uses subtree merge):
   git merge -X subtree=ts2 wezterm-upstream/main -m "Merge upstream WezTerm"

   # For cef-rs (uses subtree merge):
   git merge -X subtree=vendor/cef-rs cef-rs-upstream/dev -m "Merge upstream cef-rs"
   ```

5. **Resolve conflicts** - For ghostty, no TermSurf modifications yet so no
   conflicts expected. For wezterm and cef-rs, see repo-specific notes below.

6. **Fix build errors** - API changes may require updates to our code.

7. **Verify and test**
   - For Ghostty: `cd ts5 && zig build`
   - For WezTerm: `cargo build` (from root)
   - For cef-rs: `cd vendor/cef-rs && cargo build --example osr`

8. **Commit** any additional fixes needed after the merge.
