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

# Experiment 711: Binding Action Scroll Row

## Description

Experiment 710 added finite `scroll_page_fractional:<f32>` binding-action
support by translating a fraction of visible rows into a signed viewport delta.
Upstream Ghostty's `performBindingAction` also supports `scroll_to_row:<usize>`,
an absolute scroll action used by the macOS scroll view while the user drags the
scrollbar:

- row `0` scrolls to the top of history;
- rows at or beyond the active viewport offset scroll to the active bottom;
- intermediate rows set the viewport top to that absolute row;
- `+N` is accepted by Zig's decimal `usize` parser;
- negative, empty, whitespace, malformed, extra-colon, or out-of-range values
  are invalid;
- the action returns `true` when performed on an attached surface.

Roastty already has the underlying `PageList::scroll_to_row(row)` primitive, but
the terminal and binding-action layers do not expose it yet. This experiment
adds only the absolute row parser, a terminal/surface wrapper, and the
`scroll_to_row:<usize>` binding-action path.

This does not implement `clear_screen`, `scroll_to_selection`, prompt jumps,
search actions, clipboard actions, cursor-key actions, full keybind
storage/lookup, or app-scoped actions.

## Changes

- `roastty/src/lib.rs`
  - Add a small ASCII decimal `usize` parser that mirrors upstream
    `std.fmt.parseInt(usize, value, 10)` for this action: accept an optional
    leading `+`, require at least one digit, reject `-`, whitespace, trailing
    bytes, and values outside the local `usize` range.
  - Extend the internal parsed binding-action enum with `ScrollToRow(usize)`.
  - Extend `parse_binding_action` to accept `scroll_to_row:<usize>` and reject
    missing, empty, malformed, whitespace, negative, extra-colon, and
    out-of-range parameters.
  - Add/use a surface helper that locks the active termio worker, scrolls the
    terminal viewport to the parsed absolute row, and requests a render.
  - Return `true` for attached parsed row-scroll actions, even when no termio
    worker exists, matching action-consumed semantics.
  - Return `false` for null or detached surfaces.
  - Keep split, close, `text:`, `csi:`, `esc:`, `reset`, top/bottom scroll, page
    up/down, line-scroll, and fractional-scroll semantics unchanged.

- `roastty/src/terminal/terminal.rs`
  - Add a small `Terminal::scroll_viewport_to_row(row)` wrapper around the
    active screen/page-list row scroll primitive.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage that malformed, negative, and overflowing
    row-scroll forms are rejected and representative zero, positive, and
    explicit-plus forms can be invoked.

- Tests in `roastty/src/lib.rs`
  - Cover invalid forms returning false: missing parameter, empty parameter,
    whitespace, malformed bytes, negative values, extra colon, and values
    outside the local `usize` range.
  - Cover null and detached surfaces returning false.
  - Cover attached no-worker surfaces returning true without side effects.
  - Cover worker-backed `scroll_to_row:0` moving to the top of history.
  - Cover worker-backed intermediate rows moving the viewport to the exact
    absolute row.
  - Cover worker-backed rows at or beyond the active viewport offset moving to
    the active bottom.
  - Cover explicit-plus row syntax, such as `scroll_to_row:+1`.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty binding_action -- --nocapture`
- `cargo test -p roastty scroll_to_row -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 711 design and approved it technically. The review
confirmed that `scroll_to_row:<usize>` is an appropriately scoped continuation
of the binding-action scroll work, the upstream absolute-row semantics are
represented correctly, and the parser plan matches Zig's unsigned integer
behavior, including explicit leading `+` acceptance and rejection of negative,
empty, whitespace, malformed, and out-of-range values.

The proposed implementation path was accepted: expose the existing active
screen/page-list row-scroll primitive through a small terminal wrapper, call it
from an attached-surface binding-action helper, and keep no-worker actions
consumed without side effects. The proposed tests were accepted as sufficient
for parser rejection, null/detached/no-worker behavior, top/intermediate/active
endpoint movement, explicit-plus syntax, ABI smoke coverage, and prior-action
regression coverage.

The only required fix before plan commit was workflow provenance: adding the
design-review frontmatter, recording this review section, and updating the
README provenance tuple to `Codex/Codex/-`. The review also suggested including
negative or overflow rejection in the C ABI smoke coverage if convenient, so the
plan now includes both.

## Result

**Result:** Pass

Implemented `scroll_to_row:<usize>` binding-action support for attached
surfaces. `parse_binding_action` now accepts absolute row parameters with
optional leading `+`, rejects missing, empty, whitespace, negative, malformed,
extra-colon, and overflow values, and stores the parsed row as
`ScrollToRow(usize)`.

The terminal stack now exposes the existing page-list absolute row-scroll
primitive through `Screen::scroll_to_row` and
`Terminal::scroll_viewport_to_row`. The surface helper locks the active termio
worker, moves the viewport to the parsed absolute row, and requests a render.
Attached surfaces consume parsed row-scroll actions even when no worker exists;
null and detached surfaces still return `false`.

Verification covered parser false paths, attached no-worker consumption,
null/detached rejection, top-row movement, intermediate absolute row movement,
active-boundary and beyond-active clamping, explicit-plus syntax, C ABI smoke
coverage, and previous binding-action behavior. A parallel
`cargo test -p roastty binding_action -- --nocapture` run hit an unrelated
PTY-backed text-test race and poisoned the shared test lock after all new
`scroll_to_row` tests had already passed; the same binding-action subset passed
serially with `--test-threads=1`.

Successful commands run:

- `cargo fmt -p roastty`
- `cargo test -p roastty scroll_to_row -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

Also run:

- `cargo test -p roastty binding_action -- --nocapture` — failed in
  `surface_binding_action_text_decoded_escapes_reach_child_pty` and then
  poisoned the shared PTY test lock for later PTY-backed tests. All new
  `scroll_to_row` tests passed in that run, and the same subset passed serially
  with `--test-threads=1`.

## Conclusion

`scroll_to_row:<usize>` can use the local page-list row-scroll primitive without
new terminal state. The binding-action scroll family now covers top, bottom,
absolute row, page up/down, signed lines, and finite fractional pages. The next
small binding-action slice can move to another finite action such as
`scroll_to_selection`, while prompt jumping and clear-screen behavior remain
higher risk because they depend on selection or terminal/shell-integration
semantics.

## Completion Review

Codex reviewed the completed Experiment 711 diff and found no code correctness
blockers. The review approved the `parse_usize_ascii` behavior as matching the
documented upstream semantics: optional leading `+`, at least one digit, checked
overflow, and rejection of negative, whitespace, malformed, and extra-colon
forms.

The review also approved the dispatcher and viewport path: null and detached
surfaces return `false`, attached no-worker surfaces consume the action without
side effects, and worker-backed surfaces route through narrow screen/terminal
wrappers to the existing page-list row-scroll primitive. Test coverage was
accepted for parser false paths, ABI smoke coverage, no-worker/null/detached
behavior, exact top/intermediate rows, active-boundary clamping, beyond-active
clamping, and prior binding-action behavior.

The only required fix before result commit was workflow provenance: adding the
`[review.result]` frontmatter, recording this completion-review section,
updating the README provenance tuple to `Codex/Codex/Codex`, and making the
failed parallel binding-action verification attempt explicit in the result
record.
