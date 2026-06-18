+++
status = "open"
opened = "2026-06-17"
+++

# Issue 815: Ghostboard HelloReply Configuration

## Goal

Populate and verify Ghostboard `HelloReply` data needed by webtui, especially
homepage and browser-list configuration.

## Background

Issue 810 classified this as a `Highly likely` gap. Ghostboard replies to
`HelloRequest`, but the audit found likely missing homepage and browser-list
configuration needed by `web`.

## Analysis

The work should compare Wezboard's `HelloReply` behavior, current webtui
expectations, and Ghostboard's config sources. The implementation should avoid
inventing parallel config semantics if Ghostty/Ghostboard already has a suitable
config path.

Verification should include:

- `HelloReply` includes the expected default browser list;
- `HelloReply` includes the configured homepage or documented default;
- webtui consumes the reply and displays/uses the values correctly;
- missing or invalid config falls back deterministically.

## Experiments

- [Experiment 1: Send deterministic HelloReply defaults](01-send-deterministic-hello-reply-defaults.md)
  — **Pass**
