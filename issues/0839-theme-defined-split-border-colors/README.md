+++
status = "open"
opened = "2026-06-23"
+++

# Issue 839: Theme-Defined Split Border Colors

## Goal

Make split pane border colors theme-defined by default, so users can rely on the
selected Ghostboard theme for focused and unfocused split border colors while
retaining explicit config overrides.

When this issue is solved, a user with `theme = tokyonight` should be able to
remove these lines from `~/.config/termsurf/config` and still get the same Tokyo
Night split border colors:

```ini
focused-split-border-color = 7dcfff
unfocused-split-border-color = 565f89
```

Users must still be able to override the theme-defined values by setting
`focused-split-border-color` and `unfocused-split-border-color` directly in
their config.

## Background

Issue 823 added configurable split pane border colors:

- `focused-split-border-color`
- `unfocused-split-border-color`
- `split-border-width`

Those colors currently work as explicit user configuration, but they are not
defined by any bundled theme. This makes visually integrated split borders
depend on per-user config even when the colors are really part of the selected
theme.

Ghostboard themes are Ghostty-style config files. Each bundled theme defines a
uniform terminal color set:

- `background`
- `foreground`
- `cursor-color`
- `cursor-text`
- `selection-background`
- `selection-foreground`
- `palette = 0=...` through `palette = 15=...`

The bundled theme set currently contains 534 theme files. A local audit showed
all 534 define the full core terminal color set above. None currently define:

- `focused-split-border-color`
- `unfocused-split-border-color`
- `split-border-width`
- `split-divider-color`
- `unfocused-split-fill`

The Tokyo Night theme already contains `#7dcfff` as palette index 6 and 14. The
user's desired Tokyo Night inactive border color is `#565f89`, which is a known
Tokyo Night muted/comment color but is not present in the bundled `TokyoNight`
theme file.

## Requirements

1. Update every bundled theme so it defines:
   - `focused-split-border-color`
   - `unfocused-split-border-color`
2. For Tokyo Night themes, use the user's exact desired colors where applicable:
   - focused: `#7dcfff`
   - unfocused: `#565f89`
3. For all other themes, infer the most relevant existing colors from that
   theme's own defined colors.
4. Preserve user override behavior: explicit values in the user's config must
   still override theme-defined values.
5. Do not make `split-border-width` theme-defined unless an experiment proves
   that enabling borders by default is desired. This issue is about moving the
   color defaults into themes, not silently enabling borders for users who have
   not opted into borders.
6. Add a repeatable audit or generation script if needed so the theme updates
   can be reviewed and regenerated instead of hand-editing hundreds of files.
7. Update documentation so users understand that themes provide split border
   colors and config values override the theme.

## Analysis

There are two plausible implementation strategies:

1. **Edit every theme file directly.** Add the two border color keys to all 534
   bundled theme files. This gives complete, explicit theme data and makes
   `termsurf +show-config` reflect theme-provided values naturally. The risk is
   maintainability: future theme-pack updates may overwrite or conflict with
   local edits unless the process is scripted.
2. **Derive defaults in code from existing theme colors.** For example, use
   palette index 6 or 14 for the focused border and palette index 8 for the
   unfocused border when no explicit border colors are set. This avoids
   modifying hundreds of themes, but it does not satisfy the requirement that
   every theme define the two new colors.

The intended direction for this issue is option 1, preferably with a script that
computes and applies the colors. A script can also produce an audit report for
review, including each theme's selected focused/unfocused border colors and the
source color used to infer each value.

## Proposed Color Inference

The first experiment should design and audit an inference strategy before
changing all themes. A reasonable starting heuristic is:

- focused border: prefer a vivid accent from the theme, likely cyan/blue:
  palette 6, palette 14, palette 4, or palette 12 depending on contrast and
  theme family;
- unfocused border: prefer a muted structural color: palette 8, selection
  background, or a low-contrast blend between foreground and background;
- reject colors that have too little contrast against the theme background;
- handle light themes separately so the unfocused border remains visible but
  subdued;
- special-case Tokyo Night variants to use the exact requested values.

The exact heuristic should be reviewed with generated samples or an audit table
before applying it to all themes.

## Acceptance Criteria

- Every bundled theme defines `focused-split-border-color`.
- Every bundled theme defines `unfocused-split-border-color`.
- Tokyo Night uses `focused-split-border-color = #7dcfff`.
- Tokyo Night uses `unfocused-split-border-color = #565f89`.
- User config values still override theme values.
- A user can remove explicit Tokyo Night border color overrides from
  `~/.config/termsurf/config` and keep the intended Tokyo Night border colors.
- The implementation includes either a script or a documented repeatable process
  for auditing/generated theme border colors.
- Docs describe theme-provided split border colors and user override behavior.
