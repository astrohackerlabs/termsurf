+++
status = "open"
opened = "2026-06-19"
+++

# Issue 825: Frontend Status Documentation

## Goal

Update TermSurf documentation so it accurately describes the current frontend
strategy: Ghostboard is the primary front-end, Wezboard is deprecated, and
Roastty is a proof-of-concept rather than the production direction.

## Background

The repository documentation still contains older project-state language from
the Wezboard-focused period and the Roastty proof-of-concept work. That creates
confusion for future development because it can route new frontend work toward
deprecated or experimental implementations.

The current intended status is:

- **Ghostboard** is the primary TermSurf front-end.
- **Wezboard** is deprecated and should be treated as historical/reference code
  unless explicitly revived for a specific reason.
- **Roastty** is a proof-of-concept and should not be described as the primary
  production path.
- **Ghostboard Legacy** remains archived historical code that can be useful as a
  reference for solved behavior.

## Analysis

This is a documentation issue, not a frontend implementation issue. The work
should audit and update current, mutable documentation that describes TermSurf's
frontend direction, build/run workflow, active GUI, or prototype status.

Likely documents to audit include:

- root `AGENTS.md`;
- root `README.md`, if it contains frontend status or build/run guidance;
- docs that summarize early prototypes or archived frontends;
- open issue docs that describe current frontend direction;
- scripts or developer docs that still direct ordinary TermSurf work to
  Wezboard.

Closed issue records are historical and should remain immutable. They can be
referenced as history, but should not be rewritten.

## Acceptance Criteria

- Current mutable documentation identifies Ghostboard as the primary TermSurf
  front-end.
- Current mutable documentation identifies Wezboard as deprecated rather than
  active.
- Current mutable documentation identifies Roastty as a proof-of-concept rather
  than the primary production path.
- Historical Ghostboard Legacy documentation remains clear that it is archived
  reference material.
- Build/run guidance no longer centers Wezboard for current TermSurf frontend
  development unless explicitly labeled as deprecated/reference.
- Closed issue documents are not modified.
- Markdown formatting passes for edited markdown files.
- `git diff --check` passes.

## Experiments

- [Experiment 1: Update frontend status docs](01-update-frontend-status-docs.md)
  — **Designed**
