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

# Experiment 546: the close-on-exec pipe (os::pipe)

## Description

Continuing the `os` module (Experiments 541–545), this experiment ports upstream
`os/pipe.zig` — a **`pipe()` that sets `FD_CLOEXEC` on both ends**. A
close-on-exec pipe is a core PTY/IO building block (e.g. self-pipe wakeups,
child-process plumbing): the read and write ends must not leak into `exec`'d
children. macOS has no `pipe2`, so the close-on-exec flag is set with
`fcntl(F_SETFD, FD_CLOEXEC)` after `pipe()` — exactly what Zig's
`std.posix.pipe2` does on macOS.

## Upstream behavior

`os/pipe.zig`:

```zig
/// pipe() that works on Windows and POSIX. For POSIX systems, this sets
/// CLOEXEC on the file descriptors.
pub fn pipe() ![2]posix.fd_t {
    switch (builtin.os.tag) {
        else => return try posix.pipe2(.{ .CLOEXEC = true }),
        .windows => { ... },
    }
}
```

- On POSIX, `std.posix.pipe2(.{ .CLOEXEC = true })` creates a pipe with
  close-on-exec set. Returns `[2]fd_t` as `[read, write]`.
- On platforms with a real `pipe2` syscall, that flag is atomic; on **macOS (no
  `pipe2`)**, Zig's `pipe2` emulates it: `pipe()` then
  `fcntl(fd, F_SETFD, FD_CLOEXEC)` on each end.
- Errors propagate (e.g. `EMFILE` / `ENFILE` when the fd table is full).

## Rust mapping (`roastty/src/os/pipe.rs`)

`libc::pipe` then `fcntl` to set `FD_CLOEXEC` on both ends (the macOS path Zig's
`pipe2` takes), returning the two ends as owned `OwnedFd`s (RAII — they close on
drop, and on any error-path early return):

```rust
//! A close-on-exec pipe (port of upstream `os/pipe`).

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// Create a pipe with `FD_CLOEXEC` set on both ends, returned as `(read, write)` (upstream
/// `os.pipe.pipe`). macOS has no `pipe2`, so close-on-exec is set with `fcntl` after
/// `pipe()` — the same emulation `std.posix.pipe2` uses on macOS.
pub(crate) fn pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Take ownership immediately so the fds close on any early return below.
    let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };

    set_cloexec(read.as_raw_fd())?;
    set_cloexec(write.as_raw_fd())?;

    Ok((read, write))
}

/// Set the `FD_CLOEXEC` (close-on-exec) flag on a file descriptor.
fn set_cloexec(fd: RawFd) -> std::io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
```

`libc::pipe` writes `[read, write]` into `fds`; both are wrapped in `OwnedFd`
before `set_cloexec` so a failure there frees them (no leak — the equivalent of
the error path of Zig's `pipe2`). `set_cloexec` reads the current `F_GETFD`
flags and ORs in `FD_CLOEXEC` via `F_SETFD` (the close-on-exec descriptor flag —
`posix.pipe2`'s `.CLOEXEC` on macOS).

## Scope / faithfulness notes

- **Ported (bridged)**: `os.pipe.pipe` → `os::pipe::pipe`, plus a `set_cloexec`
  helper.
- **Faithful**: a pipe with close-on-exec set on **both** ends, returned as
  `(read, write)`; errors from `pipe` / `fcntl` propagate.
- **Faithful adaptation**: `std.posix.pipe2(.{ .CLOEXEC = true })` on macOS →
  `libc::pipe`
  - `fcntl(F_SETFD, FD_CLOEXEC)` on each fd (the exact emulation Zig uses where
    `pipe2` is absent); `[2]fd_t` (raw, caller-owned) → `(OwnedFd, OwnedFd)`
    (RAII owned ends — closed on drop, the idiomatic Rust ownership of the
    returned fds); `!` → `io::Result`. The Windows arm is dropped (macOS-only).
- **Deferred**: nothing specific to this file (fully ported on the macOS arm).
- No C ABI/header/ABI-inventory change (internal Rust). New `os::pipe` module.

## Changes

1. `roastty/src/os/pipe.rs` (new): `pipe`, `set_cloexec`.
2. `roastty/src/os/mod.rs`: add `pub(crate) mod pipe;`.
3. Tests (in `pipe.rs`):
   - **cloexec set on both ends**: `pipe()` succeeds; `fcntl(F_GETFD)` on each
     end has `FD_CLOEXEC` set.
   - **bytes transfer**: writing bytes to the write end reads them back from the
     read end (`libc::write` / `libc::read`), confirming a real, connected pipe.
   - (fds close on drop via `OwnedFd` — no explicit close needed.)
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty os::pipe
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/os/pipe.rs roastty/src/os/mod.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `os::pipe::pipe` creates a pipe with `FD_CLOEXEC` set on both ends and returns
  `(read, write)`, with `pipe`/`fcntl` errors propagated — faithful to
  `os/pipe.zig`'s macOS path;
- the tests pass (CLOEXEC set + byte transfer), and the existing tests still
  pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the close-on-exec behavior or the fd handling
diverges from upstream, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. Codex confirmed that on macOS, `libc::pipe` followed by
`fcntl(F_SETFD, flags | FD_CLOEXEC)` on both descriptors is the correct
adaptation of `std.posix.pipe2(.{ .CLOEXEC = true })` (`FD_CLOEXEC` is the
close-on-exec descriptor flag, while `O_CLOEXEC` is only the atomic
creation-time form where available); taking `OwnedFd` ownership before setting
CLOEXEC is the right Rust shape (it closes both descriptors on any `fcntl` error
and still lets callers use `as_raw_fd` / `into_raw_fd` for later `dup2`/exec
plumbing); returning `(read, write)` preserves the upstream `[read, write]`
order; and the CLOEXEC and byte-transfer tests are adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d546-prompt.md` (design)
- Result: `logs/codex-review/20260604-d546-last-message.md` (design)

## Result

**Result:** Pass

`os::pipe::pipe` was added: `libc::pipe` then `set_cloexec`
(`fcntl(F_SETFD, flags | FD_CLOEXEC)`) on each end — the macOS emulation of
`std.posix.pipe2(.{ .CLOEXEC = true })` — returning `(OwnedFd, OwnedFd)` as
`(read, write)`, with both ends taken into `OwnedFd` before `set_cloexec` so a
failure frees them. The module is registered in `os/mod.rs`. Two tests:
`FD_CLOEXEC` is set on both ends (`fcntl(F_GETFD)`), and bytes written to the
write end are read back from the read end (a real connected pipe).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3057 passed, 0 failed (two new tests; no regressions,
  up from 3055).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + os/pipe.rs + os/mod.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **one Nit** (no
Required or Optional findings): the doc had `## Result` but no `## Conclusion` —
fixed by adding the conclusion below. Codex confirmed the implementation matches
upstream `pipe.zig` and the approved design: `libc::pipe` returns
`[read, write]`, both ends are immediately owned by `OwnedFd`, and `FD_CLOEXEC`
is set with `fcntl(F_SETFD, flags | FD_CLOEXEC)` on both descriptors; the
`OwnedFd` ownership correctly closes fds on normal drop and on any `set_cloexec`
error path; and the tests soundly verify close-on-exec on both ends and the
connected read/write order.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r546-prompt.md` (result)
- Result: `logs/codex-review/20260604-r546-last-message.md` (result)

## Conclusion

`os::pipe::pipe` — a close-on-exec pipe — is faithfully ported from
`os/pipe.zig`, using the macOS `libc::pipe` + `fcntl(FD_CLOEXEC)` emulation of
`std.posix.pipe2` and returning RAII `(OwnedFd, OwnedFd)` ends. This is a core
PTY/IO building block (self-pipe wakeups, child-process plumbing) for the
eventual termio layer. The OS-utility frontier still has a few self-contained
slices (`i18n_locales`, the `fixMaxFiles` / `restoreMaxFiles` rlimit remainder
of `file.zig`, `kernel_info`). The config `loadDefaultFiles` stays deferred
pending roastty's naming decision; `background-image-opacity` stays
float-blocked.
