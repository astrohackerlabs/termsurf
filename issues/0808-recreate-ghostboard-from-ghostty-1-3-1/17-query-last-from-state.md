# Experiment 17: Reply To QueryLast From State

## Description

Experiment 16 records `last_browser_pane` and the pane's real `tab_id` after
`TabReady`. `QueryLastRequest` still returns the old no-state error from
Experiment 10. The next incremental protocol step is to make `QueryLastRequest`
use the state that now exists.

In Wezboard, `QueryLastRequest`:

- returns `No browser pane yet` if there is no last browser pane;
- returns `Last pane no longer exists` if the stored last pane id is stale;
- returns `No matching pane for profile` if the request profile is nonempty and
  does not match the last pane's profile;
- otherwise returns `pane_id`, `tab_id`, `profile`, and an empty error.

This experiment will implement the same state-backed reply in Ghostboard. It is
still a query-only step: no `BrowserReady`, browser process launch, overlay
presentation, or input forwarding.

## Changes

- `ghostboard/src/apprt/termsurf.zig`
  - update `sendQueryLastReply` to accept the decoded `QueryLastRequest`;
  - under the state mutex, look up `last_browser_pane`;
  - populate `QueryLastReply.pane_id`, `tab_id`, and `profile` when the last
    pane exists and the optional profile filter matches;
  - return the same error strings as Wezboard for no last pane, stale last pane,
    and profile mismatch;
  - preserve the Experiment 10 no-state behavior when no browser pane exists.

No changes will be made to `webtui`, `roamium`, `proto/termsurf.proto`,
branding, app config paths, icon assets, Xcode project files, CLI install
behavior, browser process launch, `BrowserReady`, overlay presentation, or input
forwarding.

## Verification

Pass criteria:

- `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  passes inside `ghostboard/`.
- The native GhosttyKit framework build passes:
  `zig build -Demit-xcframework=true -Dxcframework-target=native -Demit-macos-app=false`.
- The macOS app build passes:
  `macos/build.nu --scheme Ghostty --configuration Debug --action build`.
- Runtime harness launches `TermSurf.app`, connects to `TERMSURF_SOCKET`, and
  proves:
  - before any `TabReady`, `QueryLastRequest(profile=default)` still returns
    `error = "No browser pane yet"`;
  - after `SetOverlay -> ServerRegister -> CreateTab -> TabReady(pane-a, 42)`,
    `QueryLastRequest(profile=default)` returns `pane_id=pane-a`, `tab_id=42`,
    `profile=default`, and empty error;
  - `QueryLastRequest(profile="")` also returns the same last pane;
  - `QueryLastRequest(profile=other)` returns
    `error = "No matching pane for profile"`;
  - no `BrowserReady`, browser process launch, or overlay presentation logs are
    emitted by this experiment.
- The runtime harness also sends a normal TUI `HelloRequest` on a fresh socket
  and receives `HelloReply`, proving existing request/reply behavior still
  works.
- The harness verifies shutdown cleanup still removes the socket file and leaves
  no stale `TermSurf.app/Contents/MacOS/termsurf` process.
- `git diff --check` is clean.

Fail criteria:

- `QueryLastRequest` ignores existing `last_browser_pane` state.
- The successful reply has the wrong `pane_id`, `tab_id`, `profile`, or nonempty
  error.
- The no-state or profile-mismatch error strings diverge from Wezboard.
- The implementation sends `BrowserReady`, launches a browser process, or
  creates overlay UI in this experiment.
- Browser/TUI classification or the synchronous request/reply paths from
  Experiments 8 through 16 regress.
- Any `webtui`, `roamium`, protocol schema, app branding, config path, icon, or
  CLI install behavior changes are needed for this experiment.

## Design Review

Fresh-context adversarial design review returned **APPROVED** with no required
findings.

The reviewer confirmed the README links Experiment 17 as `Designed`, the
experiment has the required sections, the scope is narrow, the `QueryLast` plan
matches Wezboard's state-backed behavior while excluding `BrowserReady`,
rendering, and browser launch, and verification covers no-state, success,
empty-profile success, profile mismatch, builds, runtime regression checks,
cleanup, and `git diff --check`.

## Result

**Result:** Pass

Implemented state-backed `QueryLastRequest` handling in
`ghostboard/src/apprt/termsurf.zig`.

The socket handler now passes the decoded `QueryLastRequest` into
`sendQueryLastReply`. The reply helper reads `last_browser_pane` under the state
mutex, verifies that the stored pane still exists, applies the optional profile
filter, and fills `QueryLastReply.pane_id`, `tab_id`, and `profile` from the
recorded pane state. It preserves the Wezboard-compatible error strings for the
no-pane, stale-pane, and profile-mismatch cases.

Verification passed:

- `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  passed.
- Native GhosttyKit framework build passed:
  `logs/ghostboard-exp17-zig-native-xcframework-20260616-105017.log`.
- macOS app build passed:
  `logs/ghostboard-exp17-macos-build-debug-20260616-105043.log`.
- Runtime harness passed:
  `logs/ghostboard-exp17-runtime-harness-20260616-105343.log`.
- Runtime app log: `logs/ghostboard-exp17-runtime-app-20260616-105343.log`.
- `git diff --check` passed.

Observed successful runtime checks:

```text
PASS: QueryLast before TabReady returns no-browser-pane error
PASS: browser socket received pane-a CreateTab
PASS: QueryLast default returns last pane state
PASS: QueryLast empty profile returns last pane state
PASS: QueryLast mismatched profile returns profile error
PASS: fresh TUI client received HelloReply
PASS: app exited after SIGTERM
PASS: socket file removed after shutdown
PASS: no stale TermSurf process remains
PASS: app log contains TermSurf socket listening
PASS: app log contains QueryLastReply sends
PASS: app log contains pending=false TabReady
PASS: no BrowserReady emitted
PASS: no CaContext emitted
PASS: no overlay presentation message emitted
PASS: no browser launch message emitted
runtime verification passed
```

The passing harness verified these concrete replies:

- before `TabReady`, `QueryLastRequest(profile=default)` returned
  `error = "No browser pane yet"`;
- after `SetOverlay -> ServerRegister -> CreateTab -> TabReady(pane-a, 42)`,
  `QueryLastRequest(profile=default)` returned `pane_id = "pane-a"`,
  `tab_id = 42`, `profile = "default"`, and empty `error`;
- `QueryLastRequest(profile="")` returned the same last pane;
- `QueryLastRequest(profile="other")` returned
  `error = "No matching pane for profile"`.

## Conclusion

Ghostboard now answers `QueryLastRequest` from the pane/tab state introduced by
Experiments 14 through 16. This gives `webtui` a usable last-browser-pane lookup
after a browser reports `TabReady`, while keeping browser launch,
`BrowserReady`, CALayerHost overlay presentation, and input forwarding out of
scope for this experiment.

## Result Review

Fresh-context adversarial result review returned **APPROVED** with no required,
optional, or nit findings.

The reviewer confirmed:

- the changed scope is limited to the expected source file and issue docs;
- `QueryLastRequest` behavior matches the Wezboard reference;
- the runtime harness exercises no-state, success, empty-profile, and profile
  mismatch cases;
- the README status is `Pass`;
- the experiment file has `Result` and `Conclusion`;
- the result commit had not been made before review;
- `git diff --check` and `zig fmt --check` pass.
