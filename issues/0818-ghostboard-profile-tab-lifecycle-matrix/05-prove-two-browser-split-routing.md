# Experiment 5: Prove Two-Browser Split-Pane Routing

## Description

Experiment 1 left multi-pane routing only partially covered: the existing split
rows prove a single browser pane next to a terminal sibling, but they do not
prove two simultaneous browser overlays in one split layout. Experiments 2
through 4 proved multi-profile isolation, same-profile server reuse,
close/reopen, and final server cleanup in native-tab layouts, but the Issue 818
matrix still needs a direct split-pane lifecycle row.

This experiment will add and run a focused runtime scenario for two browser
panes in one split. It should open browser A, create a right split, launch
browser B in the sibling pane, prove both panes have distinct pane/tab/context
identity, prove both overlays render at separate split frames, prove mouse
hit-testing targets the pane under the cursor, prove keyboard input reaches only
the focused browser, close one browser pane, and prove the surviving browser
pane remains interactive without receiving stale events from the closed pane.

The experiment is proof-first. No app source changes are planned. If the
scenario exposes a Ghostboard-owned routing or cleanup bug, record the result as
`Partial` or `Fail` and make the fix a later design-reviewed experiment.

## Changes

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a `two-browser-split-routing` scenario.
  - Reuse the existing split-right automation shape and existing browser routing
    helpers where possible.
  - Launch browser A in the initial pane with
    `web --browser "$ROAMIUM" --profile default "$URL"`.
  - Create a right split, focus the sibling terminal pane, and launch browser B
    with `web --browser "$ROAMIUM" --profile default "$URL_B"`.
  - Assert browser B reuses the same `default/${ROAMIUM}` server/pid as browser
    A, because both panes use the same profile and browser.
  - Assert browser A and browser B have distinct pane ids, browser tab ids, CA
    context ids, terminal surface ids, and split overlay frames.
  - Assert both AppKit presented frames and pixels correspond to their split
    locations and do not overlap incoherently.
  - Click inside browser A and browser B, and assert hit-testing targets the
    expected CA context and selected pane for each click.
  - Enter Browse mode in browser A and browser B in turn, send keyboard markers,
    and assert each marker reaches only the active browser tab/pane.
  - Close browser B's split pane and assert `CloseTab` reaches Roamium for
    browser B while browser A and the shared server remain alive.
  - Click and type in browser A after browser B closes, and assert browser A
    remains interactive while closed browser B receives no input.

Planned issue-document changes:

- Record the result in this experiment file.
- Update the Issue 818 README status for Experiment 5 after verification.

Planned app source changes:

- None.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0818-ghostboard-profile-tab-lifecycle-matrix/README.md issues/0818-ghostboard-profile-tab-lifecycle-matrix/05-prove-two-browser-split-routing.md`.

Static checks:

1. `git diff --check`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh two-browser-split-routing`.

Pass criteria:

- Browser A launches successfully in the initial pane.
- Browser B launches successfully in the right split pane.
- Browser B reuses the existing `default/${ROAMIUM}` server and Roamium pid
  instead of spawning a second default-profile process.
- Browser A and browser B have distinct pane ids, browser tab ids, CA context
  ids, terminal surface ids, and non-overlapping split overlay frames.
- AppKit presents both overlays at their expected split-frame locations and
  pixel sizes.
- Clicking inside browser A produces a hit-test against browser A's CA context
  and selected pane, not browser B's.
- Clicking inside browser B produces a hit-test against browser B's CA context
  and selected pane, not browser A's.
- Keyboard input reaches browser A only when browser A is focused.
- Keyboard input reaches browser B only when browser B is focused.
- Closing browser B's split pane sends timely `CloseTab` for browser B while
  Roamium is still attached.
- Browser A remains interactive after browser B closes.
- Closed browser B receives no keyboard or mouse input after close.
- The shared server remains alive while browser A is still open.

Partial criteria:

- Two-browser split routing and input isolation pass, but the close-B cleanup
  portion exposes a separate cleanup gap.
- Geometry and hit-testing pass, but one keyboard-routing assertion is
  inconclusive because automation cannot reliably focus the desired split pane.
- The scenario exposes a distinct lifecycle bug that should be fixed in the next
  experiment.

Fail criteria:

- Browser B cannot launch in the split pane.
- Browser B spawns a second same-profile Roamium process instead of reusing the
  existing server.
- Browser A and browser B reuse the same pane id, browser tab id, CA context id,
  or terminal surface id.
- The two browser overlays overlap incorrectly or are presented in the wrong
  split locations.
- Mouse hit-testing targets the wrong browser pane.
- Keyboard input leaks between browser panes.
- Closing browser B kills or disconnects browser A.
- Closed browser B continues receiving input.

## Design Review

Fresh-context adversarial design review by Codex subagent `Lorentz the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Reviewer checks:** The reviewer confirmed the README links Experiment 5 as
  `Designed`, the experiment has the required sections, the scope is one
  proof-first harness scenario plus issue docs with no app source changes, and
  the verification criteria cover same-profile server reuse, distinct identity,
  non-overlapping split geometry, AppKit presentation, mouse hit-testing,
  keyboard isolation, browser B close cleanup, browser A post-close
  interactivity, and no closed-browser B input.

## Result

**Result:** Pass

Added the `two-browser-split-routing` runtime scenario to
`scripts/ghostboard-geometry-matrix.sh` and verified it successfully at
timestamp `20260618-023438`.

Verification run:

```bash
bash -n scripts/ghostboard-geometry-matrix.sh
git diff --check
scripts/ghostboard-geometry-matrix.sh two-browser-split-routing
```

Runtime artifacts:

- App log:
  `logs/ghostboard-geometry-two-browser-split-routing-app-20260618-023438.log`
- Roamium trace:
  `logs/ghostboard-geometry-two-browser-split-routing-roamium-20260618-023438.log`
- Harness log:
  `logs/ghostboard-geometry-two-browser-split-routing-harness-20260618-023438.log`
- Screenshots:
  `logs/ghostboard-geometry-two-browser-split-routing-screenshot-20260618-023438.png`
  and
  `logs/ghostboard-geometry-two-browser-split-routing-close-screenshot-20260618-023438.png`

Observed pass evidence:

- Browser A launched in the initial pane with pane id
  `ABC02A85-080C-465F-8787-685E8D182B8B`, browser tab id `1`, context id
  `2038435762`, selected tab id `4391`, and Roamium pid `35371`.
- Browser B launched in the right split pane with pane id
  `C39965AF-E855-4723-A589-C73A3E8CC76E`, browser tab id `2`, context id
  `491519922`, and a distinct terminal surface id.
- Browser B reused the existing `default/${ROAMIUM}` server and did not spawn a
  second default-profile Roamium process.
- Both browser panes presented split-sized AppKit overlays and Roamium received
  split pixel resizes for both panes.
- The harness proved the two browser overlays do not overlap in window-global
  split placement. During implementation we learned that AppKit's logged
  `overlay_frame` and `root_frame` values are local to each split surface, so
  two right/left split browser panes may legitimately report identical local
  overlay frames. The durable geometry assertion is distinct surface identity
  plus inferred window-global non-overlap from the left split surface width.
- Clicking inside browser A hit browser A's pane/context and produced a browser
  A pointer move without routing to browser B.
- Clicking inside browser B hit browser B's pane/context and produced a browser
  B pointer move without routing to browser A.
- Browse-mode keyboard markers reached only the focused browser pane for both
  browser A and browser B.
- Closing browser B's split pane sent timely `Pane close cleanup` and
  `CloseTab`, Roamium destroyed and removed browser B tab id `2`, and the shared
  Roamium server stayed alive because browser A remained open.
- Browser A expanded after browser B close, remained interactive, and post-close
  mouse/keyboard assertions did not route to closed browser B.
- Closed browser B did not recreate a live overlay after close.

## Conclusion

Issue 818's multi-pane routing row is now directly proven for two simultaneous
browser panes in one split layout. Ghostboard can host two same-profile browser
overlays in sibling split panes, keep their pane/tab/context/surface identities
separate, route pointer and keyboard input to the focused/targeted pane only,
close one split browser without killing the shared profile server, and keep the
surviving browser interactive afterward.

The important harness learning is that split-surface geometry logs are local to
the owning surface. Future split assertions should avoid treating equal local
`overlay_frame` or `root_frame` values as a collision; they should combine
surface identity, split layout direction, and inferred window-global placement.

## Completion Review

Fresh-context adversarial completion review by Codex subagent `Hubble the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Reviewer checks:** The reviewer verified only docs and
  `scripts/ghostboard-geometry-matrix.sh` changed, no app source was edited, the
  README marks Experiment 5 as `Pass`, the scenario is registered with
  keybindings/config and non-vacuous assertions, the claimed logs support server
  reuse, distinct identities, split routing, hit tests, keyboard isolation,
  browser B close cleanup, browser A post-close interactivity, and no post-close
  browser B input, `bash -n scripts/ghostboard-geometry-matrix.sh` passed,
  `git diff --check` passed, and the result commit had not been made before
  review.
