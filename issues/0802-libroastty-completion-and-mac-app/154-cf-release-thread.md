# Experiment 154: Phase I — CF release thread

## Description

Finish the remaining Phase I polish item, `os/cf_release_thread` (perf).
Upstream Ghostty has a dedicated `CFReleaseThread` because CoreFoundation /
CoreText releases can run expensive callback logic, and synchronous release from
the renderer/shaping path can stall high-throughput work. The concrete upstream
consumer is `font/shaper/coretext.zig`: each shaping pass accumulates temporary
`CFTypeRef`s such as `CFString`, `CFAttributedString`, `CTTypesetter`, `CTLine`,
feature fonts, and cached attribute dictionaries, then hands the batch to the
release thread at `endFrame`.

Roastty's CoreText port currently uses `CFRetained` RAII directly inside
`roastty/src/font/face/coretext.rs`. That is memory-safe, but it drops temporary
CoreText objects synchronously on the shaping/render path. Because Roastty does
not have Ghostty's long-lived `Shaper` object, the Rust design should add the
same release-thread capability in an idiomatic form: a small `os` worker plus a
per-shape release pool that consumes retained CF objects only after their last
use and releases them off-thread, with synchronous fallback if enqueueing fails.

## Changes

- `roastty/src/os/cf_release_thread.rs`
  - Add a Rust port of the useful Ghostty behavior:
    - a bounded mailbox with capacity 64, matching upstream's fixed-size
      mailbox;
    - a background worker thread named `cf_release`;
    - a message payload that owns a batch of retained raw CF pointers;
    - a safe public wrapper that consumes `CFRetained<T>` by
      `CFRetained::into_raw`, stores the raw retained pointer, and guarantees it
      is eventually passed to `CFRelease`;
    - synchronous fallback release if the worker is closed, full, or cannot be
      started;
    - `Drop`/shutdown behavior for test-owned workers so queued batches are
      drained or synchronously released instead of leaked.
  - Keep the unsafe boundary narrow: one raw-pointer wrapper with a documented
    `unsafe impl Send`, and one `CFRelease` FFI function that only receives
    pointers previously produced from owned retained CF objects.
  - Add focused unit tests for:
    - queueing a batch and observing the release path run on the worker thread
      via a test hook that records the releasing thread id, or an equivalent
      non-vacuous observable;
    - closing/dropping a worker without leaking queued refs;
    - fallback release when enqueueing cannot hand work to the thread;
    - no-op behavior for an empty pool.
- `roastty/src/os/mod.rs`
  - Export the new `cf_release_thread` module.
- `roastty/src/font/face/coretext.rs`
  - Use the new release pool in `shape_run_with_features` for temporary
    CoreText/CoreFoundation objects created during shaping:
    - the run `CFString`;
    - feature descriptor, when one is created;
    - feature-applied run font, when it is a copy rather than `self.font`;
    - attributes dictionary;
    - attributed string;
    - `CTLine`;
    - glyph-runs `CFArray`.
  - Do not pool objects until after the final use that can read through them.
    The implementation must preserve the current shaping output exactly.
  - Leave long-lived face-owned objects (`self.font`, color state, cached atlas
    data) under existing ownership; this experiment is only about temporary
    shaping releases.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Mark the `os/cf_release_thread` roadmap item complete only after the result
    proves the release worker is wired into CoreText shaping.
  - Add an operating note explaining the Rust adaptation: Ghostty owns the
    release thread on `Shaper`; Roastty has no long-lived shaper, so the worker
    is shared and the release pool is local to each shape call.

## Verification

- `cargo fmt`
- `cargo test -p roastty cf_release_thread -- --test-threads=1`
- `cargo test -p roastty coretext -- --test-threads=1`
- `cargo test -p roastty font -- --test-threads=1`
- `cargo test -p roastty -- --test-threads=1`
- `cd roastty && macos/build.nu --action test`
- `cargo fmt --check`
- `git diff --check`

**Pass** = the new `os::cf_release_thread` worker is implemented with bounded
mailbox semantics and synchronous fallback, CoreText shaping hands temporary
retained CF objects to the release pool only after last use, existing shaping
tests still produce identical glyph output, the full Roastty Rust suite passes,
the hosted macOS app tests still pass, and the README marks Phase I polish
complete.

**Partial** = the worker exists and is tested, but CoreText shaping still uses
synchronous `CFRetained` drops for some hot temporary objects, or verification
is limited to targeted tests.

**Fail** = the release thread leaks retained objects, can double-release, races
with live CoreText reads, changes shaping output, or cannot be proven by tests.

## Design Review

**Reviewer:** Codex-native adversarial subagent `Archimedes` with fresh context,
using the `adversarial-review` skill's Codex path
(`multi_agent_v1.spawn_agent`), not Claude's named `adversarial-reviewer` agent.

**Verdict:** Approved.

**Findings:** No Required findings.

**Accepted suggestions:**

- Added `cd roastty && macos/build.nu --action test` to verification because the
  implementation will touch app-linked Rust code and CoreText behavior.
- Tightened the release-thread unit-test requirement so the queue test must
  prove release occurs on the worker thread, not merely that an enqueue call
  returns success.

**Final verdict:** Approved.
