# Experiment 19: Add Ghostboard Surfari Launch Path

## Description

Experiments 15-18 proved Surfari can run as a standalone TermSurf browser engine
and now supports the same engine-owned protocol surface as Roamium. The next
step is to let Ghostboard launch Surfari by name, using the same GUI socket,
browser listen socket, pane/server bookkeeping, and overlay routing machinery it
already uses for Roamium.

This experiment is intentionally the first Ghostboard integration step, not the
full in-app behavior matrix. It should prove that a TUI can request
`browser=surfari`, Ghostboard resolves that name to a Surfari executable,
launches the process with `--ipc-socket`, `--listen-socket`, and
`--user-data-dir`, receives `ServerRegister`, sends `CreateTab`, accepts
`TabReady` and `CaContext`, records the tab lookup with browser `surfari`, and
keeps DevTools routing browser-specific.

The current registration path has a correctness problem this experiment must
fix: `ServerRegister` carries only `profile`, while Ghostboard stores servers by
`profile` and `browser`. That means two browser processes for the same profile
can attach to the wrong pending server. This experiment may modify
`termsurf.proto` narrowly to add browser identity to `ServerRegister`, update
Roamium and Surfari to send it, and update Ghostboard to attach by
`profile + browser`.

The experiment should not attempt the full pane/tab/window resize matrix yet;
that belongs in later in-app tests after the launch path is proven.

## Changes

- Update `ghostboard/src/apprt/termsurf.zig` to resolve the named browser
  `surfari` through a new `TERMSURF_SURFARI_PATH` environment variable.
- Keep `roamium` behavior unchanged, including `TERMSURF_ROAMIUM_PATH`, the
  installed Roamium fallback, and debug-build behavior.
- Make profile storage browser-specific instead of always using
  `chromium-profiles`; Surfari profiles should not share Roamium profile
  directories.
- Update `ServerRegister` to include browser identity if that is the cleanest
  way to prevent same-profile Roamium/Surfari cross-attach, then regenerate or
  update generated protobuf bindings as required by the repo.
- Update both Surfari and Roamium to send their browser identity in
  `ServerRegister`.
- Add an automated fake-engine or equivalent integration harness that drives
  Ghostboard's socket protocol far enough to prove launch arguments,
  browser-specific `ServerRegister` matching, `CreateTab`, `TabReady`,
  `CaContext`, and `BrowserReady`.
- Do not modify webtui or `webkit/src` in this experiment. Do not modify
  Surfari, Roamium, or `termsurf.proto` beyond the narrow
  `ServerRegister`-identity work needed to make Ghostboard launch routing
  deterministic.

## Verification

Pass criteria:

- `zig fmt ghostboard/src/apprt/termsurf.zig` succeeds, plus any other edited
  Zig files.
- `cargo fmt -p surfari -- --check` and the equivalent Roamium Rust formatting
  check succeed if Surfari/Roamium Rust files are edited.
- Ghostboard builds after the change:

```bash
cd ghostboard
zig build
```

- The launch-path proof verifies all of the following:
  - `browser=surfari` resolves only from `TERMSURF_SURFARI_PATH`;
  - `browser=roamium` still resolves from the existing Roamium environment
    variable/fallback behavior;
  - an unsupported non-path browser name is still rejected;
  - Surfari gets a browser-specific profile directory, not a `chromium-profiles`
    directory;
  - Ghostboard launches the Surfari process with `--ipc-socket`,
    `--listen-socket`, and `--user-data-dir`;
  - Ghostboard receives `ServerRegister` from the Surfari-side process with
    browser identity `surfari`;
  - Ghostboard rejects or leaves unattached a same-profile `ServerRegister`
    whose browser identity does not match the pending server;
  - same-profile pending Roamium and Surfari servers cannot cross-attach when
    registrations arrive in either order;
  - Ghostboard sends `CreateTab` to the Surfari-side process;
  - Ghostboard accepts `TabReady` and `CaContext` from the Surfari-side process;
  - Ghostboard emits `BrowserReady` to the TUI with browser `surfari`; and
  - DevTools routing for browser `surfari` uses the Surfari server, not a
    Roamium server.
- `git diff --check` succeeds.
- Markdown formatting succeeds for edited issue files:

```bash
prettier --check --prose-wrap always --print-width 80 \
  issues/0756-surfari/README.md \
  issues/0756-surfari/19-ghostboard-surfari-launch.md
```

Direct resolver/profile unit tests without the automated socket handoff proof
are useful but can only produce a `Partial` result. A `Pass` requires automated
evidence that reaches `BrowserReady` and proves browser-specific
`ServerRegister` matching.

## Design Review

Adversarial design review initially returned `CHANGES REQUIRED` with two
required findings. First, the fallback allowed a `Pass` without proving the
Ghostboard launch path through `BrowserReady`. The design now requires automated
socket handoff evidence for `Pass`; resolver-only coverage can only be
`Partial`. Second, the design missed Ghostboard's current same-profile
cross-attach risk: `ServerRegister` only carries `profile`, but pending servers
are keyed by `profile + browser`. The design now requires browser identity in
registration or an equivalent deterministic attach proof, and explicitly allows
the narrow protocol/Roamium/Surfari updates needed to make that correct.
