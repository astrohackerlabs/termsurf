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

# Experiment 712: Binding Action Scroll Selection

## Description

Experiment 711 added `scroll_to_row:<usize>` binding-action support by exposing
the existing absolute row-scroll primitive through the surface action path.
Upstream Ghostty's `performBindingAction` also supports `scroll_to_selection`,
which scrolls the viewport to the top-left pin of the active selection:

- the action has no parameter;
- if no active selection exists, the action returns `false`;
- forward, reverse, and rectangular selections normalize to their top-left pin;
- scrolling to a selected pin in scrollback sets that pin's row as the viewport
  top;
- scrolling to a selected pin in the active viewport clamps to the active
  bottom;
- when a selection exists, the action requests a render and returns `true`.

Roastty already has active-selection storage, selection ordering, and the
page-list pin-scroll primitive. This experiment exposes the existing
selection-top-left computation and routes `scroll_to_selection` through it.

This does not implement `clear_screen`, prompt jumps, search actions, clipboard
actions, cursor-key actions, full keybind storage/lookup, frontend selection
routing, or app-scoped actions.

## Changes

- `roastty/src/terminal/page_list.rs`
  - Expose the existing `selection_top_left(selection)` helper within the
    terminal module so higher layers can reuse the same normalization logic as
    formatting and selection containment.

- `roastty/src/terminal/screen.rs`
  - Add a small `scroll_to_selection()` helper that returns `false` when no
    active selection exists or when the selection cannot be mapped to a valid
    top-left pin, and otherwise scrolls the active page-list to that pin.

- `roastty/src/terminal/terminal.rs`
  - Add `Terminal::scroll_viewport_to_selection() -> bool`, forwarding to the
    active screen helper.

- `roastty/src/lib.rs`
  - Extend the internal parsed binding-action enum with `ScrollToSelection`.
  - Extend `parse_binding_action` to accept bare `scroll_to_selection` and
    reject any parameter.
  - Add/use a surface helper that locks the active termio worker, calls the
    terminal helper, requests a render only when the terminal scrolled to an
    existing selection, and returns whether the action was performed.
  - Return `false` for null, detached, no-worker, and no-selection surfaces.
  - Keep split, close, `text:`, `csi:`, `esc:`, `reset`, top/bottom scroll,
    row-scroll, page up/down, line-scroll, and fractional-scroll semantics
    unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage that `scroll_to_selection:`,
    `scroll_to_selection:now`, and other parameterized forms are rejected, and
    that the bare action returns `false` without crashing on the no-worker
    harness surface.

- Tests in `roastty/src/lib.rs`
  - Cover invalid parameter forms returning false, including both
    `scroll_to_selection:` and `scroll_to_selection:now`.
  - Cover null, detached, no-worker, and worker-backed no-selection surfaces
    returning false.
  - Cover worker-backed forward and reverse selections in scrollback moving the
    viewport to the normalized top-left row.
  - Cover worker-backed rectangular mirrored selection moving to the normalized
    top-left row/column's row.
  - Cover a selection in the active area clamping the viewport to the active
    bottom.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty scroll_to_selection -- --nocapture`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 712 design and found the technical slice sound:
parameterless `scroll_to_selection`, `false` for null/detached/no-worker/no
selection, normalized top-left selection pin scrolling, scrollback row
targeting, and active-area clamping all match the intended upstream behavior.

The review required two clarifications before plan commit. First, the C ABI
harness plan must explicitly assert that bare `scroll_to_selection` returns
`false` on a no-worker surface without crashing, because this action depends on
terminal selection state. Second, parser coverage must explicitly include both
void-action colon forms, `scroll_to_selection:` and `scroll_to_selection:now`,
because nearby parameterized actions intentionally accept empty parameters.

Those clarifications are now incorporated in the Changes and Verification
sections. The review also required workflow provenance, so the design-review
frontmatter and this review section were recorded and the README provenance
tuple will be updated to `Codex/Codex/-` for the plan commit. Result-review
provenance will be added only after implementation and completion review.

Codex re-reviewed the updated design and found no remaining blockers. The
re-review confirmed that the no-worker ABI expectation is explicitly `false`,
both colon forms are called out in parser coverage, and the design-review
provenance plus review section are recorded. The design is approved for the plan
commit.
