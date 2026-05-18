+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 657: URL Bar Edit Mode Color

When editing the URL bar, it's too easy to not notice you're in edit mode. The
URL bar should change color to make the active editing state obvious.

## Problem

The URL bar border uses CYAN (`#7dcfff`) for both Control mode and UrlEdit mode
(`tui/src/main.rs:313-316`). There is no visual distinction between "I've
selected the URL bar" and "I'm actively typing in it." This makes it easy to
miss that you're in an editing mode (normal, insert, visual, or search) and
accidentally send keystrokes to the editor instead of navigating.

## Solution

Add a new Tokyo Night purple constant (`#bb9af7`) and use it for the URL bar
border when in UrlEdit mode. Purple is:

- Distinct from cyan (Control mode)
- Not warning-like (rules out yellow, orange, red)
- A core Tokyo Night color (used for keywords in syntax highlighting)

### Changes

In `tui/src/main.rs`:

1. Add `PURPLE` to the Tokyo Night palette constants:
   ```rust
   const PURPLE: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
   ```
2. Update the border color match to use three arms:
   ```rust
   let (url_border, viewport_border) = match mode {
       Mode::Browse => (BORDER, CYAN),
       Mode::Control => (CYAN, BORDER),
       Mode::UrlEdit => (PURPLE, BORDER),
   };
   ```

## Experiment 1: Purple URL bar border in UrlEdit mode

### Hypothesis

Adding a purple border for UrlEdit mode will make it visually obvious when the
URL bar is being edited vs merely focused.

### Test

1. Launch the TUI: `cargo run -- http://example.com`
2. Press `Esc` to enter Control mode — URL bar border should be cyan
3. Press `i` to enter UrlEdit mode — URL bar border should turn purple
4. Press `Esc` to return to Control — border should return to cyan
5. Press `Enter` to Browse — URL bar border should be the dim border color

### Result

Pass. All three modes show distinct border colors: dim (Browse), cyan (Control),
purple (UrlEdit). The purple border makes it immediately obvious when you're in
an editing mode.

## Conclusion

Purple URL bar border for UrlEdit mode works. Two-line change in
`tui/src/main.rs`.
