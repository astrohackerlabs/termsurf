# Experiment 5: Restore TermSurf Identity Surfaces

## Description

Experiment 4 proved that the merged macOS app launches, but it also proved that
several identity surfaces are still wrong for Issue 826:

- the app bundle is `TermSurf Ghostboard.app`;
- `CFBundleName` and `CFBundleDisplayName` are `TermSurf Ghostboard`;
- the app executable is `ghostboard`;
- the debug bundle ID is `com.termsurf.ghostboard.debug`;
- local build instructions still point at `TermSurf Ghostboard.app`.

Issue 826 requires the user-facing app identity to remain `TermSurf`, the CLI
command to remain `termsurf`, and the config path to remain
`~/.config/termsurf/config`. This experiment restores and verifies those
identity surfaces without changing protocol behavior, browser overlays, pane
geometry, or Roamium/webtui behavior.

The expected final macOS debug app bundle for this experiment is
`ghostboard/macos/build/Debug/TermSurf.app`.

## Changes

- `ghostboard/macos/Ghostty.xcodeproj/project.pbxproj`
  - Rename the macOS app product reference and product name from
    `TermSurf Ghostboard.app` / `TermSurf Ghostboard` to `TermSurf.app` /
    `TermSurf`.
  - Rename the app executable from `ghostboard` to `termsurf`, unless the build
    proves that upstream requires a different executable name internally. If
    that happens, document the reason and keep the user-facing CLI artifact
    `termsurf`.
  - Update app bundle IDs from `com.termsurf.ghostboard*` to `com.termsurf*`.
    The expected debug bundle ID is `com.termsurf.debug`; the expected release
    bundle ID is `com.termsurf`.
  - Update test host paths that point at the app bundle/executable.
  - Update the Dock Tile plugin display name and bundle ID to avoid `Ghostboard`
    in user-facing metadata.
- `ghostboard/macos/Ghostty.xcodeproj/xcshareddata/xcschemes/Ghostty.xcscheme`
  - Update buildable app names from `TermSurf Ghostboard.app` to `TermSurf.app`.
- `ghostboard/macos/AGENTS.md`
  - Update local macOS build/run instructions to use `TermSurf.app`.
- `ghostboard/HACKING.md`
  - Update the macOS app output path to use `TermSurf.app`.
- `ghostboard/macos/Sources/Features/Settings/SettingsView.swift`
  - If it still says to restart `TermSurf Ghostboard`, update that text to
    `TermSurf` while preserving the documented config path.
- `issues/0826-update-ghostboard-to-latest-ghostty/README.md`
  - Link this experiment and update its status after the result is known.
- `issues/0826-update-ghostboard-to-latest-ghostty/05-restore-termsurf-identity-surfaces.md`
  - Record design, verification, result, reviews, and conclusion.

Do not rename the `ghostboard/` source directory in this experiment. The
directory name is an internal repository boundary and is not the user-facing app
identity.

Do not do broad `ghostty` to `termsurf` rewrites. Internal Ghostty names should
stay intact unless they directly affect the Issue 826 app identity, CLI command,
config path, or user-facing strings listed above.

## Verification

Before changes, capture the current state:

```bash
git status --short
plutil -extract CFBundleName raw \
  "ghostboard/macos/build/Debug/TermSurf Ghostboard.app/Contents/Info.plist"
plutil -extract CFBundleDisplayName raw \
  "ghostboard/macos/build/Debug/TermSurf Ghostboard.app/Contents/Info.plist"
plutil -extract CFBundleIdentifier raw \
  "ghostboard/macos/build/Debug/TermSurf Ghostboard.app/Contents/Info.plist"
plutil -extract CFBundleExecutable raw \
  "ghostboard/macos/build/Debug/TermSurf Ghostboard.app/Contents/Info.plist"
test -x "ghostboard/zig-out/bin/termsurf" || true
```

Build after changes:

```bash
cd ghostboard
zig build -Demit-macos-app=false \
  > ../logs/issue-0826-exp05-zig-core.log 2>&1
macos/build.nu --configuration Debug --action clean \
  > ../logs/issue-0826-exp05-macos-clean.log 2>&1
rm -rf "macos/build/Debug/TermSurf.app" \
  "macos/build/Debug/TermSurf Ghostboard.app"
macos/build.nu --configuration Debug --action build \
  > ../logs/issue-0826-exp05-macos-build.log 2>&1
```

Verify app identity from the rebuilt bundle:

```bash
test -d "ghostboard/macos/build/Debug/TermSurf.app"
test ! -d "ghostboard/macos/build/Debug/TermSurf Ghostboard.app"
test "$(plutil -extract CFBundleName raw \
  "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist")" = "TermSurf"
test "$(plutil -extract CFBundleDisplayName raw \
  "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist")" = "TermSurf"
test "$(plutil -extract CFBundleIdentifier raw \
  "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist")" = "com.termsurf.debug"
test "$(plutil -extract CFBundleExecutable raw \
  "ghostboard/macos/build/Debug/TermSurf.app/Contents/Info.plist")" = "termsurf"
test -x "ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf"
! rg -n "com\\.termsurf\\.ghostboard" \
  ghostboard/macos/Ghostty.xcodeproj/project.pbxproj \
  ghostboard/macos/Ghostty.xcodeproj/xcshareddata/xcschemes/Ghostty.xcscheme
```

Verify that the Zig CLI remains `termsurf`:

```bash
cd ghostboard
zig build -Demit-exe=true -Demit-macos-app=false \
  > ../logs/issue-0826-exp05-zig-exe.log 2>&1
test -x zig-out/bin/termsurf
zig-out/bin/termsurf --version \
  > ../logs/issue-0826-exp05-termsurf-version.log 2>&1
```

Verify the config path remains TermSurf-specific:

```bash
rg -n "\\.config/termsurf|XDG_CONFIG_HOME/termsurf|config-path" \
  ghostboard/src ghostboard/macos/Sources \
  > logs/issue-0826-exp05-config-paths.log
rg -n "\\.config/ghostty|XDG_CONFIG_HOME/ghostty|config/ghostty" \
  ghostboard/src ghostboard/macos/Sources \
  > logs/issue-0826-exp05-ghostty-config-paths.log || true
```

Verify launch still works after the rename:

```bash
APP="$PWD/ghostboard/macos/build/Debug/TermSurf.app"
osascript -e "tell application \"$APP\" to activate" \
  > logs/issue-0826-exp05-launch.log 2>&1
sleep 5
ps -axo pid,comm,args \
  | rg "TermSurf.app/Contents/MacOS/termsurf|$APP" \
  > logs/issue-0826-exp05-process.log
osascript -e "tell application \"$APP\" to quit" \
  > logs/issue-0826-exp05-quit.log 2>&1
sleep 2
ps -axo pid,comm,args \
  | rg "TermSurf.app/Contents/MacOS/termsurf|$APP" \
  | rg -v 'rg|ps -axo|zsh -lc' \
  > logs/issue-0826-exp05-post-quit-process.log || true
```

Run formatting and hygiene checks:

```bash
git diff --name-only -- '*.zig' | xargs -r zig fmt
(cd ghostboard && swiftlint lint --strict --fix)
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/05-restore-termsurf-identity-surfaces.md \
  ghostboard/HACKING.md \
  ghostboard/macos/AGENTS.md
git diff --check
```

Pass criteria:

- The rebuilt macOS bundle is `TermSurf.app`.
- `CFBundleName` and `CFBundleDisplayName` are `TermSurf`.
- The debug app bundle ID is `com.termsurf.debug`; release app bundle IDs use
  `com.termsurf`.
- No macOS app target bundle ID begins with `com.termsurf.ghostboard`.
- The rebuilt app executable is `termsurf`.
- The built app launches by absolute path and quits cleanly.
- The Zig CLI artifact remains `zig-out/bin/termsurf`.
- Config documentation and code continue to point at `~/.config/termsurf/config`
  or `$XDG_CONFIG_HOME/termsurf/config`.
- No `~/.config/ghostty` / `$XDG_CONFIG_HOME/ghostty` config path remains in the
  active Ghostboard app/config code.

Partial criteria:

- Some identity surfaces are fixed, but the app cannot be rebuilt/launched or an
  internal upstream assumption requires keeping a non-target executable or
  bundle ID. The first blocking mismatch must be documented with logs.

Fail criteria:

- The experiment expands into TermSurf protocol, browser overlay, pane geometry,
  webtui, or Roamium behavior before the identity surfaces are restored.
- The build cannot be invoked, or the tree is left with ambiguous app products.

## Design Review

An adversarial Codex subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

Required findings and fixes:

- Bundle ID acceptance was under-specified. Fixed by adding expected debug and
  release bundle IDs, mechanical `CFBundleIdentifier` equality checks, and pass
  criteria requiring no macOS app target bundle ID to begin with
  `com.termsurf.ghostboard`.
- Stale app artifacts could make verification ambiguous. Fixed by adding
  `macos/build.nu --configuration Debug --action clean` and explicit removal of
  the old and new app product paths before rebuilding.

The optional plist-check finding was also adopted by changing the rebuilt bundle
plist verification from value-printing commands to failing equality checks.

The re-review approved the design with no required findings. It confirmed that
the bundle ID expectations, stale-product cleanup, and exact plist checks now
address the prior findings.
