# Issue 740: Wezboard display name should say "TermSurf Wezboard"

## Goal

The macOS dock icon and menu bar should say "TermSurf Wezboard" instead of
"Wezboard".

## Background

The rename script (`scripts/rename-wezterm.sh`) replaced all "WezTerm"
references with "Wezboard", but the user-facing display name should include the
TermSurf brand — "TermSurf Wezboard" — so users know this is part of the
TermSurf family. Internal identifiers (crate names, binary names, bundle
identifiers) stay as `wezboard`.

## Analysis

The display name appears in these locations:

### Info.plist

`wezboard/assets/macos/Wezboard.app/Contents/Info.plist`:

- **CFBundleName** (line 14): `Wezboard` — Controls dock icon name.
- **CFBundleDisplayName** (line 36): `Wezboard` — Controls Spotlight and system
  display name.

### Menu bar (commands.rs)

`wezboard/wezboard-gui/src/commands.rs`:

- **Line 426**: Menu order vector includes `"Wezboard"` as the app menu title.
- **Line 446**: Check `if cmd.menubar[0] == "Wezboard"` to assign the app menu.
- **Line 450**: About item: `"Wezboard {version}"`.
- **Lines 745, 1268, 1276**: Menu bar entries reference `&["Wezboard"]`.
- **Line 1272**: `"Quit Wezboard"`.
- **Line 1273**: `"Quits Wezboard"`.

### Other UI strings

- `wezboard/wezboard-gui/src/main.rs` lines 817, 825: `"Wezboard panic"`,
  `"Wezboard Error"`.
- `wezboard/window/src/os/macos/app.rs` lines 25–26: `"Terminate Wezboard?"`,
  `"Detach and close all panes and terminate wezboard?"`.

### What stays unchanged

- Crate names (`wezboard`, `wezboard-gui`)
- Binary name (`wezboard-gui`)
- Bundle identifier (`com.termsurf.wezboard`)
- ObjC class names (`WezboardAppDelegate`)
- File/directory names
- Quit/about strings, error messages, termination dialogs
- Permission description strings in Info.plist

## Experiments

### Experiment 1: Rename dock and menu bar title

#### Description

Change the dock icon name and menu bar app menu title from "Wezboard" to
"TermSurf Wezboard". Only two files need changes: Info.plist for the dock, and
commands.rs for the menu bar.

#### Changes

**`wezboard/assets/macos/Wezboard.app/Contents/Info.plist`**

- Line 14: `CFBundleName` — `Wezboard` → `TermSurf Wezboard`.
- Line 36: `CFBundleDisplayName` — `Wezboard` → `TermSurf Wezboard`.

**`wezboard/wezboard-gui/src/commands.rs`**

Change the menu bar title string used for the app menu. This is the `&str` value
that appears as the first menu bar item and is matched to assign the app menu.
All six occurrences of `"Wezboard"` used as the menu title become
`"TermSurf Wezboard"`:

- Line 426: menu order vector `"Wezboard"` → `"TermSurf Wezboard"`.
- Line 446: app menu check `"Wezboard"` → `"TermSurf Wezboard"`.
- Line 745: `menubar: &["Wezboard"]` → `menubar: &["TermSurf Wezboard"]`.
- Line 1268: `menubar: &["Wezboard"]` → `menubar: &["TermSurf Wezboard"]`.
- Line 1276: `menubar: &["Wezboard"]` → `menubar: &["TermSurf Wezboard"]`.

The about item text (line 450) and quit strings (lines 1272–1273) stay as
"Wezboard" — they are descriptive labels, not the menu title.

#### Verification

1. `./scripts/build.sh wezboard --release` — builds without errors.
2. Launch the app. The dock icon says "TermSurf Wezboard".
3. The menu bar's first item (app menu) says "TermSurf Wezboard".
4. The About item still says "Wezboard {version}".
5. The Quit item still says "Quit Wezboard".

**Result:** Partial

The menu bar app menu changed to "TermSurf Wezboard" as expected. The dock icon
still says "Wezboard". macOS gets the dock name from the app bundle directory
name (`Wezboard.app`), not from `CFBundleName` or `CFBundleDisplayName` in
Info.plist. The bundle is installed to `/Applications/Wezboard.app` by the
install script, and that's what the dock displays.

#### Conclusion

The commands.rs changes worked — the menu bar shows "TermSurf Wezboard". The
Info.plist changes alone are insufficient for the dock. To fix the dock name, the
app bundle directory itself needs to be renamed from `Wezboard.app` to
`TermSurf Wezboard.app`, and the install script
(`scripts/install.sh`) needs to install to `/Applications/TermSurf Wezboard.app`.
