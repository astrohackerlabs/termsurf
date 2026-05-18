+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 647: TUI Layout Restructure

## Goal

Rearrange the TUI layout so the URL bar sits at the bottom (just above the
status line) and the profile name moves from the URL bar to the viewport title.

## Current layout

```
┌───────────────────────────── 👤 default ─┐
│ https://example.com                      │  ← URL bar (top, 3 rows)
└──────────────────────────────────────────┘
┌─ Viewport ───────────────────────────────┐
│                                          │  ← Viewport (fill)
│                                          │
└──────────────────────────────────────────┘
<q> quit  <i> edit url  <enter> browse       CONTROL  ← Status bar (1 row)
```

- URL bar is at the top with borders (`Constraint::Length(3)`).
- Profile name (`👤 default`) is a right-aligned title on the URL bar block.
- Viewport fills the middle.
- Status bar is at the bottom (hints left, mode label right).

### Code

`tui/src/main.rs:303-308` — vertical layout:

```rust
let layout = Layout::vertical([
    Constraint::Length(3), // URL bar (1 line + top/bottom border)
    Constraint::Min(1),    // Viewport (fill remaining)
    Constraint::Length(1), // Status bar
])
```

`tui/src/main.rs:317-321` — profile title on URL bar:

```rust
let profile_title = Line::from(vec![
    Span::raw(" \u{F007} ").style(Style::default().fg(COMMENT)),
    Span::raw(profile).style(Style::default().fg(FG)),
    Span::raw(" "),
]);
```

Used as `.title_top(profile_title.alignment(Alignment::Right))` on the URL bar
block (lines 331, 345).

`tui/src/main.rs:354-358` — viewport title (page title or "Viewport"):

```rust
let viewport_title = if page_title.is_empty() {
    " Viewport ".to_string()
} else {
    format!(" {} ", page_title)
};
```

## Target layout

```
┌─ Example Page ───────────────── 👤 default ─┐
│                                             │  ← Viewport (fill)
│                                             │
└─────────────────────────────────────────────┘
┌─────────────────────────────────────────────┐
│ https://example.com                         │  ← URL bar (bottom, 3 rows)
└─────────────────────────────────────────────┘
<q> quit  <i> edit url  <enter> browse          CONTROL  ← Status bar (1 row)
```

### Changes needed

1. **Reorder the vertical layout.** Viewport first (`Min(1)`), then URL bar
   (`Length(3)`), then status bar (`Length(1)`).

2. **Move profile name to viewport.** Remove `profile_title` from the URL bar
   block. Add it as a right-aligned title on the viewport block, next to the
   page title on the left.

3. **Update widget render order.** The render calls must match the new layout
   slot indices: viewport is `layout[0]`, URL bar is `layout[1]`, status bar
   uses `layout[2]`.

4. **Viewport inner rect.** `viewport_block.inner(layout[...])` must use the
   correct index so the overlay coordinates sent to the compositor are accurate.

## Experiments

### Experiment 1: Rearrange layout and move profile name

**Goal:** Move the URL bar below the viewport and relocate the profile name to
the viewport's top-right title.

#### Changes

All changes in `tui/src/main.rs`:

**1. Reorder the vertical layout (line 303-307).** Change from:

```rust
let layout = Layout::vertical([
    Constraint::Length(3), // URL bar (1 line + top/bottom border)
    Constraint::Min(1),    // Viewport (fill remaining)
    Constraint::Length(1), // Status bar
])
```

To:

```rust
let layout = Layout::vertical([
    Constraint::Min(1),    // Viewport (fill remaining)
    Constraint::Length(3), // URL bar (1 line + top/bottom border)
    Constraint::Length(1), // Status bar
])
```

**2. Move profile title from URL bar to viewport (lines 317-374).** Remove
`profile_title` from both URL bar blocks (UrlEdit theme block and static
Paragraph block). Add it as a right-aligned title on the viewport block. The
viewport block gets two titles: page title on the left, profile on the right.

The viewport block (line 359-364) changes from:

```rust
let viewport_block = Block::default()
    .borders(Borders::ALL)
    .title(viewport_title)
    .border_style(...)
    .title_style(...)
    .style(...);
```

To:

```rust
let viewport_block = Block::default()
    .borders(Borders::ALL)
    .title(viewport_title)
    .title_top(profile_title.alignment(Alignment::Right))
    .border_style(...)
    .title_style(...)
    .style(...);
```

**3. Update layout indices.** All render calls must use the new slot order:

- Viewport renders to `layout[0]` (was `layout[1]`)
- URL bar renders to `layout[1]` (was `layout[0]`)
- Status bar uses `layout[2]` (unchanged)
- `viewport_block.inner(layout[0])` (was `layout[1]`)

#### Verification

Run the TUI. Confirm:

- Viewport is at the top with page title left, profile name right.
- URL bar is below the viewport with borders.
- Status bar is at the bottom.
- Entering UrlEdit mode shows the editor in the bottom URL bar slot.
- Overlay coordinates are correct (viewport debug text shows expected origin and
  size).

**Result: Pass.** Layout rearranged, profile name moved to viewport title.

## Conclusion

The TUI layout is restructured. The viewport now occupies the top of the screen
with the page title on the left and the profile name on the right. The URL bar
sits below the viewport, just above the status line. This puts the browser
content front and center while keeping the URL bar accessible near the keyboard
hints.
