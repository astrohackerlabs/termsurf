+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 774: Config Get Bell Audio Path

## Description

Wire the optional `bell-audio-path` config key into Roastty's aggregate config
and the public `roastty_config_get` C ABI.

Upstream stores `bell-audio-path` as `?Path` with a default of `null`. Its
generic C getter returns `false` when the optional value is unset, and writes a
`Config.Path.C` struct (`path`, `optional`) when set. Roastty already exposes
the matching `roastty_config_path_s` ABI shape, but currently returns `false`
for `bell-audio-path` unconditionally.

This experiment only ports config parsing, formatting, storage, path expansion,
and lookup. It does not wire runtime bell audio playback or validation that the
audio file exists.

## Changes

- `roastty/src/config/mod.rs`
  - Reuse the existing `ConfigFilePath` required/optional representation as the
    stored Rust analogue of upstream single `Path`, but add a dedicated
    single-path parser instead of reusing `RepeatableConfigPath` semantics.
  - Add `bell_audio_path: Option<ConfigFilePath>` to `config::Config`.
  - Default it to `None`, matching upstream `null`.
  - Include `bell-audio-path` in `format_config` before `bell-audio-volume`,
    preserving the currently implemented upstream order among available fields.
  - Route `Config::set("bell-audio-path", ...)` through optional path semantics:
    missing values are `ValueRequired`, empty values reset to `None`, leading
    `?` marks optional paths, surrounding quotes are stripped, and non-empty
    paths become `Some(ConfigFilePath)`.
  - Preserve upstream single-`Path` parsed-empty behavior: `?` becomes an
    optional empty path, `""` becomes a required empty path, and `?""` becomes
    an optional empty path. Only the raw empty value resets the optional field
    to `None`.
  - Reject interior NUL bytes as `InvalidValue`, because the public ABI exposes
    path as a C string pointer.
  - Broaden the existing path-expansion helper so all load paths that currently
    expand `config-file` paths also expand `bell-audio-path`: direct file loads,
    default files, recursive files, and CLI config arguments. File-loaded
    relative paths resolve relative to the config file directory, CLI-loaded
    relative paths resolve relative to the current working directory, absolute
    paths remain unchanged, and `~/` expands with `$HOME`.
  - Add aggregate tests for defaults, formatting, set routing, optional marker,
    quoted paths, parsed-empty `?` / `""` / `?""`, raw empty reset, missing
    values, interior NUL rejection, relative file expansion, relative CLI
    expansion, home expansion, recursive/default load expansion, and
    clone/partial-eq behavior.
- `roastty/src/lib.rs`
  - Store a cached `CString` for the parsed bell audio path in `ConfigHandle`,
    so `roastty_config_get("bell-audio-path")` can return a stable
    NUL-terminated pointer with the same lifetime/mutation model as cached
    `title`.
  - Rebuild that cached path from parsed config in the same central sync path
    used for title, and initialize it for cloned handles.
  - Make `roastty_config_get("bell-audio-path")` return `false` without writing
    to the caller's output slot when unset, or write `RoasttyConfigPath` and
    return `true` when set.
  - Add C ABI tests proving the key returns `false` by default, does not write
    on unset/reset, reflects file-loaded, CLI-loaded, cloned, optional-marker,
    parsed-empty, expanded path values, stable NUL-terminated pointers while the
    handle remains unmutated, and independent cloned pointers.

## Verification

- `cargo test -p roastty bell_audio_path -- --nocapture --test-threads=1`
- `cargo test -p roastty config_get_bell_audio_path -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

The experiment passes if `bell-audio-path` is stored in aggregate config, can be
set through file and CLI loading, formats in full config output in upstream
order among implemented fields, resets to `None` on empty values, expands
relative and home-prefixed paths consistently with the existing path expansion
layer, and is returned by `roastty_config_get` as an optional
`roastty_config_path_s` from parsed state.

## Design Review

Codex reviewed the design and found two blocking issues. First, the original
plan blurred repeatable-path and single-`Path` semantics: upstream single `Path`
preserves parsed-empty forms like `?`, `""`, and `?""`, while `RepeatablePath`
ignores them. Second, the original plan did not specify stable C string storage
or interior-NUL handling for `roastty_config_path_s.path`.

The plan was updated to require a dedicated single-path parser, interior-NUL
rejection, cached `CString` storage rebuilt from parsed state, parsed-empty edge
tests, and path expansion through all existing parsed-config load paths. The
review confirmed the formatter placement before `bell-audio-volume` and the
narrow scope.

## Result

**Result:** Pass

`bell-audio-path` is now represented in aggregate config as
`Option<ConfigFilePath>`, defaults to `None`, formats before
`bell-audio-volume`, and routes through dedicated single-path semantics. Raw
empty values reset the optional field to `None`, while parsed-empty forms
preserve upstream single-`Path` behavior at the field parser level.

The shared path-expansion hook now expands `bell-audio-path` alongside
`config-file` for direct file loads, default file loads, recursive file loads,
and CLI config arguments. The C ABI now caches a NUL-terminated copy of the path
in the config handle, rebuilds it after parsed config changes and cloning, and
returns `roastty_config_path_s` with the optional marker when set. Unset and
reset values return `false` without writing to the caller's output slot.

Verification passed:

- `cargo test -p roastty bell_audio_path -- --nocapture --test-threads=1`
- `cargo test -p roastty config_get_bell_audio_path -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

The generic config getter can now return the optional path-shaped
`bell-audio-path` key through the same parsed-state and cached-pointer model
used for other C ABI config values. The remaining work in this area is the next
unimplemented `roastty_config_get` key.

## Completion Review

Codex reviewed the completed diff and first found two test coverage gaps: the C
ABI tests did not exercise parsed-empty `bell-audio-path` values, and aggregate
tests did not cover non-empty quoted path forms. Those tests were added.

The follow-up Codex review found no blocking issues, confirmed the single-path
parsing, cached `CString` lifetime model, clone cache rebuild, docs, and README
status, and approved the Experiment 774 result commit.
