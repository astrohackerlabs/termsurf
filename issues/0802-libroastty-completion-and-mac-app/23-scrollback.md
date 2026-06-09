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

# Experiment 23: Phase C — scrollback navigation (deferred Exp-20 probe)

## Description

Exp 20 deferred **scrollback navigation** (scrolling up to view history). The
design review corrected the premise: the path **does not exist**.
`roastty_surface_mouse_scroll` → `Surface::mouse_scroll` (`lib.rs:3789`) calls
**only** `dispatch_scroll_reports`, which no-ops unless the terminal is in
**mouse-reporting** mode. The `scroll_viewport_*` functions are reachable
**only** from the explicit keybinding/AppleScript FFIs — never from the wheel.
So in a plain shell, scrolling the wheel does nothing. Upstream `scrollCallback`
(`vendor/ghostty/src/Surface.zig:3505-3573`) has **three** branches that
roastty's `mouse_scroll` is missing two of:

1. **alt-screen + `mouse_alternate_scroll` mode + no mouse-reporting** →
   translate the wheel to **cursor keys** (`\x1bOA`/`\x1bOB` app-mode or
   `\x1b[A`/`\x1b[B`) written to the PTY;
2. **mouse-reporting** → button-4/5/6/7 reports (roastty has this —
   `dispatch_scroll_reports`);
3. **otherwise (plain shell)** → `scrollViewport(.delta = -y.delta)` —
   **scrollback navigation**.

This experiment **ports branches (1) and (3)** into `mouse_scroll`. The
relative-scroll primitive already exists (`Scroll::DeltaRow` /
`screen.scroll_delta_row`, `page_list.rs:4933`); it's just not exposed to the
wheel path.

## Approach

**Phase 1 — the fix (port branches 1 & 3 into `mouse_scroll`).** Faithful to
upstream `scrollCallback`: after the existing mouse-reporting handling, when
**not** mouse-reporting —

- if alt-screen + `mouse_alternate_scroll` mode → write cursor-key sequences to
  the PTY (branch 1);
- else → scroll the viewport by the wheel delta (branch 3). Expose a `Terminal`
  viewport-delta scroll (wrapping `screen.scroll_delta_row`) and call it;
  compute the line delta from the wheel `y` (line-mode; precision/fractional
  fidelity can be a follow-up — get the line-step behavior right first, matching
  upstream sign: viewport delta = `-y`).

**Phase 2 — headless regression test (per the Exp-22 lesson).** Drive
`Surface::mouse_scroll(...)` on a **non-mouse-reporting** surface with
scrollback content (`seq`-like fill) and assert, **via
`shape_run_options()`/`FrameTerminalSnapshot::collect`** (the render read-path,
not a generic dump), that scroll-up shows earlier rows and scroll-to-bottom
shows the tail. A separate test asserts branch 1 (alt-screen + alt-scroll → the
cursor-key bytes are queued to the PTY). This fails pre-port and passes after.

**Phase 3 — live confirmation (the scroll driver).** Build
`scripts/roastty-app/scroll.swift` (`CGEventCreateScrollWheelEvent` +
`CGEventPost(.cghidEventTap)` at the window center — the review confirmed
`.cghidEventTap` routes to the window **under the cursor**, avoiding the
frontmost-keystroke pitfall; raise + warp cursor over the window, **restore the
cursor after**). **Validate the driver independently** of the (newly-ported)
viewport path — against a mouse-reporting program where scroll has a
current-code effect — then probe: `ZDOTDIR/.zshrc` `seq 1 200`, capture the
tail, scroll up, capture (earlier lines render), scroll down (tail returns).
Scrollback is retained: the live surface uses `Terminal::init(.., None)` =
`usize::MAX` (`termio.rs:114`, unlimited), not disabled.

This touches **only `libroastty`** (`mouse_scroll` + a `Terminal` scroll-delta
accessor) + a test-only `scroll.swift`. No app changes.

## Verification

1. **Headless regression test** through `mouse_scroll` + the render read-path:
   fails pre-port, passes after (scroll-up shows earlier rows; scroll-bottom
   shows the tail; alt-scroll → cursor keys). **`cargo test -p roastty`** (full)
   green.
2. **Live confirmation:** the scroll driver is validated independently; then the
   captures (out-of-repo) show **history on scroll-up + the tail on
   scroll-down**. App + descendant tree killed (0 dangling); screen **unlocked**
   (check `CGSSessionScreenIsLocked` first — `screencapture` is black when
   locked); cursor restored.
3. Faithful to upstream `scrollCallback` (cite the branches ported).

**Pass** = branches (1) & (3) are ported, the headless regression test passes,
the suite is green, and the live app shows scrollback navigation (history on
scroll-up, tail on scroll-down).

**Partial** = viewport scrolling (branch 3) works + tested, but a sub-aspect is
deferred (e.g. precision/fractional scroll fidelity, or branch 1 alt-scroll if
it proves larger) — documented.

**Fail** = the port can't be made to scroll the viewport (documented with the
blocker).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It **corrected the
central premise**: `mouse_scroll` calls only `dispatch_scroll_reports`
(mouse-reporting only); the `scroll_viewport_*` functions are unreachable from
the wheel — upstream `scrollCallback` has three branches and roastty is
**missing two** (the alt-scroll cursor-keys branch and the plain-shell
viewport-scroll branch). So this is a **feature port**, not a
verify-existing-path probe. Two Required + two Optional + a Nit, folded in:

- **Required — wrong premise.** Reframed to "port the missing `scrollCallback`
  branches".
- **Required — the driver-validation gate was unsound** (a CGEvent scroll has no
  viewport effect in a plain shell _because the code is missing_, so "scroll
  moves viewport" can't validate the driver). **Fixed:** validate the driver
  independently against a mouse-reporting program; lead with a **headless**
  regression test through `mouse_scroll` + the render read-path.
- **Optional — regression-test layer** (`scroll_viewport_to_row` already works →
  a direct test is vacuous). **Fixed:** assert through `Surface::mouse_scroll` +
  `render_rows_snapshot`.
- **Optional — scrollback capacity** unstated. **Fixed:** noted
  `Terminal::init(.., None)` = `usize::MAX` (unlimited, `termio.rs:114`).
- **Nit — restore the cursor** after warping. **Fixed.**

It also confirmed `.cghidEventTap` scroll routes to the window-under-cursor (so
the driver itself is sound, unlike the frontmost-keystroke path).

## Result

**Result:** Pass — wheel scrollback navigation now works in the live app.
Implementing it uncovered and fixed **three** bugs (the design + review had
pinned the first; the other two were found during the headless test).

### Three bugs fixed (only `libroastty`)

1. **`mouse_scroll` never touched the viewport** — it only called
   `dispatch_scroll_reports` (mouse-reports). **Ported** upstream
   `scrollCallback`'s missing branches: (1) alt-screen +
   `mouse_alternate_scroll` → cursor keys to the PTY; (3) otherwise →
   `scroll_viewport_delta_row`.
2. **The mouse-reporting gate used the coarse `self.mouse_reporting` flag**
   (which `surface_new` defaults to **`true`**), so the wheel always took the
   reports branch and returned. Fixed to check
   `mouse_report_context().is_some()` — the _actual_ reporting state (flag
   **and** the terminal's mouse-event mode), matching upstream
   `isMouseReporting()`.
3. **The render read-path read the active bottom, not the viewport** —
   `render_rows_snapshot` / `shape_run_options` iterated `Point::active(...)`,
   so a viewport scroll never changed what was rendered. Fixed to
   `Point::viewport(...)` (when not scrolled, viewport == active, so the normal
   case is unchanged — confirmed by the 4405-green suite). This is the deepest
   fix and was invisible until the headless test scrolled and saw no change.

Plus four `pub(crate)` `Terminal` accessors (`scroll_viewport_delta_row`,
`is_alternate_screen`, `mouse_alternate_scroll_enabled`, `cursor_keys_enabled`)
and a test-only `scripts/roastty-app/scroll.swift` (CGEvent scroll driver).

### Verification

- **Headless regression test** `mouse_scroll_navigates_scrollback` (asserts
  through `shape_run_options` — the render read-path): drives
  `Surface::mouse_scroll` on a non-mouse-reporting surface filled past the
  screen; scroll-up reveals TOPMARKER (history), scroll-down returns to
  BOTMARKER (tail). Fails pre-fix, passes after.
- **Full `cargo test -p roastty`:** lib **4405 passed**, 0 failures — the
  `Point::viewport` change is safe (the normal unscrolled case is identical).
- **Live confirmation** (screen unlocked; app + descendant tree killed, 0
  dangling): `seq 1 200`, then the CGEvent scroll driver scrolls the wheel up —
  the window scrolls into **scrollback history** (lines 118–141 shown, up from
  the ~177–200 tail). Output + crop out-of-repo.

(Branch 1 — alt-screen alt-scroll → cursor keys — is ported faithfully from
upstream; the headless test + live probe cover the user-visible viewport branch
(3). A dedicated alt-scroll test is a small follow-up.)

## Conclusion

Wheel scrollback navigation works, after fixing a chain of three bugs — the most
consequential being that the **render read-path was not viewport-aware** (it
always rendered the active bottom), which would have blocked _any_
scroll-to-history regardless of input wiring. One of the two Exp-20-deferred
probes is now closed. **Next: mouse selection + clipboard** (the other deferred
probe), then the noted refinements (CJK wide-pitch, CVDisplayLink vsync,
DPI-change).

## Result Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It independently reran
the gates (full lib **4405 passed, 0 failed**;
`mouse_scroll_navigates_scrollback` passes and **discriminates all three fixes**
— default `mouse_reporting=true` makes fix 2 load-bearing, `Point::active`
render makes fix 3 load-bearing, reports-only `mouse_scroll` makes fix 1
load-bearing), confirmed **fix 3 is upstream-faithful** (upstream
`render.zig:269` renders from `getTopLeft(.viewport)`), the `scrollCallback`
branches/sequences/signs match, and the live PNGs substantiate
(`e23-scrolled_up` = history 118–141; `e23-tail` = tail 178–200 + prompt). Scope
clean (libroastty + test-only `scroll.swift`), no "ghostty" literals,
`fmt --check` clean. Findings:

- **Required — README index still said `Designed`.** Fixed (→ Pass).
- **Optional — branch 1 omitted upstream's selection-clear** before emitting
  cursor keys. **Fixed:** `set_selection(None)` added to the alt-scroll branch.
- **Optional — a stray cursor block renders in scrollback** (the render path
  emits `cursor_x` without viewport-gating; upstream gates on
  `cursor.viewport`). Cosmetic, scrolled-state-only (unscrolled is unchanged).
  **Noted as a follow-up** in the Conclusion (a small render fix, separate from
  the scroll wiring).
