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

# Experiment 836: Fix surface_key performable_action — a child that actually outputs

## Description

`surface_key_default_performable_action_falls_through_when_unperformed`
(`lib.rs:16291`) fails at **default** parallelism (passes at threads=4). It is
the sibling of the Exp 835 natural_text bug — it also asserts the **racy
pre-`stty -echo` echo** (`"^[[1;2D"`, the Shift+ArrowLeft legacy sequence) — but
its captured failing snapshot is **empty** (`"\n\n…"`), not a partial render.

The cause is the child command:

```
stty -echo -icanon min 8 time 0; dd bs=1 count=8 2>/dev/null | od -An -tx1 -v
```

Shift+ArrowLeft writes the 6-byte sequence `ESC [ 1 ; 2 D`
(`0x1b 5b 31 3b 32 44`), but `stty min 8` / `dd count=8` **wait for 8 bytes**.
Only 6 arrive, so `read()`/`dd` block forever (and `od`'s block-buffering would
withhold output even if they did not), and **nothing deterministic ever
renders**. The test only ever passed because the terminal **echoed** the bytes
(as `"^[[1;2D"`) in the window before `stty -echo` took effect — a race lost
under load (→ empty screen → the wait for `"^[[1;2D"` times out). The contrast
in the same failing log (`logs/exp834/v-default.log`: natural_text's
`dd count=1` flushes `05`; performable_action's snapshot is empty) is the
load-bearing proof. (A throwaway instrumentation to `min 6`/`count 6` — log not
retained — also failed to render the od hex in isolation, consistent with the
multi-byte `dd|od` pipeline not flushing within the window; corroboration, not
the basis.)

## Changes

`roastty/src/lib.rs` (test code only), one line — replace the byte-counting
`dd|od` child with `cat -v`, which echoes each received byte's visible form as
it arrives (no fixed count to block on):

```
stty -echo -icanon min 8 time 0; dd bs=1 count=8 2>/dev/null | od -An -tx1 -v
    →
stty -echo -icanon min 1 time 0; cat -v
```

`cat -v` renders the received bytes `ESC [ 1 ; 2 D` as **`^[[1;2D`** — the exact
string the test already asserts — but now as the **child's own deterministic
output** (read from the pty slave and written to stdout), not the terminal's
racy echo. With `stty -echo` the echo is suppressed, so the asserted `"^[[1;2D"`
can only come from `cat`, which outputs it whenever the bytes round-trip —
independent of echo timing, and with `min 1` it never blocks on a byte count.
The assert and key events are unchanged.

(Why `cat -v` works where `dd|od` did not: `cat` has no `count`/`min 8` barrier
— it emits each byte as it is read, so it cannot deadlock waiting for bytes that
never come, and there is no `od` block-buffering to flush.)

## Verification

Investigation already ran the full suite at **default** parallelism with this
change: **4360 passed; 0 failed**, 0 panics, 0 `PoisonError`
(`logs/exp836/catv-default.log`, `STATUS=COMPLETED rc=0`, 230 s) —
`performable_action ... ok` under load, where the echo is gone. The formal
verification repeats it per the bounded-run convention:

- **Targeted, fast:** `performable_action…` passes in isolation after the
  change.
- **Reproduce-the-fix at the failing setting:** the full suite at **default**
  parallelism run **3×** (each its own `bounded-run.sh`) —
  `performable_action_falls_through_when_unperformed` is `... ok` every run.
  (The config `$HOME`/cwd flakes — `config_path_cli`, `bell_audio_path` — may
  still appear; they are Exp 837. Pass is judged on performable_action.)
- `cargo build -p roastty --tests` — no warnings.
  `cargo fmt -p roastty -- --check` — clean. No-ghostty grep on the changed line
  — clean. `git diff --check` — clean.

**Pass** = `performable_action` passes 3/3 at default parallelism (it was
failing there). **Partial/Fail** = it still fails, or a new failure appears.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). Verified the empty-snapshot evidence, the 6-byte sequence, the green
investigation run, and the isolation slice.

**Verdict:** APPROVED, no Required findings. Confirmed: the empty failing
snapshot vs natural_text's `05` in the same log is direct proof of the
`count=8`/`min 8` barrier (Shift+ArrowLeft = exactly 6 bytes); with `stty -echo`
the `"^[[1;2D"` in the green default run can only be cat's own round-trip output
(echo would have failed as in Exp 834), and `min 1` + cat's no-withhold
streaming rule out a permanent partial render; the change touches only the
observation child (key events, the Super+D action assert, and the fall-through
assert unchanged); and a never-exiting `cat` is no worse than the suite's
existing `; sleep 5` children (the green run completed in 203 s with no hang).
Two findings, adopted: the unsaved `min6/count6` instrumentation is softened to
corroboration (not the basis), and `od`'s block-buffering is noted alongside the
count barrier.

## Result

**Result:** Pass

The one-line child swap landed (`dd count=8 | od` → `cat -v`, with `min 1`).
Build clean (no warnings), fmt clean, no-ghostty clean, `git diff --check`
clean; isolation pass.

| run (default) | STATUS                | `performable_action` | other failures         |
| ------------- | --------------------- | -------------------- | ---------------------- |
| #1            | COMPLETED rc=101 341s | **ok**               | bell_audio_path        |
| #2            | COMPLETED rc=0 266s   | **ok**               | — (fully green 4360/0) |
| #3            | COMPLETED rc=0 220s   | **ok**               | — (fully green 4360/0) |

- **`performable_action` is `... ok` 3/3 at default parallelism** — the setting
  where it failed in Exp 834/835. `cat -v` renders `"^[[1;2D"` as the child's
  own output, so the assert no longer depends on the racy echo. All runs
  `START`/`END`/`CMD` stamped, none hit `HARD_TIMEOUT`/`IDLE_KILL`.
- **Two of the three runs were fully green (4360 passed / 0 failed).** The
  single failure (run 1, `bell_audio_path`) is the config `$HOME`/cwd flake
  class — the last remaining default-parallelism flake, deferred to Exp 837.

## Conclusion

Every `surface_*` render/echo flake is now fixed (831 cascade, 832 race, 834
surface_start echo-KEY, 835 natural_text byte, 836 performable_action child).
The suite is green at default parallelism **except** for the intermittent config
`$HOME`/cwd class.

- **Exp 837 (next, the last flake):** `config::tests::config_path_cli_…` and
  `config::tests::bell_audio_path_…` — non-PTY tests that manipulate `$HOME`/cwd
  and flake when run concurrently with each other or other env-touching tests.
  Diagnose first (capture the failing assert), then fix (likely a shared
  env/cwd-mutation race needing serialization or per-test isolation).

Once 837 lands, `cargo test -p roastty` is green at **default** parallelism
across the 3× gate, the gate drops `--test-threads=4` (back to default), and
feature work (URI/regex, remaining `os/`) finally resumes.

## Completion Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). Confirmed the diff is exactly the one-line child swap
(`dd count=8 | od` → `cat -v`, `min 1`) + comment, test-only, fmt clean;
`performable_action ... ok` 3/3 at default (no `--test-threads`, no timeout),
runs 2&3 fully green (4360/0), run 1's only failure `bell_audio_path` (distinct
config flake, can't be caused by this test-child change); never-exiting `cat`
shows no leak/hang across 220–341 s. **Verdict: CHANGES REQUIRED → fixed.**

- **Required — stale README index status.** The 836 index line still read
  `Designed`. **Fixed:** flipped to `Pass`.
