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

# Experiment 785: OSC 52 Write Clipboard Handling

## Description

Experiment 784 completed the OSC 52 read path. OSC 52 write payloads still stop
at the surface boundary: `TerminalClipboardEvent::Osc52` values whose payload is
not `?` are ignored, so applications cannot set clipboard text through OSC 52.

This experiment implements only the OSC 52 write path. It decodes retained OSC
52 payloads, applies the `clipboard-write` config policy, and forwards decoded
text to the existing runtime `write_clipboard_cb`. Kitty clipboard OSC 5522
handling remains later work.

## Changes

- `roastty/src/lib.rs`
  - Add `clipboard_write: config::ClipboardAccess` to `App` and `Surface`,
    copied from `roastty_app_new` config and then into each new surface.
  - Extend `Surface::handle_terminal_clipboard_event` so OSC 52 payloads other
    than exactly `?` enter an OSC 52 write handler.
  - Decode OSC 52 write payloads using the existing `terminal::base64` decoder,
    with a pre-decode sizing guard so invalid base64 lengths or padding are
    ignored instead of being misread as empty writes.
  - Map OSC 52 kind `c` to standard clipboard, `s` to selection clipboard when
    the runtime supports it, and `p` to standard clipboard because the
    macOS-only public clipboard ABI has no primary clipboard. Unknown kinds fall
    back to standard.
  - Ignore selection writes when the runtime reports no selection clipboard
    support.
  - Forward empty decoded payloads as empty `text/plain` clipboard contents,
    matching upstream empty clipboard writes.
  - Ignore writes when `clipboard-write` is `deny`, when there is no app, when
    there is no runtime `write_clipboard_cb`, when decoded text contains a NUL
    byte that cannot be represented by the current C string clipboard ABI, or
    when base64 sizing/decoding fails.
  - Forward successful writes as one `text/plain` clipboard content item, with
    the runtime confirmation flag set when `clipboard-write` is `ask` and false
    when it is `allow`.
  - Keep OSC 52 read behavior from Experiment 784 unchanged and keep Kitty
    clipboard events ignored.
- `roastty/src/terminal/mod.rs`
  - Expose the existing base64 decoder as `pub(crate)` so the surface layer can
    reuse the ported OSC 52 / Kitty Graphics decoder instead of duplicating one.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Add the experiment index entry.
  - Scope the checklist update to say OSC 52 read/write surface handling is done
    while Kitty clipboard handling remains missing.

## Verification

- Inspect upstream reference:
  - `vendor/ghostty/src/termio/stream_handler.zig` OSC 52 write event emission.
  - `vendor/ghostty/src/Surface.zig` `clipboardWrite`.
- Run focused tests:
  - `cargo test -p roastty osc52_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_binding_action_copy_to_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty clipboard_write -- --nocapture --test-threads=1`
- New or updated OSC 52 write assertions must cover:
  - empty decoded payload writes an empty `text/plain` item;
  - kind `c` writes to the standard clipboard;
  - kind `s` writes to the selection clipboard when supported;
  - kind `s` is ignored when selection clipboard is unsupported;
  - kind `p` writes to the standard clipboard in the macOS ABI subset;
  - unknown kind falls back to standard clipboard;
  - `clipboard-write = deny` ignores the write;
  - `clipboard-write = allow` forwards with `confirm = false`;
  - `clipboard-write = ask` forwards with `confirm = true`;
  - invalid base64 length/padding is ignored;
  - invalid base64 characters are ignored;
  - decoded NUL-only payload (`AA==`) and embedded-NUL payload (`YQBj`) are
    ignored;
  - OSC 52 read events still allocate read requests and Kitty clipboard events
    remain ignored.
- Run:
  - `cargo fmt -p roastty`
  - `cargo fmt -p roastty -- --check`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/785-osc52-write-clipboard-handling.md`
- Run:
  - `git diff --check`

The experiment passes if OSC 52 write events decode base64 payloads, forward
valid text and empty writes through `write_clipboard_cb` with the correct
clipboard target and confirmation flag, honor deny/allow/ask config, do not leak
or allocate active read request state, leave invalid or unrepresentable payloads
ignored, preserve existing copy-to-clipboard behavior, and all focused tests
pass. It is Partial if only decoding or only runtime forwarding can be proven
without overclaiming. It fails if the current C ABI cannot safely represent OSC
52 write clipboard content.

## Design Review

Codex reviewed the initial design and found five blocking issues:

- Roastty's existing `terminal::base64::max_len` returns `0` on sizing errors,
  so the design needed an explicit invalid-sizing guard instead of treating
  every zero-size decode as a valid empty write;
- the design did not mention exposing `terminal::base64` outside the terminal
  module;
- decoded NUL payload handling needed explicit verification;
- target mapping and `clipboard-write` policy tests were too implicit;
- empty decoded writes were unspecified.

The design now requires a base64 sizing guard, a `pub(crate)` decoder module
change, explicit NUL/invalid-base64 tests, explicit `c`/`s`/`p`/unknown target
and deny/allow/ask policy coverage, and upstream-matching empty write behavior.

Re-review approved the revised design with no findings.

## Result

**Result:** Pass

Implemented OSC 52 write handling:

- `App` and `Surface` now carry the parsed `clipboard-write` policy.
- `Surface::handle_terminal_clipboard_event` routes OSC 52 non-read payloads to
  a write handler while preserving OSC 52 reads and ignoring Kitty clipboard
  events.
- The surface layer reuses the ported `terminal::base64` decoder, with an
  invalid-sizing guard so malformed payloads do not become empty writes.
- Valid decoded text and empty decoded payloads are forwarded through
  `write_clipboard_cb` as one `text/plain` item.
- `clipboard-write = ask` sets the runtime confirm flag, `allow` does not, and
  `deny` ignores the write.
- Kind `c`, supported `s`, unsupported `s`, `p`, and unknown target mapping are
  covered, with `p` using the standard clipboard in the current macOS ABI
  subset.
- Invalid base64 and decoded NUL-containing payloads are ignored because the
  current C clipboard content ABI is C-string based.

Verification passed:

- `cargo test -p roastty osc52_clipboard -- --nocapture --test-threads=1`
  - 13 tests passed.
- `cargo test -p roastty surface_binding_action_copy_to_clipboard -- --nocapture --test-threads=1`
  - 2 tests passed.
- `cargo test -p roastty clipboard_write -- --nocapture --test-threads=1`
  - 7 tests passed.
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/785-osc52-write-clipboard-handling.md`
- `git diff --check`

## Conclusion

OSC 52 read and write handling now both reach the surface/runtime boundary with
tested config policy and target behavior. The remaining terminal clipboard OSC
gap is Kitty clipboard OSC 5522 semantics.

## Completion Review

The first completion review found no core OSC 52 write correctness issues, but
blocked the result commit because the README experiment index was missing the
`Codex/Codex/Codex` provenance tag and the base64 sizing guard still accepted
excessive trailing padding such as `TQ===` and `TWE==`.

The README tag was added, the base64 guard now rejects excessive padding, and
tests cover those cases. Re-review approved the completed experiment with no
findings.
