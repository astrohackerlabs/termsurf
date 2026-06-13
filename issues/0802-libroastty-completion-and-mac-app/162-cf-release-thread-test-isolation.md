# Experiment 162: Phase I — CF release-thread test isolation

## Description

Experiment 154 implemented the CoreFoundation release worker and passed the full
`roastty` Rust suite when run with `--test-threads=1`. Experiment 161's default
parallel `cargo test -p roastty` rerun exposed that the focused
`os::cf_release_thread` tests are not isolated from the rest of the crate:

```text
os::cf_release_thread::tests::pool_flush_releases_on_worker_thread
os::cf_release_thread::tests::worker_drop_drains_queued_refs
```

failed because the test hook observed more releases than the two objects created
by each test. A focused module rerun then also failed `empty_pool_is_noop`,
showing persistent/global release-hook contamination rather than a production
worker regression.

The current hook records every `CFRelease` performed by `release_raw`, including
objects released by unrelated CoreText/font tests and the process-shared release
worker. That makes the oracle order-dependent and incompatible with Cargo's
default parallel test runner. This experiment fixes the test oracle without
weakening the production release-worker semantics.

## Changes

- `roastty/src/os/cf_release_thread.rs`
  - Replace the global “record every release thread id” test hook with a
    pointer-scoped test observer.
  - Let tests register the exact retained CF pointers they own before handing
    them to `CfReleasePool`.
  - Record a release only when `release_raw` receives one of the registered
    pointers, along with the thread id that performed the release.
  - Keep unregistered releases ignored so parallel CoreText/font tests and the
    process-shared global worker cannot contaminate focused expectations.
  - Preserve the existing production behavior: `CfReleasePool`, bounded mailbox,
    synchronous fallback, worker drop drain, and global release worker semantics
    must not change outside `#[cfg(test)]` observation.
  - Keep the tests non-vacuous:
    - worker flush must prove the registered pointers are released off the
      caller thread;
    - worker drop must prove queued registered pointers are drained;
    - fallback must prove the registered pointer is released synchronously on
      the caller thread;
    - empty-pool no-op must prove no registered pointer was released, without
      depending on unrelated global release traffic.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After the result, update the durable notes for Exp 154/161 so future agents
    know the CF-release worker is production-valid and the former failures were
    test-oracle contamination.
  - After the result, update the stale Phase G native-key roadmap text to
    reflect Exp 161's visible `é` pass; the remaining native-key gap should be
    permission-dependent live global shortcut installation, not the dead-key
    terminal-output oracle.

Out of scope:

- Rewriting the release worker architecture.
- Removing the shared global worker.
- Serializing the whole test suite as the fix.
- Changing CoreText shaping ownership or release timing.
- Touching dead-key/product input code beyond README status correction.

## Verification

- Format Rust:
  - `cargo fmt -p roastty`
- Focused CF tests under default test threading:
  - `cargo test -p roastty os::cf_release_thread::tests -- --nocapture`
- Focused CF tests under serial threading to preserve the old gate:
  - `cargo test -p roastty os::cf_release_thread::tests -- --test-threads=1 --nocapture`
- Full Rust suite under default threading:
  - `cargo test -p roastty`
  - Must pass. The default parallel suite is the regression this experiment is
    fixing.
- Hosted macOS app tests:
  - `cd roastty && macos/build.nu --action test`
- Markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/162-cf-release-thread-test-isolation.md issues/0802-libroastty-completion-and-mac-app/README.md`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/162-cf-release-thread-test-isolation.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Hygiene:
  - `git diff --check`

**Pass** = the CF release-thread tests remain non-vacuous, pass under both
default and serial Cargo test runners, full default `cargo test -p roastty`
passes, hosted macOS app tests pass, and the Issue 802 roadmap/notes no longer
misstate the current dead-key or CF-release-thread status.

**Partial** = the CF tests are narrowed and pass in focused runs, but the full
default Rust suite still fails elsewhere or the issue spine remains stale.

**Fail** = the fix hides releases by weakening assertions, changes production
release-worker behavior without evidence, requires serializing the whole suite,
or leaves the full Rust suite failing on the same CF release-thread tests.

## Design Review

**Reviewer:** Codex-native adversarial subagent `Bernoulli` with fresh context,
using the `adversarial-review` skill's Codex path
(`multi_agent_v1.spawn_agent`), not Claude's named `adversarial-reviewer` agent.

**Verdict:** Approved.

The reviewer found no Required findings. It approved the pointer-scoped observer
design as the right shape for the actual default-parallel-suite failure,
confirmed that the non-vacuous worker/fallback/drop/empty-pool requirements are
strong enough, and noted that the full default `cargo test -p roastty` gate
directly targets the Experiment 161 regression.

Implementation note from the review: make the pointer-observer scope explicit
with a per-test handle/guard or equivalent cleanup so registered pointer state
cannot leak across parallel tests or later pointer reuse.

## Result

**Result:** Pass

The CF release-thread failures were test-oracle contamination, not a production
release-worker regression. The old `#[cfg(test)]` hook recorded every
`CFRelease` that passed through `release_raw`, so unrelated CoreText/font tests
and the process-shared global release worker could add releases while the
focused `os::cf_release_thread` tests expected exactly two entries or none.

This experiment replaced that global count oracle with a pointer-scoped
observer:

- each CF-release-thread test registers the exact retained `CFString` pointers
  it owns before handing them to `CfReleasePool`;
- `release_raw` records a release only when the pointer is registered;
- each observer guard removes its pointer and any recorded releases on drop, so
  pointer state does not leak across parallel tests or later pointer reuse;
- unregistered production releases are ignored by the test oracle but still
  proceed through the same `CFRelease` call.

The production behavior remains unchanged: `CfReleasePool`, the bounded mailbox,
the process-shared worker, synchronous fallback, and worker drop-drain semantics
still use the same release path.

Verification:

- `cargo fmt -p roastty` — pass.
- `cargo test -p roastty os::cf_release_thread::tests -- --nocapture` — pass: 4
  passed, 0 failed, 0 ignored; ABI harness filtered run passed.
- `cargo test -p roastty os::cf_release_thread::tests -- --test-threads=1 --nocapture`
  — pass: 4 passed, 0 failed, 0 ignored; ABI harness filtered run passed.
- `cargo test -p roastty` — pass: 4,850 unit tests passed, 0 failed, 4 ignored;
  ABI harness passed; doc tests passed.
- `cd roastty && macos/build.nu --action test` — pass: `TEST SUCCEEDED`, 213
  hosted app tests in 23 suites passed; UI tests remain skipped by default.
- `cargo fmt --check -p roastty` — pass.
- `git diff --check` — pass.

## Conclusion

The full default Rust suite is green again without serializing Cargo tests or
weakening the release-thread assertions. The release-thread tests now prove
worker-thread release, worker drop drain, synchronous fallback, and empty-pool
no-op behavior only for the retained CF objects they create, so unrelated
parallel CoreText/font release traffic cannot affect the result.

The stale Phase G issue text was also updated: Experiment 161 closed the
app-visible dead-key terminal-output oracle for deterministic UTF-8 output. The
remaining native-key roadmap gap is permission-dependent live global shortcut
installation.

## Completion Review

**Reviewer:** Codex-native adversarial subagent `Kant` with fresh context, using
the `adversarial-review` skill's Codex path (`multi_agent_v1.spawn_agent`), not
Claude's named `adversarial-reviewer` agent.

**Verdict:** Approved.

The reviewer found no Required findings. It confirmed that the observer is
test-only and pointer-scoped, `release_raw` still calls `CFRelease` for every
pointer, guard cleanup prevents parallel-test contamination and pointer reuse,
the release-thread tests remain non-vacuous, the README no longer contains the
stale dead-key-output contradiction, and the result/conclusion support `Pass`.

The reviewer independently reran:

- `git diff --check`
- `cargo fmt --check -p roastty`
- `cargo test -p roastty os::cf_release_thread::tests -- --nocapture`
