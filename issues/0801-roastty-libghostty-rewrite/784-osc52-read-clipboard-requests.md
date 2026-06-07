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

# Experiment 784: OSC 52 Read Clipboard Requests

## Description

Experiments 782 and 783 retained terminal clipboard OSC actions and delivered
them across the termio worker boundary. The surface still ignores those events,
so an application that sends `OSC 52 ; kind ; ?` never receives a clipboard read
request or an OSC 52 reply.

This experiment implements only the OSC 52 read path. It converts retained OSC
52 read events into active Roastty clipboard requests, forwards them through the
existing runtime `read_clipboard_cb`, and completes confirmed reads by writing
the standard OSC 52 response back to the child PTY. OSC 52 writes and Kitty
clipboard OSC 5522 handling remain later work.

## Changes

- `roastty/src/lib.rs`
  - Add `clipboard_read: config::ClipboardAccess` to `App` and `Surface`, copied
    from `roastty_app_new` config and then into each new surface, so OSC 52 read
    completion can follow the current `clipboard-read` policy.
  - Extend `ClipboardRequestState` with the runtime clipboard source and the OSC
    52 reply kind needed to complete reads.
  - Update paste request allocation to store the existing paste request kind and
    clipboard location without changing paste behavior.
  - Add surface handling for `TermioWorkerEvent::Clipboard`:
    - ignore Kitty clipboard events for now;
    - ignore OSC 52 events whose payload is not exactly `?`;
    - store the OSC 52 reply kind as `c`, `s`, or `p`, with unknown kinds
      falling back to `c`;
    - always invoke the runtime read against `ROASTTY_CLIPBOARD_STANDARD`,
      matching upstream
      `startClipboardRequest(.standard, .{ .osc_52_read = clipboard })`;
    - treat OSC 52 kind `p` as a primary reply-kind request, but read from the
      standard clipboard because the current macOS-only public clipboard ABI has
      no primary clipboard source;
    - reject allocation when `clipboard-read` is `deny`;
    - allocate an active `ROASTTY_CLIPBOARD_REQUEST_OSC_52_READ` request before
      invoking `read_clipboard_cb`;
    - clean up the active request when the read callback refuses the request,
      including synchronous completion before refusal.
  - Update `roastty_surface_complete_clipboard_request` to handle active
    `ROASTTY_CLIPBOARD_REQUEST_OSC_52_READ` states:
    - consume empty data and still write an empty OSC 52 reply;
    - if `clipboard-read` is `ask`, `confirmed` is false, and
      `confirm_read_clipboard_cb` exists, call it with
      `ROASTTY_CLIPBOARD_REQUEST_OSC_52_READ` and preserve the same request
      state;
    - if `clipboard-read` is `ask`, `confirmed` is false, and no confirmation
      callback exists, consume the request without replying;
    - if `clipboard-read` is `allow`, unconfirmed completion replies without a
      confirmation callback;
    - on confirmed completion, write `ESC ] 52 ; kind ; base64(data) ESC \` to
      the child PTY and consume the request;
    - consume valid active OSC 52 read requests even if the app or worker has
      disappeared before completion.
  - Add a small standard-base64 encoder helper for OSC 52 replies, with focused
    tests for padding and empty input.
  - Add tests for read request allocation, `clipboard-read` deny/ask/allow
    policy, `s` and `p` reply-kind preservation with standard runtime reads,
    callback refusal cleanup, synchronous completion during `read_clipboard_cb`,
    synchronous completion during `confirm_read_clipboard_cb`, unconfirmed
    confirmation preservation, confirmed reply bytes, empty reply bytes,
    stale/double/cross surface state handling, and non-read/Kitty events being
    ignored.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Add the experiment index entry.
  - Keep the broad OSC 52 request checklist wording scoped: OSC 52 read request
    allocation/handling is done, while OSC 52 writes and Kitty clipboard
    handling remain missing.

## Verification

- Inspect upstream reference:
  - `vendor/ghostty/src/termio/stream_handler.zig` OSC 52 read event emission.
  - `vendor/ghostty/src/Surface.zig` `startClipboardRequest`,
    `completeClipboardRequest`, and `completeClipboardReadOSC52`.
- Run focused tests:
  - `cargo test -p roastty osc52_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_complete_clipboard_request -- --nocapture --test-threads=1`
  - `cargo test -p roastty clipboard_request -- --nocapture --test-threads=1`
- Run:
  - `cargo fmt -p roastty`
  - `cargo fmt -p roastty -- --check`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/784-osc52-read-clipboard-requests.md`
- Run:
  - `git diff --check`

The experiment passes if OSC 52 read events allocate stable active request
pointers, completion validates and consumes only matching active requests,
confirmed completions write correct OSC 52 replies to the PTY, unconfirmed
completions follow `clipboard-read` allow/ask/deny policy, false paths do not
leak requests, existing paste request tests still pass, and focused tests pass.
It is Partial if only allocation or only completion can be proven without
overclaiming. It fails if the current C ABI cannot safely represent OSC 52 read
request state.

## Design Review

Codex reviewed the initial design and found four blocking issues:

- it conflated the runtime clipboard read source with the OSC 52 reply kind,
  while upstream reads from the standard clipboard and stores the requested
  reply kind separately;
- it did not explicitly handle OSC 52 kind `p` even though upstream recognizes
  it as primary;
- its confirmation policy over-confirmed compared with upstream
  `clipboard-read`;
- it did not require coverage for synchronous completion during
  `confirm_read_clipboard_cb`.

The design was revised so OSC 52 reads always call the runtime standard
clipboard read, store `c`/`s`/`p` only for reply generation, use the current
`clipboard-read` config policy, and require confirm-callback reentrancy tests.

Re-review approved the revised design with no findings.
