+++
status = "open"
opened = "2026-04-05"
+++

# Issue 770: Browser does not load

## Goal

Diagnose and fix why `web ryanxcharles.com` no longer opens a browser in
Wezboard.

## Background

Running `web ryanxcharles.com` in Wezboard does not display a browser. The
browser was working prior to recent development.

## Diagnostic plan

1. Run Roamium manually with the same arguments the GUI passes to it, and
   capture its stderr/stdout.
2. Check whether the GUI logs show any clues (run with `WEZTERM_LOG=info`).
3. Investigate from findings.
