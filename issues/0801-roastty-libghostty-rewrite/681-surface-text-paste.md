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

# Experiment 681: Surface Text Paste

## Description

Experiment 680 made explicit surface close requests reach the embedded runtime
callback. The next narrow surface input gap is
`ghostty_surface_text(surface, ptr, len)`, which upstream documents as sending
raw text to the terminal as a paste rather than as key events. Core Ghostty
applies paste behavior: unsafe control bytes are replaced with spaces, newlines
become carriage returns when bracketed paste is disabled, and bracketed paste
wrappers are emitted when bracketed paste mode is enabled.

Roastty already has a paste encoder (`input::paste`) and the surface PTY worker
already supports queued writes. This experiment adds the renamed
`roastty_surface_text(surface, ptr, len)` ABI and forwards encoded paste bytes
to the attached worker. It does not implement key-event dispatch, IME preedit,
mouse input, selection reads, or frontend text routing.

## Changes

- `roastty/include/roastty.h`
  - Add
    `ROASTTY_API void roastty_surface_text(roastty_surface_t, const char*, uintptr_t);`
    near the other surface input functions.
- `roastty/src/lib.rs`
  - Add `roastty_surface_text(surface, ptr, len)`.
  - Null surfaces are a no-op.
  - Null text pointers with nonzero length are a no-op; zero-length text is a
    no-op.
  - Detached surfaces and surfaces without an attached worker are no-ops.
  - For worker-backed surfaces, copy `ptr[0..len]` into an owned buffer, encode
    it with `input::paste::encode`, and queue the encoded segments to the
    worker.
  - Determine bracketed paste mode from the worker terminal's DEC 2004 mode.
  - If queuing a segment fails, record the worker error on the surface through
    the existing termio error path.
  - Add tests:
    - null surface and null text pointer are no-ops;
    - no-worker surfaces stay unchanged;
    - unbracketed text reaches the child PTY;
    - unbracketed text maps newlines to carriage returns;
    - unsafe control bytes are replaced with spaces through the paste encoder;
    - bracketed paste mode wraps the text with bracketed paste markers;
    - detached surfaces do not dereference the cleared app pointer or write.
- `roastty/src/terminal/terminal.rs`
  - Add a narrow production `Terminal::bracketed_paste_enabled()` accessor so
    surface text can read DEC 2004 state without using test-only mode helpers or
    exposing the private mode table.
- `roastty/tests/abi_harness.c`
  - Exercise `roastty_surface_text(surface, text, len)` through the C header on
    null and live skeleton surfaces to prove the symbol exists and is null-safe.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/681-surface-text-paste.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty surface`
- `cargo test -p roastty --test abi_harness`
- `git diff --check`

## Design Review

**Result:** Approved after amendments.

Codex found two design issues. First, the plan needed an explicit production
path for reading DEC 2004 bracketed paste mode instead of relying on test-only
mode helpers or private mode internals. The plan now adds a narrow
`Terminal::bracketed_paste_enabled()` accessor for surface text. Second, the
paste-encoder wording said unsafe control bytes were stripped, but Roastty's
encoder, matching upstream behavior, replaces them with spaces. The description
and tests now require replacement with spaces.

Codex otherwise approved the scope: forwarding `roastty_surface_text` through
the existing paste encoder into `TermioWorker::queue_write` is the right slice,
and the planned null/detached/no-worker and paste-encoding tests cover the
important cases.

## Result

**Result:** Pass

Implemented `roastty_surface_text(surface, ptr, len)` in the public C header and
Rust ABI. Null surfaces, null text pointers, zero-length text, detached
surfaces, and surfaces without an attached worker are no-ops. Worker-backed
surfaces copy the input bytes, encode them through `input::paste`, and queue the
encoded paste segments to the PTY worker.

The implementation adds a production `Terminal::bracketed_paste_enabled()`
accessor and uses the attached worker terminal's DEC 2004 state to choose
bracketed or unbracketed paste encoding. Unbracketed paste maps newlines to
carriage returns, unsafe control bytes are replaced with spaces, and bracketed
mode emits the `ESC[200~` / `ESC[201~` wrappers. Queue-write failures are
recorded through the existing surface termio error path.

The Rust tests cover null/no-worker/detached no-ops, unbracketed text reaching a
child PTY, newline-to-carriage-return behavior, unsafe-byte replacement, and
bracketed paste wrappers using a real PTY child that enables DEC 2004. The C ABI
harness calls `roastty_surface_text` through `roastty.h` on null and live
skeleton surfaces to prove the symbol exists and is null-safe.

Verification passed:

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/681-surface-text-paste.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty surface`
- `cargo test -p roastty --test abi_harness`
- `git diff --check`

## Conclusion

Roastty now has the renamed surface text ABI for paste-style text input into
worker-backed surfaces. This does not implement key events, IME preedit,
frontend text routing, or selection/text reads, but it gives the embedded
surface input path a real PTY-backed paste behavior that follows the existing
Roastty paste encoder.

## Completion Review

**Result:** Approved after provenance update.

Codex found no code issues. It confirmed that the ABI declaration is correctly
added, `roastty_surface_text` checks zero/null inputs before forming a slice,
paste encoding uses the worker terminal's bracketed-paste state, and the new
terminal accessor is appropriately narrow.

Codex also confirmed the result documentation correctly scopes this to
paste-style text input and does not claim key events, IME/preedit, frontend text
routing, or selection/text reads. The first completion-review pass blocked only
because `[review.result]`, this completion-review section, and the README
`Codex/Codex/Codex` tuple had not yet been recorded.
