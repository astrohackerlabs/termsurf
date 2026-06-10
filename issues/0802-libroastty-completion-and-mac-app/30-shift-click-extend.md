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

# Experiment 30: Phase C — shift-click extends the selection

## Description

A common, faithful selection behavior: **shift + left-click extends the existing
selection** to the clicked point (keeping the original anchor), rather than
starting a new one. roastty's `mouse_button` left-press always calls
`selection_press` (a new gesture), so shift-click doesn't extend.

Upstream `Surface.zig:3763-3797` implements it: on a left **press**, if
`mods.shift and selection_gesture.left_click_count > 0 and !shift_capture`
**and** there is an active selection **and** the click is **not** within
`mouse_interval` of the last click (else it's a multi-click count increment, not
an extend), it calls `cursorPosCallback(pos)` — i.e. it treats the shift-click
as a **drag to the click point**, extending from the existing anchor — and
returns (skipping a fresh press).

## Approach

In `Surface::mouse_button`, in the **not-mouse-reporting** left-**press** branch
(the only place selection runs), branch to extend instead of starting a new
gesture when **`should_shift_extend()`**:

- `self.mouse.mods.shift`, and
- `self.selection_gesture.click_count() > 0`, and
- there **is** an active selection (`terminal.active_selection().is_some()`),
  and
- the time since the gesture's last click (`click_time_ns`) is **> 500ms**
  (`mouse_interval` — a rapid click is a double/triple-click increment, so let
  `selection_press` handle it).

When true, call **`selection_drag()`** (the existing path) — which extends from
the gesture's tracked anchor to the current mouse position (the click) — instead
of `selection_press()`. On the missing- time case (`click_time_ns()` is `None`)
**do not extend** (mirror upstream's `orelse break`), and use `saturating_sub`
so an injected/backward clock can't underflow:
`let Some(t) = gesture.click_time_ns() else { return false }; self.selection_time_ns().saturating_sub(t) > 500_000_000`.

**`!shift_capture` (deferred-faithful):** roastty does not model
`mouse-shift-capture` / `XTSHIFTESCAPE` (the shift-while-reporting path is an
explicit deferral). For the **default** config (`.false`) `!shift_capture` is
always true, so dropping it is exactly faithful; the only divergence is a
non-reporting program that issued `CSI > 1 s` (unmodeled) — out of scope.

Adds a `click_time_ns()` accessor to `SelectionGesture` (the field exists,
private). **Only `libroastty`** (`lib.rs` + the one accessor). No app change
(the app already forwards mods).

## Verification

1. **Headless regression test** (deterministic via the Exp-27 injectable clock):
   set the clock, make a selection (press A → drag B → release = "A..B");
   advance the clock **> 500ms**; **shift**- click at C (a left press with the
   shift mod, no drag) → the selection extends to span **anchor A → C** (its
   text == the A..C content), not a fresh single-cell selection at C. A control:
   the same shift-click **within** 500ms of the last click does **not** extend
   (it falls to the `selection_press` path — at a different cell,
   `press_repeat`'s one-cell distance check resets to a fresh single-cell
   gesture). Asserts via `selection_format(Plain, …, None)`. Fails pre-fix
   (shift-click starts a new selection), passes after. `cargo test -p roastty`
   (full) green.
2. **No regression:** a plain (no-shift) click still starts a new selection
   (Exp-25 path unchanged); the extend only triggers under the full shift
   condition.
3. **Live confirmation** (screen unlocked — check `CGSSessionScreenIsLocked`):
   select a word, then shift-click further along the line → the selection
   extends to the shift-click. (If the screen is locked, record
   Partial-pending-live like Exp 29; the headless test proves the logic.)
4. Faithful to upstream `Surface.zig:3763-3797` (cite).

**Pass** = `should_shift_extend` branches to `selection_drag`, the headless test
(shift-click extends to the anchor; within-interval does not) passes, the suite
is green, and the live app extends a selection on shift-click.

**Partial** = the headless test passes + suite green, but the live shift-click
can't be captured (locked screen) — documented, pending the unlock re-probe.

**Fail** = shift-click can't be made to extend from the core handler
(documented).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED.** It verified against source + upstream:
**extend lands anchor→click** (`selection_release` → `gesture.release` does NOT
untrack the anchor or zero `click_count`, so the shift-click's `selection_drag`
resolves `click_pin = validated_anchor_pin` = A and selects A→C); **interval
faithful** (upstream extends when `since > mouse_interval`; `> 500ms` matches,
and the 500ms agrees with the multi-click `repeat_interval_ns`);
**`click_time_ns` is the last press time** (untouched by drag/release); **shift
reaches `self.mouse.mods`** (`key_mods_from_raw`, `ROASTTY_MODS_SHIFT = 1<<0` →
test raw bit `1`); **borrow clean** (`should_shift_extend(&self)` returns a Copy
bool before the `&mut self` drag); **test deterministic** via the Exp-27 clock;
no-shift unaffected. Three Optional/Nit folded in: note the unmodeled
`mouse-shift-capture`/`XTSHIFTESCAPE` (faithful for the default `.false`);
`None` click-time → don't extend + `saturating_sub`; the within-interval control
phrasing (falls to `selection_press`, distance-resets).

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
