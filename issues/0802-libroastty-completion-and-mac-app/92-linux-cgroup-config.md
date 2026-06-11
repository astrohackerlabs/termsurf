+++
implementer = "codex"
review_design = "codex-adversarial"
+++

# Experiment 92: Phase F — Linux cgroup config

## Description

Port the pinned upstream Linux cgroup config group from
`vendor/ghostty/src/config/Config.zig` into `roastty/src/config/mod.rs`.

Upstream defines this group after `macos-shortcuts`:

- `linux-cgroup: LinuxCgroup = single-instance` on Linux, otherwise `never`
- `linux-cgroup-memory-limit: ?u64 = null`
- `linux-cgroup-processes-limit: ?u64 = null`
- `linux-cgroup-hard-fail: bool = false`

This experiment is parser/formatter-only. Runtime transient `systemd` scope
creation, per-surface resource limits, single-instance interaction, reload
behavior for existing surfaces, app C ABI exposure, and Linux app integration
remain later work.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config` fields for the four Linux cgroup options after
    `macos_shortcuts` and before the font-family group in the current local
    struct/default region.
  - Initialize defaults to upstream values:
    - `linux_cgroup = LinuxCgroup::SingleInstance` on Linux, otherwise
      `LinuxCgroup::Never`
    - `linux_cgroup_memory_limit = None`
    - `linux_cgroup_processes_limit = None`
    - `linux_cgroup_hard_fail = false`
  - Format the four fields after `macos-shortcuts` and before `bold-color`,
    preserving the current local formatter gap before terminal color fields.
  - Route `Config::set` for:
    - `linux-cgroup` through `set_enum_field`;
    - `linux-cgroup-memory-limit` through `set_optional_value_field` with a new
      `u64` scalar parser;
    - `linux-cgroup-processes-limit` through `set_optional_value_field` with the
      same `u64` scalar parser;
    - `linux-cgroup-hard-fail` through `set_bool_field`.
  - Add `LinuxCgroup` enum variants and exact upstream keywords:
    - `never`
    - `always`
    - `single-instance`
  - Add `parse_u64_scalar_field` using the existing
    `parse_uint(value, 0, u64::MAX)` helper, matching the local base-0 scalar
    integer parsers.
  - Extend default-value, enum-route, format-order, scalar/optional formatting,
    and enum keyword round-trip tests.
  - Add a focused test for default formatter output, enum parsing, optional
    `u64` parsing/formatting, empty reset, missing/invalid diagnostics, bool
    parsing, and clone/equality.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed` in the experiment index.
  - After implementation, add an operating note describing the parser-only
    status and runtime work left open.

## Verification

Before implementation:

- Codex-native adversarial design review approves the experiment.
- Plan commit exists before source edits begin.

After implementation:

- `cargo fmt`
- `cargo test -p roastty linux_cgroup`
- `cargo test -p roastty config_format_config`
- `cargo test -p roastty`
- `cargo fmt --check`
- `git diff --check`

Pass criteria:

- The four Linux cgroup config fields are present in defaults, formatter output,
  `Config::set`, and format-order tests in the current local formatter region.
- Enum parsing and formatting matches upstream keywords exactly.
- Optional `u64` parsing accepts normal/base-0 scalar values, resets empty
  values to `None`, diagnoses missing values as `ValueRequired`, and diagnoses
  invalid or overflowing values as `InvalidValue`.
- Runtime cgroup behavior is not claimed or changed by this experiment.

## Design Review

Codex-native adversarial reviewer `019eb551-b370-7f43-aa89-3bb8d113b8c6`
reviewed the design with fresh context and returned **Approved** with no
findings.
