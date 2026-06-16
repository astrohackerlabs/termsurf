# Experiment 30: Validate Chromium-Output Roamium Lifecycle

## Description

Experiment 29 proved that `target/debug/roamium` is not a runnable browser path
on macOS because Chromium runtime resources are not beside that binary.
TermSurf's established Roamium build flow is different:
`./scripts/build.sh roamium` builds the Cargo binary and copies it into
`chromium/src/out/Default/roamium`, next to Chromium resources such as
`icudtl.dat`, `.pak` files, and `libtermsurf_chromium.dylib`.

This experiment will repeat the normal-tab lifecycle smoke test with the correct
repo-built browser artifact:
`/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`.

The goal is to prove that Ghostboard can launch and coordinate the real Roamium
browser process without modifying `webtui`, `roamium`, Chromium, or the protobuf
schema. This remains a lifecycle experiment, not a native rendering or browser
input-forwarding experiment.

## Changes

Expected code changes are none unless the runtime validation discovers a
Ghostboard-side launch, environment, or protocol lifecycle defect.

If a fix is needed, keep it limited to the smallest relevant Ghostboard files,
likely one of:

- `ghostboard/src/apprt/termsurf.zig`
  - browser launch arguments, server matching, lifecycle state, or logging
    fixes;
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  - terminal environment propagation fixes.

No changes will be made to `webtui`, `roamium`, `proto/termsurf.proto`,
Chromium, branding, icon assets, Xcode project files, CLI install behavior,
native browser overlay presentation, CALayerHost attachment, keyboard/mouse
browser input forwarding, DevTools duplicate detection, browser direct-client
routing changes, or browser process shutdown in this experiment.

## Verification

Pass criteria:

- Build the real `webtui` binary with `cargo build -p webtui`, with the command,
  cwd, and exit status recorded in a log.
- Build and place the real Roamium runtime artifact with
  `./scripts/build.sh roamium`, with the command, cwd, and exit status recorded
  in a log.
- Verify that `/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`
  exists, is executable, and is at least as new as
  `/Users/astrohacker/dev/termsurf/target/debug/roamium`.
- Verify Chromium runtime resources exist beside the browser artifact, including
  `chromium/src/out/Default/icudtl.dat`,
  `chromium/src/out/Default/content_shell.pak`,
  `chromium/src/out/Default/shell_resources.pak`, and
  `chromium/src/out/Default/libtermsurf_chromium.dylib`.
- Record the timestamp and `otool -L` output for
  `chromium/src/out/Default/roamium` and
  `chromium/src/out/Default/libtermsurf_chromium.dylib`. This experiment assumes
  the existing Chromium output is the current repo build; it does not rebuild
  Chromium unless the runtime proves the existing output is stale or
  incompatible.
- If Rust code is modified, run `cargo fmt` as required by `AGENTS.md`. If no
  Rust code is modified, explicitly record that no Rust formatting was required.
- If Zig code is modified, run
  `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  inside `ghostboard/`, with the command, cwd, and exit status recorded in a
  log.
- If Swift code is modified, run the nested Ghostboard `swiftlint` fix and
  non-mutating lint checks for touched Swift files, with commands, cwd, and exit
  statuses recorded in logs.
- If Ghostboard code is modified, the native GhosttyKit framework build passes:
  `zig build -Demit-xcframework=true -Dxcframework-target=native -Demit-macos-app=false`,
  with the command, cwd, and exit status recorded in a log.
- The macOS app build passes:
  `macos/build.nu --scheme Ghostty --configuration Debug --action build`, with
  the command, cwd, and exit status recorded in a log.
- Runtime harness launches `TermSurf.app` with `GHOSTTY_LOG=stderr` and a
  temporary config whose command runs the actual
  `target/debug/web --browser /Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium https://example.com`
  inside the first terminal surface.
- App logs show Ghostboard spawned
  `/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium` with the
  expected browser-server arguments, including `--ipc-socket`,
  `--user-data-dir`, and `--listen-socket`.
- App logs show Roamium connected back as a browser server and sent
  `ServerRegister(profile=default)`.
- App logs show Ghostboard matched the Roamium server and sent `CreateTab` for
  the normal `webtui` pane.
- App logs show Roamium sent `TabReady` for the normal pane.
- App logs show Ghostboard sent `BrowserReady` to the normal `webtui` pane with
  a nonempty browser listen socket and browser path
  `/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`.
- Logs show Roamium sent at least one browser-originated page state message
  after `CreateTab`, preferably `CaContext`. It is acceptable in this experiment
  if Ghostboard logs that message as ignored, because native overlay
  presentation is explicitly out of scope.
- The normal `webtui` process receives `BrowserReady` and connects to Roamium's
  direct browser socket. This can be proven by downstream app/Roamium log
  activity rather than screen scraping.
- A `web status` or `web last` query against the captured normal pane
  `TERMSURF_SOCKET` and `TERMSURF_PANE_ID` returns the normal Roamium tab.
- Runtime shutdown removes the GUI socket file and leaves no stale matching
  `TermSurf.app/Contents/MacOS/termsurf`, `target/debug/web`, or
  `chromium/src/out/Default/roamium` processes.
- `git diff --check` is clean.

Fail criteria:

- The runtime uses a fake helper, installed browser, or `target/debug/roamium`
  instead of `/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`.
- Roamium is modified to accommodate Ghostboard.
- `webtui` is modified to accommodate Ghostboard.
- Ghostboard does not launch Roamium or launches it without the required
  `--ipc-socket` / `--listen-socket` arguments.
- Roamium does not connect back with `ServerRegister(profile=default)`.
- Ghostboard does not send `CreateTab` to the attached Roamium server.
- Roamium does not send `TabReady`.
- Ghostboard does not send `BrowserReady` to `webtui`.
- `web last` / `web status` cannot find the normal Roamium tab after
  `BrowserReady`.
- The implementation adds CALayerHost overlay presentation, keyboard/mouse
  browser input forwarding, DevTools duplicate detection, browser shutdown,
  browser direct-client routing changes, Chromium changes, `webtui` changes,
  `roamium` changes, or protobuf schema changes in this experiment.

## Design Review

A fresh-context adversarial Codex subagent reviewed the Experiment 30 design and
returned **APPROVED** with no required findings.

The reviewer confirmed that the README links Experiment 30 as **Designed**, the
experiment has Description, Changes, and Verification sections, the design uses
`./scripts/build.sh roamium` and
`/Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium`, avoids
`webtui`, `roamium`, Chromium, and protobuf changes, includes lifecycle proof
against the real repo-built Roamium path, checks for Chromium resources,
includes Rust/Zig/Swift hygiene, and requires `git diff --check`.

The reviewer had one optional finding: resource checks prove the Experiment 29
missing-ICU issue is avoided, but should also clarify whether the existing
Chromium output is assumed current. The design was updated to record timestamps
and `otool -L` output for the copied Roamium artifact and
`libtermsurf_chromium.dylib`, and to state that this experiment assumes the
existing Chromium output is current unless runtime evidence proves it stale or
incompatible.
