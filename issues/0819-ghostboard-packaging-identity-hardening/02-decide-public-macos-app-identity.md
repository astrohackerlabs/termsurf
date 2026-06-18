# Experiment 2: Decide Public macOS App Identity

## Description

Experiment 1 found that the debug Ghostboard app currently identifies as
`TermSurf`, while repo-level distribution still packages `TermSurf Wezboard.app`
and the project may eventually ship multiple TermSurf GUI apps. Before changing
bundle ids, app names, Homebrew artifacts, install paths, or installed browser
discovery, Issue 819 needs a deliberate public macOS identity contract.

This experiment will make the identity decision explicit and document it as the
baseline for later implementation experiments. It is decision/documentation
only; no app source, Xcode, packaging, or Homebrew behavior changes are planned.

## Changes

Planned issue-document changes:

- Add a result section to this experiment that records:
  - the chosen public app name;
  - the chosen installed app bundle path;
  - the chosen bundle identifier family for debug, local release, and
    distributable release builds;
  - the chosen executable and CLI names;
  - whether the app must coexist with Wezboard and future GUI apps;
  - which inherited Ghostty names remain implementation-only;
  - which user-visible Ghostty names must be fixed in later experiments.
- Update the Issue 819 README experiment status after verification.

Planned source changes:

- None.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0819-ghostboard-packaging-identity-hardening/README.md issues/0819-ghostboard-packaging-identity-hardening/02-decide-public-macos-app-identity.md`.

Static checks:

1. `git diff --check`.

Decision inputs:

1. Re-read the Issue 819 goal and Experiment 1 result.
2. Re-read the repo vision in `AGENTS.md`, especially the multiple-GUI product
   model.
3. Inspect current distribution naming in:
   - `homebrew/Casks/termsurf.rb`
   - `scripts/release.sh`
   - `scripts/install.sh`
   - `scripts/uninstall.sh`
4. Inspect current Ghostboard app identity in:
   - `ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist`
   - `ghostboard/macos/Ghostty.xcodeproj/project.pbxproj`

Pass criteria:

- The experiment chooses a concrete public macOS app identity for Ghostboard.
- The decision accounts for coexistence with `TermSurf Wezboard.app` and future
  TermSurf GUI apps.
- The decision specifies app name, installed bundle path, bundle id family,
  executable name, CLI name if any, and Homebrew/release artifact naming.
- The decision explicitly states which Ghostty names may remain
  implementation-only and which user-visible names must be fixed.
- The decision produces a direct implementation sequence for later experiments.
- No source or packaging behavior is changed.

Partial criteria:

- The experiment narrows the app identity options but still requires a user
  product decision before implementation.

Fail criteria:

- The experiment changes app/source/release behavior before the identity
  contract is recorded.
- The decision ignores Wezboard/future GUI coexistence.
- The result leaves bundle ids or install paths ambiguous.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Ohm the 2nd`:

- **Verdict:** Approved.
- **Findings:** None.
- **Verification:** The reviewer confirmed the README links Experiment 2 as
  `Designed`, the experiment includes the required design sections, no result is
  present yet, and the plan commit had not been made before review.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 819 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.
