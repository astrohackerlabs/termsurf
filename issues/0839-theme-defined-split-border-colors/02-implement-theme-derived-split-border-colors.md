# Experiment 2: Implement theme-derived split border colors

## Description

Implement the Experiment 1 audit result in Ghostboard without modifying bundled
theme files.

The new behavior should be:

- explicit `focused-split-border-color` config still wins;
- explicit `unfocused-split-border-color` config still wins;
- unset focused split border color derives from the active theme palette:
  - use palette 6 when its contrast against `background` is at least 2.0;
  - otherwise choose the highest-contrast available entry from palette 14, 4,
    and 12;
- unset unfocused split border color derives from palette 8;
- `split-border-width = 0` remains the default and still disables drawing;
- TokyoNight with no explicit border colors derives focused `#7dcfff` and
  unfocused `#414868`.

This experiment should make the user-facing behavior true, add targeted tests,
and update documentation. It must not vendor, patch, or modify theme files.

## Changes

- `ghostboard/macos/Sources/Ghostty/Ghostty.Config.swift`
  - Add a helper for converting `ghostty_config_color_s` to SwiftUI `Color`.
  - Add a helper for reading the effective `palette` through
    `ghostty_config_get` into `ghostty_config_palette_s`.
  - Add contrast helpers scoped to split border fallback selection.
  - Change `focusedSplitBorderColor` so an unset `focused-split-border-color`
    derives from palette 6 with the audited contrast fallback to palette 14, 4,
    and 12.
  - Change `unfocusedSplitBorderColor` so an unset
    `unfocused-split-border-color` derives from palette 8.
  - Preserve `nil` only for the no-config-object or no-palette failure case.
- `ghostboard/src/config/CApi.zig`
  - Add targeted C API tests proving both border color keys return false when
    unset and true with the exact configured color when explicitly set.
  - Keep the config fields nullable; do not convert the Zig defaults to concrete
    colors.
- `ghostboard/macos/Tests/Ghostty/ConfigTests.swift`
  - Add tests proving a TokyoNight-style palette derives focused `#7dcfff` and
    unfocused `#414868` with no explicit border colors.
  - Add tests proving explicit focused and unfocused border color config
    overrides the derived values independently.
  - Add a test or assertion proving `splitBorderWidth` remains `0` when unset.
  - Add a focused fallback test where palette 6 is below 2.0 contrast and the
    derived focused color comes from the best candidate among palette 14, 4,
    and 12.
- `website/src/content/docs/reference/config.md`
  - Update focused and unfocused split border color descriptions so unset values
    are documented as theme-derived.
- `website/src/content/docs/split-pane-borders.mdx`
  - Update the guide so users know they can omit border color values to use the
    active theme, while explicit config overrides still work.

No `ghostboard/zig-out`, theme source package, vendoring metadata, or bundled
theme file should be changed.

## Verification

1. Confirm no bundled/generated theme files or theme dependency metadata were
   changed:

   ```bash
   git status --short -- ghostboard/zig-out ghostboard/build.zig.zon
   git diff --name-only | rg '(^ghostboard/zig-out/|ghostboard/build.zig.zon|themes/)'
   ```

   Pass: no output.

2. Run Zig config/API tests:

   ```bash
   cd ghostboard
   zig build test
   ```

   Pass: targeted C API tests prove unset border colors remain nullable at the
   Zig/C boundary and explicit config values are returned exactly.

3. Run macOS tests:

   ```bash
   cd ghostboard
   macos/build.nu --action test
   ```

   Pass: `Ghostty.Config` tests prove TokyoNight derivation, focused fallback,
   explicit override behavior, and unchanged default `splitBorderWidth`.

4. Build the macOS app:

   ```bash
   cd ghostboard
   macos/build.nu --configuration Debug --action build
   ```

   Pass: the app builds successfully.

5. Run formatting and hygiene checks:

   ```bash
   cd ghostboard
   zig fmt --check src/config/CApi.zig
   swiftlint
   cd ..
   prettier --check issues/0839-theme-defined-split-border-colors/README.md \
     issues/0839-theme-defined-split-border-colors/02-implement-theme-derived-split-border-colors.md \
     website/src/content/docs/split-pane-borders.mdx
   cd website
   bun run gen:references --check
   git diff --check
   ```

   Pass: all available checks succeed. The generated
   `website/src/content/docs/reference/config.md` is validated with
   `bun run gen:references --check` instead of Prettier because it is generated
   from Ghostboard's `ghostty.5.md`. If `swiftlint` is unavailable, record that
   explicitly in the result and rely on the macOS test/build checks for Swift
   compilation.

6. Manual behavior check:

   - Use `theme = tokyonight` and `split-border-width = 2`.
   - Remove explicit `focused-split-border-color` and
     `unfocused-split-border-color`.
   - Launch a debug Ghostboard app and create at least two split panes.

   Pass: focused split border uses Tokyo Night cyan `#7dcfff`; unfocused split
   border uses `#414868`. Re-adding explicit border color config overrides the
   derived values. Removing or setting `split-border-width = 0` disables drawing
   even though derived colors are available.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Final verdict: **Approved**.

No required findings. The reviewer confirmed that the design follows the
Experiment 1 audit result, preserves nullable Zig config and explicit override
behavior, avoids theme vendoring, keeps `split-border-width` opt-in, and
includes concrete Zig, Swift, build, formatting, documentation, and manual
behavior verification.

## Result

**Result:** Pass

Implemented theme-derived split border color defaults without modifying bundled
theme files.

Changed files:

- `ghostboard/macos/Sources/Ghostty/Ghostty.Config.swift`
  - Added helpers for reading explicit colors, reading effective palette
    entries, converting C colors to SwiftUI `Color`, and calculating contrast.
  - `focusedSplitBorderColor` now returns explicit config first, then palette 6
    when contrast against `background` is at least 2.0, then the
    highest-contrast fallback among palette 14, 4, and 12.
  - `unfocusedSplitBorderColor` now returns explicit config first, then
    palette 8.
- `ghostboard/src/config/CApi.zig`
  - Added targeted tests proving focused/unfocused split border colors remain
    nullable at the Zig/C boundary and explicit values cross the C API exactly.
- `ghostboard/src/config/Config.zig`
  - Updated the authoritative config comments for the two split border color
    options so generated docs describe theme-derived defaults.
- `ghostboard/macos/Tests/Ghostty/ConfigTests.swift`
  - Added tests for TokyoNight-derived defaults, independent explicit overrides,
    `splitBorderWidth` staying `0` by default, and focused fallback selection
    when palette 6 has low contrast.
- `website/src/content/docs/reference/config.md`
  - Regenerated from Ghostboard's updated `ghostty.5.md` so unset
    focused/unfocused border colors are documented as theme-derived.
- `website/src/content/docs/split-pane-borders.mdx`
  - Updated the guide so `split-border-width` enables borders with theme-derived
    colors, while explicit colors remain optional overrides.

Verification:

- No theme files, generated theme output, theme dependency metadata, or vendored
  theme source changed.
- `cd ghostboard && zig build test` passed.
- `cd ghostboard && zig build test -Dtest-filter="split border"` passed after
  the generated-doc source fix.
- `cd ghostboard && macos/build.nu --action test` passed, including:
  - `ConfigTests/splitBorderColorsDeriveFromTokyoNightPalette`
  - `ConfigTests/splitBorderColorOverridesWinIndependently`
  - `ConfigTests/focusedSplitBorderUsesFallbackWhenPaletteSixContrastIsLow`
  - `ConfigTests/splitBorderWidthDefaultsToZero`
- `cd ghostboard && macos/build.nu --configuration Debug --action build` passed.
- `cd ghostboard && zig build -Demit-docs=true -Demit-macos-app=false` passed.
- `cd website && bun run gen:references` passed.
- `cd ghostboard && zig fmt --check src/config/CApi.zig` passed.
- `cd ghostboard && swiftlint` exited 0. It reported one existing warning in
  `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`,
  which was not touched by this experiment.
- `prettier --check` passed for hand-authored markdown changed by this
  experiment. The generated config reference is intentionally checked with
  `bun run gen:references --check` instead.
- `git diff --check` passed.

The manual GUI behavior check was not run as an interactive visual session. The
same behavior is covered by automated config tests plus the existing draw gate
in `SurfaceView.swift`, which still requires `splitBorderWidth > 0` before a
border is drawn.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes Required**.

- Required: the authoritative config comments in
  `ghostboard/src/config/Config.zig` still said unset split border colors draw
  no border, and the generated website config reference had been edited without
  regenerating it from Ghostboard's generated `ghostty.5.md`.

Fixes:

- Updated the `Config.zig` comments for focused and unfocused split border
  colors.
- Regenerated Ghostboard docs with
  `zig build -Demit-docs=true -Demit-macos-app=false`.
- Regenerated website references with `bun run gen:references`.
- Updated verification so the generated config reference is checked with
  `bun run gen:references --check` instead of Prettier.

Final verdict: **Approved**.

## Conclusion

The implementation satisfies the issue requirements:

- TokyoNight now derives focused `#7dcfff` and unfocused `#414868` when explicit
  border colors are omitted.
- Explicit focused and unfocused border color config values still override the
  derived defaults independently.
- The Zig config values remain nullable; derivation happens in the macOS config
  accessor layer, not in the theme files or Zig defaults.
- `split-border-width = 0` remains the default and continues to disable border
  drawing even though derived colors are available.
- The website docs now describe the theme-derived behavior and override model.
