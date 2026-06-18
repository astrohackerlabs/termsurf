+++
status = "open"
opened = "2026-06-17"
+++

# Issue 811: Ghostboard Cursor Feedback

## Goal

Implement and verify Ghostboard browser cursor feedback so links, text fields,
and ordinary page regions show the correct visible cursor.

## Background

Issue 810 classified this as a `Highly likely` Ghostboard gap. Roamium emits
`CursorChanged`, the protocol defines `CursorChanged`, and Wezboard has GUI-side
cursor handling. Current Ghostboard evidence shows the message name exists for
logging, but no runtime path applies browser cursor updates in the GUI.

Relevant Issue 810 findings:

- Experiment 3: `CursorChanged` is GUI-owned and cannot be satisfied by webtui's
  direct Roamium socket.
- Experiment 11: historical mouse work reinforced cursor appearance sync.
- Experiment 12: `0324-cursor-feedback` directly maps to the current gap.

## Analysis

The work should prove the current behavior first, then implement the smallest
Ghostboard-side path that consumes `CursorChanged`, stores cursor state by pane
or tab, and applies the corresponding AppKit cursor over browser content.

Verification should include at least:

- hover over a link shows a pointing-hand cursor;
- hover over selectable text or an input shows an I-beam cursor;
- hover over ordinary page background returns to the default cursor;
- cursor state follows pane/tab focus and does not leak between browser panes.

## Experiments

- [Experiment 1: Wire browser cursor updates into AppKit](01-wire-browser-cursor-updates.md)
  — **Pass**
