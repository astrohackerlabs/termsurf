# Experiment 165: Phase F — macOS tail config

## Description

Remove the four remaining macOS scalar keys from the Phase F public-config tail:
`macos-dock-drop-behavior`, `macos-auto-secure-input`,
`macos-secure-input-indication`, and `macos-applescript`.

These are bounded parser/formatter fields. They should match pinned upstream
Ghostty's `Config.zig` defaults and keywords, but their runtime app behavior
dock drop routing, Secure Input heuristics/indicator, and AppleScript handling
remains copied-app/platform work outside this experiment.

## Changes

- `roastty/src/config/mod.rs`
  - Add a `MacOSDockDropBehavior` enum with upstream keywords `new-tab` and
    `new-window`, defaulting to `new-tab`.
  - Add `Config` fields for the enum plus the three bools:
    `macos_auto_secure_input`, `macos_secure_input_indication`, and
    `macos_applescript`, all defaulting to `true`.
  - Format `macos-dock-drop-behavior` in upstream order immediately after
    `macos-titlebar-proxy-icon` and before `macos-option-as-alt`.
  - Format `macos-auto-secure-input`, `macos-secure-input-indication`, and
    `macos-applescript` in upstream order after `macos-hidden` and before
    `macos-icon`.
  - Route `Config::set` for all four keys, including empty-value resets,
    missing-value diagnostics, and invalid enum/bool diagnostics.
  - Preserve upstream's compatibility alias `macos-dock-drop-behavior = window`
    as `new-window`, matching `compatMacOSDockDropBehavior`.
  - Update config field-order tests and add focused parse/format/reset/load
    tests for the new keys.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Mark Experiment 165 as `Designed`.
  - After result, update the Phase F remaining-public-options count from 28 to
    24 and remove the remaining `macos-*` scalar wording if this passes.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty macos_tail_config`
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
- `cargo test -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/165-macos-tail-config.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = all four keys parse, format, reset, load, and report diagnostics with
upstream defaults/order/keywords, including the `window` compatibility alias,
and the full roastty test suite passes.

**Partial** = the direct parser/formatter fields land, but compatibility,
ordering, load/replay behavior, or full-suite verification remains incomplete.

**Fail** = the fields cannot be added without conflicting with existing config
storage, formatting, or copied-app expectations.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Dirac`, fresh context.

**Verdict:** Approved after one required upstream-order fix.

**Findings:**

- Required: the initial design put all four fields immediately after
  `macos-titlebar-proxy-icon`, but upstream only places
  `macos-dock-drop-behavior` there. The three bool fields belong after
  `macos-hidden` and before `macos-icon`.

**Fix:** Updated the design to specify the two upstream insertion points
separately.

The reviewer re-reviewed the fix and approved the design with no remaining
required findings.
