# Experiment 160: Link preview context runtime

## Description

This experiment narrows the remaining `RUNTIME-012B2B2B2B2B`
notification/link/bell gap by proving the deterministic runtime slice behind
link previews and link-specific context-menu selection.

Pinned Ghostty's `Surface.zig` link handling has three separable behaviors in
this area:

- regular detected links preview only when `link-previews = true`;
- OSC 8 hyperlinks preview when `link-previews != false`;
- right-click `context-menu` selects an existing link at the cursor position and
  returns unhandled so the app can show the native context menu.

Roastty already proves generic open-url dispatch, renderer link matching,
non-link `right-click-action`, and copied macOS hover-banner plumbing. This
experiment does not claim actual native menu display, OS URL opening, or live
mouse hover/cursor UI parity. Those remain GUI gaps until a real app walkthrough
proves them.

## Changes

- Add focused Rust unit coverage in `roastty/src/lib.rs` for:
  - `link-previews` regular-link and OSC 8 preview gating semantics;
  - right-click `context-menu` selecting a link-shaped cell range at the cursor
    position and returning unhandled;
  - right-click `context-menu` preserving an existing selection when the click
    is inside it and still returning unhandled.
- Add
  `issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py` as
  the durable Issue 805 guard for this slice.
- Split a new `RUNTIME-012B2B2B2B2B1` row out of `RUNTIME-012B2B2B2B2B` in
  `config-runtime-inventory.md`, then reduce the remaining gap row to
  `RUNTIME-012B2B2B2B2B2`.
- Update `config_runtime_inventory.py`, `config-matrix.md`, and the issue
  learnings with the new row counts and reusable finding.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml link_preview_context_runtime`
  passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/link_preview_context_runtime_parity.py`
  passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  passes and reports the updated CFG-223 counts.

The experiment passes only if the remaining gap row still explicitly lists the
unproven actual GUI/OS behaviors: OS banner/sound delivery, actual audio/dock/
border/title effects, real app link hover/cursor UI, real app link previews, and
native context/menu link flows.

## Design Review

**Reviewer:** Helmholtz the 2nd (`019eca6f-7a0c-7721-9993-6165d8e3242f`)

**Verdict:** Approved

The reviewer found that the design is narrow enough, does not overclaim GUI
parity, uses sufficient verification for this deterministic runtime slice, and
follows the Issue 805 one-experiment-at-a-time workflow. The reviewer added a
non-blocking implementation note: the Python guard should prove the behavior
against pinned Ghostty semantics, not only Roastty internals in isolation.
