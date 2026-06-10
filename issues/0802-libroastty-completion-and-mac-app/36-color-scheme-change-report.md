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

# Experiment 36: Phase C — report color-scheme changes live (DECSET 2031)

## Description

Mode **2031** (`DECSET ?2031`, "report color scheme") lets a program ask the
terminal to **notify it when the OS light/dark appearance changes**, so
shells/editors can re-theme live. roastty implements the **query** half — a DSR
`CSI ? 996 n` triggers `terminal.color_scheme()` → `CSI ? 997 ; 1 n` (dark) /
`; 2 n` (light) (`terminal.rs:3487`, tested at `lib.rs:34115`). But the
**change** half is missing: `roastty_surface_set_color_scheme` (the FFI the app
calls on an OS appearance change) only stores `surface.color_scheme` — it never
emits the proactive `997` report. So a program that enabled 2031 is **never
told** when the system theme flips.

Upstream `Surface.colorSchemeCallback` (`Surface.zig:4678`): on an appearance
change, if the scheme **actually changed**, it updates state and queues
`color_scheme_report { force = false }`; `Termio.colorSchemeReportLocked(force)`
(`Termio.zig:711`) then emits `997` **only when**
`force || modes.get(.report_color_scheme)`. The query path is `force = true`
(always); the change path is `force = false` (gated on 2031).

## Approach

1. **`terminal.rs`** — extract the emit and add the change path:
   - `fn write_color_scheme_report(&mut self, scheme: i32)` — the shared
     `match scheme { DARK=>997;1n, LIGHT=>997;2n, _=>{} }` (0=light, 1=dark;
     matches `ROASTTY_COLOR_SCHEME_*`).
   - `color_scheme()` (the DSR query) calls `write_color_scheme_report(scheme)`
     after the callback — unchanged behavior (force=true: always emits).
   - **`pub(crate) fn report_color_scheme_change(&mut self, scheme: i32)`** —
     the force=false path:
     `if !self.modes.get(modes::Mode::ReportColorScheme) { return; } self.write_color_scheme_report(scheme);`.
2. **`lib.rs`** — `roastty_surface_set_color_scheme`: detect the change, store,
   and on a change notify the worker's terminal (mirrors `colorSchemeCallback`):
   ```rust
   let changed = surface.color_scheme != color_scheme;
   surface.color_scheme = color_scheme;
   if changed {
       if let Some(worker) = surface.termio_worker.as_ref() {
           worker.with_termio_mut(|termio| termio.terminal_mut().report_color_scheme_change(color_scheme));
       }
   }
   ```

`c_int == i32`, so `color_scheme` passes straight through. **Only `libroastty`**
(`lib.rs` + `terminal.rs`). No app change. The change-detection (only report
when the scheme flips) mirrors upstream's `if theme == new_scheme return`.

## Verification

1. **Headless test** (`lib.rs`, surface-level): a surface + worker; enable mode
   2031 (`next_slice(b"\x1b[?2031h")`); `clear_pty_response`;
   `roastty_surface_set_color_scheme(surface, DARK)` (a change from the default
   light) → assert the worker terminal's `pty_response()` contains
   `\x1b[?997;1n`. Controls: (a) calling it **again** with `DARK` (no change) →
   **no** new report; (b) with mode 2031 **off** → **no** report even on a
   change; (c) a change to `LIGHT` with 2031 on → `\x1b[?997;2n`. Fails pre-fix
   (no report ever emitted on a change), passes after.
2. **No regression:** the DSR-query path (`color_scheme()` via `CSI ? 996 n`)
   still emits `997` unconditionally — the existing query test (`lib.rs:34115`)
   still passes (the refactor only extracts the shared emit).
3. **No live confirmation needed** — this is a protocol emission to the pty,
   fully observable via `pty_response()`; the model assertion is the proof.
   **Completes fully while the screen is locked.**
4. Faithful to upstream `Surface.colorSchemeCallback` /
   `Termio.colorSchemeReportLocked` (force=false, mode-2031-gated, change-only).

**Pass** = an OS theme change emits `997` when mode 2031 is on (and only on a
real change), the query path is unchanged, the headless test passes, and the
suite is green. Fully headless — no Partial-pending-live.

**Partial** = the emit + query work but the change-detection/mode-gating needs
more (unlikely).

**Fail** = the change report can't be wired from `set_color_scheme`
(documented).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED (no Required findings).** Verified against
code + vendored upstream: the **gap is real** (`set_color_scheme` today only
stores the value) and the **fix is faithful** — upstream
`Surface.colorSchemeCallback` is change-only (`if theme==new return`) then
`color_scheme_report{force= false}`, and `colorSchemeReportLocked` gates on
`!force && !modes.get(.report_color_scheme)`; the plan mirrors this
(change-detection in `set_color_scheme`, mode-gate in
`report_color_scheme_change`, query path force=true unchanged). **Scheme mapping
correct, not inverted** (`LIGHT=0`/`DARK=1` match; dark→ `997;1n`,
light→`997;2n`; `c_int==i32`). **Change-detection right** (surface default
`color_scheme=0` LIGHT at lib.rs:14148, so the first flip to DARK is detected;
repeated same-value → `changed=false`). **Test sound + not vacuous**
(`pty_response()` buffers the emit; `set_surface_worker_dec_mode` feeds `?2031h`
through the SAME `with_termio_mut → terminal_mut().next_slice` path
`report_color_scheme_change` reads — same Terminal/modes, confirmed; controls
load-bearing). **No regression** (the query test drives `?996n` without 2031 and
still expects `997` — proving force=true; the refactor only extracts the shared
`match`; no borrow conflict). Two non-blocking notes: passing `scheme` as a
param vs upstream re-reading stored state is functionally equivalent (equals the
just-stored value); an out-of-range scheme (99) hits `_ => {}` — a graceful
no-op superset.

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
