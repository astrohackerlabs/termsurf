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

# Experiment 668: Background Termio Worker

## Description

Experiment 667 added a deterministic synchronous `Termio` pump that coordinates
PTY child output, terminal stream processing, terminal-generated PTY responses,
queued caller input, resize forwarding, and child exit checks. The next missing
PTY layer is a persistent background loop that repeatedly drives that pump and
accepts commands from the owning app code.

This experiment adds an internal background termio worker. The worker owns no
new terminal semantics; it wraps the synchronous `Termio` from Experiment 667,
drains a command queue, calls `pump_once` with a short timeout, and emits events
when output, EOF, child exit, or errors occur.

The worker is intentionally not App/surface presentation yet. It does not add a
C ABI, renderer wakeup, mailbox integration, surface invalidation, shell
configuration, foreground process tracking, or terminal grid resize. It creates
the internal loop shape that future App/surface experiments can connect to.

## Changes

- `roastty/src/termio.rs`
  - Add internal command and event types:
    - `TermioWorkerCommand::Write(Vec<u8>)`;
    - `TermioWorkerCommand::ResizePty(PtySize)`;
    - `TermioWorkerCommand::Shutdown`;
    - `TermioWorkerEvent::Pump(TermioPump)`;
    - `TermioWorkerEvent::Error(String)`.
  - Add `TermioWorkerError` with variants for command-channel disconnect and
    thread join failure. Command send failures mean the worker has already
    stopped or is stopping.
  - Add an internal `TermioWorker` handle that contains:
    - a command sender;
    - an event receiver;
    - an `Arc<Mutex<Termio>>` for future surface-side terminal reads and for
      tests;
    - `Option<JoinHandle<()>>` so explicit shutdown and `Drop` are idempotent.
  - Add `TermioWorker::spawn(termio, pump_timeout_ms, max_read_bytes)` to start
    a background thread.
  - In the worker thread:
    - drain all currently pending commands before each pump cycle;
    - map write commands to `Termio::queue_write`;
    - map resize commands to `Termio::resize_pty`;
    - break on shutdown, command-channel disconnect, child exit, or terminal
      EOF;
    - call `Termio::pump_once(pump_timeout_ms, max_read_bytes)`;
    - emit `TermioWorkerEvent::Pump` whenever the pump reads bytes, writes
      bytes, reports pending writes, reports EOF, or reports child exit;
    - when EOF or child exit occurs, send that final `Pump` event first and then
      exit the loop, so consumers can observe the terminal's final state;
    - emit `TermioWorkerEvent::Error` and break on IO, terminal, or invalid
      readiness errors.
  - Add handle methods:
    - `queue_write(&[u8]) -> Result<(), TermioWorkerError>`;
    - `resize_pty(PtySize) -> Result<(), TermioWorkerError>`;
    - `try_recv_event() -> Option<TermioWorkerEvent>`, returning `None` when no
      event is currently queued or the worker event channel is disconnected;
    - `with_termio<R>(&self, f: impl FnOnce(&Termio) -> R) -> R` for read-only
      inspection;
    - `shutdown(&mut self) -> Result<(), TermioWorkerError>` to send shutdown
      when the command channel is still connected and join the thread if it has
      not already been joined.
  - Define shutdown semantics exactly:
    - `shutdown` takes `&mut self`;
    - it may be called more than once;
    - after the first successful join, later calls return `Ok(())`;
    - if the worker already exited and the command send fails, `shutdown` still
      joins the thread and returns `Ok(())` unless the join itself fails;
    - queued events are not drained by `shutdown`; callers can read any events
      already received before dropping the handle.
  - Implement `Drop` for `TermioWorker` so tests and future callers do not leave
    a background thread or child process running if they forget to call
    `shutdown`.
  - Keep the worker lock scope explicit: the worker may hold the `Termio` mutex
    while polling for at most the configured `pump_timeout_ms`. This is
    acceptable for this internal experiment, and tests use a small timeout. A
    future App/surface integration can refine the ownership model if a renderer
    needs nonblocking terminal snapshots.
  - Do not add a wake pipe in this experiment. Write, resize, and shutdown
    command latency is bounded by `pump_timeout_ms` because commands do not wake
    an in-progress PTY poll. Tests use a small timeout and assert eventual
    behavior, not immediate wakeup.
- Tests in `roastty/src/termio.rs`
  - Use the shared `os::pty::PTY_COMMAND_LOCK` for all worker subprocess tests.
  - Spawn a worker around `/bin/sh -c "printf hello"` and assert a pump event is
    emitted and the shared terminal screen contains `hello`.
  - Spawn a shell with echo disabled, send input with `queue_write`, and assert
    output returns through the worker loop.
  - Send a resize command and assert the underlying PTY winsize changes.
  - Spawn a short-lived child and assert the worker emits a child-exit or EOF
    pump event and then joins.
  - Start a long-lived child, call `shutdown`, and assert the thread joins
    without leaving the handle active.
  - Start a long-lived child, drop the worker without calling `shutdown`, and
    assert the child process is cleaned up by the worker/drop path.

## Design Review

**Result:** Approved after amendments.

Codex found five concrete design gaps: handle methods needed exact return types
and stopped-channel behavior, shutdown needed idempotent join ownership, final
EOF/child-exit event ordering was ambiguous, command latency needed to be stated
because there is no wake pipe yet, and the promised `Drop` cleanup needed a
test.

The design now defines `TermioWorkerError`, exact handle method signatures,
`Option<JoinHandle<()>>`, idempotent `shutdown(&mut self)` behavior, final
`Pump`-before-exit ordering, bounded command latency by `pump_timeout_ms`, and a
drop-without-shutdown cleanup test.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/668-background-termio-worker.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty termio`
- `cargo test -p roastty os::pty`
- `git diff --check`

## Result

**Result:** Pass.

Roastty now has an internal background `TermioWorker` that wraps the synchronous
`Termio` pump. The worker drains queued write, PTY resize, and shutdown
commands, calls `pump_once` with a bounded timeout, emits pump events for
output, writes, pending writes, EOF, and child exit, and emits stringified error
events before stopping on pump or resize failures.

The worker handle exposes fallible command sends, event polling, read-only
`with_termio` inspection, and idempotent `shutdown(&mut self)`. Command sends
fail with `CommandDisconnected` after the worker has stopped. `Drop` calls
shutdown so forgetting to stop the worker does not leave its thread or child
process running.

Because the current terminal type owns raw internal pointers, `Termio` has a
narrow `unsafe impl Send` with the invariant that worker access is serialized
through the worker mutex and Termio does not create aliases to the terminal's
raw pointers. `TermioWorker::spawn` rejects terminals with installed callbacks,
because callback userdata may be thread-affine and `Terminal::next_slice` can
invoke callbacks from the worker thread.

This experiment does not add a wake pipe. Write, resize, and shutdown command
latency is bounded by `pump_timeout_ms`, matching the approved design for this
internal slice. App/surface presentation, renderer wakeup, mailbox integration,
and terminal grid resize remain out of scope.

Focused tests cover worker-delivered child output, command-driven input/output,
command-driven PTY resize, final pump event delivery before child exit, stopped
command-send failures, callback-bearing terminal rejection, explicit shutdown
idempotence, implicit `Drop` cleanup of a long-lived child, and the existing
synchronous termio behavior. PTY subprocess tests continue to share the
`os::pty::PTY_COMMAND_LOCK`.

Verification passed:

- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty termio` — 14 passed, 0 failed
- `cargo test -p roastty os::pty` — 13 passed, 0 failed
- `git diff --check`

## Conclusion

The PTY path now has both a synchronous pump and a persistent internal worker
loop. The remaining user-visible gap is App/surface presentation: connecting
worker events to application state, renderer wakeups, surface invalidation, and
terminal snapshots suitable for the macOS frontend.

## Completion Review

**Result:** Approved after fixes.

Codex found three result-commit issues. First, the initial
`unsafe impl Send for Termio` did not account for terminal callbacks and
callback userdata that may be thread-affine. Second, stopped-channel command
send behavior was implemented but not tested. Third, the issue record still
lacked result-review provenance.

The worker now rejects callback-bearing terminals before moving `Termio` to the
worker thread, and the `unsafe impl Send` comment documents that invariant.
Tests now cover callback rejection and `CommandDisconnected` after worker stop.
The experiment frontmatter and README agent tuple now record the result review.
Codex re-reviewed the code fixes and confirmed the unsafe `Send` finding and
stopped-channel test finding are resolved.
