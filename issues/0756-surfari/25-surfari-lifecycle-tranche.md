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
