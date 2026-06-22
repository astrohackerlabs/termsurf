# Experiment 8: Homebrew Install Verification

## Description

Stage 7 published GitHub Release `v1.4.0` and pushed the Homebrew cask. Stage 8
verifies that a real Homebrew upgrade installs the published release and that
the installed artifacts work.

This machine currently has TermSurf `1.0.0` installed from Homebrew, and the
local `termsurf/termsurf` tap is still stale at the old `v1.0.0` commit. This
experiment should update the tap, upgrade the installed cask to `1.4.0`, verify
installed file layout and signatures, prove the installed `web` binary matches
the release artifact, and run the installed Ghostboard plus installed WebTUI
plus installed Surfari launch test.

## Changes

No TermSurf source-code changes are planned. Expected system changes:

- Homebrew tap `termsurf/termsurf` updates to include the pushed `v1.4.0` cask.
- Installed cask `termsurf` upgrades from `1.0.0` to `1.4.0`.
- `/Applications/TermSurf.app`, `/opt/homebrew/bin/web`,
  `/opt/homebrew/opt/termsurf-roamium`, and `/opt/homebrew/opt/termsurf-surfari`
  are installed from the `1.4.0` cask.

## Verification

Preflight:

```bash
brew list --cask termsurf 2>&1 || true
brew info --cask termsurf
brew tap-info termsurf/termsurf
git status --short
```

Update the tap and upgrade the cask:

```bash
brew update
brew upgrade --cask termsurf 2>&1 | tee /tmp/termsurf-issue838-exp8-brew-upgrade.log
```

If Homebrew reports that `termsurf` is already installed at `1.4.0`, run:

```bash
brew reinstall --cask termsurf 2>&1 | tee /tmp/termsurf-issue838-exp8-brew-reinstall.log
```

Verify installed cask metadata and artifacts:

```bash
brew info --cask termsurf
brew info --cask termsurf | rg 'termsurf.*1\.4\.0|==> termsurf .*1\.4\.0'
brew list --cask termsurf
test -d /Applications/TermSurf.app
test -x /Applications/TermSurf.app/Contents/MacOS/termsurf
test -x /opt/homebrew/bin/web
test -x /opt/homebrew/opt/termsurf-roamium/roamium
test -x /opt/homebrew/opt/termsurf-surfari/surfari
test -f /opt/homebrew/opt/termsurf-surfari/libtermsurf_webkit.dylib
test -f /opt/homebrew/opt/termsurf-surfari/libWebKitSwift.dylib
test -d /opt/homebrew/opt/termsurf-surfari/WebKit.framework
test -d /opt/homebrew/opt/termsurf-surfari/WebCore.framework
test -d /opt/homebrew/opt/termsurf-surfari/JavaScriptCore.framework
test -d /opt/homebrew/opt/termsurf-surfari/WebKitLegacy.framework
test -d /opt/homebrew/opt/termsurf-surfari/WebInspectorUI.framework
test -d /opt/homebrew/opt/termsurf-surfari/WebGPU.framework
```

Verify installed `web` is the published release artifact. This ties the
installed WebTUI to the release build that contains the Issue 836 top-controls
fix and was tested in source:

```bash
test "$(shasum -a 256 /opt/homebrew/bin/web | awk '{print $1}')" = \
  "$(shasum -a 256 dist/release/web | awk '{print $1}')"
cargo test -p webtui issue_836_after_ -- --nocapture
```

Verify installed Surfari runtime integrity:

```bash
source scripts/surfari-resources.sh
broken_symlinks="$(find /opt/homebrew/opt/termsurf-surfari -type l -exec sh -c '
  for l do
    if [ ! -e "$l" ]; then
      printf "%s -> %s\n" "$l" "$(readlink "$l")"
    fi
  done
' sh {} +)"
test -z "$broken_symlinks"

surfari_signed_artifacts=(
  "/opt/homebrew/opt/termsurf-surfari/surfari"
  "/opt/homebrew/opt/termsurf-surfari/libtermsurf_webkit.dylib"
)
for resource in "${SURFARI_REQUIRED_RUNTIME_RESOURCES[@]}"; do
  surfari_signed_artifacts+=("/opt/homebrew/opt/termsurf-surfari/$resource")
done
for artifact in "${surfari_signed_artifacts[@]}"; do
  codesign --verify --deep --strict "$artifact"
done
```

Run the installed app with installed WebTUI and installed Surfari, without
`TERMSURF_SURFARI_PATH`, first through the explicit installed-path harness
override and then through Ghostboard's production default installed path:

```bash
env -u TERMSURF_SURFARI_PATH \
  TERMSURF_GHOSTBOARD_APP=/Applications/TermSurf.app \
  TERMSURF_WEB=/opt/homebrew/bin/web \
  TERMSURF_INSTALLED_SURFARI=/opt/homebrew/opt/termsurf-surfari/surfari \
  scripts/ghostboard-geometry-matrix.sh installed-surfari-release-launch

env -u TERMSURF_SURFARI_PATH \
  -u TERMSURF_INSTALLED_SURFARI \
  -u TERMSURF_INSTALLED_SURFARI_PATH \
  TERMSURF_GHOSTBOARD_APP=/Applications/TermSurf.app \
  TERMSURF_WEB=/opt/homebrew/bin/web \
  scripts/ghostboard-geometry-matrix.sh installed-surfari-release-launch
```

Final hygiene:

```bash
prettier --check issues/0838-deploy-next-homebrew-version/README.md \
  issues/0838-deploy-next-homebrew-version/08-homebrew-install-verification.md
git diff --check
git status --short
```

Pass criteria:

- Homebrew upgrades or reinstalls `termsurf` at version `1.4.0`.
- Installed artifacts include `TermSurf.app`, `web`, Roamium, Surfari,
  `libtermsurf_webkit.dylib`, `libWebKitSwift.dylib`, and the required Surfari
  WebKit runtime resources.
- Installed `web` matches the published release artifact, and the Issue 836
  top-controls tests pass.
- Installed Surfari runtime has no broken symlinks and passes strict
  code-signature verification.
- Installed Ghostboard launches installed `web --browser surfari` without
  `TERMSURF_SURFARI_PATH`, resolves Surfari through both the explicit installed
  harness override and Ghostboard's production default installed path, spawns
  installed Surfari, reaches AppKit overlay presentation, and receives
  `BrowserReady` for browser `surfari`.

Fail criteria:

- Homebrew cannot update the tap or install/upgrade the cask.
- The installed cask remains at a version older than `1.4.0`.
- Required installed artifacts or runtime resources are missing.
- Installed `web` differs from the release artifact or the Issue 836 tests fail.
- Installed Surfari has broken symlinks or invalid signatures.
- The installed app cannot launch installed Surfari via `web --browser surfari`
  without `TERMSURF_SURFARI_PATH`.

## Design Review

An adversarial subagent reviewed the design with fresh context.

**Verdict:** Approved.

The reviewer had no Required findings. I accepted both optional suggestions:

- Added an explicit command assertion that `brew info --cask termsurf` reports
  version `1.4.0`.
- Added a second installed Surfari launch run that omits
  `TERMSURF_INSTALLED_SURFARI` and `TERMSURF_INSTALLED_SURFARI_PATH`, so
  Ghostboard must use its production default `/opt/homebrew` Surfari path.
