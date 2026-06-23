+++
status = "open"
opened = "2026-06-23"
+++

# Issue 839: Theme-Derived Split Border Colors

## Goal

Make split pane border colors theme-derived by default, so users can rely on the
selected Ghostboard theme for focused and unfocused split border colors without
modifying bundled theme files and while retaining explicit config overrides.

When this issue is solved, a user with `theme = tokyonight` should be able to
remove these lines from `~/.config/termsurf/config` and still get Tokyo
Night-appropriate split border colors:

```ini
focused-split-border-color = 7dcfff
unfocused-split-border-color = 414868
```

Users must still be able to override the theme-derived values by setting
`focused-split-border-color` and `unfocused-split-border-color` directly in
their config.

## Background

Issue 823 added configurable split pane border colors:

- `focused-split-border-color`
- `unfocused-split-border-color`
- `split-border-width`

Those colors currently work as explicit user configuration, but they are not
defined by any bundled theme and have no theme-derived fallback. This makes
visually integrated split borders depend on per-user config even when the colors
can be derived from the selected theme's existing terminal palette.

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
user's accepted Tokyo Night inactive border color is `#414868`, which is already
present as palette index 8 in the bundled `TokyoNight` and `TokyoNight Night`
theme files.

## Requirements

1. Do not vendor, patch, or modify the bundled upstream theme files for this
   issue.
2. Keep `focused-split-border-color` and `unfocused-split-border-color` as
   nullable/optional config values.
3. When either border color is unset, derive a default from the already loaded
   theme colors.
4. For Tokyo Night, the derived colors should be:
   - focused: `#7dcfff` from palette index 6 or 14;
   - unfocused: `#414868` from palette index 8.
5. For all other themes, infer the most relevant existing colors from that
   theme's own defined colors.
6. Preserve user override behavior: explicit values in the user's config must
   still override theme-derived values.
7. Do not make `split-border-width` theme-derived unless an experiment proves
   that enabling borders by default is desired. This issue is about moving the
   color defaults to theme-derived behavior, not silently enabling borders for
   users who have not opted into borders.
8. Add focused tests or an audit command proving the fallback colors are derived
   from theme data and config overrides still win.
9. Update documentation so users understand that unset border colors are derived
   from the active theme and explicit config values override the derived
   defaults.

## Theme Source

The bundled theme files in `ghostboard/zig-out/share/ghostty/themes/` are not
source files. They are generated install output and are ignored by git.

The current theme source is an external Zig package dependency named
`iterm2_themes`, declared in `ghostboard/build.zig.zon`:

```zig
.iterm2_themes = .{
    .url = "https://deps.files.ghostty.org/ghostty-themes-release-20260608-160426-8c84dd1.tgz",
    ...
}
```

`ghostboard/src/build/GhosttyResources.zig` installs that dependency into
`share/ghostty/themes` during the build. The downloaded package may appear in
`~/.cache/zig/p/...`, but the Zig cache is not source either and must not be
edited.

## Analysis

The intended implementation is to use Ghostboard's existing nullable config
value pattern:

- `focused-split-border-color` and `unfocused-split-border-color` remain
  `?Color = null` in Zig config.
- `ghostty_config_get` continues returning false when those values are unset.
- The macOS Swift config accessors stop treating unset border colors as "no
  color" and instead derive a default from the loaded theme's effective color
  palette.
- Explicit user config values continue returning true through
  `ghostty_config_get` and therefore bypass the derived fallback.

This matches existing Ghostty/Ghostboard behavior for unset optional values. For
example:

- `unfocused-split-fill` is nullable and falls back to `background` when unset.
- `split-divider-color` is nullable and falls back to a derived color based on
  `background` when unset.

Using the same pattern for split border colors avoids taking ownership of the
external Ghostty theme package while still making borders feel theme-native. The
theme files continue defining the standard terminal colors, and Ghostboard
derives secondary UI colors from those standard colors.

## Proposed Color Inference

The first experiment should design and audit an inference strategy before
changing runtime behavior. A reasonable starting heuristic is:

- focused border: prefer a vivid accent from the loaded theme, likely palette 6
  or palette 14, then palette 4 or palette 12 if needed;
- unfocused border: prefer a muted structural color from the loaded theme,
  likely palette 8, then selection background or a contrast-checked blend if
  needed;
- reject colors that have too little contrast against the theme background;
- handle light themes separately so the unfocused border remains visible but
  subdued;
- ensure Tokyo Night derives focused `#7dcfff` and unfocused `#414868` from
  existing theme palette entries.

The exact heuristic should be reviewed with generated samples or an audit table
before changing runtime behavior.

## Acceptance Criteria

- Bundled theme files are not vendored, patched, or modified for this issue.
- `focused-split-border-color` remains user-configurable and nullable.
- `unfocused-split-border-color` remains user-configurable and nullable.
- When `focused-split-border-color` is unset, Ghostboard derives a focused
  border color from the active theme colors.
- When `unfocused-split-border-color` is unset, Ghostboard derives an unfocused
  border color from the active theme colors.
- Tokyo Night derives focused `#7dcfff`.
- Tokyo Night derives unfocused `#414868`.
- User config values still override theme values.
- A user can remove explicit Tokyo Night border color overrides from
  `~/.config/termsurf/config` and keep the intended Tokyo Night border colors.
- `split-border-width = 0` still disables border drawing.
- Docs describe theme-derived split border colors and user override behavior.

## Experiments

- [Experiment 1: Audit theme palette border candidates](01-audit-theme-palette-border-candidates.md)
  — **Pass**
- [Experiment 2: Implement theme-derived split border colors](02-implement-theme-derived-split-border-colors.md)
  — **Pass**
