# Experiment 1: Move Browser Controls Above Viewport

## Description

Move WebTUI's visible browser chrome above the browser viewport. The URL bar,
mode indicator, and keybinding/status strip should appear at the top of the
terminal pane in both Browse mode and Control mode, with the browser viewport
starting underneath them.

The current layout in `webtui/src/main.rs` allocates the viewport first, then
the URL bar and status strip. The `:viewport height <rows>` override follows the
same bottom-chrome model by placing filler between the viewport and controls.
This experiment should invert that order without changing keybinding behavior.

## Changes

Planned code changes:

1. `webtui/src/main.rs`

   - Extract the URL/status/viewport rectangle allocation into a small helper,
     so normal layout and `:viewport height <rows>` layout share the same
     top-controls invariant.
   - Change the normal layout order to:
     1. URL bar, height 3
     2. status/keybinding strip, height 1
     3. viewport, remaining height
   - Change the viewport-height override layout order to:
     1. URL bar, height 3
     2. status/keybinding strip, height 1
     3. viewport, requested rows plus viewport border
     4. filler, remaining height
   - Keep rendering behavior for Browse, Control, Edit, Command, Dialog, and
     Auth modes otherwise unchanged.
   - Keep the returned viewport inner rectangle as the authoritative overlay
     geometry sent to Ghostboard.
   - Add focused Rust tests for the layout helper:
     - default layout places URL/status at the top and viewport below;
     - viewport-height override keeps URL/status at the top and viewport below;
     - viewport-height override preserves exact requested content height when
       space allows, e.g. for `rows = 10` in an `80x30` terminal, URL
       `Rect { x: 0, y: 0, width: 80, height: 3 }`, status
       `Rect { x: 0, y: 3, width: 80, height: 1 }`, viewport outer
       `Rect { x: 0, y: 4, width: 80, height: 12 }`, and viewport inner
       `Rect { x: 1, y: 5, width: 78, height: 10 }`;
     - very small terminal heights clamp the viewport instead of panicking, and
       do not place controls below the viewport, e.g. for an `80x5` terminal
       with an oversized viewport override, URL
       `Rect { x: 0, y: 0, width: 80, height: 3 }`, status
       `Rect { x: 0, y: 3, width: 80, height: 1 }`, viewport outer
       `Rect { x: 0, y: 4, width: 80, height: 1 }`, and viewport inner
       `Rect { x: 1, y: 5, width: 78, height: 0 }`.

## Verification

Commands:

```bash
cargo fmt
git diff --check
cargo test -p webtui
cargo build -p webtui
```

Manual or captured verification:

- Record a before terminal capture or screenshot from the pre-change WebTUI
  rendering that shows the URL bar and keybinding/status strip below the
  viewport.
- Record an after terminal capture or screenshot from the changed WebTUI
  rendering that shows the URL bar at the top, status/keybindings directly below
  it, and viewport below both.
- Include layout-test output as additional geometry evidence, not as the only
  before/after capture evidence.

Pass criteria:

- Browse mode controls render above the viewport.
- Control mode controls render above the viewport.
- The returned viewport inner rectangle starts below the URL bar and
  status/keybinding strip.
- `:viewport height <rows>` preserves the same top-controls order and exact
  geometry when space allows. For `rows = 10` in an `80x30` terminal, the exact
  expected geometry is URL `Rect { x: 0, y: 0, width: 80, height: 3 }`, status
  `Rect { x: 0, y: 3, width: 80, height: 1 }`, viewport outer
  `Rect { x: 0, y: 4, width: 80, height: 12 }`, and viewport inner
  `Rect { x: 1, y: 5, width: 78, height: 10 }`.
- Small-height layouts clamp safely and keep controls above the viewport. For an
  `80x5` terminal with an oversized viewport override, the exact expected
  geometry is URL `Rect { x: 0, y: 0, width: 80, height: 3 }`, status
  `Rect { x: 0, y: 3, width: 80, height: 1 }`, viewport outer
  `Rect { x: 0, y: 4, width: 80, height: 1 }`, and viewport inner
  `Rect { x: 1, y: 5, width: 78, height: 0 }`.
- Existing mode/keybinding code paths remain behaviorally unchanged.
- Rust formatting, tests, and build pass.

## Design Review

Adversarial review, fresh-context Codex subagent:

**Verdict:** Changes required.

Findings and fixes:

- Required: `:viewport height <rows>` verification was not concrete enough.
  Fixed by requiring exact rectangle assertions for the normal and override
  layouts, including viewport inner height and the exact `80x5` small-height
  clamp behavior.
- Required: before/after capture verification did not satisfy the issue
  acceptance criterion. Fixed by requiring real before and after terminal
  captures or screenshots, with layout-test output only as additional evidence.
- Required: hygiene commands omitted `git diff --check`. Fixed by adding it to
  the verification command list.

Re-review:

**Verdict:** Changes required.

The reviewer confirmed that before/after capture verification and
`git diff --check` were fixed, but required one concrete small-height clamp
case. Fixed by adding an exact `80x5` terminal expectation to the planned tests
and pass criteria.

Second re-review:

**Verdict:** Changes required.

The reviewer required complete small-height rectangles, not only y/height and
inner height. Fixed by spelling out the full `Rect` values for the `80x30`
override case and the `80x5` clamp case.

Third re-review:

**Verdict:** Approved.

The reviewer confirmed that the small-height case now specifies exact URL,
status, viewport outer, and viewport inner rectangles with x, y, width, and
height. No required findings remain.
