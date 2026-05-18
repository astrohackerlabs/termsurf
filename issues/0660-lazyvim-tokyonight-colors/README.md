+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 660: LazyVim Tokyo Night Mode Colors

Match the editor submode indicator colors to LazyVim's Tokyo Night lualine
palette.

## Problem

The URL bar and command bar submode indicators (NORMAL, INSERT, VISUAL, SEARCH)
use a single color — purple for the URL bar, yellow for the command bar. In
LazyVim with Tokyo Night, each vim mode has its own distinct color. The TUI
should match this convention so the submode is recognizable at a glance by color
alone.

## Solution

Add BLUE and GREEN to the Tokyo Night palette constants. Color each submode
indicator to match LazyVim's lualine:

| Submode | Color  | Hex       | Status      |
| ------- | ------ | --------- | ----------- |
| Normal  | Blue   | `#7aa2f7` | New         |
| Insert  | Green  | `#9ece6a` | New         |
| Visual  | Purple | `#bb9af7` | Already set |
| Search  | Yellow | `#e0af68` | Already set |

### Changes

In `tui/src/main.rs`:

1. **Add palette constants.**

   ```rust
   const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
   const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
   ```

2. **Color submode indicators per mode.** In both the URL bar and command bar
   rendering branches, set the submode label color based on the editor mode
   instead of using a single color:
   - `EditorMode::Normal` → BLUE
   - `EditorMode::Insert` → GREEN
   - `EditorMode::Visual` → PURPLE
   - `EditorMode::Search` → YELLOW

## Experiment 1: Per-mode submode colors

### Hypothesis

Adding BLUE and GREEN palette constants and a `submode_color` helper function
will let both the URL bar and command bar color their submode indicators per
editor mode, matching LazyVim's Tokyo Night lualine palette.

### Changes

In `tui/src/main.rs`:

1. **Add palette constants** after the existing PURPLE and YELLOW:

   ```rust
   const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
   const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
   ```

2. **Add `submode_color` helper** that maps an `EditorMode` to its Tokyo Night
   color:

   ```rust
   fn submode_color(mode: &EditorMode) -> Color {
       match mode {
           EditorMode::Normal => BLUE,
           EditorMode::Insert => GREEN,
           EditorMode::Visual => PURPLE,
           EditorMode::Search => YELLOW,
       }
   }
   ```

3. **Command bar submode indicator** (line ~506) — replace hardcoded `YELLOW`
   with `submode_color(&cmd_state.mode)`.

4. **URL bar submode indicator** (line ~548) — replace hardcoded `PURPLE` with
   `submode_color(&editor_state.mode)`.

### Test

1. Launch TUI, press `⌃esc` to Control, press `i` — URL bar submode says INSERT
   in **green**
2. Press `Esc` — URL bar submode says NORMAL in **blue**
3. Press `v` — URL bar submode says VISUAL in **purple**
4. Press `/` — URL bar submode says SEARCH in **yellow**
5. Press `⌃esc` to Control, press `:` — command bar submode says INSERT in
   **green**
6. Press `Esc` — command bar submode says NORMAL in **blue**
7. Each mode has a distinct, recognizable color matching LazyVim's convention

### Result

Pass. Each editor submode has its own distinct color in both the URL bar and
command bar: Normal (blue), Insert (green), Visual (purple), Search (yellow).
Colors match LazyVim's Tokyo Night lualine palette.

## Conclusion

One experiment delivered per-mode submode colors matching LazyVim's Tokyo Night
lualine palette. Two new palette constants (BLUE `#7aa2f7`, GREEN `#9ece6a`) and
a `submode_color` helper function color each submode indicator by editor mode in
both the URL bar and command bar. Normal is blue, Insert is green, Visual is
purple, Search is yellow — recognizable at a glance.
