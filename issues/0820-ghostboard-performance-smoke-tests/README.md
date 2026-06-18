+++
status = "closed"
opened = "2026-06-17"
closed = "2026-06-18"
+++

# Issue 820: Ghostboard Performance Smoke Tests

## Goal

Create lightweight Ghostboard performance and repeated-run smoke tests after the
functional parity gaps are bounded.

## Background

Issue 810 grouped performance methodology as a `Maybe` finding. Old CEF and XPC
performance bugs do not directly apply to current CALayerHost/Roamium
architecture, but the historical archive showed that performance regressions can
hide behind single passing runs.

## Analysis

This issue should avoid building a large benchmark suite prematurely. The goal
is a small set of durable smoke tests that catch obvious regressions without
making ordinary testing too slow.

Candidate coverage:

- repeated browser startup;
- resize and split responsiveness;
- scroll smoothness;
- mouse move responsiveness;
- CPU use when idle;
- simple frame/update latency markers where available.

The final design should separate fast CI-suitable checks from slower diagnostic
benchmarks.

## Experiments

- [Experiment 1: Add bounded performance smoke runner](01-add-bounded-performance-smoke-runner.md)
  — **Partial**
- [Experiment 2: Unblock pointer-dependent diagnostics](02-unblock-pointer-dependent-diagnostics.md)
  — **Partial**
- [Experiment 3: Add non-pointer performance diagnostics](03-add-non-pointer-performance-diagnostics.md)
  — **Pass**

## Conclusion

Issue 820 is closed. Ghostboard now has a lightweight performance smoke shape
with two profiles:

- `scripts/ghostboard-performance-smoke.sh --fast` runs three repeated
  resolver-only startup rows for a quick regression guard.
- `scripts/ghostboard-performance-smoke.sh --diagnostic` runs the same startup
  rows plus non-pointer window-resize and split responsiveness rows, each under
  bounded-run time caps.

The diagnostic split row uses Ghostboard's existing `OpenSplit` protocol path
instead of pointer or keyboard focus automation, so the smoke remains reliable
inside this macOS VM. Pointer-dependent mouse, scroll, and browser-input
performance rows are intentionally excluded from the default diagnostic profile
because Experiment 2 proved VM click delivery still fails before those rows can
measure performance.
