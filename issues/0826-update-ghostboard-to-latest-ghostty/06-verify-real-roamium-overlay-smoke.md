# Experiment 6: Verify Real Roamium Overlay Smoke

## Description

Experiment 5 restored the macOS app identity to `TermSurf.app` with executable
`termsurf`. The next Issue 826 acceptance item is to prove that the updated and
renamed Ghostboard still speaks the TermSurf protocol to the existing `webtui`
and real Chromium-output Roamium binary without modifying either component.

This experiment updates only the local automation needed to point at the new app
bundle/executable name, then runs a focused real-browser smoke test. The test
must launch the rebuilt `TermSurf.app`, run the repo-built `web` TUI against the
real Roamium artifact at `chromium/src/out/Default/roamium`, present
`https://example.com`, and collect log plus screenshot evidence that the browser
overlay is visible inside the terminal pane.

This experiment does not attempt the full pane/tab/window geometry matrix. If
this smoke path fails, the result should identify the first failing boundary:
launch, socket discovery, webtui connection, Roamium spawn, protobuf lifecycle,
CAContext receipt, AppKit presentation, visual rendering, input, or cleanup.

## Changes

- `scripts/ghostboard-geometry-matrix.sh`
  - Update the default debug app path from `TermSurf Ghostboard.app` to
    `TermSurf.app`.
  - Update the default release app path for the installed-Roamium scenario from
    `TermSurf Ghostboard.app` to `TermSurf.app`.
  - Update the app executable path from `Contents/MacOS/ghostboard` to
    `Contents/MacOS/termsurf`.
  - Keep `TERMSURF_GHOSTBOARD_APP`, `TERMSURF_WEB`, and `TERMSURF_ROAMIUM`
    overrides intact so older or alternate harness targets can still be tested
    explicitly.
- `issues/0826-update-ghostboard-to-latest-ghostty/README.md`
  - Link this experiment with status `Designed`, then update the status after
    the result is known.
- `issues/0826-update-ghostboard-to-latest-ghostty/06-verify-real-roamium-overlay-smoke.md`
  - Record design, verification, result, reviews, and conclusion.

Do not modify `webtui/`, `roamium/`, `chromium/`, or `proto/termsurf.proto` in
this experiment. Do not add new protocol messages. Do not broaden the harness
into a full matrix run until the simple real-browser smoke path is proven.

## Verification

Confirm starting state:

```bash
git status --short
test -x ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf
test -x target/debug/web
test -x chromium/src/out/Default/roamium
```

Build the existing components needed for the smoke test:

```bash
cargo build -p webtui \
  > logs/issue-0826-exp06-webtui-build.log 2>&1
cd ghostboard
macos/build.nu --configuration Debug --action build \
  > ../logs/issue-0826-exp06-macos-build.log 2>&1
```

Run the focused real-browser overlay scenario with the renamed app defaults:

```bash
env -u TERMSURF_GHOSTBOARD_APP \
  -u TERMSURF_WEB \
  -u TERMSURF_ROAMIUM \
  -u TERMSURF_INSTALLED_ROAMIUM \
  scripts/ghostboard-geometry-matrix.sh initial-open \
  > logs/issue-0826-exp06-initial-open.log 2>&1
```

The scenario must use:

```text
ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf
target/debug/web
chromium/src/out/Default/roamium
https://example.com
```

Capture and record the latest harness artifacts:

```bash
ls -t logs/ghostboard-geometry-initial-open-* | head -20 \
  > logs/issue-0826-exp06-artifacts.log
```

Inspect the latest app and harness logs for protocol and presentation evidence:

```bash
APP_LOG="$(ls -t logs/ghostboard-geometry-initial-open-app-*.log | head -1)"
HARNESS_LOG="$(ls -t logs/ghostboard-geometry-initial-open-harness-*.log | head -1)"
ROAMIUM_TRACE="$(ls -t logs/ghostboard-geometry-initial-open-roamium-*.log | head -1)"
WEBTUI_TRACE="$(ls -t logs/ghostboard-geometry-initial-open-webtui-*.log | head -1)"

rg -n "TermSurf message decoded type=HelloRequest|ServerRegister|CreateTab|TabReady|BrowserReady|SetOverlay|CaContext|PresentOverlay|TermSurf geometry layer=appkit event=presented|TitleChanged|Example Domain" \
  "$APP_LOG" \
  > logs/issue-0826-exp06-app-evidence.log

rg -n "PASS: scenario initial-open|correlation_screenshot=|correlation_pane_id=|correlation_browser_tab_id=|correlation_appkit_pixel=" \
  "$HARNESS_LOG" \
  > logs/issue-0826-exp06-harness-evidence.log

{
  rg -n "app=.*/ghostboard/macos/build/Debug/TermSurf\\.app" "$HARNESS_LOG" &&
    rg -n "web=.*/target/debug/web" "$HARNESS_LOG" &&
    rg -n "roamium=.*/chromium/src/out/Default/roamium" "$HARNESS_LOG" &&
  rg -n "url=https://example\\.com" "$HARNESS_LOG"
} > logs/issue-0826-exp06-resolved-targets.log

rg -n "resize tab_id=.*ffi=ts_set_view_size" "$ROAMIUM_TRACE" \
  > logs/issue-0826-exp06-roamium-evidence.log

rg -n "event=url_changed.*https://example.com|event=loading_state.*state=done|event=title_changed.*Example Domain" \
  "$WEBTUI_TRACE" \
  > logs/issue-0826-exp06-webtui-evidence.log
```

Verify screenshot evidence:

```bash
SCREENSHOT="$(rg -o "correlation_screenshot=.*" "$HARNESS_LOG" \
  | tail -1 \
  | cut -d= -f2-)"
test -s "$SCREENSHOT"
```

If the harness already performs automated visual checks, record the pass lines.
If it only captures a screenshot, inspect the screenshot and record whether
recognizable `Example Domain` browser content is visible in the expected overlay
region. AppKit `CALayerHost` logs alone are not enough for a passing result.

Run hygiene checks:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/06-verify-real-roamium-overlay-smoke.md
git diff --check
git diff --name-only
git status --short -- webtui roamium proto/termsurf.proto chromium/README.md chromium/patches
git -C chromium/src status --short
git -C chromium/src diff --name-only
```

Pass criteria:

- The harness default app target is the rebuilt
  `ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf`.
- `cargo build -p webtui` passes.
- The debug macOS `TermSurf.app` build passes.
- `scripts/ghostboard-geometry-matrix.sh initial-open` passes without requiring
  a `TERMSURF_GHOSTBOARD_APP`, `TERMSURF_WEB`, `TERMSURF_ROAMIUM`, or
  `TERMSURF_INSTALLED_ROAMIUM` override.
- The runtime uses `target/debug/web` and `chromium/src/out/Default/roamium`,
  not an installed browser, fake helper, or `target/debug/roamium`.
- The harness log's resolved `app=`, `web=`, `roamium=`, and `url=` lines match
  the expected `TermSurf.app`, `target/debug/web`,
  `chromium/src/out/Default/roamium`, and `https://example.com` targets.
- Logs prove the expected TermSurf lifecycle through `HelloRequest`,
  `SetOverlay`, Roamium spawn/register, `CreateTab`, `TabReady`, `BrowserReady`,
  and `CaContext`.
- Logs prove AppKit presentation with a nonzero context id and concrete pane id.
- Roamium trace proves `ts_set_view_size` received the AppKit pixel size.
- Screenshot evidence proves real browser content is visible inside the terminal
  pane.
- Cleanup leaves no stale matching `TermSurf.app/Contents/MacOS/termsurf`,
  `target/debug/web`, or `chromium/src/out/Default/roamium` processes.
- `git diff --check` is clean.
- No forbidden paths are modified: `webtui/`, `roamium/`, `chromium/`, or
  `proto/termsurf.proto`.
- The nested `chromium/src` checkout has no uncommitted status or diff from this
  experiment.

Partial criteria:

- The harness defaults are repaired, but the real-browser scenario fails at a
  clearly identified boundary with logs.
- The protocol and AppKit logs pass, but visual screenshot proof is unavailable
  because macOS screen capture permissions or VM behavior prevents inspection.

Fail criteria:

- The experiment changes `webtui`, Roamium, Chromium, or the protobuf protocol
  to make the smoke path pass.
- The runtime uses a fake browser, installed browser, or stale app bundle.
- The harness passes without evidence that real browser pixels are visible.

## Design Review

An adversarial Codex subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

Required findings and fixes:

- The smoke command could be contaminated by inherited environment overrides for
  the app, web TUI, or Roamium paths. Fixed by running the harness through
  `env -u TERMSURF_GHOSTBOARD_APP -u TERMSURF_WEB -u TERMSURF_ROAMIUM -u TERMSURF_INSTALLED_ROAMIUM`
  and requiring logged resolved `app=`, `web=`, `roamium=`, and `url=` evidence.
- Top-level `git diff --name-only` would not detect changes inside the nested
  Chromium checkout. Fixed by adding explicit top-level forbidden-path status
  checks plus `git -C chromium/src status --short` and
  `git -C chromium/src diff --name-only`.

The first re-review found that the resolved-target evidence used one `rg`
alternation, which could pass after matching only one of the required targets.
Fixed by splitting the evidence into four separate required `rg` commands.

The second re-review found that the brace group would still return only the last
command status outside `set -e`. Fixed by chaining the four required `rg`
commands with `&&`.

The final re-review approved the design with no findings.
