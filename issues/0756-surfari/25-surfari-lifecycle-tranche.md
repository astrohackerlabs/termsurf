# Experiment 25: Run Surfari lifecycle tranche

## Description

Experiment 24 created the Surfari real-app matrix and recommended the first
execution tranche: lifecycle/navigation/resize/shutdown/restart. This tranche
should upgrade the current smoke-level evidence into direct matrix evidence for
explicit navigation and restart while preserving the already-proven resize and
shutdown behavior.

This experiment should stay single-window and single-pane. It should not expand
into pane resize, split panes, tab switching, window switching, focus switching,
profile isolation, crash handling, click/drag details, or the full
Ghostboard/Roamium comparison.

## Changes

- Add or extend a focused Surfari lifecycle harness under `scripts/`.
- Use deterministic local fixtures so navigation can be proven without network
  dependencies:
  - fixture A for initial load;
  - fixture B for explicit navigation after the browser is already ready.
- Prove the lifecycle tranche in the real Debug `TermSurf.app`:
  - Surfari launch and `BrowserReady`;
  - visible CAContext overlay;
  - initial load state;
  - explicit navigation from fixture A to fixture B;
  - WebTUI and Surfari URL/title/state evidence after navigation;
  - real app window resize causes Surfari `resize`;
  - `CloseTab` removes the tab and cleanly shuts Surfari down;
  - a second launch after shutdown starts a fresh Surfari process, registers,
    presents the overlay, and reaches fixture A or B without stale state.
- Update `issues/0756-surfari/real-app-matrix.md` after verification:
  - mark navigation `Proven` if explicit navigation passes;
  - keep resize and shutdown `Proven` with the new lifecycle evidence;
  - mark restart `Proven` if the second launch proof passes.

## Verification

Pass criteria:

- Required builds/artifacts exist:

```bash
surfari/libtermsurf_webkit/build.sh
cargo build -p surfari
cargo build -p webtui
cd ghostboard && zig build
```

- Run the lifecycle tranche harness.
- The harness must prove:
  - initial Surfari `BrowserReady`;
  - initial fixture title/URL state;
  - explicit navigation after initial load;
  - post-navigation title/URL state in Surfari and WebTUI traces;
  - window resize produces Surfari resize evidence;
  - close/shutdown evidence;
  - second launch/restart evidence with a new Surfari process or new
    registration after shutdown.
- Update `real-app-matrix.md` only for rows directly proven by this experiment.
- Run hygiene checks:

```bash
git diff --check
bash -n <new-or-updated-lifecycle-harness>
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/25-surfari-lifecycle-tranche.md \
  issues/0756-surfari/real-app-matrix.md
```

Result classification:

- `Pass` means navigation and restart become directly proven while resize and
  shutdown remain proven in the same real-app lifecycle harness.
- `Partial` means the harness improves lifecycle evidence but one or more of
  navigation, resize, shutdown, or restart remains unproven.
- `Fail` means the harness cannot launch Surfari or cannot produce stronger
  lifecycle evidence than Experiments 20-24.

## Design Review

Adversarial design review returned `APPROVED` with no Required findings. The
reviewer confirmed that the README links Experiment 25 as `Designed`, the file
has Description, Changes, and Verification sections, the scope stays within
lifecycle/navigation/resize/shutdown/restart, the design explicitly excludes
panes, tabs, windows, focus switching, profiles, crash handling, click/drag, and
the full comparison, the verification requires proof for navigation, resize,
shutdown, and restart, matrix updates are guarded against overclaiming, hygiene
checks are present, and the plan commit had not already been made.

## Result

**Result:** Pass

Added `scripts/test-issue-756-surfari-lifecycle-tranche.sh` and ran it
successfully with run ID `20260621-190346`.

Evidence:

- `logs/issue-756-exp25-surfari-lifecycle/app-20260621-190346.log`
- `logs/issue-756-exp25-surfari-lifecycle/surfari-trace-20260621-190346.log`
- `logs/issue-756-exp25-surfari-lifecycle/webtui-20260621-190346.log`
- `logs/issue-756-exp25-surfari-lifecycle/harness-20260621-190346.log`

The harness proved the lifecycle tranche inside the real Debug `TermSurf.app`:

- run 1 produced `BrowserReady` for `browser=surfari`;
- AppKit presented the Surfari overlay;
- Surfari created fixture A and loaded title `Issue 756 Lifecycle A`;
- WebTUI observed the fixture A title;
- the harness sent an explicit `Navigate` protobuf to Surfari;
- Surfari loaded fixture B and emitted the fixture B URL/title;
- WebTUI observed the fixture B URL/title;
- resizing the real TermSurf window from `950, 720` to `900x680` produced a
  Surfari `resize ... ffi=ts_set_view_size` trace;
- direct `CloseTab` removed the tab and reached `no-tabs-remaining`;
- relaunch produced a fresh Surfari trace init, `BrowserReady`, AppKit overlay
  presentation, fixture A creation, and WebTUI title state.

Updated `issues/0756-surfari/real-app-matrix.md` conservatively:

- `Navigation` is now `Proven` for single-pane explicit navigation after initial
  load.
- `Resize` and `Shutdown` remain `Proven` with Experiment 25 as additional
  evidence.
- `Restart` is now `Proven` for clean relaunch after shutdown.

## Conclusion

The first real-app Surfari tranche is complete. Surfari can launch through
Ghostboard, navigate after initial load, resize with the real app window, close
cleanly, and restart without stale overlay state in the single-window,
single-pane case.

The next experiment should move to pane/split/tab/window/focus geometry. It
should not claim profile isolation, crash handling, click/drag parity, or full
Roamium comparison until those rows have their own direct evidence.

## Completion Review

Adversarial completion review returned `APPROVED` with no Required findings. The
reviewer independently confirmed that the result commit had not been made, the
expected uncommitted docs and harness were present, the harness assertions were
non-vacuous for launch, overlay presentation, explicit navigation, WebTUI
URL/title updates, resize, `CloseTab` shutdown, and relaunch, and the run
`20260621-190346` logs support the matrix updates. The reviewer also verified
`bash -n`, scoped `git diff --check`, and Prettier checks; they did not rerun
build commands because those can mutate build artifacts.
