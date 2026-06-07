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
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 773: Config Get Window Position

## Description

Wire the optional `window-position-x` and `window-position-y` config keys into
Roastty's aggregate config and the public `roastty_config_get` C ABI.

Upstream stores both keys as `?i16` with defaults of `null`. Its generic C
getter returns `false` when an optional value is unset, and writes an `i16` as C
`short` when it is set. Roastty currently returns `false` for both keys
unconditionally, so configured window positions are invisible through
`roastty_config_get`.

This experiment only ports config parsing, formatting, storage, and lookup. It
does not wire runtime window placement behavior.

## Changes

- `roastty/src/config/mod.rs`
  - Add `window_position_x: Option<i16>` and `window_position_y: Option<i16>` to
    `config::Config`.
  - Default both to `None`, matching upstream `null`.
  - Include both keys in `format_config` after `window-theme` and before
    `window-save-state`, preserving the currently implemented upstream order
    among available fields.
  - Update the full key-order test to assert `window-theme`,
    `window-position-x`, `window-position-y`, then `window-save-state`.
  - Add a signed integer parser for type-magic `i16` fields matching upstream
    `std.fmt.parseInt(i16, value, 0)`: base-0 prefixes, optional sign,
    underscore separators, `ValueRequired` on missing values, and `InvalidValue`
    on malformed or out-of-range values.
  - Route both keys through optional reset semantics: empty values reset to
    `None`; non-empty values parse to `Some(i16)`.
  - Add aggregate tests for defaults, formatting, set routing, empty reset,
    missing values, invalid values, combined signed base-0 values (`-0x10`,
    `+0b101`), i16 boundaries (`-32768`, `32767`), out-of-range boundaries
    (`-32769`, `32768`), malformed signs/prefixes (`-`, `+`, `0x`), bad
    underscores, and clone/partial-eq behavior.
- `roastty/src/lib.rs`
  - Import/use C `short` for the getter output type.
  - Make `roastty_config_get("window-position-x")` and
    `roastty_config_get("window-position-y")` return `false` without writing to
    the caller's output slot when unset, or write the parsed value as C `short`
    and return `true` when set.
  - Add C ABI tests proving both keys return `false` by default, reflect
    file-loaded, CLI-loaded, cloned, reset-to-default, base-0, and diagnostic
    values.
  - Use `c_short` output slots in the C ABI tests, including sentinel output
    values for default-null and reset-to-null cases to prove unset optionals do
    not write, and negative values to prove signed C `short` output is correct.

## Verification

- `cargo test -p roastty window_position -- --nocapture --test-threads=1`
- `cargo test -p roastty config_get_window_position -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

The experiment passes if both window-position keys are stored in aggregate
config, can be set through file and CLI loading, format in full config output in
upstream order among implemented fields, reset to `None` on empty values, reject
missing/invalid/out-of-range values consistently with upstream `i16` parsing,
and are returned by `roastty_config_get` as optional C `short` values from
parsed state.

## Design Review

Codex reviewed the design and found four concrete gaps before implementation.
The plan was updated to require unset optional getters to return `false` without
touching the caller's output slot, exact formatter insertion after
`window-theme` and before `window-save-state`, explicit `c_short` ABI tests, and
signed parser edge cases covering signed base-0 values, i16 boundaries,
out-of-range boundaries, malformed signs/prefixes, and bad underscores.

The review confirmed the scope is otherwise correct: config
storage/parsing/formatting and C ABI lookup only, with runtime window placement
left out of scope.

## Result

**Result:** Pass

Implemented aggregate config storage for `window-position-x` and
`window-position-y`. `config::Config` now stores both fields as `Option<i16>`,
defaults them to `None`, formats them between `window-theme` and
`window-save-state`, and routes both keys through optional reset semantics and a
signed base-0 `i16` parser.

`roastty_config_get("window-position-x")` and
`roastty_config_get("window-position-y")` now return `false` without writing to
the caller's output slot when unset, or write the parsed value as C `short` and
return `true` when set.

Verification passed:

- `cargo test -p roastty window_position -- --nocapture --test-threads=1`
- `cargo test -p roastty config_get_window_position -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

`window-position-x` and `window-position-y` now report parsed optional signed
coordinate config state through the C ABI. Runtime window placement behavior
remains follow-up work.

## Completion Review

Codex reviewed the completed implementation and found no blocking code
correctness issues. The review confirmed that optional fields default to `None`,
format between `window-theme` and `window-save-state`, parse signed base-0 `i16`
values, reset on empty values, and return `false` without writing to the
caller's output slot when unset.

The review also confirmed the focused tests cover default-null no-write
behavior, file and CLI values, clone, CLI reset-to-null, missing and invalid
diagnostics, signed base-0 values, i16 boundaries, malformed signs/prefixes, bad
underscore cases, and C `short` output for negative values. A required
provenance frontmatter update was applied before the result commit.
