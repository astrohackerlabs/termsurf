# Experiment 1: Resolve Named Roamium for Debug Launch

## Description

Ghostboard can launch Roamium when webtui passes an absolute browser path, but
the normal/default webtui path can leave the browser field empty. Ghostboard
then falls back to the named default browser `roamium`, creates a pending
server, and currently logs `named browser launch not implemented` instead of
spawning a browser process.

This experiment will make the first named-browser path deterministic for local
debug testing without accidentally using a stale installed Roamium. The narrow
target is named/default `roamium` resolution through an explicit environment
override supplied by the harness. Broader installed-app discovery and packaging
identity can remain for later Issue 814 or Issue 819 experiments.

## Changes

Planned source changes:

- `ghostboard/src/apprt/termsurf.zig`
  - Add a small browser executable resolver used by `handleSetOverlay` before
    spawning a new browser server.
  - Preserve the current absolute-path behavior: if `browser` is an absolute
    path, spawn exactly that path and keep the server/browser key unchanged.
  - Resolve named `roamium` through a dedicated environment variable such as
    `TERMSURF_ROAMIUM_PATH`.
  - Keep the Ghostboard pane/server/browser key as the requested browser name
    (`roamium`) while using the resolved absolute path only for
    `spawnBrowserProcess`. Roamium registers by profile, and BrowserReady should
    continue to report the browser key webtui used.
  - Log the resolution decision clearly, including whether an absolute browser
    path was used directly, a named browser resolved through the environment, or
    resolution failed.
  - If `roamium` is named but the environment variable is missing, empty, or not
    absolute, do not fall through to an installed binary silently. Log a clear
    failure such as
    `SetOverlay: named browser unresolved browser=roamium env=TERMSURF_ROAMIUM_PATH`.

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a focused scenario such as `named-roamium-debug-launch`.
  - Launch Ghostboard with `TERMSURF_ROAMIUM_PATH` set to the repo-built debug
    Roamium path already used by the harness.
  - Run `web` without `--browser`, so webtui exercises its default browser path
    and Ghostboard receives the named/default browser instead of an absolute
    path.
  - Verify `TERMSURF_SOCKET` discovery still works by requiring the normal
    `HelloRequest`, `SetOverlay`, `BrowserReady`, and Roamium direct-socket
    evidence.
  - Verify the app log shows named-browser resolution to the debug Roamium path,
    and verify no stale installed path such as `/usr/local/roamium`,
    `/usr/local/bin/roamium`, or `/opt/homebrew/opt/termsurf-roamium` is used.
  - Keep the existing absolute-path geometry scenarios unchanged.

Planned issue-doc changes:

- Record the result, final logs, verification commands, reviewer verdict, and
  whether any remaining discovery/packaging work should become Experiment 2.
- Update the Issue 814 README experiment status.

## Verification

Static/build checks:

1. `zig fmt ghostboard/src/apprt/termsurf.zig`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
4. `cd ghostboard && zig build -Demit-macos-app=false`.
5. `cd ghostboard && macos/build.nu --scheme Ghostty --configuration Debug --action build`.
6. `cargo check -p webtui`.
7. `cargo check -p roamium`.
8. `git diff --check`.

Runtime checks:

1. Run the existing absolute-path scenario, such as
   `scripts/ghostboard-geometry-matrix.sh initial-open`, to confirm the
   established debug launch path still works.
2. Run `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch`.
3. Verify webtui was launched without `--browser`.
4. Verify Ghostboard receives `SetOverlay` with the named/default browser key
   `roamium`.
5. Verify Ghostboard resolves `roamium` through `TERMSURF_ROAMIUM_PATH` to
   `/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium` and logs
   that resolution.
6. Verify Ghostboard spawns that resolved debug path, Roamium registers, and
   `BrowserReady` reaches webtui.
7. Verify the browser content reaches the existing geometry/correlation proof:
   `TabReady`, `CaContext`, AppKit overlay presentation, hit-test, and Roamium
   resize evidence.
8. Verify the app log does not contain stale installed Roamium launch paths.
9. If practical, run a negative resolver probe with the environment unset or
   invalid and verify Ghostboard logs a clear unresolved named-browser error
   without spawning an installed binary.

Pass criteria:

- Absolute-path browser launch remains green.
- Named/default `roamium` launch works when `TERMSURF_ROAMIUM_PATH` points at
  the repo-built debug Roamium.
- The named/default scenario proves socket discovery, browser spawn,
  BrowserReady, direct browser socket connection, CA context, and visible
  overlay geometry.
- The named/default scenario proves no stale installed Roamium path was used.
- Missing or invalid named-browser configuration fails clearly instead of
  silently choosing an installed binary.

Partial criteria:

- Named `roamium` launches correctly with the environment override, but the
  negative missing-env path needs a separate experiment because the GUI harness
  cannot safely isolate it.
- Runtime launch works, but a broader packaging/default installed path remains
  intentionally deferred to a later issue.

Fail criteria:

- Named/default `roamium` still logs `named browser launch not implemented`.
- Ghostboard silently spawns a stale installed Roamium during debug testing.
- Absolute-path launch regresses.
- `TERMSURF_SOCKET` discovery or `BrowserReady` regresses.
- The app no longer builds.

## Design Review

Fresh-context adversarial review by Codex subagent `Nash`:

- **Verdict:** Approved.
- **Findings:** None.
- **Notes:** The reviewer verified that the README links Experiment 1 as
  `Designed`, the experiment has the required Description/Changes/Verification
  sections, the scope matches Issue 814, the planned Ghostboard/webtui/Roamium
  launch flow is technically consistent with current code, and the working tree
  contains only the Issue 814 plan docs.

## Result

**Result:** Pass

Implemented named/default `roamium` debug launch resolution in
`ghostboard/src/apprt/termsurf.zig` and added two focused
`scripts/ghostboard-geometry-matrix.sh` scenarios:

- `named-roamium-debug-launch` runs `web` without `--browser`, sets
  `TERMSURF_ROAMIUM_PATH` to the repo-built debug Roamium path, and proves
  Ghostboard resolves the named/default `roamium` browser to that executable.
- `named-roamium-invalid-env` sets `TERMSURF_ROAMIUM_PATH=roamium`, proves
  webtui reaches Ghostboard over `TERMSURF_SOCKET`, verifies Ghostboard logs a
  clear invalid named-browser failure, verifies no pending `default/roamium`
  server is created, and verifies no browser process is spawned.

The resolver preserves the existing absolute-path behavior, keeps the Ghostboard
pane/server/browser key as the requested browser string, uses the resolved
absolute path only when spawning the browser process, and refuses to silently
fall through to installed Roamium paths when the named debug override is missing
or invalid.

Verification commands:

1. `zig fmt ghostboard/src/apprt/termsurf.zig`
2. `bash -n scripts/ghostboard-geometry-matrix.sh`
3. `cd ghostboard && zig build -Demit-macos-app=false`
4. `cd ghostboard && macos/build.nu --scheme Ghostty --configuration Debug --action build`
5. `cargo check -p webtui`
6. `cargo check -p roamium`
7. `git diff --check`
8. `scripts/ghostboard-geometry-matrix.sh initial-open`
9. `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch`
10. `scripts/ghostboard-geometry-matrix.sh named-roamium-invalid-env`

Notes:

- `shellcheck` is not installed on this VM, so that optional check was skipped.

Final runtime evidence:

- Absolute-path baseline:
  - Harness: `logs/ghostboard-geometry-initial-open-harness-20260617-210018.log`
  - App: `logs/ghostboard-geometry-initial-open-app-20260617-210018.log`
  - Roamium trace:
    `logs/ghostboard-geometry-initial-open-roamium-20260617-210018.log`
- Named debug launch:
  - Harness:
    `logs/ghostboard-geometry-named-roamium-debug-launch-harness-20260617-210006.log`
  - App:
    `logs/ghostboard-geometry-named-roamium-debug-launch-app-20260617-210006.log`
  - Roamium trace:
    `logs/ghostboard-geometry-named-roamium-debug-launch-roamium-20260617-210006.log`
- Invalid-env negative path:
  - Harness:
    `logs/ghostboard-geometry-named-roamium-invalid-env-harness-20260617-210006.log`
  - App:
    `logs/ghostboard-geometry-named-roamium-invalid-env-app-20260617-210006.log`

The named debug launch run proved:

- the generated command omitted `--browser`;
- webtui discovered `TERMSURF_SOCKET` and sent `HelloRequest`;
- Ghostboard received `SetOverlay` with `browser=roamium`;
- Ghostboard resolved `roamium` through `TERMSURF_ROAMIUM_PATH` to
  `/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`;
- Ghostboard spawned that debug path;
- Roamium registered, sent `TabReady` and `CaContext`, and received the AppKit
  corrective resize;
- `BrowserReady` preserved `browser=roamium`;
- no stale installed path under `/usr/local` or
  `/opt/homebrew/opt/termsurf-roamium` was spawned.

The invalid-env run proved:

- webtui still discovered `TERMSURF_SOCKET`;
- Ghostboard received `SetOverlay` with `browser=roamium`;
- Ghostboard logged
  `SetOverlay: named browser unresolved browser=roamium env=TERMSURF_ROAMIUM_PATH value=roamium`;
- no pending `default/roamium` server was created;
- no browser process was spawned.

## Completion Review

Fresh-context adversarial review by Codex subagent `Carver`:

- **Initial verdict:** Changes required.
- **Required finding:** The invalid named-browser path resolved too late in the
  new-server branch, after Ghostboard had already created a pending
  `default/roamium` server. A later retry in the same app process could reuse
  that pending server and skip resolver retry.
- **Optional finding:** Named launch recorded the child PID using the resolved
  executable path instead of the server key, so `recordServerChild` could not
  match the `default/roamium` server.
- **Resolution:** Moved browser executable resolution before server reservation,
  clear the pane/server on unusable spawn arguments or listen-socket failure,
  record the child PID with the requested browser key, and added an invalid-env
  harness assertion that no pending `default/roamium` server is created.
- **Re-review verdict:** Approved. The reviewer confirmed the required finding
  is resolved by resolving named browsers before `reserveServer()` and by the
  invalid-env harness assertion at `scripts/ghostboard-geometry-matrix.sh:2368`.
  The reviewer also confirmed `recordServerChild(profile_z, browser, pid)` now
  records the requested server key and found no new required findings.

## Conclusion

Ghostboard now has a deterministic debug-safe path for default/named Roamium
launches. `web https://example.com` can exercise webtui's default browser path
inside the real Ghostboard app when the harness supplies
`TERMSURF_ROAMIUM_PATH`, and misconfigured named-browser launch fails clearly
instead of silently using a stale installed binary.

The remaining Issue 814 work should decide whether normal installed-app
resolution, broader app discovery, or documentation should be handled in a
follow-up experiment.
