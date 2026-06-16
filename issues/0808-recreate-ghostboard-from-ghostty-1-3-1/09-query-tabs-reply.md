# Experiment 9: Reply To QueryTabsRequest

## Description

Experiment 8 proved that Ghostboard can accept TermSurf socket clients, decode
the current protobuf schema, and answer `HelloRequest`. The next smallest step
toward running the existing `webtui` is to answer `QueryTabsRequest`, because
`webtui` already sends that message through the same synchronous request/reply
path and expects a `QueryTabsReply`.

This experiment will implement only the baseline GUI-side reply for the current
Ghostboard state. Ghostboard does not yet launch Roamium, register browser
servers, or maintain browser tab inventory, so the correct initial reply is an
empty successful inventory:

- `gui_panes = 0`
- `chromium_tabs = 0`
- `chromium_browser = 0`
- `chromium_devtools = 0`
- `tabs = []`
- `error = ""`

This intentionally mirrors the current Wezboard behavior before any browser tabs
exist, while keeping browser launch, overlay setup, pane tracking, and tab
registration for later experiments.

## Changes

- `ghostboard/src/apprt/termsurf.zig`
  - recognize `QueryTabsRequest` in the decoded `TermSurfMessage` switch;
  - log the request's `pane_id` and `profile`;
  - send a length-prefixed `QueryTabsReply` with zero counts, no tab entries,
    and an empty error string.

No changes will be made to `webtui`, `roamium`, `proto/termsurf.proto`,
branding, app config paths, icon assets, Xcode project files, or CLI install
behavior.

## Verification

Pass criteria:

- `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  passes inside `ghostboard/`.
- The native GhosttyKit framework build passes.
- The macOS app build passes.
- Runtime harness launches `TermSurf.app`, connects to `TERMSURF_SOCKET`, sends
  a length-prefixed current-schema `QueryTabsRequest`, and decodes a
  length-prefixed `QueryTabsReply`.
- The decoded reply has `gui_panes = 0`, `chromium_tabs = 0`,
  `chromium_browser = 0`, `chromium_devtools = 0`, no `tabs`, and empty `error`.
- The runtime harness also sends `HelloRequest` before or after
  `QueryTabsRequest` to prove Experiment 8's behavior still works on the same
  socket implementation.
- The app log contains `TermSurf message decoded type=QueryTabsRequest` and a
  reply-sent log for `QueryTabsReply`.
- Shutdown cleanup still removes the socket file and leaves no stale
  `TermSurf.app/Contents/MacOS/termsurf` process.
- `git diff --check` is clean.

Fail criteria:

- `QueryTabsRequest` is ignored or returns no frame.
- The reply has the wrong oneof message type.
- Any count is nonzero before Ghostboard has implemented browser/tab state.
- Any `webtui`, `roamium`, protocol schema, app branding, config path, icon, or
  CLI install behavior changes are needed for this experiment.

## Design Review

Fresh-context adversarial design review returned `APPROVED` with no required
findings.

The reviewer checked that the README links Experiment 9 as `Designed`, the
experiment has the required sections, the scope is limited to
`ghostboard/src/apprt/termsurf.zig`, the protocol schema supports the proposed
`QueryTabsReply` fields, Wezboard returns an empty Chromium inventory when no
browser tabs exist, Ghostboard currently has no browser/tab registry, and the
verification covers formatting, builds, runtime harness behavior, shutdown
cleanup, and `git diff --check`.
