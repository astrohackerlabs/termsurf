+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 87: Phase F — custom shader config

## Description

Experiment 86 completed `vt-kam-allowed`. The next upstream config field is
`custom-shader`, immediately before the already-ported
`custom-shader-animation`.

Upstream declares `custom-shader: RepeatablePath = .{}`. It is a repeatable list
of GLSL/Shadertoy-compatible shader source file paths. Repeated entries append
and are run in order by the renderer. A raw empty value clears the list, while
parsed-empty values such as `?`, `""`, and `?""` are ignored. The local
`RepeatableConfigPath` / `ConfigFilePath` machinery already models this upstream
`RepeatablePath` behavior for `config-file`, including optional `?` prefixes and
quoted-literal handling.

This experiment wires the `custom-shader` config parser/formatter surface and
stores it on `Config` in upstream order. Runtime shader loading, shader
cross-compilation, Metal/OpenGL custom shader pipelines, animation loop
behavior, and app C ABI accessors are out of scope.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config::custom_shader: RepeatableConfigPath` in upstream declaration
    order after `vt-kam-allowed` and before `custom-shader-animation`.
  - Default it to an empty list, matching upstream.
  - Format it after `vt-kam-allowed` and before `custom-shader-animation`,
    emitting `custom-shader = ` when empty and one line per configured shader
    when non-empty.
  - Route `custom-shader` through `Config::set`, `load_str`, `load_file`,
    `set_cli_args_from_base`, diagnostics, clone/equality, and `format_config`
    by reusing the existing repeatable path parser.
  - Expand `custom-shader` paths from the same config-file / CLI base hook used
    by local repeatable path fields, matching upstream `expandPaths` for
    `RepeatablePath`.
  - Extend the config format-order test so `custom-shader` lands between
    `vt-kam-allowed` and `custom-shader-animation`.
  - Add focused tests covering default empty list, append ordering, optional `?`
    paths, quoted literal `?` paths, raw empty reset, parsed-empty ignored
    values, missing-value diagnostics, `load_str` preservation around
    neighboring valid lines, file-base expansion, CLI-base expansion,
    formatting, clone/equality, and order.

Out of scope:

- Loading shader file contents.
- Shader validation, compilation, `#include` processing, Shadertoy translation,
  or renderer pipeline integration.
- `custom-shader-animation`, which is already present locally.
- Runtime config update effects beyond ensuring the parsed `Config` value is
  stored faithfully and path-expanded for later renderer work.
- App settings UI or C ABI accessors for the shader list.

## Verification

- Run formatting:
  - `cargo fmt`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/87-custom-shader-config.md`
- Run targeted tests:
  - `cargo test -p roastty custom_shader`
  - `cargo test -p roastty config_format_config`
- Add concrete tests proving:
  - `Config::default().custom_shader.list` is empty;
  - `format_config` emits `custom-shader = ` for an empty list;
  - repeated `custom-shader = path` entries append and preserve order;
  - `?path` is stored/formatted as optional and quoted `"?path"` is a required
    literal path beginning with `?`;
  - parsed-empty path forms (`?`, `""`, `?""`) are ignored without clearing
    existing entries;
  - raw `custom-shader =` clears the list;
  - a missing value reports `ValueRequired`;
  - missing-value diagnostics from `load_str` preserve neighboring valid lines;
  - `load_file` expands relative shader paths against the config file directory
    while preserving required/optional status;
  - `set_cli_args_from_base` expands relative shader paths against the CLI base
    while preserving required/optional status;
  - clone/equality preserves the shader list;
  - default `format_config` places `custom-shader` after `vt-kam-allowed` and
    before `custom-shader-animation`.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = `custom-shader` is represented faithfully on `Config`, round-trips
through config loading/formatting with upstream repeatable path semantics, and
targeted/full tests pass.

**Partial** = the field lands for simple paths, but optional/quoted/reset
semantics or diagnostics need a follow-up.

**Fail** = `custom-shader` cannot be represented faithfully without first
implementing renderer custom shader loading.

## Design Review

Codex adversarial reviewer `019eb4f1-f6b9-7161-a41e-ae15138254f7` returned
**Changes Required** with one required finding:

- The original design omitted upstream `RepeatablePath` path expansion for
  `custom-shader`. Accepted: this design now requires `custom-shader` to expand
  through the local config-file / CLI base hook and requires tests proving both
  `load_file` and CLI-base expansion while preserving optional/required path
  status.

Codex adversarial reviewer `019eb4f3-a33b-75a2-9453-2be47f191a53` re-reviewed
the fix and returned **Approved** with no remaining findings. The reviewer
confirmed the design now requires path expansion through the local config-file /
CLI base hook and tests for both `load_file` and `set_cli_args_from_base`.

## Result

**Result:** Pass

Implemented `custom-shader` in `roastty/src/config/mod.rs` as a
`RepeatableConfigPath` field in upstream order after `vt-kam-allowed` and before
`custom-shader-animation`. `Config::default` starts with an empty shader list,
and `format_config` emits `custom-shader = ` when empty or one
`custom-shader = ...` line per configured shader.

The parser reuses the local repeatable path implementation, matching upstream
`RepeatablePath` behavior for this config surface:

- repeated `custom-shader = path` lines append in order;
- `?path` stores an optional path;
- quoted `"?path"` stores a required literal path beginning with `?`;
- parsed-empty path forms such as `?`, `""`, and `?""` are ignored without
  clearing existing entries;
- raw `custom-shader =` clears the list;
- missing values diagnose as `ValueRequired`;
- `load_str` preserves neighboring valid entries around a missing-value
  diagnostic;
- clone/equality preserve the shader list.

`expand_config_file_paths_from_base` now expands `custom-shader` paths from both
config-file and CLI bases, preserving required/optional status. This matches
upstream `expandPaths` applying to all `RepeatablePath` fields.

Verification passed:

- `cargo fmt`
- `cargo test -p roastty custom_shader`
  - 15 targeted tests passed, including the new config tests and existing
    custom-shader renderer-support tests selected by the filter
- `cargo test -p roastty config_format_config`
  - 1 targeted order test passed
- `cargo test -p roastty`
  - 4531 unit tests passed
  - ABI harness passed with the existing 10 enum-conversion warnings
  - doc tests passed
- `cargo fmt --check`
- `git diff --check`

## Conclusion

`custom-shader` now has a faithful config parser/formatter/path-expansion
surface for the pinned upstream `RepeatablePath` field. Runtime shader file
loading, Shadertoy translation, shader compilation, custom shader render
pipelines, animation-loop behavior, and app C ABI exposure remain later work.

## Completion Review

Codex adversarial reviewer `019eb4fa-0e1d-7d83-a7fe-fdc41de4abae` returned
**Approved** with no required findings. The reviewer verified that the result
commit had not been made yet, the working tree contained only the expected three
modified files, and the implementation matches the approved scope and upstream
`RepeatablePath` behavior for default, append/reset/parsed-empty semantics,
formatting, clone/equality, diagnostics, and config-file / CLI base expansion.

The reviewer reran and passed:

- `cargo fmt --check`
- `git diff --check`
- `cargo test -p roastty custom_shader`
- `cargo test -p roastty config_format_config`
- `cargo test -p roastty`
