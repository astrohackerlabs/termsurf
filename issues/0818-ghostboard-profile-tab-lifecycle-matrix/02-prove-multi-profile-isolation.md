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

Browser tab id uniqueness is interpreted with the profile/process identity. If
two profiles run in distinct Roamium processes, both processes may legitimately
report browser tab id `1`; in that case, profile/server key, pane id, Roamium
pid, and CA context id must be distinct.

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

## Result

**Result:** Pass

The `multi-profile-isolation` runtime scenario was added to
`scripts/ghostboard-geometry-matrix.sh` and passed at timestamp
`20260618-013435`.

Verification run:

```bash
bash -n scripts/ghostboard-geometry-matrix.sh
git diff --check
scripts/ghostboard-geometry-matrix.sh multi-profile-isolation
```

Runtime artifacts:

- App log:
  `/Users/astrohacker/dev/termsurf/logs/ghostboard-geometry-multi-profile-isolation-app-20260618-013435.log`
- Roamium trace:
  `/Users/astrohacker/dev/termsurf/logs/ghostboard-geometry-multi-profile-isolation-roamium-20260618-013435.log`
- Harness log:
  `/Users/astrohacker/dev/termsurf/logs/ghostboard-geometry-multi-profile-isolation-harness-20260618-013435.log`

Observed pass evidence:

- Profile A launched with `profile=profilea`, server key
  `profilea//Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`,
  Roamium pid `10803`, and a `--user-data-dir` under
  `chromium-profiles/profilea`.
- Profile B launched with `profile=profileb`, server key
  `profileb//Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`,
  Roamium pid `11195`, and a `--user-data-dir` under
  `chromium-profiles/profileb`.
- Profile A and profile B had distinct native tab/window ids, pane ids, Roamium
  pids, and CA context ids.
- Profile B hit-testing targeted profile B's context and profile B keyboard
  input reached only profile B.
- Switching back to profile A focused profile A's pane, hit-testing targeted
  profile A's context, and profile A keyboard input reached only profile A.
- Both profiles loaded the same local origin. Profile A first observed
  `before=none after=profilea`; profile B first observed
  `before=none after=profileb`; after profile B ran, profile A reloaded and
  observed `before=profilea after=profilea`.

The only pass-criteria adjustment discovered during implementation was that
browser tab ids are process-local. Both profile A and profile B legitimately
reported browser tab id `1` because they were backed by separate Roamium
processes. The durable cross-profile identity is therefore the combination of
profile/server key, pane id, Roamium pid, and CA context id, not browser tab id
alone.

## Conclusion

Ghostboard's runtime multi-profile isolation is proven for simultaneous profiles
in separate native tabs. Distinct profiles create distinct server keys, spawn
distinct Roamium processes with profile-specific user-data directories, maintain
separate CA contexts and panes, isolate same-origin `localStorage`, and do not
leak keyboard or mouse routing across active profiles.

The next experiment should move to another uncovered Issue 818 lifecycle row,
such as warm reconnect/server reuse or stale process cleanup, rather than
continuing multi-profile isolation.

## Completion Review

Fresh-context adversarial completion review by Codex subagent `Ampere the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Optional finding:** The original pass/fail criteria still said browser tab
  ids must be distinct, while the result documented that tab ids are
  process-local. Fixed by adding a criteria note that browser tab id uniqueness
  is interpreted with the profile/process identity, and that distinct profiles
  in distinct Roamium processes may both report browser tab id `1`.
- **Checks performed by reviewer:**
  `bash -n scripts/ghostboard-geometry-matrix.sh`, `git diff --check`,
  confirmation that the last commit was the plan commit `7d44f7203`,
  confirmation that the working-tree diff was limited to the harness and Issue
  818 docs, and inspection of the claimed `20260618-013435` runtime logs.
