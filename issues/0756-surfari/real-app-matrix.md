# Surfari Real-App Matrix

This matrix tracks Issue 756's remaining real-app Surfari coverage. Status
values are deliberately conservative:

- `Proven` means current issue evidence directly proves the row.
- `Partial` means the row has some evidence, but the exact requirement is not
  fully proven.
- `Missing` means there is no direct real-app Surfari evidence yet.
- `Blocked` means the row cannot currently be tested without a known external
  fix or permission change.

## Matrix

| Area               | Status  | Current Evidence                                                                                                                                                                                                      | Required Proof To Mark Proven                                                                                                        | Proposed Harness / Scenario                                     |
| ------------------ | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| Navigation         | Proven  | Experiment 25 run `20260621-190346` loaded fixture A, sent explicit `Navigate` to fixture B, and observed URL/title updates in Surfari and WebTUI traces.                                                             | Already proven for single-pane explicit navigation after initial load; pane/tab/window navigation remains part of later matrix rows. | `scripts/test-issue-756-surfari-lifecycle-tranche.sh`.          |
| Keyboard input     | Proven  | Experiments 21 and 23 proved Surfari `key-event` and fixture `kind=input value=a` in the real app.                                                                                                                    | Already proven for single-pane real app; future matrix rows must ensure it survives tabs/windows/focus changes.                      | `scripts/test-issue-756-surfari-input-regression.sh`.           |
| Click              | Partial | Experiments 21-23 proved Surfari receives `mouse-event`; DOM click remains warning-only and absent.                                                                                                                   | Prove DOM `click` or intentionally document that click is unsupported/handled differently, with a follow-up issue if needed.         | Input-detail tranche using the existing fixture click zone.     |
| Drag               | Missing | No real-app Surfari drag evidence yet.                                                                                                                                                                                | Prove drag reaches Surfari and produces page-visible behavior, such as text selection, drag-scroll, or pointer-move state.           | Input-detail tranche with a drag/selection fixture.             |
| Scroll / wheel     | Proven  | Experiment 22 and guard run `20260621-183959` proved Surfari `scroll-event` and fixture `kind=wheel`.                                                                                                                 | Already proven for page-visible wheel delivery; coordinate fidelity still needs a later assertion if required.                       | `scripts/test-issue-756-surfari-input-regression.sh`.           |
| Resize             | Proven  | Experiments 20 and 25 proved real-app window resize produced Surfari resize to the new pixel size.                                                                                                                    | Already proven for a single-window app resize; pane-specific resize remains separate.                                                | `scripts/test-issue-756-surfari-lifecycle-tranche.sh`.          |
| Pane resize        | Missing | No Surfari-specific pane divider resize evidence yet.                                                                                                                                                                 | Prove overlay frame and Surfari pixel resize update after pane divider movement.                                                     | Geometry tranche adapted from Roamium `split-right-resize`.     |
| Split panes        | Missing | No Surfari-specific split-pane overlay evidence yet.                                                                                                                                                                  | Prove opening right/down splits moves/resizes the Surfari overlay, keeps hit testing correct, and does not leak into sibling panes.  | Geometry tranche adapted from Roamium split scenarios.          |
| Tab switching      | Missing | No Surfari-specific tab visibility/switching evidence yet.                                                                                                                                                            | Prove browser overlay is visible only on the tab owning it and restores when switching back.                                         | Geometry tranche adapted from tab scenarios.                    |
| Window switching   | Missing | No Surfari-specific multi-window evidence yet.                                                                                                                                                                        | Prove Surfari overlay attaches to the correct window and does not appear in unrelated windows.                                       | Geometry tranche adapted from window scenarios.                 |
| Focus changes      | Partial | Experiments 21-23 proved Browse-mode focus and Ghostboard remaining frontmost for the active pane.                                                                                                                    | Prove focus changes across panes/tabs/windows send active/inactive state only to the correct Surfari tab.                            | Focus tranche adapted from `gui-active-multi-tab`.              |
| Shutdown           | Proven  | Experiments 20, 22, 23, and 25 proved direct `CloseTab`, tab removal, no-tabs-remaining shutdown, and clean guard shutdown.                                                                                           | Already proven for single-tab close/no-tabs-remaining shutdown; crash handling is separate.                                          | `scripts/test-issue-756-surfari-lifecycle-tranche.sh`.          |
| Restart            | Proven  | Experiment 25 run `20260621-190346` closed the first Surfari tab/process path, relaunched TermSurf, saw a fresh Surfari trace init, `BrowserReady`, overlay presentation, fixture A creation, and WebTUI title state. | Already proven for same-fixture app relaunch after clean shutdown; crash restart and profile isolation remain separate.              | `scripts/test-issue-756-surfari-lifecycle-tranche.sh`.          |
| Profile isolation  | Missing | No Surfari profile isolation evidence yet.                                                                                                                                                                            | Prove separate profiles maintain separate localStorage/cookies/state and route to the correct Surfari process.                       | Profile tranche adapted from Roamium `multi-profile-isolation`. |
| Crash handling     | Missing | No Surfari renderer/process crash evidence yet.                                                                                                                                                                       | Prove renderer/process crash notification, UI state, cleanup, and restart behavior.                                                  | Crash tranche adapted from Roamium `renderer-crash-smoke`.      |
| Roamium comparison | Missing | No full Ghostboard/Roamium feature-matrix comparison has been rerun against Ghostboard/Surfari yet.                                                                                                                   | Re-run each comparable Roamium matrix row against Surfari, mark parity/difference/unsupported, and document engine-specific gaps.    | Final comparison tranche after Surfari real-app rows stabilize. |

## Roamium Scenario Map

The existing `scripts/ghostboard-geometry-matrix.sh` is Roamium-oriented and too
broad to reuse wholesale for Surfari. The relevant scenario names to mine are:

- Lifecycle/navigation/resize: `browser-command-navigation`, `window-resize`,
  `browser-navigation-geometry`.
- Pane and split geometry: `split-right`, `split-down`, `split-right-resize`,
  `split-right-equalize`, `split-right-zoom`, `split-right-close-sibling`,
  `split-right-close-browser-pane`.
- Tabs/windows/focus: `new-terminal-tab-visibility`, `open-browser-in-new-tab`,
  `close-browser-tab`, `open-browser-in-new-window`,
  `multiple-windows-with-browsers`, `keyboard-after-tab-window-switch`,
  `gui-active-multi-tab`.
- Input details: `browser-input-granularity`, `mouse-after-geometry-change`.
- Profiles/lifecycle/crash: `multi-profile-isolation`,
  `same-profile-server-lifecycle`, `tui-disconnect-reconnect`,
  `renderer-crash-smoke`.

Surfari experiments should reuse the assertions and fixtures from these
scenarios where practical, but they should not require Roamium-specific paths or
trace names. Surfari logs currently use `surfari-trace` files and
WebKit-specific callbacks.

## Recommended Tranches

1. **Lifecycle/navigation/resize/shutdown/restart.** Extend the existing Surfari
   smoke harness to prove explicit navigation after initial load and restart
   after close. This should also preserve the existing resize and shutdown
   proof.
2. **Pane/split/tab/window/focus geometry.** Add Surfari-specific variants of
   the core geometry scenarios: split right/down, pane resize, tab visibility,
   window attachment, and active/inactive focus routing.
3. **Input details.** Keep the existing keyboard/wheel guard as baseline, then
   add click, drag, and coordinate-fidelity checks. DOM click is currently
   partial and should not be treated as passing without new evidence.
4. **Profile isolation and crash handling.** Prove profile storage separation
   and Surfari crash/restart behavior after the normal lifecycle and geometry
   rows are stable.
5. **Ghostboard/Roamium comparison.** Re-run the comparable Roamium matrix
   against Surfari, record feature parity, and document engine-specific
   differences.

## Next Experiment Recommendation

Experiment 25 executed the first tranche:
lifecycle/navigation/resize/shutdown/restart. The next experiment should move to
the pane/split/tab/window/focus geometry tranche without mixing in profile
isolation, crash handling, or the final Roamium comparison.
