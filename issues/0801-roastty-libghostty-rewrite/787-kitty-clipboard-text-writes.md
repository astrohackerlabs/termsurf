+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 787: Kitty Clipboard Text Writes

## Description

Experiment 786 implemented Kitty OSC 5522 read replies for the current
plain-text runtime clipboard ABI. Kitty write transactions are still explicitly
unsupported: `type=write`, `type=wdata`, and `type=walias` receive `ENOSYS`.

This experiment implements the text subset of Kitty clipboard writes. It accepts
`type=write` transactions, collects one or more `type=wdata` chunks for
`text/plain`, writes the assembled text through the existing runtime
`write_clipboard_cb`, and replies with `type=write:status=DONE`. The current
runtime clipboard ABI represents MIME/data values as C strings, so binary data,
NUL-containing data, arbitrary MIME objects, aliases, passwords, and durable
permission passwords remain later work.

## Changes

- `roastty/src/lib.rs`
  - Add per-surface Kitty clipboard write transaction state containing:
    - target clipboard (`standard` or supported `selection`);
    - sanitized request `id`;
    - original request terminator;
    - accumulated `text/plain` bytes;
    - failed/ignore-until-next-write state for write errors.
  - Extend Kitty clipboard event handling:
    - `type=write` starts a new transaction.
    - `type=wdata:mime=<base64 text/plain>;<base64 chunk>` appends decoded data
      to the active transaction.
    - final `type=wdata` with no `mime` and no payload completes the active
      transaction.
    - `type=walias` replies with `ENOSYS` and moves the active transaction into
      failed/ignore state.
  - Map `type=write:loc=primary` to the selection clipboard only when the
    runtime reports selection clipboard support; otherwise reply
    `type=write:status=ENOSYS`.
  - Honor `clipboard-write` policy:
    - `deny` replies `type=write:status=EPERM` and does not start a transaction.
    - `allow` forwards the completed write with `confirm = false`.
    - `ask` forwards the completed write with `confirm = true`.
  - Decode MIME and data payloads with the existing base64 guard. Invalid base64
    replies `type=write:status=EINVAL` and moves the transaction into
    failed/ignore state.
  - Accept only decoded MIME `text/plain` in this experiment. Other MIME types
    reply `type=write:status=ENOSYS` and move the transaction into failed/ignore
    state.
  - Reject decoded NUL-containing text payloads with `type=write:status=EINVAL`
    because the current clipboard write ABI passes C strings.
  - When completion succeeds, forward one `text/plain` clipboard content item to
    `write_clipboard_cb` and queue `type=write:status=DONE` with the transaction
    id and terminator.
  - If the app, termio worker, or write callback is unavailable at completion,
    consume the transaction and reply `type=write:status=EIO` when a termio
    worker is available.
  - After any write error, ignore further `wdata`/`walias` packets until the
    next `type=write`, matching the Kitty protocol's recovery rule.
  - Keep Kitty read behavior from Experiment 786 and OSC 52 read/write behavior
    unchanged.
- `roastty/src/terminal/clipboard.rs`
  - Reuse the existing Kitty option parser (`operation`, `loc`, `mime`,
    `sanitized_id`) without changing parser semantics.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Add the experiment index entry.
  - Narrow the remaining surface lifecycle checklist from Kitty write/multipart
    handling to arbitrary MIME/alias/password write handling after this
    experiment passes.

## Verification

- Inspect references:
  - Kitty clipboard protocol documentation for write transaction ordering,
    `wdata` completion, error statuses, aliases, and the "ignore until next
    write" recovery rule.
  - `vendor/ghostty/src/terminal/osc/parsers/kitty_clipboard_protocol.zig`
    parser behavior.
  - Existing OSC 52 write handling in `roastty/src/lib.rs` for runtime callback,
    `clipboard-write` policy, and C-string limitations.
- Run focused tests:
  - `cargo test -p roastty kitty_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty osc52_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty clipboard_write -- --nocapture --test-threads=1`
- New or updated Kitty clipboard assertions must cover:
  - `type=write` starts a transaction and does not call `write_clipboard_cb`
    until final `type=wdata`;
  - `type=wdata:mime=<text/plain>;chunk` appends chunks, including multiple
    chunks for one text write;
  - final `type=wdata` forwards one `text/plain` content item and replies
    `type=write:status=DONE`;
  - valid request ids are copied to the `DONE` or error reply after
    sanitization;
  - BEL and ST request terminators are preserved in write status replies;
  - `loc=primary` routes to selection when supported and replies `ENOSYS` when
    unsupported;
  - `clipboard-write = deny` replies `EPERM` without starting a transaction;
  - `clipboard-write = allow` forwards with `confirm = false`;
  - `clipboard-write = ask` forwards with `confirm = true`;
  - invalid base64 MIME/data replies `EINVAL`;
  - decoded NUL-containing chunks reply `EINVAL`;
  - unsupported MIME and `type=walias` reply `ENOSYS`;
  - `wdata` without an active transaction replies `EINVAL`;
  - after an error, further `wdata` packets are ignored until the next
    `type=write`;
  - a new `type=write` replaces a failed transaction and can complete
    successfully;
  - existing Kitty read and OSC 52 write tests still pass.
- Run:
  - `cargo fmt -p roastty`
  - `cargo fmt -p roastty -- --check`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/787-kitty-clipboard-text-writes.md`
- Run:
  - `git diff --check`

The experiment passes if Kitty text/plain write transactions assemble chunks,
honor clipboard target and write policy, forward text through the existing
runtime write callback, emit protocol status replies with id/terminator
preserved, ignore post-error write packets until the next transaction, preserve
existing Kitty read and OSC 52 behavior, and all focused tests pass. It is
Partial if only single-chunk text writes can be proven. It fails if the current
C-string clipboard ABI cannot safely support even the text/plain subset.

## Design Review

Codex reviewed the design and found no blocking findings. The review approved
the text-only write transaction scope as coherent with the current C-string
runtime clipboard ABI and the planned verification coverage.
