# Experiment 4: Prove HTTP Auth Runtime Flow

## Description

Issue 816 still needs Ghostboard-specific runtime proof for HTTP auth. Issue 799
added protocol-mediated HTTP Basic Auth, and Issue 810 classified the current
Ghostboard evidence as `Maybe`: static direct-path evidence is strong, but no
current Ghostboard runtime test proves that webtui receives auth challenges,
renders the credential prompt, sends replies, and unblocks Chromium.

This experiment will prove the normal direct Roamium-to-webtui path for
origin-server HTTP Basic Auth under Ghostboard. It will use a contained local
HTTP server and test credentials only. It must not add password persistence,
native dialogs, proxy auth, OS keychain integration, or broad Chrome credential
UI.

## Changes

Planned investigation:

- Inspect the current auth request/reply path in:
  - `proto/termsurf.proto`;
  - `roamium/src/dispatch.rs`;
  - `roamium/src/ipc.rs`;
  - `webtui/src/ipc.rs`;
  - `webtui/src/main.rs`;
  - `ghostboard/src/apprt/termsurf.zig`;
  - the Issue 799 HTTP auth result in
    `issues/0799-browser-api-automation-triage/08-http-basic-auth.md`.
- Confirm that existing Roamium diagnostics do not log passwords. If additional
  stable trace evidence is needed, trace only request metadata, accepted/cancel
  state, and username presence or length; never trace the password.

Planned harness changes:

- Add an `http-auth-smoke` scenario to `scripts/ghostboard-geometry-matrix.sh`.
- Serve a local HTTP Basic Auth fixture with:
  - a protected success page requiring `user:passwd`;
  - a protected cancel page where Esc cancellation should leave the browser
    usable and not authenticate;
  - a public page that can be loaded after auth cancellation to prove recovery.
- Launch debug Ghostboard, debug webtui, and debug Roamium using the same
  no-installed-binary guarantees as the existing Issue 816 scenarios.
- Capture app log, Roamium trace, webtui state trace, screenshots, and terminal
  input coordinates.
- Extend the test-only webtui state trace if needed so auth request, rendered
  auth mode, key-driven reply, cancellation, and restored mode are observable
  without OCR.
- Drive credentials with automated keyboard input in the actual Ghostboard
  window:
  - type username, Enter to move to the password field;
  - type password, Enter to submit;
  - Esc to cancel a separate auth prompt.

Planned fix policy:

- If Roamium receives and sends `HttpAuthRequest` but webtui does not enter or
  render auth mode, fix webtui.
- If webtui sends `HttpAuthReply` but Roamium does not call the Chromium reply
  FFI, fix Roamium dispatch.
- If Chromium/libtermsurf emits the auth request before any direct client can
  receive it, prove the timing with trace evidence and then fix the owning
  component. Do not pass by dropping the blocking auth request.
- If the direct path passes but Ghostboard compositor fallback is the only
  missing path, record that as a lower-priority resilience finding rather than
  broadening this experiment into fallback routing.

Planned issue-doc changes:

- Add this experiment to the Issue 816 README with status `Designed`.
- Record request metadata, rendered UI evidence, reply evidence, page-observed
  authenticated result, cancel result, and owner.
- Record remaining Issue 816 gaps for later experiments, especially renderer
  crash recovery, color scheme, and copy-current-URL.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/04-prove-http-auth-runtime-flow.md`.
2. If Rust files change, `cargo fmt -- <changed-rust-files>`.
3. If Zig files change, `zig fmt <changed-zig-files>`.

Static/build checks:

1. `prettier --check --prose-wrap always --print-width 80 issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/04-prove-http-auth-runtime-flow.md`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. `cargo check -p webtui` if webtui changes.
4. `cargo build -p webtui` if webtui changes.
5. `cargo check -p roamium` if Roamium changes.
6. `./scripts/build.sh roamium` if Roamium changes.
7. `./scripts/build.sh chromium` only if Chromium changes.
8. If Ghostboard Zig or non-`macos/` Ghostboard files change, run
   `cd ghostboard && zig build -Demit-macos-app=false`.
9. If Ghostboard app files change or a Ghostboard rebuild is needed, run
   `cd ghostboard && macos/build.nu --configuration Debug --action build`.
10. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
11. `git diff --check`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh http-auth-smoke`.
2. Confirm Roamium logs or traces an `HttpAuthRequest` with matching `tab_id`,
   `request_id`, URL, scheme, challenger, realm, proxy flag, and first-attempt
   flag.
3. Confirm webtui records or renders auth mode for the request before the
   automated reply.
4. Confirm the accepted reply uses the expected username, does not expose the
   password in logs/traces, and Roamium records a matching reply with `ok=true`.
5. Confirm the authenticated page loads and reports a unique success marker.
6. Confirm a second auth request can be canceled with Esc, Roamium records
   `accepted=false`, the password is not logged, and a later public navigation
   still works.
7. Confirm webtui returns to the previous mode after accepted and canceled auth
   replies.

Pass criteria:

- HTTP Basic Auth success and cancellation both pass under debug Ghostboard with
  request, rendered UI, reply, and page-observed result evidence.
- The password is not present in app log, Roamium trace, webtui state trace, or
  harness log.
- The harness contains durable assertions for both success and cancellation.
- Any app code change is owned by the component proven responsible and is no
  broader than needed.

Partial criteria:

- The auth request reaches webtui and ownership is proven, but one of success,
  cancellation, or post-cancel recovery fails.
- The owner is proven, but the fix requires Chromium branch work that cannot be
  completed in this experiment.

Fail criteria:

- The harness cannot distinguish request delivery, visible auth mode, reply
  delivery, and page-observed result.
- The scenario passes only by reading Roamium logs without proving webtui UI or
  page behavior.
- The implementation logs passwords, relies on native OS dialogs, drops a
  blocking auth request, or weakens auth assertions.

## Design Review

Fresh-context adversarial review by Codex subagent `Helmholtz`:

- **Initial verdict:** Changes required.
- **Required finding:** The design allowed Ghostboard/Zig app fixes but did not
  include concrete Ghostboard build/check commands beyond `zig fmt`.
- **Resolution:** Accepted. The verification plan now requires
  `cd ghostboard && zig build -Demit-macos-app=false` if Ghostboard Zig or
  non-`macos/` files change, and
  `cd ghostboard && macos/build.nu --configuration Debug --action build` if
  Ghostboard app files change or a Ghostboard rebuild is needed.
- **Re-review verdict:** Approved. The reviewer confirmed the prior finding was
  resolved and no new required findings were introduced.
