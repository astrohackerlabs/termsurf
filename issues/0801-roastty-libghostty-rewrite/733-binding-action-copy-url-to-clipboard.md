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

# Experiment 733: Binding Action Copy URL To Clipboard

## Description

Upstream Ghostty's `copy_url_to_clipboard` binding copies the URL under the
mouse cursor to the standard clipboard. It supports both renderer-detected regex
links and OSC 8 hyperlinks.

Roastty already exposes OSC 8 hyperlink storage through terminal grid refs and
already has a surface mouse position, viewport geometry conversion, and standard
clipboard write callback. This experiment adds the OSC 8 hyperlink path for
`copy_url_to_clipboard` as the next narrow binding-action parity step.

Renderer regex URL detection remains out of scope because Roastty does not yet
have the upstream renderer link-scanning state that Ghostty uses for regex
links. That behavior should land in a later renderer/link-detection experiment.

## Changes

- `roastty/src/lib.rs`
  - Add a parsed binding-action variant, or equivalent handling, for
    `copy_url_to_clipboard`.
  - Extend `parse_binding_action` to accept parameterless
    `copy_url_to_clipboard`.
  - Reject `copy_url_to_clipboard:` and non-empty parameters such as
    `copy_url_to_clipboard:now`.
  - Add a surface helper that:
    - returns `false` for null/detached surfaces, missing workers, missing
      clipboard callbacks, missing mouse positions, invalid/out-of-viewport
      mouse positions, cells without OSC 8 hyperlink URIs, invalid C-string
      conversion, or failed terminal access;
    - converts the current mouse pixel position to a viewport cell using the
      existing surface mouse-report geometry;
    - reads the terminal grid ref at that viewport cell and extracts its OSC 8
      hyperlink URI;
    - writes one standard clipboard item with MIME type `text/plain` and the
      hyperlink URI data;
    - passes `false` for the clipboard confirmation flag, matching existing
      write-only clipboard actions.
  - Preserve the existing `copy_to_clipboard` selection behavior.

- `roastty/tests/abi_harness.c`
  - Add malformed `copy_url_to_clipboard` parser rejection checks.
  - Add valid no-callback coverage returning `false`.

- Tests in `roastty/src/lib.rs`
  - Cover parser false paths for empty-colon and non-empty parameters.
  - Cover null, detached, no-worker, no-mouse, out-of-viewport, no-link, and
    missing-callback cases returning `false`.
  - Cover a successful OSC 8 hyperlink copy from the current mouse cell to the
    standard clipboard with one `text/plain` entry and no confirmation.
  - Cover copying the URI when the mouse is on any cell inside the OSC 8
    hyperlink text, not only the first cell.
  - Cover that existing `copy_to_clipboard` selection tests still pass.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty copy_url_to_clipboard -- --nocapture --test-threads=1`
- `cargo test -p roastty copy_to_clipboard -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 733 design and approved the scoped first pass. The
review found no blockers with supporting only OSC 8 hyperlinks under the current
mouse cell while explicitly deferring renderer-detected regex URLs until Roastty
has the upstream-style renderer link-scanning state.

The review also confirmed that the parser false paths, null/detached/no-worker,
no-mouse, out-of-viewport, no-link, missing-callback cases, clipboard
MIME/confirmation behavior, and existing selection-copy regression verification
are covered by the plan.

## Result

**Result:** Pass

Experiment 733 added parameterless `copy_url_to_clipboard` support for OSC 8
hyperlinks under the current mouse cell. The binding converts the surface mouse
pixel position to a viewport cell, reads the terminal grid ref at that cell,
extracts its OSC 8 hyperlink URI, and writes one standard clipboard item with
MIME type `text/plain` and `confirm = false`.

The action returns `false` for null surfaces, detached surfaces, missing
workers, missing mouse positions, out-of-viewport positions, cells without OSC 8
hyperlinks, and missing clipboard write callbacks. The parser rejects
`copy_url_to_clipboard:` and `copy_url_to_clipboard:now`.

Renderer-detected regex URLs remain out of scope for this experiment because
Roastty still lacks the renderer link-scanning state Ghostty uses for that path.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty copy_url_to_clipboard -- --nocapture --test-threads=1`
  - 2 passed
- `cargo test -p roastty copy_to_clipboard -- --nocapture --test-threads=1`
  - 2 passed
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
  - 112 passed
- `cargo test -p roastty --test abi_harness`
  - 1 passed
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Roastty now covers the OSC 8 half of upstream `copy_url_to_clipboard` with the
same standard clipboard shape used by existing write-only clipboard actions. The
remaining URL-copy parity gap is renderer-detected regex links, which should
wait for the renderer/link-detection state rather than being folded into this
binding parser step.

## Completion Review

Codex reviewed the completed Experiment 733 result and implementation diff. The
first completion-review attempt correctly blocked because the implementation
diff was not visible to the reviewer. The review was rerun with both the
experiment file and explicit diff context.

With the diff available, Codex found no implementation blockers. The review
confirmed that the binding is parameterless, malformed parser inputs are
rejected, OSC 8 lookup uses the current mouse cell, successful copies write one
standard `text/plain` clipboard item with `confirm = false`, false paths are
covered, and the recorded result and verification match the implementation.
