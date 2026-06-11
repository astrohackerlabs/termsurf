# Experiment 96: Phase F — async backend and auto-update config

## Description

Port the next three upstream parser/formatter fields after `enquiry-response`:

- `async-backend`
- `auto-update`
- `auto-update-channel`

Upstream declares these fields as:

```zig
@"async-backend": AsyncBackend = .auto,
@"auto-update": ?AutoUpdate = null,
@"auto-update-channel": ?build_config.ReleaseChannel = null,
```

This experiment is intentionally parser/formatter-only. It should make Roastty
accept, store, reset, format, clone, and diagnose the config values with
upstream-compatible keywords. It must not implement Linux async backend
selection, Sparkle update checks, macOS update behavior, or the upstream
`finalize()` step that fills a null `auto-update-channel` from
`build_config.release_channel`.

The upstream keyword sets are:

- `async-backend`: `auto`, `epoll`, `io_uring`
- `auto-update`: `off`, `check`, `download`
- `auto-update-channel`: `tip`, `stable`

## Changes

- `roastty/src/config/mod.rs`
  - Add fields to `Config`:
    - `async_backend: AsyncBackend` defaulting to `AsyncBackend::Auto`
    - `auto_update: Option<AutoUpdate>` defaulting to `None`
    - `auto_update_channel: Option<ReleaseChannel>` defaulting to `None`
  - Add `AsyncBackend`, `AutoUpdate`, and `ReleaseChannel` enum types with
    `from_keyword` and `format_entry` implementations.
  - Route `Config::set` keys for the three upstream config names.
  - Format the entries after `enquiry-response`, preserving upstream field
    order:
    - `async-backend` always formats its value
    - unset optional `auto-update` and `auto-update-channel` format as bare void
      lines, matching existing optional-field formatter behavior
  - Add tests for defaults, valid keywords, empty reset to defaults, missing
    values, invalid values, load diagnostics, clone/equality, and formatter
    ordering.
  - Extend enum round-trip/format tests so all new keywords are covered.

No other files should change except documentation and formatter output caused by
these edits.

## Verification

Pass criteria:

1. `cargo test -p roastty async_update_config`
2. `cargo test -p roastty enum_format_entries`
3. `cargo test -p roastty config_format_config`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run must pass. The existing ABI harness may
print its known enum-conversion warnings, but no new failures are acceptable.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb5a3-6ede-7d13-8244-603265836b19`.

Verdict: **APPROVED**

Findings: None.

## Result

**Result:** Pass

Implemented parser/formatter support for `async-backend`, `auto-update`, and
`auto-update-channel` in `roastty/src/config/mod.rs`.

The implementation adds the raw parser/default state for the three upstream
fields:

- `async_backend: AsyncBackend`, defaulting to `auto`
- `auto_update: Option<AutoUpdate>`, defaulting to unset
- `auto_update_channel: Option<ReleaseChannel>`, defaulting to unset

It also adds exact upstream keyword parsing/formatting for:

- `async-backend`: `auto`, `epoll`, `io_uring`
- `auto-update`: `off`, `check`, `download`
- `auto-update-channel`: `tip`, `stable`

Runtime async backend selection, Sparkle update behavior, and
`auto-update-channel` finalization to the build release channel remain
deliberately out of scope for this parser/formatter slice.

Verification:

1. `cargo test -p roastty async_update_config` — pass
2. `cargo test -p roastty enum_format_entries` — pass
3. `cargo test -p roastty config_format_config` — pass
4. `cargo test -p roastty` — pass: 4541 unit tests, ABI harness pass, doc tests
   pass. The ABI harness printed the existing 10 enum-conversion warnings. After
   completion review found one non-reproducing full-suite failure in a
   foreground-PID test, this full command was rerun and passed again with the
   same 4541 unit-test, ABI harness, and doc-test result.
5. `cargo fmt --check` — pass
6. `git diff --check` — pass

An initial focused test run failed before verification because the new enum
types were missing from the test module's explicit import list. That was fixed
before the passing verification runs above.

## Conclusion

Roastty now covers the next upstream config fields through parser/formatter
parity. The next experiment should continue with the following upstream config
field after `auto-update-channel`.

## Completion Review

Codex-native adversarial review ran in fresh context with subagent
`019eb5ab-9b68-7760-a6f6-f87899227a05`.

Initial verdict: **CHANGES REQUIRED**

Required finding:

- The reviewer's independent `cargo test -p roastty` run failed once in
  `tests::surface_foreground_pid_reports_worker_foreground_pid_after_start`. The
  reviewer noted that the isolated rerun of that test passed.

Fix:

- Reran the full `cargo test -p roastty` gate. It passed again: 4541 unit tests,
  ABI harness pass with the existing 10 enum-conversion warnings, and doc tests
  pass.

Final verdict after re-review: **APPROVED**

Final findings: None.
