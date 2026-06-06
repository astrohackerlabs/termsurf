+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 664: PTY Foundation

## Description

Experiment 663 connected tmux DCS control mode to the terminal byte stream and
PTY response path, but Roastty still has no real PTY primitive. Upstream
Ghostty's termio `Exec` backend starts by opening a PTY with an initial
character/pixel size, later resizes it with `TIOCSWINSZ`, and uses the master
file descriptor for subprocess IO.

This experiment ports only that low-level PTY foundation for macOS/POSIX:

- open a PTY with an initial rows/columns/pixel size;
- own and close the master/slave file descriptors safely;
- set the PTY size after open;
- expose the master/slave raw file descriptors internally for the future
  subprocess/read-loop slice;
- verify basic master/slave byte flow.

Subprocess spawning, environment setup, foreground process queries, tty-name ABI
surface, nonblocking read loops, polling/quit pipes, and termio mailbox
integration remain out of scope.

## Changes

- `roastty/src/os/pty.rs`
  - Add `PtySize` with `u16` `rows`, `cols`, `width_px`, and `height_px` fields
    so conversion into `libc::winsize` cannot truncate.
  - Add `Pty` owning `master` and `slave` `OwnedFd`s, matching the close-on-drop
    pattern in `os::pipe`.
  - Implement `Pty::open(size)` using `libc::openpty` and an initial
    `libc::winsize`.
  - Take ownership with `OwnedFd` immediately after `openpty` so both
    descriptors are closed on any post-open error.
  - Set `FD_CLOEXEC` on both descriptors after open, matching `os::pipe`, so the
    original PTY descriptors are not accidentally inherited across future exec
    calls.
  - Implement `Pty::set_size(size)` using `libc::ioctl(TIOCSWINSZ)`.
  - Expose internal `master_fd()` and `slave_fd()` accessors with
    `OwnedFd::as_raw_fd()` for later termio experiments.
  - Keep the module POSIX/macOS focused; Windows ConPTY is out of scope for
    Issue 801.
- `roastty/src/os/mod.rs`
  - Add the new `pty` module.
- Tests in `roastty/src/os/pty.rs`
  - Open a PTY and verify both descriptors are valid.
  - Verify `FD_CLOEXEC` is set on both descriptors.
  - Verify initial size through `TIOCGWINSZ`.
  - Verify resize updates the reported size.
  - Verify byte flow without hanging by putting the slave into raw mode and
    using `poll` or `select` with a short timeout before any read.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/664-pty-foundation.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty os::pty`
- `git diff --check`

## Design Review

**Result:** Approved after amendments.

Codex first found four concrete PTY safety and testability issues: the design
should use `OwnedFd` rather than raw `RawFd` ownership and manual `Drop`, set
`FD_CLOEXEC` on both descriptors, define non-truncating `PtySize` field types,
and avoid hanging byte-flow tests by using raw mode plus a timeout.

The design now follows the existing `os::pipe` ownership pattern with `OwnedFd`,
takes ownership immediately after `openpty`, sets `FD_CLOEXEC`, uses `u16` size
fields for direct `winsize` conversion, and requires raw-mode plus `poll` or
`select` timeout in byte-flow tests. Codex re-reviewed the amended design and
approved it for plan commit and implementation with no remaining blockers.
