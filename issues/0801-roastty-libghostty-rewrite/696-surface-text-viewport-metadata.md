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

# Experiment 696: Surface Text Viewport Metadata

## Description

Experiments 683 and 684 added surface text reads for explicit and active
selections. The C ABI already includes Ghostty-style metadata fields on
`roastty_text_s`: top-left pixel coordinates plus a flattened viewport offset
range. Roastty currently leaves those fields at the empty defaults
(`tl_px_x = -1`, `tl_px_y = -1`, `offset_start = 0`, `offset_len = 0`) even when
the selection is visible.

Upstream Ghostty uses this metadata so app/frontends can correlate returned text
with the visible viewport. This experiment populates the existing fields for
surface text reads using the attached worker terminal's selection endpoints and
the surface geometry Roastty currently exposes.

This does not implement app-level clipboard routing, copy-on-select,
selection-clear-on-typing policy, partial-selection byte maps, font-baseline
positioning, surface padding, new C ABI fields, or Swift/frontend integration.
It also does not change the allocated text ownership model.

## Changes

- `roastty/src/lib.rs`
  - Add a focused helper that computes `roastty_text_s` viewport metadata for a
    `TerminalSelection` on an attached `Surface`.
  - Derive the selection top-left and bottom-right viewport points by ordering
    the selection through the terminal, then converting the endpoint grid refs
    to `TerminalPointTag::Viewport`.
  - Mirror upstream's coarse viewport-overlap behavior:
    - if the selection bottom-right pin is before the viewport top-left pin,
      report empty/default metadata;
    - if the viewport bottom-right pin is before the selection top-left pin,
      report empty/default metadata;
    - if an overlapping selection starts before the viewport, clamp the reported
      top-left point to `(0, 0)`;
    - if an overlapping selection ends after the viewport, clamp the reported
      bottom-right point to the viewport bottom-right.
  - Return Ghostty's empty/default metadata when:
    - the selection cannot be revalidated or ordered;
    - the selection is wholly outside the visible viewport;
    - surface cell metrics are unavailable.
  - For visible selections, populate:
    - `tl_px_x` from `viewport_x * cell_width`, scaled by content scale;
    - `tl_px_y` from `viewport_y * cell_height`, scaled by content scale;
    - `offset_start` from `viewport_y * columns + viewport_x`;
    - `offset_len` from the ordered endpoint viewport span.
  - Document that `tl_px_y` is currently the cell top, not the upstream text
    baseline, because Roastty's surface state does not yet carry renderer font
    baseline metrics. Document that surface padding is treated as zero because
    `roastty_surface_size_s` does not expose padding.
  - Thread the metadata into `try_surface_selection_text` after text allocation
    so the existing text pointer/length allocation and free behavior are
    preserved.
  - Keep `roastty_surface_read_selection` and `roastty_surface_read_text`
    false/no-op behavior unchanged for null results, detached surfaces, missing
    workers, invalid selections, and failed selection formatting.

- Tests in `roastty/src/lib.rs`
  - Update existing surface text expectations so visible selections report
    non-default metadata.
  - Add focused coverage for:
    - active selection reads and explicit selection reads both reporting the
      same visible viewport metadata;
    - content scale affecting the reported pixel position;
    - wholly off-viewport selections retaining empty/default metadata;
    - partially visible selections clamping metadata to viewport bounds;
    - repeated read/free behavior keeping metadata and pointer reset semantics
      intact.

- `roastty/src/terminal/terminal.rs`
  - Add small public-to-crate helpers only if needed to expose active-screen
    viewport top-left/bottom-right grid refs and pin ordering to `lib.rs`.

- `roastty/include/roastty.h`
  - No C ABI shape change is expected because the metadata fields already exist.

- `roastty/tests/abi_harness.c`
  - No C ABI shape change is expected.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_read -- --nocapture`
- `cargo test -p roastty surface_text -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex initially blocked the design on three real issues: partial viewport
clamping was underspecified, padding was assumed even though
`roastty_surface_size_s` has no padding fields, and `tl_px_y` diverged from
upstream baseline positioning without explanation. The design was revised to
spell out overlap/clamping behavior, use only current surface geometry, treat
padding as zero, and document that Roastty reports the cell top until renderer
font baseline metrics exist on the surface.

Codex then approved the revised design for the plan commit.

## Result

**Result:** Pass.

Roastty now fills the existing `roastty_text_s` viewport metadata fields for
surface text reads when the selection overlaps the visible viewport and surface
cell metrics are available. Active selection reads and explicit selection reads
share the same path: the selected text allocation/free behavior is unchanged,
while `tl_px_x`, `tl_px_y`, `offset_start`, and `offset_len` are populated from
the attached worker terminal's ordered selection endpoints and current viewport.

Selections wholly outside the viewport keep Ghostty's empty/default metadata.
Partially visible selections clamp the reported metadata to the viewport bounds.
Pixel coordinates use the current Roastty surface geometry contract: zero
padding and cell-top Y coordinates, scaled by content scale. Baseline
positioning remains deferred until renderer font baseline metrics exist on the
surface.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_read -- --nocapture`
- `cargo test -p roastty surface_text -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

One parallel verification attempt ran `surface_text`, `abi_harness`, and
`cargo fmt --check` at the same time. `surface_text` hit a PTY timing assertion
and then poisoned its process-local test lock; rerunning
`cargo test -p roastty surface_text -- --nocapture` by itself passed.

## Conclusion

The surface text ABI now exposes the viewport correlation data its struct
already reserved. The remaining selection work is frontend behavior around
routing, clipboard/copy policy, and full presentation integration rather than
text metadata storage.

## Completion Review

Codex reviewed the staged result and found no implementation blockers. It
approved the metadata path's ownership behavior, terminal helper scope,
zero-padding and cell-top geometry contract, viewport overlap/clamping logic,
focused tests, and recorded verification. The only review finding was missing
result-review provenance; this section, the `[review.result]` frontmatter, and
the README reviewer tuple address that workflow issue.
