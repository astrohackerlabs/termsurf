# Experiment 1: Send Deterministic HelloReply Defaults

## Description

Ghostboard currently replies to `HelloRequest`, but the reply is initialized
with protobuf defaults and does not populate `homepage` or `browsers`. webtui
expects `HelloReply` to provide live GUI config, uses the first browser as the
default when `--browser` is omitted, and falls back to
`https://termsurf.com/welcome` only when the homepage is absent or empty.

This experiment will make Ghostboard's baseline `HelloReply` deterministic by
sending the documented default homepage and a default browser list containing
`roamium`. It will not add custom Ghostboard config parsing yet. The result will
establish a known-good fallback contract that later config-specific experiments
can override.

## Changes

Planned source changes:

- `ghostboard/src/apprt/termsurf.zig`
  - Add constants for the default HelloReply homepage and browser list.
  - Populate `reply.homepage`, `reply.browsers`, and `reply.n_browsers` in
    `sendHelloReply`.
  - Log the sent homepage and browser list so the real app harness can assert
    the send-side homepage/browser data. webtui browser consumption will be
    proven separately by the existing omitted-`--browser` path reaching
    `SetOverlay` with `browser=roamium`.
  - Keep storage static or otherwise valid for the duration of `sendProtobuf`.

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add log assertions to the existing named/default Roamium launch path proving
    Ghostboard sends a non-empty `HelloReply` with homepage and `roamium`.
  - Keep the existing `named-roamium-debug-launch` evidence that webtui omits
    `--browser`, receives the default browser, sends `SetOverlay` with
    `browser=roamium`, and reaches `BrowserReady`.
  - If needed, add a lightweight no-GUI or negative scenario to assert the
    static fallback values without repeating the full GUI path.

Planned issue-doc changes:

- Record the default values and explain that custom config parsing remains out
  of scope for Experiment 1.
- Record build/runtime evidence and reviewer verdict.

## Verification

Static/build checks:

1. `prettier --write --prose-wrap always --print-width 80 issues/0815-ghostboard-hello-reply-config/README.md issues/0815-ghostboard-hello-reply-config/01-send-deterministic-hello-reply-defaults.md`.
2. `zig fmt ghostboard/src/apprt/termsurf.zig`.
3. `bash -n scripts/ghostboard-geometry-matrix.sh`.
4. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
5. `cd ghostboard && zig build -Demit-macos-app=false`.
6. `cd ghostboard && macos/build.nu --scheme Ghostty --configuration Debug --action build`.
7. `git diff --check`.

Runtime checks:

1. Run `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch`.
2. Verify Ghostboard logs a decoded `HelloRequest`.
3. Verify Ghostboard logs a `HelloReply` with homepage
   `https://termsurf.com/welcome` and browser `roamium`.
4. Verify webtui's omitted-browser path still produces `SetOverlay` with
   `browser=roamium`.
5. Verify `BrowserReady` preserves `browser=roamium`.

Pass criteria:

- Ghostboard sends `HelloReply.homepage=https://termsurf.com/welcome`.
- Ghostboard sends `HelloReply.browsers=["roamium"]`.
- webtui consumes the default browser from `HelloReply` in the
  omitted-`--browser` path.
- Existing named/default Roamium debug launch remains green.
- The app builds and the diff passes formatting checks.

Partial criteria:

- Static defaults work and are verified, but a separate experiment is needed for
  custom config-file parsing.

Fail criteria:

- `HelloReply` remains empty.
- webtui falls back to an empty browser and fails to launch named/default
  Roamium.
- The change breaks absolute-path launch, named/default launch, or the app
  build.

## Design Review

Fresh-context adversarial review by Codex subagent `Copernicus`:

- **Verdict:** Approved.
- **Findings:** None required.
- **Optional finding:** Add markdown formatting to the verification list because
  the experiment edits issue docs.
- **Nit:** Clarify that the homepage is proven by Ghostboard's send-side log,
  while webtui browser consumption is proven by `SetOverlay browser=roamium`.
- **Resolution:** Accepted both suggestions and updated the Changes and
  Verification sections.
