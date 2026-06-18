# Experiment 2: Prove Multi-Profile Isolation

## Description

Experiment 1 showed that multi-profile isolation is the first fully uncovered
Issue 818 lifecycle row. Ghostboard and webtui already carry a profile string in
`SetOverlay`, Ghostboard keys browser servers by `profile/browser`, and Roamium
is spawned with a profile-specific `--user-data-dir`, but there is no runtime
row proving two profiles can be open at once without routing or storage-state
collisions.

This experiment will add a focused runtime scenario for two simultaneous
profiles. It should prove that profile A and profile B produce distinct
Ghostboard server keys, distinct Roamium processes/user-data dirs, distinct
browser tab/context identities, isolated keyboard/mouse routing, and
browser-observed storage isolation.

## Changes

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a `multi-profile-isolation` scenario.
  - Serve a local same-origin HTML fixture for both profiles. Use query
    parameters or paths only to tell the fixture whether the active browser is
    profile A or profile B.
  - Launch browser A with
    `web --browser "$ROAMIUM" --profile profilea "$PROFILE_URL_A"`.
  - Open a second native tab or window and launch browser B with
    `web --browser "$ROAMIUM" --profile profileb "$PROFILE_URL_B"`.
  - Assert Ghostboard logs `SetOverlay` for both profiles.
  - Assert Ghostboard creates or uses distinct server keys `profilea/${ROAMIUM}`
    and `profileb/${ROAMIUM}`, where `${ROAMIUM}` is the absolute browser path
    sent through `SetOverlay`.
  - Assert Ghostboard spawns Roamium once for each profile and that each spawn
    uses the corresponding profile-specific `--user-data-dir`.
  - Assert browser A and browser B have distinct pane ids, browser tab ids, and
    CA context ids.
  - Assert keyboard and hit-test routing do not leak between profiles.
  - In the same-origin fixture, write a profile-specific `localStorage` marker
    in profile A and a different marker in profile B. Assert profile A can read
    only A's marker and profile B can read only B's marker from the same origin.

Planned issue-document changes:

- Record the result in this experiment file.
- Update the Issue 818 README status for Experiment 2 after verification.

Planned app source changes:

- None. If the scenario exposes a Ghostboard-owned bug, record `Partial` or
  `Fail` and make the fix a later design-reviewed experiment.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0818-ghostboard-profile-tab-lifecycle-matrix/README.md issues/0818-ghostboard-profile-tab-lifecycle-matrix/02-prove-multi-profile-isolation.md`.

Static checks:

1. `git diff --check`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh multi-profile-isolation`.

Pass criteria:

- The scenario launches both profiles successfully.
- Ghostboard logs `SetOverlay` with `profile=profilea` and `profile=profileb`.
- Ghostboard logs distinct server keys for `profilea/${ROAMIUM}` and
  `profileb/${ROAMIUM}`, where `${ROAMIUM}` is the absolute path sent through
  `SetOverlay`.
- Ghostboard logs two distinct spawned Roamium pids, one for each profile.
- Each spawn line includes the expected profile-specific `--user-data-dir`.
- Browser A and browser B have distinct pane ids, browser tab ids, context ids,
  and selected native tab/window ids as appropriate for the chosen layout.
- Browser A receives keyboard/mouse input only when profile A's pane is active.
- Browser B receives keyboard/mouse input only when profile B's pane is active.
- Both profiles load the same local fixture origin.
- Profile A reports only the profile A `localStorage` marker from that origin.
- Profile B reports only the profile B `localStorage` marker from that origin.

Partial criteria:

- Profile-specific process/server identity and routing isolation are proven, but
  same-origin browser-observed storage isolation is not proven.
- Routing isolation passes, but reconnect/server-reuse behavior remains
  untested.
- The scenario exposes a distinct lifecycle bug that should be fixed in the next
  experiment.

Fail criteria:

- Both profiles cannot launch.
- Both profiles reuse the same Ghostboard server key, Roamium process, browser
  tab id, context id, or user-data directory.
- Keyboard or mouse input leaks between profile A and profile B.
- Same-origin `localStorage` markers leak between profile A and profile B.
- The scenario requires app source changes before the multi-profile behavior can
  be classified.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Aristotle the 2nd`:

- **Initial verdict:** Changes required.
- **Finding 1:** The pass criteria could approve multi-profile isolation without
  proving browser-observed storage isolation. Fixed by making same-origin
  `localStorage` isolation mandatory for Pass and only Partial if process/server
  identity is proven without storage isolation.
- **Finding 2:** The planned storage check could be invalid if profile A and
  profile B loaded different origins. Fixed by requiring a same-origin local
  fixture for both profiles and using only query parameters or paths to identify
  which marker to write.
- **Optional finding:** Server-key wording could be misread as named-browser
  behavior. Fixed by spelling out that `${ROAMIUM}` is the absolute path sent
  through `SetOverlay`.
- **Final verdict:** Approved. The reviewer confirmed the prior Required
  findings were resolved and no new Required finding was introduced.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 818 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.
