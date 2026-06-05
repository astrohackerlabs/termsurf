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

# Experiment 549: open a URL in the default handler (os::open)

## Description

Continuing the `os` module (Experiments 541–548), this experiment ports upstream
`os/open.zig` — **`open()`, which opens a URL/file in the system's default
handler**. On macOS this runs `open` (with `-t` for plain text); it is how the
terminal opens OSC 8 hyperlinks. stdout is ignored, stderr is drained
off-thread, and the child is reaped in a detached thread so the call never
blocks (some `open` implementations block, some don't).

## Upstream behavior

`os/open.zig`:

```zig
pub fn open(alloc, kind: apprt.action.OpenUrl.Kind, url: []const u8) !void {
    var exe: std.process.Child = switch (builtin.os.tag) {
        .macos => .init(switch (kind) {
            .text => &.{ "open", "-t", url },
            .html, .unknown => &.{ "open", url },
        }, alloc),
        // …linux/windows arms…
    };
    exe.stdout_behavior = .Ignore;
    exe.stderr_behavior = .Pipe;
    // …snap LD_LIBRARY_PATH scrubbing (Linux-only)…
    try exe.spawn();

    // Reap + drain stderr on a detached thread (some `open`s block, some don't).
    const thread = try std.Thread.spawn(.{}, openThread, .{exe});
    thread.detach();
}

fn openThread(exe_: std.process.Child) void {
    var exe = exe_;
    if (exe.stderr) |stderr| {
        // read stderr line by line, log.warn each line
    }
    _ = exe.wait() catch {};
}
```

- The `kind` (`text` / `html` / `unknown`) selects the macOS argv:
  `open -t <url>` for text, `open <url>` otherwise.
- `stdout` is ignored; `stderr` is piped. The process is spawned on the calling
  thread (so spawn failure is detected synchronously and propagates), then a
  **detached thread** drains stderr (logging each line as a warning) and
  `wait()`s on the child, so `open` never blocks the caller.
- The snap `LD_LIBRARY_PATH` scrubbing is Linux-only.

## Rust mapping (`roastty/src/os/open.rs`)

A local `Kind` enum (roastty has no apprt), a testable `open_command_args` seam
for the platform argv, and `open` building a `std::process::Command` and
detaching a reaper thread:

```rust
//! Open a URL/file in the default handler (port of upstream `os/open`).

use std::process::{Command, Stdio};

/// The kind of URL being opened, which selects the opener arguments (upstream
/// `apprt.action.OpenUrl.Kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    Html,
    Unknown,
}

/// The macOS `open` argv for a URL of the given `kind` (upstream's per-`kind` `Child.init`):
/// `open -t <url>` for text, `open <url>` otherwise.
fn open_command_args(kind: Kind, url: &str) -> Vec<&str> {
    match kind {
        Kind::Text => vec!["open", "-t", url],
        Kind::Html | Kind::Unknown => vec!["open", url],
    }
}

/// Open `url` in the default handling application (upstream `os.open.open`). stdout is
/// ignored; stderr is drained and the child reaped on a detached thread so this never
/// blocks. Returns an error if the opener fails to spawn or the reaper thread fails to start.
pub(crate) fn open(kind: Kind, url: &str) -> std::io::Result<()> {
    let args = open_command_args(kind, url);
    let mut command = Command::new(args[0]);
    command.args(&args[1..]);
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());

    // Spawn on this thread so a spawn failure is detected synchronously.
    let mut child = command.spawn()?;

    // Drain stderr and reap on a detached thread (some `open` implementations block, some
    // don't), matching upstream's `openThread`. A thread-creation failure propagates (upstream
    // uses `try std.Thread.spawn`); the returned `JoinHandle` is dropped to detach.
    std::thread::Builder::new().spawn(move || {
        if let Some(mut stderr) = child.stderr.take() {
            // Upstream logs each stderr line; roastty has no logger here, so we drain it to a
            // sink (a bounded internal buffer) so the pipe can't fill and stall the child.
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        }
        let _ = child.wait();
    })?;

    Ok(())
}
```

`open_command_args` is the faithful per-`kind` argv selection (`-t` for text).
`open` sets `stdout` to null and `stderr` to a pipe before spawning, spawns
synchronously (so a spawn error propagates, like `try exe.spawn()`), then
detaches a thread (via `thread::Builder::spawn`, so a thread-creation failure
also propagates — like `try std.Thread.spawn` — and the `JoinHandle` is dropped
to detach) that drains stderr to a sink and `wait()`s — the equivalent of
`openThread` + `thread.detach()`. The Linux snap scrubbing and the non-macOS
arms drop away (macOS-only).

## Scope / faithfulness notes

- **Ported (bridged)**: `os.open.open` → `os::open::open`;
  `apprt.action.OpenUrl.Kind` → a local `os::open::Kind` (`Text` / `Html` /
  `Unknown`); `openThread` → the detached reaper closure.
- **Faithful**: the macOS argv (`open -t <url>` for text, `open <url>`
  otherwise); stdout ignored, stderr piped; synchronous spawn (errors
  propagate); a detached thread drains stderr and reaps the child so the caller
  never blocks.
- **Faithful adaptation**: `std.process.Child` → `std::process::Command` /
  `Child`; `try std.Thread.spawn(...)` then `.detach()` →
  `std::thread::Builder::new().spawn(...)?` with the `JoinHandle` dropped (so a
  thread-creation failure propagates and the thread is detached); the stderr
  **log** of each line → a bounded drain to `std::io::sink()` via
  `std::io::copy` (roastty has no logger in this module — documented; still
  prevents the pipe filling); `!void` → `io::Result<()>`. The Linux/Windows arms
  and the snap `LD_LIBRARY_PATH` scrubbing drop (macOS-only).
- **Deferred**: nothing macOS-relevant (the actual subprocess spawn is exercised
  at runtime, not in a unit test — see Verification).
- No C ABI/header/ABI-inventory change (internal Rust). New `os::open` module.

## Changes

1. `roastty/src/os/open.rs` (new): `Kind`, `open_command_args`, `open`.
2. `roastty/src/os/mod.rs`: add `pub(crate) mod open;`.
3. Tests (in `open.rs`): the **argv-selection seam** (the actual spawn is _not_
   unit-tested — it would launch an external application) —
   - `open_command_args(Kind::Text, url) == ["open", "-t", url]`.
   - `open_command_args(Kind::Html, url) == ["open", url]`.
   - `open_command_args(Kind::Unknown, url) == ["open", url]` (same as `Html`).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty os::open
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/os/open.rs roastty/src/os/mod.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `open_command_args` selects `open -t <url>` for `Text` and `open <url>` for
  `Html` / `Unknown`, and `open` spawns that command with stdout ignored /
  stderr drained on a detached reaper thread, propagating a spawn error —
  faithful to `os/open.zig`'s macOS path;
- the argv-seam tests pass, and the existing tests still pass (the live spawn is
  verified by construction, not unit-tested, since it launches an external app);
- the non-macOS arms and snap scrubbing stay dropped;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the argv selection or the spawn/stderr/reap handling
diverges from upstream, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex's first design review raised **two Required** findings, both now fixed;
the corrected design was **re-reviewed and approved** (one Nit, also applied).

- **Thread-spawn error propagation (Required, fixed)**: `std::thread::spawn`
  panics on OS-thread-creation failure, but upstream uses `try std.Thread.spawn`
  (error propagates). Fixed by `std::thread::Builder::new().spawn(...)?` with
  the `JoinHandle` dropped to detach.
- **Unbounded stderr drain (Required, fixed)**: `read_to_end` into a `Vec` can
  grow without bound; upstream drains incrementally. Fixed by
  `std::io::copy(&mut stderr, &mut std::io::sink())` (bounded internal buffer).
- **Doc comment (Nit, fixed)**: the `open` doc now notes it errors if the opener
  fails to spawn _or the reaper thread fails to start_.

On re-review Codex confirmed both fixes are correct and faithful
(`Builder::spawn(...)?` matches `try std.Thread.spawn`, dropping the
`JoinHandle` is the Rust detach, and `io::copy(.., sink())` drains stderr
without unbounded allocation while preventing pipe backpressure), and the macOS
argv behavior and seam-only tests remain appropriate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d549-prompt.md` (design),
  `logs/codex-review/20260604-d549b-prompt.md` (design re-review)
- Result: `logs/codex-review/20260604-d549-last-message.md` (design),
  `logs/codex-review/20260604-d549b-last-message.md` (design re-review)
