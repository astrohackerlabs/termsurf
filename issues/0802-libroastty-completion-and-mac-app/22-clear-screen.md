+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 22: Phase C — diagnose + fix the `clear` gap

# Description

Exp 20 found that after `clear`, post-clear content (echoes + the shell prompt)
doesn't render — only a home-positioned cursor shows. macOS `clear` emits
**`\033[3J\033[H\033[2J`** (erase scrollback + cursor home + erase screen). The
cause is unknown: it could be the terminal model (an erase-display / scrollback
handling bug), the renderer (frame-rebuild dirty/`reset_contents` after a full
erase), or the live present path (the driver's present after a clear). This
experiment **narrows the cause with targeted probes + code inspection, then
fixes it.**

## Approach

**Phase 1 — narrow the layer (diagnostic probes, `ZDOTDIR/.zshrc` drive +
capture per Exp 20).** Probe set corrected per the design review (match
`clear`'s real order `3J,H,2J`; exercise 3J with **prior scrollback**, else
`erase_history_basic` no-ops and falsely exonerates it):

- **A.** `printf '\033[H\033[2JAFTER_2J\n'` — `clear`'s tail (home +
  erase-screen, **no 3J**). Isolates the 2J-after-home path. Does `AFTER_2J`
  render?
- **B.** `printf '\033[3J\033[H\033[2JAFTER_FULL\n'` — the exact `clear`
  sequence (reproduce).
- **C.** `seq 1 100; printf '\033[3JAFTER_3J\n'` — fill scrollback **first**, then
  erase-history + text, so `erase_history_basic` actually runs. Does `AFTER_3J` render?
- **D. Control:** `printf 'BEFORE\nAFTER_NOCLEAR\n'` (no erase) — confirms the
  drive works.

A-vs-B isolates whether `\033[3J` is necessary; C exercises history-erase in
isolation.

**Phase 2 — locate (the review pinned the prime suspect).** `present_live` reads
the terminal **only** through `render_rows_snapshot()` + `shape_run_options()`
(`frame_rebuild.rs:79-85`, `RenderDirty::Full` every present, so `row_dirty` is
**not** a candidate — dropped). The symptom (cursor renders at home, rows blank)
is exactly a divergence between the active page and that **render read-path**
after a clear. So, **front-load this:**

- **Render read-path (first):** a headless unit test that feeds the
  Phase-1-isolated sequence (clear + text) to a `Terminal`, then asserts via
  **`FrameTerminalSnapshot::collect(...)`** (the actual pixel-feeding path:
  `render_rows_snapshot()` + `shape_run_options()`) that the post-clear text
  rows are present — NOT a generic active-page `dump_string`, which could be
  correct while the render accessors return blank (the green-test/blank-app
  trap). This is the failing test.
- **Terminal model (only if the render read-path looks right):** then
  `screen.rs::erase_display_basic` + the page-list/viewport/pin handling of `\033[3J`
  (`pages.erase_history_basic`) — likely the viewport pin is stale after history-erase shuffles
  the page list, so `render_rows_snapshot()` reads the wrong (now-erased) region.

**Phase 3 — fix** the identified cause (faithful to upstream
`vendor/ghostty/src/terminal`), with a **regression test** at the layer of the
bug (terminal unit test and/or renderer readback), and re-run the live probe to
confirm `clear; echo X` shows `X` + the prompt.

This is expected to touch **only `libroastty`** (terminal or renderer). No app
changes. The fix location is unknown until Phase 1/2, so the precise files are
TBD — the experiment commits to finding and fixing the root cause, not a guessed
file.

## Verification

1. **Phase 1 probes** captured + characterized (which of 2J / 3J / full breaks),
   reproducing the gap and isolating the trigger.
2. **A failing test at the bug's layer** is written first (terminal-model
   assertion or renderer readback) that reproduces the gap headlessly, then
   **passes after the fix** — so the bug is pinned by a regression test, not
   only a screenshot.
3. **`cargo test -p roastty`** (full) green including the new test.
4. **Live re-probe (the binding gate for the driver layer):**
   `clear; echo AFTER_CLEAR` (+ the prompt) now renders in the launched app. The
   headless test covers the model/render read-path but NOT the live driver's
   `dirty`/`tick_termio` interaction — so the live re-probe, not the headless
   test, is the gate that the driver layer is fixed; never skip it on a green
   headless test. (Capture out-of-repo; app + descendant tree killed, 0
   dangling.)
5. The fix is faithful to upstream (cite the `vendor/ghostty` erase/render
   behavior it matches).

**Pass** = the trigger is isolated, a regression test reproduces then (post-fix)
passes, the suite is green, and the live app renders post-`clear` content +
prompt.

**Partial** = the cause is isolated + a test written, but the fix is larger than
one experiment (e.g. a deep page-list change) — documented with the precise next
step, the diagnostic locked in by the failing test.

**Fail** = the gap can't be reproduced headlessly or isolated (documented;
unlikely given the 2-run live repro).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It confirmed from source
that `present_live` reads the terminal only via `render_rows_snapshot()` +
`shape_run_options()` (`RenderDirty::Full` every present) and pinned the prime
suspect (a divergence between the active page and that render read-path after a
clear). Two Required + two Optional, folded in:

- **Required — the test must assert through the render accessors**
  (`FrameTerminalSnapshot::collect`), not a generic grid dump, else it can pass
  green while the app stays blank. **Fixed.**
- **Required — the probe set had a wrong order + under-exercised 3J** (real
  `clear` is `3J,H,2J`; `\033[3J` no-ops without prior scrollback). **Fixed:**
  probes A (`H,2J`), B (full), C (scrollback then 3J), D (control).
- **Optional — drop the `row_dirty` suspect** (`RenderDirty::Full` makes it
  irrelevant). **Fixed**; front-loaded the render-read-path test.
- **Optional — the live re-probe is the driver-layer gate** (neither headless
  test covers it). **Fixed:** noted explicitly.

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
