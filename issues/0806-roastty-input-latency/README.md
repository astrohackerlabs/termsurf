+++
status = "open"
opened = "2026-06-16"
+++

# Issue 806: Roastty input latency

## Goal

Make Roastty respond to terminal keyboard input with normal interactive latency.
The issue is complete only after the current delay is reproduced, profiled to a
specific root cause or set of root causes, fixed, and guarded by a lightweight
regression check.

## Background

Roastty is currently unusable as an interactive terminal. In manual testing,
typed characters or commands can take roughly 50 seconds to visibly register in
the terminal window, including when launching the `ReleaseLocal` app bundle.
This delay is far beyond normal debug-build overhead and must be treated as a
blocking correctness and performance bug.

Recent inspection suggests the basic keyboard path should queue input quickly:
AppKit receives `keyDown`, Swift calls `roastty_surface_key`, Rust encodes the
key event, and the termio worker queue is polled with a short timeout. That
makes the most likely failure area downstream of initial key handling: termio
event draining, display-link/app tick scheduling, terminal rendering,
presentation, or lock contention between those components. This is only a
working hypothesis; the issue must be solved from measured evidence, not
assumption.

The incorrect debug-build banner in `ReleaseLocal` was a separate build-mode
reporting bug. It does not explain a multi-second or multi-tens-of-seconds input
delay.

## Analysis

The first experiment should establish a controlled reproduction of the observed
latency. It should record timestamps at each major stage of the input-to-screen
pipeline so the delay can be localized before attempting a fix.

The profiling work should cover at least:

- Swift `keyDown` entry and return from `roastty_surface_key`
- Rust key encoding and worker queueing
- termio worker command receipt and PTY write
- PTY read/output arrival
- `tick_termio` event draining
- terminal state mutation
- `present_live` begin/end
- display-link or app-tick cadence
- main-thread stalls, long locks, and expensive render/present work

Useful tools may include timestamp trace logs, `sample`, `spindump`, Instruments
Time Profiler, macOS signposts, and narrowly scoped automated input tests. Trace
logging must be designed carefully so the measurement path does not become the
source of the latency.

## Proposed Solution

Work one experiment at a time:

1. Reproduce and measure the delay in a controlled `ReleaseLocal` run. The
   reproduction should demonstrate a visible or measured delay greater than 30
   seconds from keyboard input to terminal effect, or document why the manual
   observation cannot be reproduced.
2. Profile the reproduced delay until the blocking subsystem is identified with
   evidence.
3. Fix the proven root cause or root causes. Do not perform a broad rewrite or
   switch implementation strategies unless profiling shows that the current
   architecture is the root cause.
4. Add a lightweight regression guard that verifies local shell input/output
   latency stays within a practical budget without creating a slow exhaustive
   GUI suite.

The regression guard should be intentionally small: launch a controlled Roastty
instance, inject minimal keyboard input or write through the same path used by
the GUI, observe terminal output, and fail if the round trip exceeds a generous
CI/VM-safe timeout. Broader Ghostty parity testing belongs in separate parity
work.

## Experiments

- [Experiment 1: Measure live input latency](01-measure-live-input-latency.md) —
  **Pass**
