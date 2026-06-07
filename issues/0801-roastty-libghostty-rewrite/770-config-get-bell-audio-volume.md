+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 770: Config Get Bell Audio Volume

## Description

Wire the `bell-audio-volume` config key into Roastty's aggregate config and the
public `roastty_config_get` C ABI.

Upstream stores `bell-audio-volume` as an `f64` with a default of `0.5`. Roastty
currently returns that default directly from `roastty_config_get`, and
`config::Config` has no bell audio volume field. Experiment 769 added the shared
scalar `f64` setter and generalized float formatting, so this experiment should
reuse that machinery instead of adding another parser path.

This experiment only ports config parsing, formatting, storage, and lookup. It
does not wire runtime bell audio playback, `bell-features`, or
`bell-audio-path`.

## Changes

- `roastty/src/config/mod.rs`
  - Add `bell_audio_volume: f64` to `config::Config`.
  - Default it to `0.5`, matching upstream.
  - Include `bell-audio-volume` in `format_config` immediately before
    `notify-on-command-finish`, preserving the currently implemented upstream
    order among available fields.
  - Update the full key-order test to assert `background-opacity`,
    `bell-audio-volume`, then `notify-on-command-finish`.
  - Route `Config::set("bell-audio-volume", ...)` through the existing
    `set_f64_field` helper.
  - Add aggregate tests for defaults, formatting, set routing, empty reset,
    missing values, invalid values, finite out-of-range values that should stay
    unclamped, f64 precision, and clone/partial-eq behavior.
- `roastty/src/lib.rs`
  - Make `roastty_config_get("bell-audio-volume")` read
    `config.parsed.bell_audio_volume` instead of writing the hard-coded default.
  - Add C ABI tests proving the key reflects default, file-loaded, CLI-loaded,
    cloned, reset-to-default, unclamped, and f64-precision values.
  - Add C ABI diagnostic tests proving bare CLI values report `ValueRequired`,
    invalid CLI values report `InvalidValue`, and the getter remains at the
    `0.5` default after those failed parses.

## Verification

- `cargo test -p roastty bell_audio_volume -- --nocapture --test-threads=1`
- `cargo test -p roastty config_get_bell_audio_volume -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

The experiment passes if `bell-audio-volume` is stored in aggregate config, can
be set through file and CLI loading, formats in the full config output in
upstream order among implemented fields, rejects missing and invalid values
consistently with upstream scalar floats, resets on an empty value, preserves
unclamped finite values, preserves values that would visibly narrow if stored as
`f32`, and is returned by `roastty_config_get` from parsed state.

## Design Review

Codex reviewed the design and found no blocking findings. The review confirmed
that `bell-audio-volume` has the same scalar `f64` shape as
`background-opacity`, that the ABI getter should continue to write a C `double`,
and that reusing the Experiment 769 `set_f64_field` path is the right scope.

The plan was amended to make two non-blocking test expectations explicit: public
C ABI diagnostics for bare and invalid CLI values, and full formatter key-order
coverage that inserts `bell-audio-volume` between `background-opacity` and
`notify-on-command-finish`.
