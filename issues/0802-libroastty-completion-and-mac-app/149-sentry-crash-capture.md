# Experiment 149: Phase I — Sentry crash capture

## Description

Port the init/capture half of upstream `crash/sentry.zig` into Roastty. Roastty
already has the local crash directory, Sentry envelope parser, and local
envelope persistence from the first half of `crash/`; this experiment wires a
real Sentry client into `roastty_init` so panic/crash events are captured as
Sentry envelopes and written locally as `.roasttycrash` reports.

The privacy boundary must match upstream: Sentry may be used to collect crash
metadata and produce envelopes, but Roastty must not send crash data over the
network. Upstream accomplishes that with a Sentry SDK custom transport; Roastty
will do the same with the Rust Sentry SDK.

This experiment intentionally does not port upstream's thread-local surface
metadata or the `crash:io` / `crash:render` mailbox behavior. Those require the
later thread-specific crash channels called out by Experiment 126. The scope
here is process-wide init, panic capture, local-only transport, and event
envelope persistence.

## Changes

- `roastty/Cargo.toml` / `Cargo.lock`
  - Add the Rust `sentry` SDK with `default-features = false` and
    `features = ["panic"]`; add any other Sentry feature only if it is
    deliberately chosen and does not enable a network transport.
  - Do not enable Sentry's default HTTP transport features (`transport`,
    `reqwest`, TLS, `ureq`, `curl`).
- `roastty/src/crash.rs`
  - Replace the module note that says Roastty does not initialize Sentry.
  - Add process-wide Sentry state that stores the `ClientInitGuard` for the
    lifetime of the library process.
  - Add an idempotent `init()` entry point used by `roastty_init`.
  - Configure `sentry::ClientOptions` with:
    - a fixed local DSN sufficient to enable the client;
    - a custom transport implementing `sentry::Transport`;
    - `default_integrations = true` only if the selected feature set includes
      panic capture without network transport;
    - `shutdown_timeout = 0` or another bounded nonblocking timeout, since the
      transport writes synchronously to disk.
  - Implement the transport by serializing each Sentry envelope and feeding the
    bytes into `CrashDir::default().persist_event_envelope(...)`, preserving the
    existing behavior that non-event envelopes are discarded and malformed
    envelopes report an error instead of creating a directory.
  - Set upstream-equivalent baseline tags where they are available in Roastty
    today, such as build mode and renderer; leave app-runtime/font-backend
    values as explicit TODOs if the Rust port has no faithful source of truth.
  - Add tests proving:
    - `init()` is idempotent;
    - the custom transport writes event envelopes to the configured crash
      directory;
    - non-event envelopes are discarded;
    - captured panic/event paths use the local transport rather than any network
      transport.
- `roastty/src/lib.rs`
  - Call `crash::init()` from `roastty_init` after argv capture succeeds.
  - Return `ROASTTY_SUCCESS` even if a second `roastty_init` call repeats crash
    initialization; duplicate app init must not panic.

## Verification

- `cargo fmt`
- `cargo test -p roastty crash -- --test-threads=1`
- `cargo test -p roastty sentry -- --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty -- --test-threads=1`
- `cd roastty && macos/build.nu --action test`
- `cargo fmt --check`
- `git diff --check`
- `rg -n "ghosttycrash|ghostty/sentry|ghostty/crash" roastty/src roastty/Cargo.toml`
  must show no newly introduced Ghostty crash-report paths or extensions.
- `cargo tree -p roastty -i reqwest`, `cargo tree -p roastty -i ureq`,
  `cargo tree -p roastty -i curl`, `cargo tree -p roastty -i native-tls`, and
  `cargo tree -p roastty -i rustls` must fail to find those crates through the
  Sentry dependency path, proving the Sentry network/TLS transports were not
  enabled.

**Pass** = `roastty_init` initializes Sentry capture once, a captured Rust panic
or event produces a local `.roasttycrash` event envelope through the custom
transport, no Sentry HTTP transport dependencies are present, and all focused,
ABI, full Rust, hosted macOS, and hygiene checks pass.

**Partial** = local transport and persistence work, but panic capture needs a
follow-up hook or the full test suite reveals lifecycle ordering that must be
fixed separately.

**Fail** = the Rust Sentry SDK cannot be integrated without network transport
dependencies or without changing the copied app / embedded ABI contract.

## Design Review

**Reviewer:** Codex-native adversarial subagent with fresh context, using the
`adversarial-review` skill's Codex path (`multi_agent_v1.spawn_agent`), not
Claude's named `adversarial-reviewer` agent.

**Status:** Approved.

**Findings:** No Required findings. The reviewer approved the design and noted
two Optional improvements:

- Make the Sentry feature set explicit because `default-features = false`
  disables the panic integration unless `features = ["panic"]` is selected.
- Add TLS dependency checks because the design forbids TLS transport features,
  but the initial verification list only checked HTTP transport crates.

**Fixes:** Accepted both Optional findings. The dependency plan now explicitly
requires `features = ["panic"]`, and verification now checks `native-tls` and
`rustls` in addition to `reqwest`, `ureq`, and `curl`.

**Final verdict:** Approved.
