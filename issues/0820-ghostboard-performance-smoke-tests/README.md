+++
status = "open"
opened = "2026-06-17"
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
