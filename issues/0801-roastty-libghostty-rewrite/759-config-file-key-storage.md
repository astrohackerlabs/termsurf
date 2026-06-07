+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 759: Config File Key Storage

## Description

Add internal config storage and parsing for the `config-file` and
`config-default-files` keys. Experiments 755-758 wired default and explicit file
loading, but recursive file loading cannot be implemented faithfully until the
typed config model can remember user-specified `config-file` entries and the
CLI-only `config-default-files` switch.

This experiment is a foundation slice. It does not implement
`roastty_config_load_recursive_files`, relative-path expansion, cycle detection,
file type checks, replay steps, or C ABI exposure for config-file values.

## Upstream Behavior

In `vendor/ghostty/src/config/Config.zig`:

- `config-file` is a `RepeatablePath`:
  - each non-empty parse appends one required or optional path;
  - a raw empty value clears the accumulated list;
  - a leading `?` marks the path optional;
  - surrounding quotes are stripped after optional-prefix detection, so
    `"?file"` is a required literal `?file`, while `?"file"` is optional `file`;
  - zero-length paths after optional/quote handling are ignored.
- `config-default-files` is a `bool` defaulting to `true`.
- `config-default-files` is CLI-only. Setting it in a configuration file has no
  effect and is not an error.

## Changes

- `roastty/src/config/mod.rs`
  - Add `ConfigFilePath` with required/optional variants and owned `String`
    paths.
  - Add `RepeatableConfigPath` as the Rust analogue of upstream `RepeatablePath`
    for config-file values.
  - Implement parsing for:
    - required path: `config.1`
    - optional path: `?config.2`
    - required literal question path: `"?config.3"`
    - optional quoted path: `?"config.4"`
    - raw empty reset: `config-file =` clears accumulated values;
    - zero-length ignored paths: `?`, `""`, `?""`
    - missing value as `ValueRequired`
  - Add `Config` fields:
    - `config_file: RepeatableConfigPath`
    - `config_default_files: bool`
  - Add source-aware config setting so:
    - file loads and `Config::set` treat `config-default-files` as accepted but
      ignored;
    - CLI argument loading mutates `config_default_files`;
    - `config-file` parses and accumulates from both sources.
  - Format `config-file` entries in `format_config` using upstream-style `?path`
    output for optional paths, and an empty entry when no values exist.
  - Format `config-default-files` as a normal bool field.
- Tests in `roastty/src/config/mod.rs`
  - path parser coverage matching upstream `Path.parse` /
    `RepeatablePath.parseCLI` cases above;
  - config-file accumulation through `Config::set`;
  - raw empty `config-file =` reset behavior, including formatted empty output
    round-tripping back to an empty list;
  - `config-default-files` ignored by `load_str` / file-source setting;
  - `config-default-files` applied by CLI argument loading;
  - clone/equality and formatter behavior for the new fields.

## Verification

- `cargo test -p roastty config_file -- --nocapture --test-threads=1`
- `cargo test -p roastty config_default_files -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

The experiment passes if Roastty can store and format `config-file` values,
preserve upstream optional-path parsing semantics, and distinguish the CLI-only
`config-default-files` switch from no-op config-file occurrences.

## Design Review

Codex reviewed the first design draft and found one semantic blocker. The draft
covered zero-length paths produced after optional/quote handling (`?`, `""`,
`?""`) but missed the distinct upstream behavior for a raw empty value:
`config-file =` clears the accumulated repeatable path list. The design was
updated to require raw-empty reset behavior and formatter round-trip coverage
for an empty `config-file` entry.

Codex reviewed the updated design and approved it for the plan commit with no
blocking findings. The follow-up review confirmed that the design now covers the
key upstream `RepeatablePath` distinction and keeps `config-default-files`
correctly scoped as CLI-only, with recursive loading and C ABI exposure
deferred.

## Result

**Result:** Pass

Implemented typed storage and parsing for `config-file` and
`config-default-files` in `roastty/src/config/mod.rs`.

`config-file` now stores required and optional paths in a
`RepeatableConfigPath`, preserves upstream parsing order for leading `?` and
quote stripping, ignores parsed-empty path values, and clears accumulated paths
on raw empty `config-file =`. `format_config` now emits `config-file` entries in
the current config order, including an empty entry when the list is empty.

`config-default-files` now defaults to `true` and is source-aware: file-source
setting and `load_str` accept but ignore it, while CLI argument loading mutates
the stored bool.

Verification passed:

- `cargo test -p roastty config_file -- --nocapture --test-threads=1`
- `cargo test -p roastty config_default_files -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Completion Review

Codex reviewed the completed implementation and found no blocking findings. The
review confirmed that the implementation matches the approved `RepeatablePath`
semantics, keeps `config-default-files` CLI-only, and stays within the approved
foundation scope: storage, parsing, source-aware setting, and formatting only.
No recursive loading, path expansion, C ABI exposure, or default-file behavior
wiring was introduced.

## Conclusion

Roastty can now remember `config-file` entries and the CLI-only
`config-default-files` switch in the typed Rust config model. This gives the
next recursive-loading slice the field storage it needs without implementing
recursive file traversal yet.
