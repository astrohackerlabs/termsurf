# Experiment 6: Prove TUI Disconnect Reconnect

## Description

Experiment 1 left warm reconnect uncovered. Experiments 3 and 4 proved
same-profile server reuse, normal close/reopen, and final server cleanup through
native tab close. Experiment 5 proved two-browser split routing. What remains
for the reconnect row is the disconnect path: a `web` TUI process can disappear
without a native tab close, and Ghostboard should clean up only that TUI's pane
while preserving the warm profile server if another pane is still using it.

This experiment will add a focused runtime scenario that opens browser A and
browser B with the same profile/server, kills or otherwise terminates browser
B's `web` TUI process without closing browser A, proves Ghostboard runs
TUI-disconnect cleanup for browser B, proves browser A and the shared Roamium
server remain alive, then launches browser C with the same profile and proves C
reuses the warm server instead of spawning a new profile process.

The experiment is proof-first. No app source changes are planned. If the
disconnect/reconnect behavior is missing, record the result as `Partial` or
`Fail` with exact evidence and make any fix a later design-reviewed experiment.

## Changes

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a `tui-disconnect-reconnect` scenario.
  - Reuse the native-tab automation shape from `same-profile-server-lifecycle`.
  - Launch browser A with `web --browser "$ROAMIUM" --profile default "$URL"`.
  - Open a second native tab and launch browser B with
    `web --browser "$ROAMIUM" --profile default "$URL_B"`.
  - Capture the shared default-profile Roamium pid.
  - Assert browser B reuses the existing `default/${ROAMIUM}` server and does
    not spawn a second default-profile Roamium process.
  - Identify browser B's `web` TUI process using the explicit command/URL_B
    arguments or a wrapper-recorded pid, then terminate that TUI process without
    using Ghostboard's native tab close command.
  - Assert Ghostboard logs `TUI disconnect cleanup` for browser B's pane and
    sends `CloseTab` for browser B while Roamium is still attached.
  - Assert Roamium destroys/removes browser B's tab while preserving browser A
    and the shared server.
  - Switch back to browser A and prove keyboard routing still reaches browser A
    and not browser B.
  - Launch browser C with the same profile/browser after browser B's TUI
    disconnect, and assert C reuses the same `default/${ROAMIUM}` server/pid.
  - Assert browser C receives fresh pane/tab/context identity and routes
    keyboard input only to C.

Planned issue-document changes:

- Record the result in this experiment file.
- Update the Issue 818 README status for Experiment 6 after verification.

Planned app source changes:

- None.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0818-ghostboard-profile-tab-lifecycle-matrix/README.md issues/0818-ghostboard-profile-tab-lifecycle-matrix/06-prove-tui-disconnect-reconnect.md`.

Static checks:

1. `git diff --check`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh tui-disconnect-reconnect`.

Pass criteria:

- Browser A launches successfully and creates the `default/${ROAMIUM}` server.
- Browser B launches successfully with the same profile/browser and reuses the
  existing server/pid.
- Browser B's TUI process is terminated without using a native tab close, native
  split close, or browser pane close keybinding.
- Ghostboard logs `TUI disconnect cleanup` for browser B's pane.
- Ghostboard sends timely `CloseTab` for browser B while Roamium is attached.
- Roamium destroys and removes browser B's tab.
- Browser A remains interactive after browser B's TUI disconnect.
- The shared Roamium pid remains alive after browser B's TUI disconnect because
  browser A is still using the profile server.
- Browser C launches after browser B's disconnect and reuses the same
  `default/${ROAMIUM}` server/pid.
- Browser C gets fresh pane id, browser tab id, context id, and selected native
  tab id rather than reusing browser B's closed identity.
- Browser C receives keyboard input only while focused, and browser A still
  receives keyboard input only while focused after the reconnect.
- Closed browser B receives no input after disconnect cleanup.

Partial criteria:

- Browser B TUI-disconnect cleanup is proven, but browser C reconnect cannot be
  automated reliably.
- Browser C reconnects to the warm server, but one input-routing assertion is
  inconclusive due to focus automation.
- The scenario exposes a distinct lifecycle bug that should be fixed in the next
  experiment.

Fail criteria:

- Browser B's TUI process cannot be identified or terminated without closing the
  native tab/pane.
- Browser B's disconnect does not produce `TUI disconnect cleanup`.
- Browser B cleanup relies on a native tab close or pane close path rather than
  the TUI disconnect path.
- Browser B disconnect kills or respawns browser A's shared server.
- Browser C spawns a second default-profile Roamium process instead of reusing
  the warm server.
- Keyboard input leaks to disconnected browser B or the wrong active browser.

## Design Review

Fresh-context adversarial design review by Codex subagent `Bacon the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Optional finding:** TUI pid discovery should be deterministic. The design
  originally allowed either argv matching or wrapper-recorded pid discovery, but
  argv matching can be racy if stale `web` processes share the same URL/profile.
  Accepted: implementation should prefer a wrapper-recorded browser B `web` pid
  and assert the killed pid matches the B command.
- **Reviewer checks:** The reviewer confirmed the README links Experiment 6 as
  `Designed`, the experiment has the required sections, scope stays inside the
  harness/docs proof boundary, and the pass/fail criteria are coherent with
  `cleanupTuiPanes`: decrement pane count, send `CloseTab`, preserve the server
  while browser A remains alive, then prove browser C reuses the same pid.

## Result

**Result:** Pass

Implemented `tui-disconnect-reconnect` in
`scripts/ghostboard-geometry-matrix.sh` as a proof-only harness scenario. The
scenario opens browser A with the default profile, opens browser B in a second
native tab using the same profile/browser, records browser B's `web` TUI pid via
an explicit wrapper, kills only that TUI process, proves Ghostboard runs TUI
disconnect cleanup and sends `CloseTab` for browser B, proves Roamium destroys
and removes browser B's tab while preserving the shared profile server, then
opens browser C with the same profile and proves C reuses the warm server.

Verification passed:

1. `bash -n scripts/ghostboard-geometry-matrix.sh`
2. `git diff --check`
3. `scripts/ghostboard-geometry-matrix.sh tui-disconnect-reconnect`

The passing runtime run was timestamped `20260618-025307` with logs:

- `logs/ghostboard-geometry-tui-disconnect-reconnect-harness-20260618-025307.log`
- `logs/ghostboard-geometry-tui-disconnect-reconnect-app-20260618-025307.log`
- `logs/ghostboard-geometry-tui-disconnect-reconnect-roamium-20260618-025307.log`
- `logs/ghostboard-geometry-tui-disconnect-reconnect-screenshot-20260618-025307.png`

Key observed identities:

- Browser A selected native tab `4498`, pane
  `F3947F7C-0A37-445F-A691-EFFE2B63FF86`, browser tab `1`, context `2092037997`.
- Browser B selected native tab `4511`, pane
  `18E738EB-734E-471A-A219-9B76827307D2`, browser tab `2`, context `3816631262`,
  wrapper-recorded TUI pid `41013`.
- Shared default-profile Roamium pid `40670` stayed alive after browser B's TUI
  disconnect.
- Browser C selected native tab `4515`, pane
  `9E7353AC-E0FF-4884-B3E4-561A131F006C`, browser tab `3`, context `2833149316`.

The harness also proved keyboard routing after reconnect:

- browser A received input after browser B disconnected and browser B did not;
- browser C received focused input after warm reconnect and neither browser A
  nor disconnected browser B did;
- browser A again received focused input after switching back from browser C;
- disconnected browser B received no later keyboard input.

Two harness-only corrections were made while implementing the scenario:

- Browser B pid discovery uses the wrapper-recorded pid, as requested by the
  design review, instead of relying on process-table argv matching alone.
- Browser C selection is detected by waiting for a selected native tab id that
  is neither browser A nor browser B. `ctrl+t` already selects the new tab.

## Conclusion

Warm reconnect is now proven for the default-profile Ghostboard path. A TUI
process can disconnect without a native tab close, Ghostboard cleans up only
that pane/tab, Roamium removes the closed browser tab, the shared profile server
stays alive while another pane still uses it, and a later TUI can reconnect by
opening a fresh browser tab on the warm server.

## Completion Review

Fresh-context adversarial completion review by Codex subagent `Harvey the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Reviewer checks:** The reviewer verified the diff only touches
  `scripts/ghostboard-geometry-matrix.sh`, the Issue 818 README, and this
  experiment file; `bash -n scripts/ghostboard-geometry-matrix.sh` passed;
  `git diff --check` passed; Markdown `prettier --check` passed; and no result
  commit existed before review.
- **Runtime evidence review:** The reviewer did not rerun the runtime scenario
  because it would create logs and drive/kill processes, but reviewed the
  `20260618-025307` logs. The logs support the result: browser B used wrapper
  pid `41013`, Ghostboard logged `TUI disconnect cleanup` and `CloseTab`,
  Roamium destroyed/removed tab `2`, only one default Roamium spawn `40670`
  appeared, browser B and browser C reused the warm server, browser C got fresh
  native/pane/tab/context identities, and keyboard routing assertions passed
  without later input to browser B.
