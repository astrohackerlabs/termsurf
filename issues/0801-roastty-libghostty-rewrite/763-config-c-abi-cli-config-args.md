+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 763: Config C ABI CLI Config Args

## Description

Extend `roastty_config_load_cli_args` so the C ABI applies normal config CLI
arguments through Roastty's parsed config layer, not only the custom `--keybind`
parsing added by earlier experiments.

The internal config module already supports upstream-style CLI config parsing
with `Config::set_cli_args`: `--key=value` and bare boolean flags are applied as
config fields, diagnostics are collected for unknown fields and invalid values,
and `config-file` paths are expanded relative to the current working directory.
The ABI currently ignores those arguments, so callers using the public load
sequence cannot configure fields such as `confirm-close-surface` or
`config-file` through argv.

This experiment keeps action and config parsing distinct. `--keybind` remains
handled by the existing keybind path because it feeds Roastty's binding trigger
table. Other argv entries are forwarded to the parsed config layer only when
they are config-looking flags that begin with `--`; positional/action/runtime
arguments remain outside this layer.

Because `roastty_init` stores raw C argv bytes and `Config::set_cli_args`
accepts `&str`, this experiment also defines the ABI conversion behavior:
non-keybind config-looking flags that are not valid UTF-8 are skipped and
reported through the existing ABI diagnostics channel. They must not panic
across the FFI boundary or be converted lossily.

## Changes

- `roastty/src/lib.rs`
  - Update `roastty_config_load_cli_args` to collect non-keybind argv entries
    after argv[0] only when they begin with `--`, then apply valid UTF-8 entries
    with `config.parsed.set_cli_args`.
  - Preserve the existing `--keybind=value` and `--keybind value` behavior,
    including missing-value and invalid-action diagnostics.
  - Skip non-config-looking argv entries instead of passing them to
    `Config::set_cli_args`.
  - Record invalid UTF-8 config-looking argv entries as ABI diagnostics instead
    of panicking or converting them lossily.
  - Record parsed config CLI diagnostics through the existing
    `push_config_diagnostic` helper, using a CLI source label.
  - Call `sync_from_parsed_config` after applying parsed CLI args so ABI-visible
    app and surface behavior reflects parsed CLI config fields.
- Tests in `roastty/src/lib.rs`
  - CLI config args apply ABI-visible state such as
    `--confirm-close-surface=always`.
  - CLI config-file args populate the typed config-file list with current-dir
    expansion, then recursive loading can consume that list.
  - CLI config diagnostics for unknown fields or invalid values are exposed
    through `roastty_config_diagnostics_count` /
    `roastty_config_get_diagnostic`.
  - Positional/action/runtime argv entries are ignored by this config layer.
  - Invalid UTF-8 config-looking argv entries produce diagnostics and do not
    panic.
  - Existing keybind CLI behavior still passes, including paired
    `--keybind value` handling.
  - A mixed argv list with config args before, between, and after keybind args
    applies both config settings and keybind triggers.

## Verification

- `cargo test -p roastty config_c_abi_cli_config -- --nocapture --test-threads=1`
- `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

The experiment passes if the C ABI CLI loader applies parsed config CLI fields,
keeps existing keybind parsing behavior intact, surfaces parsed CLI diagnostics
through the existing diagnostic channel, ignores non-config-looking outer argv
entries, handles invalid UTF-8 without panicking, and leaves later full upstream
replay sequencing for a separate experiment.

## Design Review

Codex reviewed the initial design and found two blocking issues:

- forwarding all non-keybind argv entries would violate `Config::set_cli_args`'s
  caller contract because the stored C argv can include
  positional/action/runtime arguments that belong to an outer filtering layer;
- raw C argv bytes require explicit UTF-8 conversion behavior before they can be
  passed to the string-based config parser.

The design was updated to forward only config-looking `--...` entries, ignore
non-config-looking outer arguments, and report invalid UTF-8 config-looking
entries through ABI diagnostics instead of panicking or converting lossily.

Codex re-reviewed the revised design and approved it with no blocking findings.
The review confirmed that the two original blockers were resolved. The remaining
non-blocking suggestion is to include a regression where `--keybind` is missing
its value and the following valid config flag is still parsed by the config
path.

## Result

**Result:** Pass

Implemented parsed config CLI argument loading through
`roastty_config_load_cli_args`. The ABI loader now preserves the existing
`--keybind=value` and `--keybind value` handling, collects other config-looking
`--...` argv entries, applies valid UTF-8 entries through
`config::Config::set_cli_args`, reports invalid UTF-8 config-looking entries
through ABI diagnostics, records parsed config diagnostics through the existing
diagnostic channel, and syncs ABI-visible parsed state afterward.

Verification passed:

- `cargo test -p roastty config_c_abi_cli_config -- --nocapture --test-threads=1`
- `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Completion Review

Codex reviewed the completed implementation and found no blocking findings. The
review confirmed that the implementation matches the approved design: valid
UTF-8 non-keybind `--...` args are forwarded into `config.parsed.set_cli_args`,
positional/action-looking args are ignored by this config layer, invalid UTF-8
config-looking args are diagnosed without lossy parsing, keybind parsing is
preserved, diagnostics are surfaced, and parsed state is synced afterward.

Non-blocking notes from the review: the cwd-changing recursive config-file test
could use a small panic-safe guard as future test hygiene, and future structured
diagnostics should preserve argv-order fidelity if diagnostic ordering becomes
user-visible.

## Conclusion

The public config CLI ABI now reaches the same parsed config layer used by
internal config tests while keeping keybind parsing and outer positional argv
separate. This enables CLI-set config fields and CLI-provided recursive config
files to affect ABI-visible app and surface state.
