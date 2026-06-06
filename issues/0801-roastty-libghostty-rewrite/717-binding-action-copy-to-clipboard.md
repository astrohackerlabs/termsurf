+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 717: Binding Action Copy To Clipboard

## Description

Experiment 716 added `adjust_selection:<direction>` binding-action support.
Upstream Ghostty's `performBindingAction` also supports `copy_to_clipboard`,
which copies the active selection to the standard clipboard in one of four
formats:

- `plain`
- `vt`
- `html`
- `mixed` (default)

Roastty already has the pieces needed for a narrow standard-clipboard copy
slice:

- active selection tracking on worker-backed surfaces;
- selection formatting as plain, VT, and HTML;
- runtime clipboard write callback storage in `RoasttyRuntimeConfig`;
- `roastty_clipboard_e` and `roastty_clipboard_content_s` ABI types.

This experiment wires those pieces into
`roastty_surface_binding_action("copy_to_clipboard")`.

This does not implement paste actions, OSC 52 request allocation/completion,
selection clipboard copying, copy-on-select, copy URL/title actions,
clear-on-copy configuration, clipboard access policy prompts, or keybind
storage/lookup.

## Changes

- `roastty/src/lib.rs`
  - Add an internal `CopyToClipboardFormat` enum with `Plain`, `Vt`, `Html`, and
    `Mixed`.
  - Extend the internal parsed binding-action enum with
    `CopyToClipboard(CopyToClipboardFormat)`.
  - Extend `parse_binding_action` to accept:
    - `copy_to_clipboard` as `Mixed`;
    - `copy_to_clipboard:plain`;
    - `copy_to_clipboard:vt`;
    - `copy_to_clipboard:html`;
    - `copy_to_clipboard:mixed`.
  - Reject empty, whitespace-padded, unknown, and extra-colon parameters.
  - Add a surface helper that:
    - returns `false` for null and detached surfaces;
    - returns `false` for no-worker surfaces;
    - returns `false` when the worker-backed terminal has no active selection;
    - returns `false` when the runtime has no `write_clipboard_cb`;
    - formats the active selection with unwrap enabled and trim enabled,
      matching upstream `copySelectionToClipboards`, which sets `unwrap = true`
      and trims with the clipboard trim configuration; Roastty does not expose
      that config yet, so this experiment uses the existing selection text
      behavior's `trim = true` default;
    - invokes the runtime write callback for the standard clipboard;
    - passes `confirm = false`, matching upstream's direct `setClipboard` call;
    - returns `true` after a callback invocation.
  - Format payloads and MIME types to match upstream:
    - `Plain`: one `text/plain` item with plain text;
    - `Vt`: one `text/plain` item with VT text;
    - `Html`: one `text/html` item with HTML;
    - `Mixed`: one callback containing two items, `text/plain` plus `text/html`,
      in that order.
  - Keep split, close, text/CSI/ESC, reset, clear-screen, scroll, prompt-jump,
    select-all, and adjust-selection action semantics unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage that malformed `copy_to_clipboard` forms are
    rejected.
  - Add no-worker coverage that valid copy forms return `false` without
    crashing.

- Tests in `roastty/src/lib.rs`
  - Cover parser false paths for empty, unknown, whitespace-padded, and
    extra-colon copy formats.
  - Cover null, detached, no-worker, no-selection, and no-write-callback
    surfaces returning `false`.
  - Cover valid no-worker parser acceptance for bare/default, `plain`, `vt`,
    `html`, and `mixed`.
  - Cover worker-backed `plain`, `vt`, `html`, and `mixed` copies invoking the
    runtime callback with the expected standard clipboard, MIME list, data
    payloads, item order, and `confirm = false`.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty copy_to_clipboard -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 717 design and found the scope otherwise sound:
standard clipboard only, no policy prompts or request machinery, `false` for
null, detached, no-worker, no-selection, and no-callback surfaces, parser
coverage for default and explicit formats, and callback assertions for MIME
types, payload order, and `confirm = false`.

The review raised one technical blocker before plan commit: clipboard payload
bytes are the core behavior, so the design needed to justify the `unwrap` and
`trim` formatting options. The plan now cites upstream
`copySelectionToClipboards`: upstream sets `unwrap = true` and trims according
to clipboard trim configuration. Roastty does not expose that config yet, so
this experiment uses the existing selection text behavior's trimmed formatting
until config parity reaches that setting.

The review also asked that mixed-format behavior be explicit. The plan now
states that mixed copy invokes one write callback containing two MIME items in
`text/plain`, then `text/html` order.

The review raised the normal workflow provenance requirement. Design-review
frontmatter and this review section are now present, and the README provenance
tuple will be updated to `Codex/Codex/-` before the plan commit. Result-review
provenance will be added only after implementation and completion review.
