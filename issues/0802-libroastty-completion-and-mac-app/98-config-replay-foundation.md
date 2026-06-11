# Experiment 98: Phase F — config replay foundation

## Description

Add the first replay foundation that upstream uses for theme loading and
conditional reload.

Upstream `Config.loadTheme()` cannot simply read a theme file into the current
config: theme values must be lower priority than the user's config. It therefore
loads the theme into a fresh `Config`, then replays the user's prior config
inputs on top. The same replay list is also the basis for
`changeConditionalState()`.

Roastty currently applies config lines directly and discards the input stream.
That makes faithful theme loading impossible without guessing which values came
from user config versus defaults. This experiment should add replay recording
for ordinary config entries while preserving current parser behavior. It should
not load themes yet.

This is intentionally a foundation slice, not the full upstream `Replay` port.
It should record the ordinary `key = value` / `--key=value` inputs needed for
theme overlay ordering. Later experiments can extend the replay model with
conditional entries, diagnostics, explicit path-expansion steps, `-e`, and the
theme loader itself.

## Changes

- `roastty/src/config/mod.rs`
  - Add a small internal replay entry type for ordinary config inputs:
    - config key
    - optional value
    - source (`File` or `Cli`)
  - Add replay storage to `Config`, preserving clone/equality behavior.
  - Record successful entries from:
    - `Config::load_str`
    - `Config::set_cli_args_from_base`
  - Do not record failed entries, comments, blanks, or direct programmatic
    `Config::set` calls.
  - Add an internal helper to replay the recorded ordinary entries onto a fresh
    config without recursively recording them.
  - Keep existing path-expansion behavior unchanged. This experiment should not
    claim complete replay support for path-expansion steps; that remains a later
    extension before full theme loading of relative path-bearing entries.
  - Add tests proving:
    - file and CLI successful entries are recorded in order
    - failed entries are not replay-recorded but diagnostics still behave as
      before
    - direct `Config::set` remains non-recording
    - replaying entries onto a fresh `Config` reconstructs the same values for
      representative scalar, enum, optional, and repeatable fields
    - replaying does not append duplicate replay entries

No theme loading, conditional reload, path-expansion replay step, diagnostic
replay, `-e` replay, or app runtime behavior should be implemented in this
experiment.

## Verification

Pass criteria:

1. `cargo test -p roastty config_replay`
2. `cargo test -p roastty config_set_cli_args_applies_and_collects_diagnostics`
3. `cargo test -p roastty config_load_str_applies_lines_and_collects_diagnostics`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run must pass. The existing ABI harness may
print its known enum-conversion warnings, but no new failures are acceptable.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb5d1-a224-73f0-9116-09bd6593935b`.

Verdict: **APPROVED**

Findings: None.

## Result

**Result:** Pass

Implemented the replay foundation in `roastty/src/config/mod.rs`.

- Added private `ConfigReplayEntry` storage to `Config`.
- Recorded successful ordinary entries from file config loading and CLI config
  arg loading.
- Kept failed entries, comments, blank lines, and direct programmatic
  `Config::set` calls out of the replay list.
- Added `Config::replay_into` to apply recorded entries onto a fresh config
  through the existing setter without recursively recording replay entries.
- Preserved the current CLI `font-family` overwrite behavior for contiguous CLI
  replay segments and separate CLI parse batches.

Verification passed:

1. `cargo test -p roastty config_replay`
2. `cargo test -p roastty config_set_cli_args_applies_and_collects_diagnostics`
3. `cargo test -p roastty config_load_str_applies_lines_and_collects_diagnostics`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run passed with 4547 unit tests, the ABI
harness, and doc tests. The ABI harness printed the existing 10 enum-conversion
warnings.

## Conclusion

Roastty now has enough ordinary-entry replay state for the next theme-loading
slice to load theme defaults into a fresh config and replay user file/CLI config
on top. Replay still intentionally excludes conditional entries, diagnostics,
explicit path-expansion replay steps, `-e`, and theme loading itself; those
remain later extensions.

## Completion Review

Codex-native adversarial review ran in fresh context with subagent
`019eb5dc-7655-7cf3-aae0-aadce974d003`.

Initial verdict: **CHANGES REQUIRED**

- Required: replay lost CLI invocation boundaries, so two separate
  `set_cli_args(["--font-family=A"])` and `set_cli_args(["--font-family=B"])`
  calls could replay as one contiguous CLI segment and reconstruct `["A", "B"]`
  instead of `["B"]`.

Fix:

- Added `begin_cli_batch` to replay entries.
- Marked the first successful entry in each CLI parse batch.
- Restarted CLI replay state at recorded batch boundaries.
- Added `config_replay_preserves_separate_cli_repeatable_overwrites`.

Final verdict after re-review: **APPROVED**

Findings: None remaining.
