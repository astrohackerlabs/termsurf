+++
status = "closed"
opened = "2026-06-13"
closed = "2026-06-19"
+++

# Issue 803: Roastty Debug Overlay

## Goal

Implement or intentionally retire the remaining optional Debug `Overlay` feature
that was deferred from Issue 802, and prove the copied Roastty macOS app handles
the chosen behavior correctly.

## Background

Issue 802 closed the copied, lightly renamed Ghostty macOS app port to
`libroastty`. Its final roadmap left exactly one unchecked item:

- Debug `Overlay` (optional)

That item was explicitly non-blocking for Issue 802, because the required app
surface had already been proven through the live renderer, config/theme,
keybinding, native-key, shell, terminal, clipboard, palette, update, and hosted
app integration experiments. This issue tracks the optional follow-up separately
so the closed Issue 802 record stays immutable.

## Analysis

The first experiment should identify what Ghostty's Debug `Overlay` means in the
current upstream app and renderer, then decide whether Roastty should implement
the same behavior, expose an equivalent diagnostic surface, or document a
deliberate no-op if the upstream feature is not applicable to the copied macOS
app.

The investigation should avoid broad renderer churn. It should focus on:

- the upstream Ghostty code path and user-facing trigger for the debug overlay;
- the corresponding Roastty/libroastty renderer or app boundary;
- whether any existing Roastty inspector, overlay image, or text-overlay code is
  related or merely name-adjacent;
- concrete verification in the copied macOS app, preferably through an automated
  hosted test or a narrowly scoped renderer test.

As with other current issues, experiments should be created one at a time. Do
not add the `## Experiments` index until Experiment 1 is designed.

## Conclusion

Roastty is no longer active work, so this optional deferred debug overlay item
is intentionally retired without experiments. If Roastty work resumes later,
open a new issue against the then-current Roastty direction instead of reviving
this placeholder.
