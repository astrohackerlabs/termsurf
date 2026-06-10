+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 67: Phase F — link URL and maximize config

## Description

Experiment 66 added the `scrollbar` config surface. The next upstream config
fields that can land as one small parser/formatter slice are:

- `link-url`
- `maximize`

Upstream declares `link-url: bool = true` immediately after the still-TODO
repeatable `link` field, and `maximize: bool = false` immediately after
`link-previews` in `vendor/ghostty/src/config/Config.zig`.

This experiment ports those two boolean config surfaces only: fields, defaults,
parsing/reset behavior, formatting, diagnostics, and focused tests. Runtime URL
hover/link activation behavior and startup window maximization are intentionally
out of scope because they depend on later link/action and app-window wiring.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config::link_url: bool = true`.
  - Add `Config::maximize: bool = false`.
  - Route both keys through defaults, `Config::set`, `format_config`,
    clone/equality, and diagnostics.
  - Preserve local formatter order around the upstream sequence:
    - `scrollbar`
    - `link-url`
    - `link-previews`
    - `maximize`
    - `fullscreen`
  - Leave upstream `link` out of scope because upstream still marks it
    `TODO: This can't currently be set!` and a faithful port needs the
    repeatable link/action parser rather than a placeholder.

Out of scope:

- The repeatable `link` config surface and URL/action parser.
- Runtime URL matching, hover previews, and open-link action dispatch.
- Applying `maximize` to app/window creation.

## Verification

- Run formatting:
  - `cargo fmt -- roastty/src/config/mod.rs`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/67-link-url-maximize-config.md`
- Run targeted tests:
  - `cargo test -p roastty link_url_maximize_config`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - defaults are `link-url = true` and `maximize = false`;
  - explicit `true` and `false` values parse and format for both keys;
  - bare/missing CLI-style values set both bools to `true`;
  - empty values reset to their upstream defaults;
  - invalid values return `InvalidValue`;
  - `Config::load_str` records `ConfigDiagnostic` line/key/error entries for
    invalid `link-url` and `maximize` lines while keeping valid neighboring
    lines;
  - formatter order places `link-url` after `scrollbar`, `link-previews` after
    `link-url`, `maximize` after `link-previews`, and `fullscreen` after
    `maximize`;
  - clone/equality preserves both values.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = `link-url` and `maximize` are represented faithfully on `Config`,
round-trip through config loading/formatting, match upstream boolean parser
behavior, and have targeted and full tests passing.

**Partial** = one field lands faithfully but the other needs a follow-up, or a
parser/diagnostic/formatter-order edge remains before runtime use.

**Fail** = either field cannot be represented faithfully without first porting
broader link/action or app-window infrastructure.

## Design Review

Codex adversarial reviewer `019eb3d0-f897-73a2-846d-44d8a3565cd0` returned
**Approved** with no findings.

The reviewer verified that the README links Exp67 as `Designed`, the experiment
has the required sections, the scope is narrow, the planned `link-url` and
`maximize` defaults and ordering match upstream, and the verification plan
includes markdown/Rust formatting, targeted tests, full `cargo test -p roastty`,
`git diff --check`, and clean-status inspection.
