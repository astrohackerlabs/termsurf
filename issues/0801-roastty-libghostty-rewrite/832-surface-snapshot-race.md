+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.result]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 832: Eliminate the surface snapshot first-render race

## Description

With the poison cascade killed (Exp 831), the suite's residual failures are now
visible and few: **2–3 per run**, dominated by the surface-snapshot
**first-render race**. This experiment eliminates that race class so
`cargo test -p roastty` runs clean.

### Root cause (confirmed across Exp 830/831)

`surface_snapshot_text` (`lib.rs:15310`) and `surface_snapshot_text_after_start`
(`lib.rs:15305`, which is `roastty_surface_start` + `surface_snapshot_text`)
take the snapshot on the **first** `roastty_surface_needs_render` returned by
`wait_until`. After `start`, the surface's initial paint can set `needs_render`
**before** the child's round-trip output arrives, so the snapshot is the empty
initial screen and the following `assert!(…contains(EXPECTED))` fails. It is
timing-dependent: most tests win the race, a few lose it per run. Exp 831's runs
showed `surface_key_default_performable_action_falls_through_when_unperformed`
(`lib.rs:16314`) and
`surface_key_default_natural_text_editing_writes_legacy_bytes` (`lib.rs:16358`)
losing it 5/5.

The fix is the proven Exp 830 template: wait for the **expected output token**
(`surface_snapshot_text_until`, `lib.rs:15325`, which polls up to 300 × 10 ms
for the needle and only then renders) instead of snapshotting on the first
render. Every call site already names its expected token in `.contains(NEEDLE)`.

## Changes

`roastty/src/lib.rs` (test code only). Two helpers do the waiting; the call
sites become token-waits.

1. **Add**
   `surface_snapshot_text_after_start_until(app, surface, needle) -> String` =
   `roastty_surface_start(surface)` +
   `surface_snapshot_text_until(app, surface, needle)` (mirrors the existing
   `_after_start` wrapper but waits for the token; returns the snapshot for
   multi-assert callers).

2. **97 simple sites** — mechanical conversion (single literal needle):

   ```
   assert!(surface_snapshot_text_after_start(app, S).contains("LIT"));
       →   surface_snapshot_text_after_start_until(app, S, "LIT");
   ```

   The `_until` helper panics with the latest snapshot if `"LIT"` never appears,
   so it is exactly equivalent to the old assert, minus the race.

3. **~10 `let text = surface_snapshot_text_after_start(app, surface);` sites**
   (e.g. `surface_start_uses_copied_config…` `lib.rs:29158`, which asserts
   `contains(current_dir)` **and** `contains("owned")`). Convert to
   `let text = surface_snapshot_text_after_start_until(app, surface, KEY);`
   where `KEY` is the **last-arriving** expected token; the existing
   multi-asserts then run against the settled snapshot. Each `KEY` is chosen per
   test from its asserts (recorded in the result). KEY-selection is safe here
   because all 10 of these sites have **single-`printf`, single-burst** children
   (every asserted token renders together) — verified in design review; the
   result must call out any later site where the asserted tokens span multiple
   output bursts (a too-early KEY would return before a later token).

4. **9 `surface_snapshot_text(app, surface)` round-trip sites** (the
   non-`after_start` ones:
   `lib.rs:16313/16357/16425/16510/16646/21166/21187/21223/21489`) — convert to
   `surface_snapshot_text_until(app, surface, NEEDLE)`. One special case:
   - `lib.rs:16646` asserts `contains("^X") || contains("18")` — under the
     test's `stty -echo` the `^X` echo never renders and only the `0x18` byte
     reaches `dd|od`, so the awaited token is **`"18"`**; the `||` assert is
     kept (the `^X` arm is dead, so `_until("18")` cannot be defeated by it).

   **Left unchanged (not a race):** `lib.rs:21419` asserts
   **`!contains("ready")`** after a `reset` binding action. Design review
   established this is **deterministic, not a first-render race**: the `reset`
   action runs `worker.with_termio_mut(|t| t.terminal_mut().reset())`
   synchronously on the caller (`termio.rs:297`), so the grid is cleared and
   `needs_render` set _before_ the action returns; the snapshot is stable and
   the negative assert is sound. `_until` cannot wait for an _absent_ token, and
   no positive marker exists post-reset, so this site keeps
   `surface_snapshot_text`.

No production code changes; all edits are in `#[cfg(test)]` test bodies and
helpers. The few remaining `surface_snapshot_text` callers that assert on
first-render content (not child round-trip) are left unchanged and identified in
the result.

## Verification

Using the **no-progress watchdog** for every bare run (kill + `sample` if the
test log is silent > 90 s, hard ceiling 600 s, and record each run's wall-clock)
so a hang self-reports in ≤ 90 s instead of waiting indefinitely:

- **Targeted (fast) first:** the two 5/5 originators
  (`surface_key_default_performable_action_falls_through_when_unperformed`,
  `surface_key_default_natural_text_editing_writes_legacy_bytes`) pass in
  isolation after conversion.
- **Full suite:** `cargo test -p roastty` (bare, in-process, under the watchdog)
  run **5×** — **zero failures** in every run (no snapshot-race originators; and
  still zero `PoisonError`, confirming 831 holds). The only acceptable residual
  is the non-PTY `config_path_cli` flake (Exp 833), which must be **explicitly
  identified** if it appears, not silently counted.
- `cargo build -p roastty --tests` — no warnings.
  `cargo fmt -p roastty -- --check` — clean. No-ghostty grep on the added lines
  — clean. `git diff --check` — clean.

**Pass** = `cargo test -p roastty` shows **zero snapshot-race failures across 5
watchdog-bounded runs** (only the separately-tracked `config_path_cli` flake may
remain, → Exp 833). **Partial/Fail** = any snapshot-race test still fails, or a
new failure class appears.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). Verified the helper semantics, the 97/10/12 call-site counts, and
the sed-safety of the needles against the actual `lib.rs` sites.

**Verdict:** CHANGES REQUIRED → fixed → approvable. Confirmed sound: the
`surface_snapshot_text_until` mechanics, the `String` return supporting
multi-assert callers, the 97-site mechanical equivalence, and the "last-arriving
token as KEY" rule (all 10 `let text =` sites are single-burst children).

- **Required — 21419 is not a race.** Its `!contains("ready")` assert follows a
  `reset` binding action that runs `with_termio_mut` **synchronously**
  (`termio.rs:297`), so the cleared grid + `needs_render` are set before the
  action returns — the snapshot is deterministic. `_until` cannot wait for an
  _absent_ token and no positive marker exists post-reset. **Fixed:**
  reclassified 21419 as **left unchanged** (keeps `surface_snapshot_text`); the
  round-trip conversion list is now 9 sites.
- **Optional — 16646 awaited token.** **Adopted:** under `stty -echo` the `^X`
  echo never renders, so the awaited token is `"18"`; recorded.
- **Optional — KEY single-burst assumption.** **Adopted:** noted that KEY-safety
  relies on all asserted tokens sharing one output burst (true for all 10
  sites); the result must flag any multi-burst exception.

## Conclusion

_(to be written after the run)_
