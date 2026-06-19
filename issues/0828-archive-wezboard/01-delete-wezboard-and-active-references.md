# Experiment 1: Delete Wezboard and Remove Active References

## Description

Archive Wezboard by deleting `wezboard/` from the repository and updating active
tooling and documentation so the current repo no longer treats Wezboard as a
buildable, installable, or packaged component.

The archive is git history. Because a commit cannot contain its own hash, this
experiment will make one result commit that deletes `wezboard/`; the next
issue-document commit will record that exact deletion commit hash in this
experiment result and in the issue conclusion.

## Changes

- `wezboard/`
  - Delete the directory from the repository with `git rm -r wezboard`.
- `Cargo.toml`
  - Remove the stale `exclude = ["wezboard", ...]` entry because the directory
    no longer exists.
- `scripts/build.sh`
  - Remove the `wezboard` component, `build_wezboard`, and Wezboard from `all`.
  - Keep supported components aligned with the post-archive repo.
- `scripts/install.sh`
  - Remove the `wezboard` component, `install_wezboard`, and Wezboard from
    `all`.
- `scripts/uninstall.sh`
  - Remove the `wezboard` component and `uninstall_wezboard`.
  - Decide whether `all` should still remove old installed Wezboard artifacts as
    cleanup; if so, document that this is cleanup of legacy installs, not an
    installable current component.
- `scripts/release.sh`
  - Stop packaging `wezboard` and `TermSurf Wezboard.app`.
  - Package the current Ghostboard `TermSurf.app`, `web`, and Roamium artifacts
    so the release script remains aligned with the current frontend.
- Wezboard-only helper scripts
  - Delete or retire active helper scripts that depend on `wezboard/` paths,
    including `scripts/rename-wezterm.sh`, `scripts/test-issue-776-pdf.sh`,
    `scripts/test-issue-792-devtools-screenshot.sh`,
    `scripts/test-issue-794-pdf-interactions.sh`,
    `scripts/test-issue-794-real-pane-resize.sh`,
    `scripts/test-issue-794-real-wheel.sh`, and any helper used only by those
    Wezboard-only scripts.
- `homebrew/Casks/termsurf.rb`
  - Stop installing `TermSurf Wezboard.app` and the `wezboard` binary.
  - Point the cask at `TermSurf.app` and current packaged artifacts.
- `NOTICE`
  - Remove WezTerm/Wezboard notices for code that is no longer distributed in
    this repository.
- Active documentation and website files
  - Update current-facing documentation to say Wezboard is archived in git
    history and Ghostboard is the primary front-end.
  - Remove or rewrite current Wezboard build/install instructions.
  - Remove Wezboard from current navigation where it is presented as a current
    component.
- `webtui/src/ipc.rs` and active test assets
  - Rename stale comments or test labels that mention Wezboard as the current
    GUI. If Rust files change, run `cargo fmt`.

Do not edit closed historical issue files. Do not delete `ghostboard-legacy/`;
that is a separate historical archive.

## Verification

Confirm Wezboard was removed from tracked files:

```bash
test ! -d wezboard
test "$(git ls-files wezboard | wc -l | tr -d ' ')" = "0"
```

Search active files for remaining Wezboard references:

```bash
rg -n "Wezboard|wezboard|TermSurf Wezboard" \
  --glob '!wezboard/**' \
  --glob '!issues/[0-9][0-9][0-9][0-9]-*/**' \
  --glob '!logs/**' \
  --glob '!chromium/src/**' \
  --glob '!vendor/**' \
  .
```

Any remaining matches must be intentional archive/history notes, not active
build, install, release, package, or current-component instructions. Record the
remaining matches and rationale in the result.

Validate shell scripts:

```bash
bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh scripts/release.sh
```

Verify active scripts no longer route any current or `all` path through
Wezboard:

```bash
rg -n "wezboard|Wezboard|TermSurf Wezboard" \
  scripts/build.sh scripts/install.sh scripts/release.sh homebrew/Casks/termsurf.rb \
  && exit 1 || true
```

If `scripts/uninstall.sh` keeps legacy cleanup for already-installed Wezboard
artifacts, verify that every remaining Wezboard reference in that file is under
an explicitly named legacy cleanup path and not a supported component:

```bash
rg -n "wezboard|Wezboard|TermSurf Wezboard" scripts/uninstall.sh
```

Verify the supported component lists and release/cask artifact paths are
internally consistent without building:

```bash
rg -n "Components:.*(ghostboard|roamium|webtui|chromium|all)" \
  scripts/build.sh scripts/install.sh scripts/uninstall.sh
rg -n "TermSurf\\.app|ghostboard/macos/build/Release/TermSurf\\.app" \
  scripts/release.sh homebrew/Casks/termsurf.rb
rg -n "TermSurf Wezboard\\.app|wezboard" \
  scripts/release.sh homebrew/Casks/termsurf.rb && exit 1 || true
```

Validate component dispatch behavior without building:

```bash
if ./scripts/build.sh wezboard > logs/issue-0828-exp01-build-wezboard.log 2>&1; then
  echo "build.sh unexpectedly accepted wezboard"
  exit 1
fi
if ./scripts/install.sh wezboard > logs/issue-0828-exp01-install-wezboard.log 2>&1; then
  echo "install.sh unexpectedly accepted wezboard"
  exit 1
fi
if ./scripts/uninstall.sh wezboard > logs/issue-0828-exp01-uninstall-wezboard.log 2>&1; then
  echo "uninstall.sh unexpectedly accepted wezboard"
  exit 1
fi
rg -n "Unknown component: wezboard|Components:" \
  logs/issue-0828-exp01-build-wezboard.log \
  logs/issue-0828-exp01-install-wezboard.log \
  logs/issue-0828-exp01-uninstall-wezboard.log
```

Also verify `all` dispatch definitions by source inspection without launching
heavy builds:

```bash
rg -n "build_wezboard|install_wezboard|uninstall_wezboard|TermSurf Wezboard" \
  scripts/build.sh scripts/install.sh scripts/uninstall.sh scripts/release.sh \
  && exit 1 || true
```

Validate documentation formatting:

```bash
prettier --write --prose-wrap always --print-width 80 \
  README.md AGENTS.md docs/*.md issues/0828-archive-wezboard/*.md
```

If `webtui/src/ipc.rs` changes, run:

```bash
cargo fmt
```

Run broad hygiene:

```bash
git diff --check
```

Review response:

- Required: fix the active reference audit command. Fixed by matching the
  bounded audit command and excluding `wezboard/`, issue history, logs,
  `chromium/src/`, and `vendor/`.
- Required: add positive checks that supported build/install/release/cask paths
  remain coherent and no longer route through Wezboard. Fixed by adding script
  source checks for supported components, `all` dispatch references, and
  release/cask artifact names.
- Optional: specify which root-level Wezboard-only scripts are in scope. Fixed
  by listing the known scripts that launch `wezboard/target/...` and covering
  helpers used only by those scripts.

After the result review approves the deletion, make the result commit. Then run
`git rev-parse HEAD`, record that exact deletion commit hash in this experiment
result and the issue README conclusion, and commit the documentation update that
records the hash.

Pass criteria:

- `wezboard/` no longer exists in the worktree.
- `git ls-files wezboard` returns no tracked paths.
- Active build/install/uninstall/release/cask surfaces no longer advertise
  Wezboard as a current component.
- Current docs and website no longer present Wezboard as active or installable.
- Remaining non-historical Wezboard mentions, if any, are explicit archive notes
  or legacy cleanup references with rationale recorded.
- The issue records the exact deletion commit hash in a follow-up documentation
  commit after the deletion commit exists.

Fail criteria:

- `wezboard/` remains tracked or present after the experiment.
- Active scripts still try to build, install, or package Wezboard.
- Current docs still direct users to build or run Wezboard as a current
  frontend.
- The issue cannot identify the deletion commit hash.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

Findings:

- Required: the active reference audit command was not bounded consistently and
  could scan irrelevant local trees.
- Required: verification lacked positive checks that supported
  build/install/release/cask paths remain coherent after deletion.
- Optional: Wezboard-only helper scripts were under-specified.

Fixes:

- Replaced the active reference audit with a bounded command excluding
  `wezboard/`, issue history, logs, `chromium/src/`, and `vendor/`.
- Added source checks for supported component lists, release/cask artifact
  paths, and `all` dispatch references.
- Listed the known root-level Wezboard-only helper scripts and covered helpers
  used only by those scripts.

Re-review verdict: **Approved**.

## Result

**Result:** Pass

Implemented the archive deletion and current-surface cleanup:

- Deleted the tracked `wezboard/` tree and removed leftover ignored
  `wezboard/target` artifacts from the worktree.
- Removed stale Wezboard submodule entries from `.gitmodules`.
- Removed `wezboard` from the root Cargo workspace exclusions.
- Updated `scripts/build.sh`, `scripts/install.sh`, `scripts/uninstall.sh`, and
  `scripts/release.sh` so current supported components are Ghostboard, Roamium,
  Web TUI, Chromium, and `all`.
- Updated the Homebrew cask in the `homebrew/` submodule to install
  `TermSurf.app`, `web`, and Roamium, with no `wezboard` binary.
- Deleted root-level Wezboard-only helper scripts and the old active Wezboard
  component website page.
- Updated active documentation, website prose, test labels, and comments so
  Wezboard is no longer presented as the current frontend.

Verification run:

```bash
test ! -d wezboard
test "$(git ls-files wezboard | wc -l | tr -d ' ')" = "0"
bash -n scripts/build.sh scripts/install.sh scripts/uninstall.sh scripts/release.sh
cargo fmt
prettier --write --prose-wrap always --print-width 80 \
  README.md AGENTS.md scripts/ghostty-app/README.md issues/0828-archive-wezboard/*.md
git diff --check
```

All of the above passed.

Component dispatch verification:

```bash
./scripts/build.sh wezboard
./scripts/install.sh wezboard
./scripts/uninstall.sh wezboard
```

All three commands now reject `wezboard` with `Unknown component: wezboard`
before doing any build, install, uninstall, or sudo work.

Current script/cask reference checks passed:

```bash
rg -n "wezboard|Wezboard|TermSurf Wezboard" \
  scripts/build.sh scripts/install.sh scripts/release.sh homebrew/Casks/termsurf.rb

rg -n "TermSurf Wezboard\\.app|wezboard" \
  scripts/release.sh homebrew/Casks/termsurf.rb

rg -n "build_wezboard|install_wezboard|uninstall_wezboard|TermSurf Wezboard" \
  scripts/build.sh scripts/install.sh scripts/uninstall.sh scripts/release.sh
```

Each of those commands produced no matches.

The bounded active-reference audit still has intentional historical/archive
matches only:

- `AGENTS.md` and `website/src/pages/docs/architecture.astro` explicitly say
  Wezboard is archived in git history.
- `issues/README.md` contains historical closed issue titles and the open Issue
  828 title.
- `chromium/patches/**` contains old patch commit-message text from historical
  Wezboard-era Chromium patch exports.

Deletion commit hash: `d92ba788058d2569497789f6deabbffe7f2508b9`.

## Conclusion

Experiment 1 archived Wezboard from the active repo surface. The code directory
is deleted, current scripts and package metadata no longer expose Wezboard as a
component, and current docs point at Ghostboard/TermSurf as the frontend. The
deletion commit hash is `d92ba788058d2569497789f6deabbffe7f2508b9`.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer independently confirmed:

- `wezboard/` is absent and no tracked `wezboard` paths remain.
- `bash -n` passed for build/install/uninstall/release scripts.
- `build.sh`, `install.sh`, and `uninstall.sh` reject `wezboard` before sudo.
- Script/cask Wezboard reference grep produced no matches.
- `git diff --check`, `cargo fmt --check`, and `prettier --check` passed.
- `HEAD` was still the plan commit before the result commit.
- Recording the deletion commit hash in a follow-up documentation commit is
  sound.
