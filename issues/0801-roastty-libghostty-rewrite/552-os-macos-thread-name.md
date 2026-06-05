+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 552: set the running thread's name (os::macos)

## Description

Continuing `os::macos` (Experiment 550 added the QoS helpers), this experiment
ports the remaining libc-only piece of upstream `os/macos.zig`:
`pthread_setname_np`, which **names the running thread**. roastty will use this
(alongside the QoS class) to label its render / IO / mux threads so they're
identifiable in a debugger or in Activity Monitor. With this, the libc-only
surface of `macos.zig` is complete (only the objc / bundle-id helpers remain
deferred).

## Upstream behavior

`os/macos.zig`:

```zig
pub extern "c" fn pthread_setname_np(name: [*:0]const u8) void;
```

- On macOS `pthread_setname_np(const char *)` takes **only** the name and sets
  the **calling** thread's name (unlike Linux's `(thread, name)` form). The name
  is limited to `MAXTHREADNAMESIZE` (64 bytes including the NUL); an over-long
  name fails with `ENAMETOOLONG`.
- Upstream merely declares the `extern` (returning `void`) for callers to use
  when setting up named threads.

## Rust mapping (`roastty/src/os/macos.rs`)

`libc` already exposes `pthread_setname_np(name: *const c_char) -> c_int`, so no
`extern` block is needed. A safe `set_thread_name(&CStr)` wraps it, surfacing
the result (upstream's `extern` discards it, but the call genuinely returns an
errno — e.g. `ENAMETOOLONG` — that is worth reporting):

```rust
use std::ffi::CStr;

/// Set the name of the **running** thread (upstream `os.macos.pthread_setname_np`). On macOS
/// `pthread_setname_np` names the calling thread; the name is limited to `MAXTHREADNAMESIZE`
/// (64 bytes including the NUL), and an over-long name fails with `ENAMETOOLONG`.
pub(crate) fn set_thread_name(name: &CStr) -> std::io::Result<()> {
    // Returns 0 on success, -1 with `errno` set on failure (runtime-verified on this macOS
    // SDK: a 100-byte name yields rc = -1, errno = ENAMETOOLONG). Unlike
    // `pthread_set_qos_class_self_np`, this is the `-1`/`errno` convention, so read `errno`.
    let rc = unsafe { libc::pthread_setname_np(name.as_ptr()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
```

`name: &CStr` is the faithful Rust form of upstream's `[*:0]const u8` (a
NUL-terminated C string, no extra allocation). On this macOS SDK
`pthread_setname_np` uses the `-1`/`errno` convention (verified at runtime —
`rc = -1`, `errno = ENAMETOOLONG` for an over-long name), **not** the
direct-errno return of `pthread_set_qos_class_self_np`, so the failure errno is
read via `io::Error::last_os_error()`. Surfacing the result (vs upstream's
`void`) lets callers see an `ENAMETOOLONG` rather than silently dropping it.

## Scope / faithfulness notes

- **Ported (bridged)**: `os.macos.pthread_setname_np` →
  `os::macos::set_thread_name` (a safe wrapper over `libc::pthread_setname_np`).
- **Faithful**: sets the calling thread's name on macOS via
  `pthread_setname_np(name)`.
- **Faithful adaptation**: the bare `extern`
  `pthread_setname_np([*:0]const u8) void` → `libc::pthread_setname_np` wrapped
  in `set_thread_name(&CStr) -> io::Result<()>`; `[*:0]const u8` → `&CStr`; the
  discarded `void` return → an `io::Result` that reads `errno` on the `-1`
  failure (this macOS SDK uses the `-1`/`errno` convention here, verified at
  runtime) and surfaces e.g. `ENAMETOOLONG` — strictly more information than
  upstream, never less.
- **Deferred**: `isAtLeastVersion` (objc `NSProcessInfo`); `appSupportDir` /
  `cacheDir` and the `commonDir` / `NSSearchPath*` helpers (objc
  `NSFileManager` + `build_config.bundle_id` — blocked on roastty's
  product-naming decision); `NSOperatingSystemVersion`. With this slice the
  libc-only surface of `macos.zig` is complete.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/os/macos.rs`: add `set_thread_name`.
2. Tests (in `macos.rs`):
   - **round-trip**: `set_thread_name(c"roastty-552")` returns `Ok(())`, and
     `pthread_getname_np(pthread_self(), …)` reads back exactly `roastty-552`.
     (This renames the running cargo-test thread — benign.)
   - **too long ⇒ `ENAMETOOLONG`**: a name of 100 `a`s (over the 64-byte limit)
     returns `Err(e)` with `e.raw_os_error() == Some(libc::ENAMETOOLONG)`, and
     the thread name is unchanged from the round-trip value.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty os::macos
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/os/macos.rs roastty/src/os/mod.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `set_thread_name` sets the calling thread's name via `pthread_setname_np` and
  returns `Ok` on success or the errno (e.g. `ENAMETOOLONG`) as an `io::Error` —
  faithful to `os/macos.zig`'s `pthread_setname_np`;
- the tests pass (round-trip + too-long), and the existing tests still pass;
- the objc version check and bundle-id directory helpers stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the thread-naming behavior diverges from upstream,
an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex's first design review raised **one Required** finding, now fixed; the
corrected design was **re-reviewed and approved with no findings**.

- **errno convention (Required, fixed)**: the design used
  `from_raw_os_error(rc)`, but Codex's runtime probe showed `pthread_setname_np`
  on this macOS SDK uses the `-1`/`errno` convention (`rc = -1`,
  `errno = ENAMETOOLONG` for an over-long name) — **not** the direct-errno
  return of `pthread_set_qos_class_self_np` (Experiment 550). Fixed by reading
  the errno via `std::io::Error::last_os_error()` on failure.

On re-review Codex confirmed the fix is correct (`last_os_error()` yields
`ENAMETOOLONG` for the long-name test), and the `&CStr` API, the
one-argument/current-thread macOS behavior, the round-trip and too-long tests,
and the objc-helper deferrals are all sound.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d552-prompt.md` (design),
  `logs/codex-review/20260604-d552b-prompt.md` (design re-review)
- Result: `logs/codex-review/20260604-d552-last-message.md` (design),
  `logs/codex-review/20260604-d552b-last-message.md` (design re-review)
