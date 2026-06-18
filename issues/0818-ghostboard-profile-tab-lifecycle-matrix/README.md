+++
status = "open"
opened = "2026-06-17"
+++

# Issue 818: Ghostboard Profile, Tab, and Lifecycle Matrix

## Goal

Design and run a focused Ghostboard matrix for multi-profile, multi-pane,
multi-tab, DevTools, reconnect, close/reopen, and process cleanup behavior.

## Background

Issue 810 grouped profile, tab, and process lifecycle as a `Maybe` finding.
Current Ghostboard has credible code shape for pane, server, tab, profile, and
DevTools state, but the full runtime matrix is not proven.

The matrix should cover:

- multi-profile isolation;
- multi-pane routing;
- multi-tab routing;
- warm reconnect;
- server reuse;
- close/reopen behavior;
- stale process cleanup;
- DevTools target lookup;
- profile display or user-visible profile identity.

## Analysis

This issue should prove the lifecycle invariants before making fixes. Tests
should include enough logging or screenshots to distinguish wrong pane routing,
wrong profile routing, stale tab lookup, duplicate server spawn, and premature
process exit.

## Experiments

- [Experiment 1: Establish lifecycle baseline](01-establish-lifecycle-baseline.md)
  — **Partial**
- [Experiment 2: Prove multi-profile isolation](02-prove-multi-profile-isolation.md)
  — **Pass**
- [Experiment 3: Prove same-profile server reuse and cleanup](03-prove-same-profile-server-reuse-cleanup.md)
  — **Partial**
- [Experiment 4: Fix native tab-close TermSurf cleanup](04-fix-native-tab-close-termsurf-cleanup.md)
  — **Pass**
- [Experiment 5: Prove two-browser split-pane routing](05-prove-two-browser-split-routing.md)
  — **Pass**
- [Experiment 6: Prove TUI disconnect reconnect](06-prove-tui-disconnect-reconnect.md)
  — **Pass**
