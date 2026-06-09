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

# Experiment 20: Phase C — conformance smoke test (map the feature landscape)

## Description

The render + interaction foundation is done (Exp 14–19): the renamed Ghostty app
boots, renders text, takes input, runs commands, and updates live on libroastty.
Workstream 3 is now feature-by-feature conformance. Rather than guess which
feature to tackle first, this **diagnostic** experiment (no `libroastty` code
change — like Exp 14) drives the live app through a representative set of
terminal behaviors, captures each, and **maps what renders correctly vs. what
has gaps** — producing a prioritized list of the next experiments.

## Approach

**Drive+capture (Exp 19 lessons):** launch via `open` (activates Roastty);
**type the probe command FIRST** while the window is fresh-frontmost (before any
window move — a move steals first-responder focus and the keystroke is dropped);
then move the window to a clear area (`{0,31}`) + raise it for a full-screen
`screencapture` + `crop.swift`; resolve the window by `list-windows.swift`
(`name="👻"`). The IOSurface layer defeats `screencapture -l`/`-R`, hence
full-screen + crop.

**All probes are SELF-TERMINATING** (review-required) — `printf`/`seq`/`clear`
commands that draw and return to the prompt, never an interactive `vi`/`top`
(which leaves a grandchild outside the kill scope and needs a post-capture quit
keystroke that the focus-steal drops). The alt-screen probe enters the alt
screen + draws + `sleep`s (capture during the sleep); the kill then tears it
down.

**Safe teardown (review-required):** after each probe, kill the **descendant
tree** of the launched app PID, not just the build-path match:
`pkill -9 -P <appPID>` reaps the shell, and to be safe also collect+kill any
remaining descendants of `<appPID>` (`ps -axo pid,ppid` walk), then run
`stop-app.sh`. Verify **0 descendants of `<appPID>`** remain (pgrep the
descendant set, not just the build path). NEVER `osascript … quit` / broad
`pkill vi`/`killall`.

Probes (one capture each):

1. **Output + scroll:** `seq 1 60` (> one screen → does the latest content show,
   scrolled, in order?). Eyeball-verifiable.
2. **ANSI colors:**
   `printf '\033[31mRED \033[42;30mGRNBG \033[1;34mBLU\033[0m\n'` +
   `printf '\033[38;2;255;128;0mTRUECOLOR\033[0m\n'`. Eyeball: _are_ there
   distinct colors; **needs-oracle**: exact palette index / truecolor accuracy →
   compare to the real Ghostty.
3. **Clear:** `clear; echo AFTER_CLEAR` — does the screen clear to just the
   prompt + AFTER_CLEAR? Eyeball-verifiable.
4. **Alt screen + cursor addressing (self-terminating):**
   `printf '\033[?1049h\033[2J\033[5;10HALT_OK\033[10;3Habc\033[0m'; sleep 3` —
   enters the alt screen, clears, positions text at (5,10) and (10,3); capture
   during the sleep. Eyeball: alt content shown at the right cells;
   **needs-oracle**: exact cursor cell → compare to real Ghostty. The kill tears
   down the alt screen + the `sleep`.
5. **Resize:** capture, then `osascript … set size of window 1 to {W,H}`,
   capture again — do the columns/rows update and content re-lay-out?
   (`set_size` → present is wired; the question is the reflow.)
   Eyeball-verifiable (does the wrap/column count change).
6. **Wide/Unicode:** `printf '日本語 🎉 café\n'` — wide CJK + emoji + combining
   accent. Eyeball: do glyphs render + advance correctly (no overlap/gaps);
   **needs-oracle**: exact wide-cell advance → compare to real Ghostty.

For probes 2/4/6, **capture the identical command on the upstream-named Ghostty
app** (`scripts/ghostty-app/`) and compare — the conformance oracle, not
eyeball, for palette/cursor/ wide-advance correctness.

For each probe, record: **works / partial / broken** (+ for the oracle ones,
match/mismatch vs. Ghostty), the captured evidence, and a one-line cause
hypothesis for gaps.

**Deferred (not in this smoke test), with rationale:** mouse-drag **selection +
clipboard copy** and **scrollback navigation** (shift-pageup) are hard to drive
via `osascript` keystrokes and each warrants its own experiment — noted here as
known next probes, not characterized.

## Verification

1. The app is driven through all six probes; each produces an out-of-repo
   capture, and the app + children are killed after (0 dangling PIDs; verified
   with `pgrep`).
2. Each probe is characterized (works / partial / broken) from its capture,
   cross-checked where ambiguous (e.g. a blank capture vs. a focus/z-order
   artifact — type before moving, per Exp 19).
3. The Result records a **prioritized gap list** → the next experiments (e.g.
   "scrollback reflow broken → Exp 21", "256-color works, truecolor partial →
   Exp 22"), plus what already works.
4. **No `libroastty` code changes** (diagnostic only); no screenshots committed.

**Pass** = all six probes were driven + captured + characterized, with a
prioritized gap-list/next-experiments produced and the app cleaned up each time.

**Partial** = most probes characterized but some couldn't be driven/captured
reliably (e.g. a TUI that won't launch from the harness) — documented with the
tooling gap.

**Fail** = the app can't be driven through the probes at all (documented as a
harness blocker).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** Three Required + two
Optional, folded in:

- **Required — TUI grandchild not reaped.** An interactive `vi`/`top` is
  `/usr/bin/vi`, outside `stop-app.sh`'s build-path scope; `pkill -P` reaches
  only direct children, and name-kill is forbidden — so it could be left
  running. **Fixed:** all probes are self-terminating (no interactive TUI);
  teardown kills the app's **descendant tree** + verifies 0 descendants.
- **Required — `vi` quit lands after the focus-stealing capture** → `:q!`
  dropped → modal hang. **Fixed:** removed the modal TUI; the alt-screen probe
  is a self-terminating `printf` + `sleep`.
- **Required — color/cursor correctness isn't eyeball-verifiable** → false
  "works". **Fixed:** probes 2/4/6 compare against the real-Ghostty **oracle**;
  each probe marks what's eyeball-verifiable vs. needs-oracle.
- **Optional — `top -l 1` never enters the alt screen** (logging mode).
  **Fixed:** the alt-screen probe uses explicit `\033[?1049h`/`l`.
- **Optional — missing selection/clipboard + scrollback-navigation.** **Fixed:**
  explicitly deferred with rationale (hard to drive via osascript; each its own
  experiment).

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
