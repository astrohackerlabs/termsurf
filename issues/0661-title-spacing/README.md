+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 661: Title Spacing

Remove padding spaces from block title labels across the TUI.

## Problem

Every block title in the TUI has manual padding spaces on both sides: `" URL "`,
`" COMMAND "`, `" BROWSE "`, `" profile "`, and the submode indicators
(`" INSERT "`, etc.). This takes up extra horizontal space and may look bulky
compared to a tighter presentation.

## Solution

Remove the leading and trailing spaces from all block title labels and submode
indicators. Evaluate whether the tighter look is an improvement.

### Changes

In `tui/src/main.rs`:

1. **Command bar title.** `" COMMAND "` → `"COMMAND"`.
2. **Command bar submode indicator.** Remove the padding `Span::raw(" ")` before
   and after the submode text.
3. **URL bar title.** `" URL "` → `"URL"` (in both Edit and non-Edit branches).
4. **URL bar submode indicator.** Remove the padding `Span::raw(" ")` before and
   after the submode text.
5. **Viewport title.** `" BROWSE "` / `" CONTROL "` / `" EDIT "` / `" COMMAND "`
   → no padding.
6. **Viewport profile label.** Remove padding spaces around the profile name.

## Experiment 1: Remove title padding

### Hypothesis

Removing the manual padding spaces from all block titles will produce a tighter,
cleaner look without losing readability, since ratatui already places titles on
the border line.

### Changes

In `tui/src/main.rs`:

1. **Command bar title** (line ~522) — `" COMMAND "` → `"COMMAND"`.

2. **Command bar submode indicator** (lines ~517–519) — remove the leading
   `Span::raw(" ")` and trailing `Span::raw(" ")` from the `submode_label`
   `Line::from` vec.

3. **URL bar title in Edit mode** (line ~565) — `" URL "` → `"URL"`.

4. **URL bar submode indicator** (lines ~560–562) — same as command bar: remove
   the leading and trailing padding spans.

5. **URL bar title in non-Edit mode** (line ~587) — `" URL "` → `"URL"`.

6. **Viewport title** (lines ~607–609) — `" Viewport "` → `"Viewport"` and
   `format!(" {} ", page_title)` → `page_title.to_string()`.

7. **Viewport profile label** (lines ~602–604) — remove the trailing
   `Span::raw(" ")` and change `" \u{F007} "` to `"\u{F007} "` (remove leading
   space, keep space between icon and name).

### Test

1. Launch TUI — viewport title says `Viewport` (no padding), profile label has
   no extra spacing on the edges
2. Press `⌃esc` to Control — URL bar title says `URL` flush against the border
3. Press `i` to Edit — submode indicator has no padding spaces
4. Press `⌃esc`, press `:` — command bar title says `COMMAND`, submode indicator
   has no padding
5. All titles sit tighter against the border corners
6. Evaluate whether the tighter look is an improvement or too cramped

### Result

Pass. All titles render without padding spaces. The tighter look is cleaner and
saves horizontal space. Titles sit flush against the border corners with no loss
of readability.

## Experiment 2: Remove colon trailing space

### Hypothesis

The command bar `:` prefix uses a 2-char layout slot with `": "`. Shrinking it
to 1 char with `":"` removes the extra space between the colon and the cursor.

### Changes

In `tui/src/main.rs`:

1. **Prefix layout constraint** (line ~533) — `Constraint::Length(2)` →
   `Constraint::Length(1)`.

2. **Prefix text** (line ~535) — `": "` → `":"`.

### Test

1. Press `:` from Control — cursor appears immediately after the colon with no
   gap
2. Type text — characters flow directly after `:`
3. Evaluate whether it looks too cramped or appropriately tight

### Result

Pass. The colon and cursor sit flush with no gap, matching vim's `:` prompt
behavior.

## Conclusion

Two experiments stripped all unnecessary padding from the TUI:

1. **Title padding** — removed leading/trailing spaces from every block title
   (URL bar, command bar, viewport, profile label, submode indicators). Titles
   sit flush against border corners.

2. **Colon spacing** — shrunk the command bar `:` prefix from 2 chars to 1,
   eliminating the gap between colon and cursor.

The result is a tighter, cleaner UI with no loss of readability.
