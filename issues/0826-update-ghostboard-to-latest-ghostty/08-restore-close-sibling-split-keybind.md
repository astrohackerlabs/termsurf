# Experiment 8: Restore Close-Sibling Split Keybind

## Description

Experiment 7 stopped at `split-right-close-sibling`. The matrix and a standalone
rerun both failed before the sibling close step: the harness injected Control-D,
expected `new_split:right`, and timed out waiting for the split-right overlay
frame. The app log then showed the browser overlay being cleared and a
`CloseTab` send failing with `error.NotOpenForWriting`.

The failure evidence points first to a harness configuration regression, not a
Ghostboard product regression. The reusable matrix harness logs
`split_keybind=ctrl+d=new_split:right` for `split-right-close-sibling`, but the
scenario-specific config block currently adds only:

```text
confirm-close-surface = false
keybind = ctrl+k=close_surface
```

Unlike other split scenarios, `split-right-close-sibling` is not included in the
generic config block that adds:

```text
keybind = ctrl+d=new_split:right
```

That means Control-D can fall through to the focused browser/TUI instead of
being consumed as a Ghostboard split keybinding. This experiment restores the
missing harness keybinding, reruns the failing row, and then resumes the
remaining inherited matrix rows if the row passes.

## Changes

- `scripts/ghostboard-geometry-matrix.sh`
  - Add `split-right-close-sibling` to the scenario group that writes
    `keybind = ctrl+d=new_split:right`, or otherwise add that keybind directly
    to the `split-right-close-sibling` config block.
  - Keep the existing `confirm-close-surface = false` and
    `keybind = ctrl+k=close_surface` behavior intact.
- `issues/0826-update-ghostboard-to-latest-ghostty/README.md`
  - Link this experiment with status `Designed`, then update the status after
    the result is known.
- `issues/0826-update-ghostboard-to-latest-ghostty/08-restore-close-sibling-split-keybind.md`
  - Record design, verification, result, reviews, and conclusion.

Do not modify Ghostboard product code, `webtui/`, `roamium/`, `chromium/`, or
`proto/termsurf.proto` in this experiment unless the repaired harness proves
that the same product failure still occurs with the correct keybinding present.

## Verification

Before changes, record the current mismatch:

```bash
sed -n '3046,3060p' scripts/ghostboard-geometry-matrix.sh \
  > logs/issue-0826-exp08-before-split-config.log
sed -n '8754,8766p' scripts/ghostboard-geometry-matrix.sh \
  > logs/issue-0826-exp08-before-close-sibling-code.log
```

After the harness change, verify that `split-right-close-sibling` config now
contains both required keybindings:

```bash
awk '
  /if \[.*split-right-close-sibling/ { in_block = 1 }
  in_block { print }
  in_block && /keybind = ctrl[+]d=new_split:right/ { has_split = 1 }
  in_block && /keybind = ctrl[+]k=close_surface/ { has_close = 1 }
  in_block && /^fi$/ { in_block = 0 }
  END { exit !(has_split && has_close) }
' scripts/ghostboard-geometry-matrix.sh \
  > logs/issue-0826-exp08-keybind-evidence.log
```

Run syntax and formatting checks:

```bash
bash -n scripts/ghostboard-geometry-matrix.sh
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/08-restore-close-sibling-split-keybind.md
git diff --check
```

Build the app used by the harness:

```bash
(cd ghostboard && macos/build.nu --configuration Debug --action build \
  > ../logs/issue-0826-exp08-macos-build.log 2>&1)
```

Rerun the previously failing scenario with overrides unset:

```bash
env -u TERMSURF_GHOSTBOARD_APP \
  -u TERMSURF_WEB \
  -u TERMSURF_ROAMIUM \
  -u TERMSURF_INSTALLED_ROAMIUM \
  scripts/ghostboard-geometry-matrix.sh split-right-close-sibling \
  > logs/issue-0826-exp08-split-right-close-sibling.log 2>&1
```

Extract and record the latest scenario artifacts:

```bash
ls -t logs/ghostboard-geometry-split-right-close-sibling-* | head -20 \
  > logs/issue-0826-exp08-close-sibling-artifacts.log
APP_LOG="$(ls -t logs/ghostboard-geometry-split-right-close-sibling-app-*.log | head -1)"
HARNESS_LOG="$(ls -t logs/ghostboard-geometry-split-right-close-sibling-harness-*.log | head -1)"
ROAMIUM_TRACE="$(ls -t logs/ghostboard-geometry-split-right-close-sibling-roamium-*.log | head -1)"

rg -n 'PASS: scenario split-right-close-sibling|split_keybind=ctrl\\+d=new_split:right|close_keybind=ctrl\\+k=close_surface|split_overlay_frame_size=|close_overlay_frame_size=' \
  "$HARNESS_LOG" \
  > logs/issue-0826-exp08-close-sibling-harness-evidence.log
rg -n 'dispatching action target=surface action=.new_split|TermSurf geometry layer=appkit event=presented .*scenario=split-right-close-sibling|CloseTab send failed|panic' \
  "$APP_LOG" \
  > logs/issue-0826-exp08-close-sibling-app-evidence.log || true
rg -n 'resize tab_id=.*ffi=ts_set_view_size' "$ROAMIUM_TRACE" \
  > logs/issue-0826-exp08-close-sibling-roamium-evidence.log
```

If `split-right-close-sibling` passes, resume the inherited matrix from the next
unverified row:

```bash
SUMMARY="logs/issue-0826-exp08-remaining-matrix-summary-$(date +%Y%m%d-%H%M%S).log"
SCENARIOS=(
  split-right-close-browser-pane
  split-right-focus-switch
  new-terminal-tab-visibility
  open-browser-in-new-tab
  close-browser-tab
  open-browser-in-new-window
  multiple-windows-with-browsers
  display-move-backing-scale
  fullscreen-unfullscreen
  minimize-hide-restore
  font-size-cell-metrics
  tui-overlay-resize-command
  terminal-scrollback-movement
  browser-navigation-geometry
  devtools-split-geometry
  mouse-after-geometry-change
  keyboard-after-tab-window-switch
)

set -o pipefail
for scenario in "${SCENARIOS[@]}"; do
  printf 'RUN %s\n' "$scenario" | tee -a "$SUMMARY"
  if env -u TERMSURF_GHOSTBOARD_APP \
    -u TERMSURF_WEB \
    -u TERMSURF_ROAMIUM \
    -u TERMSURF_INSTALLED_ROAMIUM \
    scripts/ghostboard-geometry-matrix.sh "$scenario" 2>&1 |
    tee -a "$SUMMARY"; then
    printf 'RESULT %s PASS\n' "$scenario" | tee -a "$SUMMARY"
  else
    rc=$?
    printf 'RESULT %s FAIL exit=%s\n' "$scenario" "$rc" | tee -a "$SUMMARY"
    exit "$rc"
  fi
done
printf 'REMAINING MATRIX PASS\n' | tee -a "$SUMMARY"
```

Reject masked failures:

```bash
rg -n '^RUN |^RESULT |^FAIL:|REMAINING MATRIX' "$SUMMARY" \
  > logs/issue-0826-exp08-remaining-summary-status.log
! rg -n '^FAIL:|RESULT .*FAIL' "$SUMMARY"
```

Run final cleanup and forbidden-path checks:

```bash
ps -axo pid,comm,args \
  | rg 'TermSurf\\.app/Contents/MacOS/termsurf|target/debug/web|chromium/src/out/Default/roamium' \
  | rg -v 'rg|ps -axo|zsh -lc' \
  > logs/issue-0826-exp08-post-cleanup-processes.log || true
git diff --name-only > logs/issue-0826-exp08-git-diff-name-only.log
git status --short -- webtui roamium proto/termsurf.proto chromium/README.md chromium/patches \
  > logs/issue-0826-exp08-forbidden-top-status.log
git -C chromium/src status --short > logs/issue-0826-exp08-chromium-status.log
git -C chromium/src diff --name-only > logs/issue-0826-exp08-chromium-diff-name-only.log
```

Pass criteria:

- The harness config for `split-right-close-sibling` includes
  `keybind = ctrl+d=new_split:right` and `keybind = ctrl+k=close_surface`.
- `bash -n scripts/ghostboard-geometry-matrix.sh` passes.
- The debug macOS app build passes.
- `split-right-close-sibling` passes with overrides unset.
- The app log shows `new_split` dispatch for the split key and does not contain
  `CloseTab send failed` or `panic` during the split phase.
- Roamium receives resize calls for the split and post-close AppKit pixel sizes.
- The remaining inherited matrix rows either pass, or the first remaining
  failure is recorded with logs for the next experiment.
- Cleanup leaves no stale matching app, web, or Roamium processes.
- `git diff --check` is clean.
- No forbidden paths are modified: `webtui/`, `roamium/`, `chromium/`, or
  `proto/termsurf.proto`.
- The nested `chromium/src` checkout has no uncommitted status or diff from this
  experiment.

Partial criteria:

- The harness keybinding omission is fixed and `split-right-close-sibling`
  passes, but a later remaining matrix row fails with clear evidence.
- The corrected keybinding is present but `split-right-close-sibling` still
  fails, proving that a product-code bug remains to localize in the next
  experiment.

Fail criteria:

- The experiment changes product code before proving the harness keybinding
  omission is not sufficient.
- The scenario is marked passing while `CloseTab send failed` or `panic` remains
  in the app log for the split phase.
- A matrix failure is hidden by shell pipeline behavior.

## Design Review

An adversarial Codex subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

Required finding and fix:

- The planned keybinding evidence used a whole-file `rg` that would already pass
  on the broken harness because it could match `split-right-close-sibling`,
  `ctrl+d`, and `ctrl+k` in unrelated blocks. Fixed by replacing it with an
  `awk` check that only accumulates keybindings from config blocks whose
  condition includes `split-right-close-sibling`, then exits nonzero unless both
  `ctrl+d=new_split:right` and `ctrl+k=close_surface` are present in those
  blocks.

The first re-review found that the `awk` snippet was not executable because the
regex was over-escaped and the variable name `split` conflicted with awk's
built-in function. Fixed by using `/if \[.*split-right-close-sibling/`,
`ctrl[+]d`, `ctrl[+]k`, and `has_split` / `has_close`.

The final re-review approved the design with no findings.

## Result

**Result:** Partial

The harness keybinding omission was fixed, and the previously failing
`split-right-close-sibling` row now passes with the normal default app, `web`,
Roamium, and installed-Roamium overrides unset.

Implementation change:

- `scripts/ghostboard-geometry-matrix.sh`
  - Added `split-right-close-sibling` to the scenario group that writes
    `keybind = ctrl+d=new_split:right`.
  - Left the existing `confirm-close-surface = false` and
    `keybind = ctrl+k=close_surface` block intact.

Verification evidence:

- `logs/issue-0826-exp08-keybind-evidence.log` shows that config blocks whose
  condition includes `split-right-close-sibling` now include both
  `keybind = ctrl+d=new_split:right` and `keybind = ctrl+k=close_surface`.
- `bash -n scripts/ghostboard-geometry-matrix.sh` passed.
- `prettier --write --prose-wrap always --print-width 80` passed for the issue
  README and this experiment file.
- `git diff --check` passed.
- The debug macOS app build passed with output captured in
  `logs/issue-0826-exp08-macos-build.log`.
- `logs/ghostboard-geometry-split-right-close-sibling-harness-20260619-122413.log`
  shows:
  - `split_keybind=ctrl+d=new_split:right`
  - `split_overlay_frame_size=496x544`
  - `close_keybind=ctrl+k=close_surface`
  - `close_overlay_frame_size=1024x544`
  - `PASS: scenario split-right-close-sibling`
- `logs/ghostboard-geometry-split-right-close-sibling-app-20260619-122413.log`
  shows `dispatching action target=surface action=.new_split value=.right`, plus
  AppKit presented-frame logs for the initial, split, and post-close overlay
  geometry.
- `logs/issue-0826-exp08-close-sibling-roamium-evidence.log` shows Roamium
  resize calls for the initial full-pane size, the split-pane size, and the
  post-close full-pane size.

However, the same passing app log also shows a cleanup-time product failure
after the scenario has already passed:

```text
warning(termsurf): CloseTab send failed pane_id=... err=error.NotOpenForWriting
thread ... panic: reached unreachable code
```

Spot checks found the same cleanup-time `CloseTab send failed` followed by a
panic in earlier rows that had been reported as passing, including
`initial-open`, `window-resize`, and `split-right`. This means the harness
keybinding omission was real and repaired, but the inherited matrix is still
masking a broader Ghostboard shutdown/cleanup bug. The remaining matrix rows
were not resumed in this experiment because continuing would produce additional
scenario results on top of a known teardown panic.

Final scope checks:

- No product code changed in this experiment.
- No forbidden paths were modified: `webtui/`, `roamium/`, `chromium/`, or
  `proto/termsurf.proto`.
- The nested `chromium/src` checkout had no experiment changes.

## Conclusion

Experiment 8 restored the missing split keybinding for
`split-right-close-sibling` and proved that the close-sibling row can now drive
split creation, close the sibling pane, and return the browser overlay to the
full terminal pane. The result is Partial because app teardown still attempts to
send `CloseTab` on a browser socket that is already not open for writing, then
panics during shutdown. The next experiment should localize and fix that cleanup
path before the viewport matrix is resumed.

## Result Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

**Verdict:** Approved.

Findings: none.

The reviewer independently checked that the implementation commit only added
`split-right-close-sibling` to the `ctrl+d=new_split:right` harness keybinding
group, that the README status and experiment result both say Partial, that the
referenced harness/app/Roamium logs support the pass and cleanup-panic claims,
and that the result documentation had not yet been committed before review.
