+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 65: Phase F — scalar launch config

## Description

Experiment 64 added the repeatable `env` config surface. The next upstream
launch/runtime config fields after `input` include three scalar values that are
self-contained and do not require the larger readable-IO parser or runtime
launch wiring:

- `wait-after-command`
- `abnormal-command-exit-runtime`
- `scrollback-limit`

Upstream declares them in `vendor/ghostty/src/config/Config.zig` as a bool, a
`u32`, and a `usize` respectively. This experiment ports their config surface
only: fields, defaults, parsing/reset behavior, formatting, diagnostics, and
focused tests.

`input` remains intentionally deferred because upstream `RepeatableReadableIO`
has raw/path sources and startup-time file-read semantics. Runtime use of these
scalar fields is also out of scope; this experiment only makes `Config` able to
represent and round-trip them faithfully.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config` fields:
    - `wait_after_command: bool = false`
    - `abnormal_command_exit_runtime: u32 = 250`
    - `scrollback_limit: usize = 10_000_000`
  - Route all three through defaults, `Config::set`, `format_config`,
    clone/equality, and diagnostics.
  - Parse `wait-after-command` with the existing bool field helper so bare CLI
    values behave as true and empty values reset to the default.
  - Parse `abnormal-command-exit-runtime` as upstream scalar `u32` config magic:
    `std.fmt.parseInt(u32, value, 0)`, which autodetects `0x`, `0o`, and `0b`
    prefixes.
  - Add base-0 unsigned scalar helpers for `u32` and `usize` if needed, using
    the existing `parse_uint(..., base = 0, max = T::MAX)` implementation rather
    than the decimal-only `parse_u32_field`.
  - Add and use a `usize` field parser for `scrollback-limit` with the same
    base-0 behavior and `usize::MAX` overflow target.
  - Preserve local formatter order after `env`, with the fields emitted in
    upstream declaration order before later scroll/link/window fields.

Out of scope:

- `input` / `RepeatableReadableIO`.
- Applying `wait-after-command` to surface-exit behavior.
- Applying `abnormal-command-exit-runtime` to early-exit diagnostics.
- Applying `scrollback-limit` to terminal allocation or launch snapshots.
- `Config::finalize` or runtime inherited config behavior.

## Verification

- Run formatting:
  - `cargo fmt -- roastty/src/config/mod.rs`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/65-scalar-launch-config.md`
- Run targeted tests:
  - `cargo test -p roastty scalar_launch_config`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - defaults are `false`, `250`, and `10_000_000`;
  - `format_config` emits all three fields in order after `env`;
  - `wait-after-command` accepts `true`, `false`, bare/missing CLI-style value
    through `Config::set(..., None)`, and empty reset;
  - `abnormal-command-exit-runtime` accepts valid `u32` decimal and prefixed
    `0x`/`0o`/`0b` values, rejects missing, non-numeric, negative, and overflow
    values, and resets on empty;
  - `scrollback-limit` accepts valid `usize` decimal and prefixed `0x`/`0o`/`0b`
    values, rejects missing, non-numeric, negative, and overflow values, and
    resets on empty;
  - `Config::load_str` records `ConfigDiagnostic` line/key/error entries for
    invalid scalar launch config lines while keeping valid neighboring lines;
  - clone/equality preserves all three values.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = all three scalar launch/runtime fields are represented faithfully on
`Config`, round-trip through config loading/formatting, match upstream scalar
parser behavior, and have targeted and full tests passing.

**Partial** = one or two fields land but a parser edge or formatter-order issue
requires a follow-up before runtime wiring.

**Fail** = these scalar launch fields cannot be represented faithfully without
first porting broader launch/finalize infrastructure.

## Design Review

Codex adversarial reviewer `019eb3b9-76e7-7f41-a517-6544f3349fbf` returned
**Changes Required** with one required parser-fidelity finding: the initial
design planned decimal-only parsing for the integer fields, but upstream generic
config scalar parsing uses `std.fmt.parseInt(Int, value, 0)` and therefore
supports `0x`, `0o`, and `0b` prefixes.

The design was updated to require base-0 unsigned scalar parsing and explicit
prefix coverage for both `abnormal-command-exit-runtime` and `scrollback-limit`.

Re-review returned **Approved** with no findings after confirming the design now
requires base-0 integer parsing and concrete `0x`/`0o`/`0b` verification.
