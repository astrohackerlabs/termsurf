# Experiment 97: Phase F — config finalize scalar tail

## Description

Port the remaining scalar tail of upstream `Config.finalize()` that is now
unblocked by the completed public config field set:

- reset an empty `term` back to upstream's `xterm-ghostty`
- clamp `minimum-contrast` to `[1, 21]`
- clamp `faint-opacity` to `[0, 1]`
- fill a missing `auto-update-channel` from the build release channel

Upstream performs these in `vendor/ghostty/src/config/Config.zig`:

```zig
if (self.term.len == 0) {
    self.term = "xterm-ghostty";
}

self.@"minimum-contrast" = @min(21, @max(1, self.@"minimum-contrast"));

if (self.@"auto-update-channel" == null) {
    self.@"auto-update-channel" = build_config.release_channel;
}

self.@"faint-opacity" = std.math.clamp(self.@"faint-opacity", 0.0, 1.0);
```

The upstream release channel is derived at build time from the semantic version:
stable when there is no prerelease component, tip otherwise. This issue's pinned
Ghostty source is `version = "1.3.2-dev"`, so the matching pinned build channel
for Roastty is `tip`. This experiment should add a small local constant for that
pinned channel rather than implementing a broader build-options system.

This experiment must not implement theme loading, conditional reload,
working-directory default resolution, app-runtime-specific GTK defaults, link
matcher mutation, or key-remap finalization.

## Changes

- `roastty/src/config/mod.rs`
  - Add a local pinned build release-channel constant set to
    `ReleaseChannel::Tip`, with a comment tying it to the issue's pinned
    `1.3.2-dev` Ghostty source.
  - Extend `Config::finalize()` to:
    - restore `term` to `xterm-ghostty` if it is empty
    - clamp `minimum_contrast` to `[1.0, 21.0]`
    - clamp `faint_opacity` to `[0.0, 1.0]`
    - set `auto_update_channel` to the pinned build channel when unset
  - Add tests for the new finalize behavior while preserving raw parser state
    before finalization.
  - Update any stale comments that still claim `faint-opacity` is not finalized.

## Verification

Pass criteria:

1. `cargo test -p roastty config_finalize_scalar_tail`
2. `cargo test -p roastty config_opacity_options_parse_and_round_trip`
3. `cargo test -p roastty async_update_config`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run must pass. The existing ABI harness may
print its known enum-conversion warnings, but no new failures are acceptable.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb5b9-8be3-7981-b4a8-bf92125b4e26`.

Verdict: **APPROVED**

Findings: None.
