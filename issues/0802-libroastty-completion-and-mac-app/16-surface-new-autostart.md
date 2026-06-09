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

# Experiment 16: Phase C — `surface_new` auto-starts the IO (the shell-start divergence)

## Description

Exp 15 found the surface's shell never starts: the renamed app calls
`roastty_surface_new` but **never `roastty_surface_start`**
(`SurfaceView_AppKit.swift:352` is the only surface call), and `ghostty.h` has
**no `surface_start`** at all — `ghostty_surface_new` starts the IO itself
(`embedded.zig` → `Surface.init`/`core_surface.init`). roastty split new/start
(the interim API), so the shell never runs → `termio_worker` is `None` → the
(already-wired) live present skips → blank terminal.

## Approach

Make `roastty_surface_new` start the surface IO, matching ghostty — but
reconcile with roastty's **test harness**, which injects `termio_worker`
manually (`new_test_surface` → `= Some(test_worker(...))`) and must NOT spawn
real shells.

1. **At the end of `roastty_surface_new`** (after the `Surface` is fully built +
   registered + `app` set), call `surface.start_termio()` — the existing method
   (lib.rs:2273) that spawns the surface's stored command/working-dir/env and
   sets `termio_worker`.
2. **Gate it on a RUNTIME signal — `platform_tag == ROASTTY_PLATFORM_MACOS`
   (1)** — NOT `#[cfg(not(test))]`. The design review showed `cfg(test)` is
   **not** hermetic: the `roastty/tests/abi_harness.rs` integration test links
   the **cdylib** (where `cfg(test)` is OFF → auto-start ON) and calls
   `roastty_surface_new` 20+ times, which would spawn real shells and flip
   worker-gated FFIs. Instead, auto-start only for **real macOS app surfaces**:
   the app sets `platform_tag == MACOS` + a real `nsview`; the abi_harness +
   unit tests use the default `platform_tag == 0` (verified) and inject
   `termio_worker` manually. This is the same condition that already gates the
   nsview capture, and it's faithful (ghostty's real surfaces all carry a
   platform → all auto-start; the `platform_tag == 0` surfaces are roastty-test
   artifacts ghostty has no equivalent of). `start_termio` guards re-entry, and
   the app calls `surface_new` once and never `surface_start`, so there is no
   double-start.
3. **Re-launch the app** (Exp-14/15 harness): the shell now runs, so
   `present_live` reaches `render_and_present_frame` with a live
   `termio_worker`. Note: `start_termio`'s present fires during `surface_new`
   when `size` is still 0 → a clamped **1×1 throwaway frame** (the compositor
   reallocates its target on the later real-size `set_size` present, so it's a
   no-op, not fatal) — the meaningful present is the subsequent `set_size` one.
   Verify via the live-present log / window capture.

This touches **only `roastty/src/lib.rs`** (one runtime-gated call). No app
source changes. It does **not** yet make text appear — Exp 17 (atlas coherence)
is still required — but it unblocks the present path so it actually renders the
terminal's background/cells path.

## Verification

1. **Full `cargo test -p roastty`** (NOT `--lib` — must include the
   `abi_harness` integration test, which links the cdylib) green, AND **no shell
   processes spawned/leaked by the harness** (the harness surfaces use
   `platform_tag == 0`, so the runtime gate excludes them). The unit tests
   inject `termio_worker` manually and use null nsview / `platform_tag == 0`, so
   the auto-start is skipped there too.
2. **App launch:** the shell starts (a real `termio_worker`), so the live
   present no longer skips on `worker is None`. Confirm `present_live` reaches
   `render_and_present_frame` (no "worker is None" path) — e.g. the window shows
   the terminal background frame, or the live present error log is clean.
   Capture out-of-repo; **kill the spawned app** (0 dangling PIDs).
3. **No regression / no double-start** in the app (the surface starts exactly
   once).

**Pass** = `surface_new` auto-starts the IO in non-test builds (test build
unchanged, suite green), and the launched app's surface has a running shell so
the wired present reaches `render_and_present_frame` (worker present). (Text
still needs Exp 17.)

**Partial** = the shell starts + tests green, but an unexpected interaction
surfaces (e.g. the present still skips for another reason) — documented.

**Fail** = auto-starting in `surface_new` can't be reconciled with the test
harness or breaks launch (documented).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It confirmed faithfulness
(ghostty's `Surface.init` spawns the IO thread unconditionally —
`Surface.zig:723` via `embedded.zig:248/580` — so auto-starting matches
upstream), no double-start (app calls `surface_new` once, never `surface_start`;
`start_termio` guards re-entry), and that `start_termio` reads only fields the
`surface_new` `Box` already populates. Two Required + one Optional, all
addressed:

- **Required — `#[cfg(not(test))]` is NOT hermetic.** `roastty` is a `cdylib`;
  the `tests/abi_harness.rs` integration test links it (cfg(test) OFF →
  auto-start ON) and `abi_harness.c` calls `surface_new` 20+ times → would fork
  20+ real shells per `cargo test` and flip worker-gated FFIs. **Fixed:** gate
  on the **runtime** `platform_tag == MACOS` (the harness uses
  `platform_tag == 0`), not `cfg`.
- **Required — verification used `--lib`,** masking the abi_harness regression.
  **Fixed:** verify with the **full** `cargo test -p roastty` + zero shell
  leaks.
- **Optional — `start_termio`'s present fires during `surface_new`** at size 0 →
  a 1×1 throwaway frame (compositor reallocates later). **Fixed:** documented.

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
