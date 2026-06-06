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

# Experiment 737: Binding Action Write Screen File Paste

## Description

Experiment 736 added `write_screen_file:copy` and refactored write-file helpers
around explicit targets. The screen target can now share the paste behavior
implemented for `write_selection_file:paste`: write formatted content to a
retained temporary file and queue the canonical file path into the terminal.

This experiment adds `write_screen_file:paste`, including plain/vt/html formats.
It keeps `write_screen_file:open`, `write_selection_file:open`, and
`write_scrollback_file` out of scope.

## Changes

- `roastty/src/lib.rs`
  - Extend `write_screen_file` parsing to accept `paste`, `paste,plain`,
    `paste,vt`, and `paste,html`.
  - Reuse the existing target-aware write-file helper and paste branch so screen
    paste writes `screen.txt` / `screen.html`, retains the temp directory, and
    queues exactly the canonical path bytes with no trailing newline or NUL.
  - Preserve the readonly gate for paste: return `false` before creating a temp
    file or queueing bytes when the surface is readonly.
  - Preserve queue-failure behavior: return `false` and surface the worker error
    if the queued write fails.
  - Keep rejecting `write_screen_file:open` and malformed screen-file forms.

- `roastty/tests/abi_harness.c`
  - Move the valid `write_screen_file:paste*` forms from rejected parser
    coverage into valid no-worker / no-callback false-path coverage.
  - Keep `write_screen_file:open` and malformed forms rejected.

- Tests in `roastty/src/lib.rs`
  - Cover `write_screen_file:paste`, `paste,plain`, `paste,vt`, and `paste,html`
    writing the active screen to the expected temp-file extension, retaining the
    directory, and queueing exactly the canonical path bytes to the child
    process.
  - Assert the written file contents match the active-screen formatter output
    for each requested format with unwrap enabled and trim disabled.
  - Cover that screen paste does not require an active selection.
  - Cover readonly returns `false` before creating/retaining a temp file.
  - Cover queue-failure returns `false`.
  - Keep existing `write_screen_file:copy` and `write_selection_file` tests
    passing.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty write_screen_file -- --nocapture --test-threads=1`
- `cargo test -p roastty write_selection_file -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 737 design and found no technical blockers. The
review approved the screen-paste-only scope, deferring open, selection-open, and
scrollback support; reuse of the target-aware helper; exact canonical path byte
queueing; readonly and queue-failure behavior; parser coverage for newly valid
paste forms; and tests covering formatter contents, no-selection behavior,
temp-file lifetime, and write-file regressions.

The review required recording `[review.design]` frontmatter, this review
section, and the README tuple before the plan commit. Those workflow records are
now present.

## Result

**Result:** Pass

Experiment 737 added `write_screen_file:paste` support. The screen-file parser
now accepts `paste`, `paste,plain`, `paste,vt`, and `paste,html` alongside the
existing screen-copy forms. Malformed paste forms, unsupported formats,
`write_screen_file:open`, `write_selection_file:open`, and scrollback-file
actions remain out of scope.

Screen paste reuses the target-aware write-file helper and paste branch: Roastty
formats the active screen with no active selection required, writes `screen.txt`
or `screen.html` in a retained temporary directory, and queues exactly the
canonical file path bytes to the terminal worker with no trailing newline or
NUL. Readonly mode returns `false` before creating or retaining a temp file, and
worker queue failure returns `false`.

Existing `write_screen_file:copy` and `write_selection_file` copy/paste behavior
passed regression coverage after the parser scope change.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty write_screen_file -- --nocapture --test-threads=1`
  - 4 passed
- `cargo test -p roastty write_selection_file -- --nocapture --test-threads=1`
  - 5 passed
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
  - 121 passed
- `cargo test -p roastty --test abi_harness`
  - 1 passed
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

The screen-file target now supports both copy and paste actions with the same
temp-file lifetime and exact path semantics as the selection-file target. The
remaining write-file surface is the OS open action and scrollback target
support; open still needs a runtime URL/open integration decision before it can
be implemented faithfully.

## Completion Review

Codex reviewed the completed Experiment 737 result and implementation diff. It
found no implementation blockers.

The review confirmed that the parser accepts the intended screen paste forms and
keeps malformed/open forms rejected, readonly returns `false` before retaining
temp directories or queueing, queue failure returns `false`, the implementation
queues `path.as_bytes()` with no newline or NUL, and tests cover formatter
contents, no-selection behavior, retained readable paths, and existing
screen/selection write-file regressions.
