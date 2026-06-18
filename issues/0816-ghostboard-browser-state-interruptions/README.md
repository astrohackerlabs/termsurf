+++
status = "open"
opened = "2026-06-17"
+++

# Issue 816: Ghostboard Browser State and Interruption Walkthrough

## Goal

Prove or reject the medium-likelihood browser-state and interruption-flow gaps
from Issue 810.

## Background

Issue 810 grouped these as `Maybe` findings. Static or partial evidence exists
for many paths through direct Roamium sockets, but Ghostboard runtime proof is
missing for the full walkthrough.

Covered behaviors include:

- loading state;
- page title;
- hover target URL;
- console messages;
- JavaScript dialogs;
- HTTP auth;
- renderer crash recovery;
- color scheme;
- target blank;
- refresh/reload;
- copy-current-URL;
- default white page background.

## Analysis

This issue should start as a walkthrough and regression-design issue. It should
only fix app code after a focused experiment proves a specific missing behavior.
Because many flows are engine- or webtui-owned, each finding must identify the
owning component before any fix.

## Experiments

- [Experiment 1: Prove direct browser state smoke](01-prove-direct-browser-state-smoke.md)
  — **Partial** (initial load reports `progress`/`done` but not literal
  `loading`)
- [Experiment 2: Fix initial loading-state start](02-fix-initial-loading-state-start.md)
  — **Pass**
- [Experiment 3: Prove JavaScript dialog runtime flow](03-prove-javascript-dialogs.md)
  — **Pass**
- [Experiment 4: Prove HTTP auth runtime flow](04-prove-http-auth-runtime-flow.md)
  — **Pass**
- [Experiment 5: Prove renderer crash recovery](05-prove-renderer-crash-recovery.md)
  — **Pass**
- [Experiment 6: Prove runtime color scheme](06-prove-runtime-color-scheme.md) —
  **Pass**
- [Experiment 7: Prove copy-current-URL](07-prove-copy-current-url.md) —
  **Pass**
