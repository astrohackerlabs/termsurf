+++
status = "open"
opened = "2026-06-17"
+++

# Issue 812: Ghostboard GUI Active State

## Goal

Implement and verify Ghostboard GUI active/inactive signaling to Roamium using
`SetGuiActive`.

## Background

Issue 810 classified this as a `Highly likely` Ghostboard gap. `SetGuiActive` is
GUI-owned state: webtui's direct Roamium socket cannot know whether the macOS
app or window is active. Wezboard sends app/window activation state, while
Ghostboard did not show an equivalent runtime path during the audit.

## Analysis

The work should identify the correct Ghostty/AppKit activation and deactivation
hooks, translate them into TermSurf `SetGuiActive` messages for relevant browser
tabs or servers, and verify Roamium receives the state transitions.

Verification should include:

- app activation sends active state;
- app deactivation sends inactive state;
- window focus changes do not produce stale or duplicate state;
- browser input/focus behavior still works after activation changes.

## Experiments

- [Experiment 1: Wire GUI active state into Roamium](01-wire-gui-active-state.md)
  — **Designed**
