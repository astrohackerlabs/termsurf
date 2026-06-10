+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 63: Phase F — command config surface

## Description

Experiment 62 completed the search highlight color group. The next upstream
config group starts terminal launch configuration:

- `command`
- `initial-command`

Upstream represents both as optional `Command` values. `Command` is a
self-contained parser/formatter in `vendor/ghostty/src/config/command.zig` with
two modes:

- `shell` — a trimmed shell-expanded command string, used when no explicit
  prefix is present or when the input uses `shell:`;
- `direct` — a space-split argv vector, used when the input uses `direct:`.

This experiment ports that config surface only: the `Command` type, the two
optional config fields, parser/formatter behavior, diagnostics, and focused
tests. Runtime launch behavior is intentionally deferred because faithful
`initial-command` needs app-level first-surface state, CLI `-e` replay
semantics, and default command/finalize behavior; mixing those into the parser
slice would make the experiment too broad.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Command` as a cloneable/equatable enum:
    - `Shell(String)`
    - `Direct(Vec<String>)`
  - Port upstream `Command.parseCLI` behavior:
    - missing, empty, or all-space values return `ValueRequired`;
    - edge spaces are trimmed;
    - `shell:` selects shell mode and trims the payload;
    - `direct:` selects direct mode, trims the payload, and splits on ASCII
      space;
    - unknown prefixes such as `foo:bar` are not errors; they stay shell mode
      with the full trimmed input;
    - `direct:` with an empty payload is accepted as an empty argv vector if
      upstream accepts it, otherwise it returns the same error upstream returns.
      Verify this directly before implementation and record the chosen behavior
      in tests.
  - Port upstream `Command.formatEntry` behavior:
    - shell writes the raw shell string;
    - direct writes `direct:` followed by args joined with single spaces.
  - Add optional `Config` fields:
    - `command: Option<Command> = None`
    - `initial_command: Option<Command> = None`
  - Route both keys through `Config::set`, `format_config`, default
    construction, clone/equality, and diagnostics.
  - Preserve upstream declaration/formatter order immediately after
    `search-selected-background` and before `notify-on-command-finish`.
  - Empty values reset each field to `None`; missing values return
    `ValueRequired`; invalid values, if any are found in upstream parser tests,
    return `InvalidValue`.

Out of scope:

- Applying `command` to `Surface::start_termio`.
- `initial-command` first-surface semantics.
- CLI `-e` parsing/replay behavior and its side effects.
- `Config::finalize` default command lookup from `SHELL` / passwd.
- Shell-integration wrapping or command environment mutation.

## Verification

- Run formatting:
  - `cargo fmt -- roastty/src/config/mod.rs`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/63-command-config-surface.md`
- Run targeted tests:
  - `cargo test -p roastty command_config`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - defaults for both fields are `None` and format as empty entries;
  - shell command parsing trims edge spaces and formats without a prefix;
  - `shell:` parsing trims the payload and formats without the `shell:` prefix;
  - `direct:` parsing trims the payload, splits on ASCII spaces, and formats
    with `direct:`;
  - unknown prefixes remain shell commands;
  - empty values reset to `None`;
  - missing values and any upstream-invalid values produce expected diagnostics;
  - clone/equality preserves both shell and direct variants;
  - formatter order places `command` and `initial-command` after
    `search-selected-background`.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = `command` and `initial-command` are represented faithfully on
`Config`, round-trip through config loading/formatting, match upstream parser
semantics, and have targeted and full tests passing.

**Partial** = the parser lands but upstream direct-empty or formatting semantics
require a smaller follow-up before wiring both config fields.

**Fail** = command config cannot be represented faithfully without first porting
broader launch/finalize infrastructure.

## Design Review

Codex adversarial reviewer `019eb39e-c5b7-7203-8218-5bc3963db9f5` returned
**Approved** with no findings.

The reviewer verified that the README links Exp63 as `Designed`, the experiment
has the required sections and pass/partial/fail criteria, upstream `command` and
`initial-command` are optional `Command` fields in the planned order, the
parser/formatter claims match upstream `Command.parseCLI` / `formatEntry` at the
design level, deferring runtime launch behavior is acceptable because faithful
runtime/defaulting behavior involves `Config.finalize`, CLI `-e`, and surface
launch state beyond the parser, and Exp62's result commit exists before this
design.

## Result

**Result:** Pass

Experiment 63 added the config-only `command` / `initial-command` surface to
`roastty/src/config/mod.rs`. Both fields are optional, default to `None`, format
as empty entries when unset, and route through `Config::set` using a ported
`Command` parser/formatter.

The parser follows upstream `vendor/ghostty/src/config/command.zig` semantics:
edge spaces are trimmed, unprefixed and `shell:` values become shell strings,
`direct:` values trim the payload and split on ASCII spaces, unknown prefixes
remain shell commands, all-space values report `ValueRequired`, and `direct:`
parses as a direct command with one empty argument before formatting back to
`direct:`.

The experiment intentionally did not wire these fields into surface launch,
first-surface `initial-command` behavior, CLI `-e`, default shell lookup, or
shell integration wrapping. Those runtime behaviors remain out of scope for a
later experiment.

Verification run:

- `cargo fmt -- roastty/src/config/mod.rs`
- `cargo test -p roastty command_config`
- `cargo test -p roastty config_format_config`
- `cargo test -p roastty`
- `cargo fmt --check`
- `git diff --check`

`cargo test -p roastty` passed with 4,498 unit tests, the C ABI harness, and doc
tests. The C ABI harness still emits existing enum-conversion warnings unrelated
to this config change.

## Conclusion

`command` and `initial-command` now have a faithful parser/formatter config
surface with defaults, reset behavior, diagnostics, format order, clone/equality
coverage, and focused tests. The next experiment can move to the following Phase
F config gap or separately design the larger runtime launch semantics if command
application becomes the priority.

## Completion Review

Codex-native adversarial reviewer `019eb3a7-3bb4-7632-a2f6-013f85fb0738`
returned **Approved** with no findings.

The reviewer checked the completed experiment with fresh context, including the
workflow contract, issue README, experiment file, implementation diff since the
plan commit, `roastty/src/config/mod.rs`, and upstream
`vendor/ghostty/src/config/command.zig` / `Config.zig`.
