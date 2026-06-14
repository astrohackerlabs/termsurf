+++
status = "open"
opened = "2026-06-13"
+++

# Issue 805: Roastty Parity with Ghostty 2c62d182

## Goal

Prove that Roastty is functionally equivalent to Ghostty commit
`2c62d182cec246764ff725096a70b9ef44996f7f` for the copied macOS app, the
embedded C ABI surface, and the terminal/runtime behavior reimplemented in
`libroastty`.

The issue is complete only when every relevant Ghostty feature, configuration
option, app workflow, input path, renderer behavior, terminal behavior, and
integration point is either proven passing in Roastty, explicitly marked not
applicable, or accepted as an intentional documented divergence.

## Background

Roastty is a Rust reimplementation of Ghostty's `libghostty` behavior with a
renamed/adapted macOS GUI on top. Issues 800 through 804 established the
architecture, ported the terminal/runtime surface, copied and renamed the macOS
app, finished GUI automation readiness, and proved keyboard and mouse automation
can drive the current app in this VM.

The upstream reference is fixed:

- Ghostty commit: `2c62d182cec246764ff725096a70b9ef44996f7f`
- Branch: `main`
- `git describe`: `tip-1608-g2c62d182c`
- Date recorded in Issue 802: 2026-05-29
- `build.zig.zon`: `version = "1.3.2-dev"`
- Zig version: `0.15.2`

Issue 802 records this exact commit as the vendored Ghostty pin and states that
the copied app and embedded ABI must match it. This issue turns that pin into a
full parity certification gate.

## Scope

Parity means user-visible and integration-visible behavior, not byte-for-byte
implementation identity. Roastty may differ internally when the Rust
implementation produces equivalent behavior.

Required audit domains:

- source-code parity against `vendor/ghostty/`;
- macOS app behavior and workflows;
- embedded C ABI shape and semantics;
- terminal parser/state behavior;
- PTY and process lifecycle behavior;
- renderer/display behavior;
- font discovery, shaping, glyph fallback, and metrics behavior;
- keyboard, mouse, dead-key, IME/preedit, and shortcut handling;
- selection, clipboard, links, menus, tabs, splits, windows, and command palette
  behavior;
- shell integration and terminfo behavior;
- configuration parsing, defaults, formatting, diagnostics, precedence, reload,
  and runtime effects;
- tests, fixtures, and app walkthrough evidence.

Allowed outcomes for each audited item:

- **Pass**: Roastty behavior is proven equivalent with tests, logs, screenshots,
  or another deterministic oracle.
- **Gap**: Roastty behavior diverges and must be fixed before closing this
  issue.
- **Intentional divergence**: Roastty deliberately differs; the difference,
  reason, user impact, and acceptance decision are documented.
- **Not applicable**: The Ghostty behavior does not apply to Roastty, with a
  concrete reason.

## Required Artifacts

This issue should maintain durable parity matrices as experiments progress:

- `feature-matrix.md` — upstream feature/workflow inventory and Roastty status.
- `config-matrix.md` — every relevant Ghostty config option, default, parser,
  formatter, diagnostic, precedence rule, and runtime behavior.
- `source-audit.md` — source subsystem audit findings and fixes.
- `walkthrough-matrix.md` — app walkthrough scenarios, automation commands,
  oracles, and results.
- `divergences.md` — accepted intentional divergences and not-applicable items.

Do not treat the matrices as optional notes. They are the proof surface for
closing the issue.

Each matrix row should record its regression guard:

- guard tier;
- guard command or checklist path;
- when it should run;
- why that tier is sufficient.

Passing behavior without a durable guard remains provisional and does not count
toward final parity certification unless it is explicitly accepted as a manual
walkthrough item.

## Regression Guard Policy

Every passing experiment result should leave behind the cheapest sufficient
guard that would catch a later regression. Do not default to slow GUI tests when
a static check, unit test, or focused integration test proves the behavior.

Guard tiers:

- **Tier 0: Static or matrix check.** Fastest. Use for inventories, ABI symbol
  drift, forbidden-name scans, generated table completeness, config coverage,
  and documentation/matrix consistency.
- **Tier 1: Unit test.** Fast. Use for parsers, config defaults, formatters,
  terminal state transitions, key encoding, selection math, renderer data
  structures, and pure helper behavior.
- **Tier 2: Focused integration test.** Medium. Use for PTY worker behavior,
  config load/reload, shell integration, clipboard helpers, process lifecycle,
  and non-GUI end-to-end paths.
- **Tier 3: GUI smoke test.** Slower. Use when the real macOS app surface is the
  behavior under test, such as launch, keyboard delivery, mouse
  click/drag/scroll, screenshots, menu actions, or accessibility state.
- **Tier 4: Full A/B parity walkthrough.** Slowest. Use for milestone checks,
  nightly/manual certification, or broad Ghostty-versus-Roastty workflow
  comparisons. Do not require this tier for every code change.

Rules:

- Add the cheapest guard that would have caught the regression.
- Slow GUI or full A/B guards require a short justification in the relevant
  matrix row or experiment result.
- Most config behavior should be guarded by static matrix checks and
  parser/default/formatter/runtime unit tests. Only representative runtime
  config behavior should require GUI proof.
- GUI tests should be small, stable, and high-signal. Prefer one representative
  smoke per app/input/rendering surface over exhaustive GUI permutations.
- If a behavior cannot yet be automated, record it as a manual walkthrough row
  with the reason and revisit automation before final certification.

## Proposed Stages

Experiments must still be created one at a time. The stages below define the
intended coverage, not a precommitted experiment list.

### Stage 1: Pin, Scope, and Inventory Schema

Create the parity matrices and define the row schema. Every row should include:

- upstream behavior or source path;
- Roastty implementation path;
- verification method;
- current status;
- evidence artifact;
- owner experiment.

### Stage 2: Source Code Audit

Audit the Roastty implementation against the pinned Ghostty source tree. Look
for bugs, missing behavior, stale assumptions, renamed-app mistakes, and
semantic mismatches. Fix gaps discovered during the audit when they are clearly
in scope for parity.

This stage should cover at least:

- `include/ghostty.h` versus `roastty/include/roastty.h`;
- `src/App.zig`, `src/Surface.zig`, and `src/apprt/embedded.zig` behavior;
- terminal, termio, renderer, font, input, config, shell-integration, and
  macOS-facing bridge behavior;
- Swift app code copied from `vendor/ghostty/macos/Sources`.

### Stage 3: Configuration Parity

Inventory every relevant option in Ghostty's pinned `Config.zig` and related
config helper files. For each option, prove or fix:

- default value;
- parser behavior;
- formatter behavior and order where relevant;
- validation/finalization behavior;
- file, CLI, environment, and runtime precedence;
- diagnostics for invalid values;
- runtime effect in the app when applicable.

Config options that are platform-specific, GTK-only, Linux-only, or otherwise
not applicable to Roastty must still be listed with a reason.

### Stage 4: Upstream Tests and Fixture Port

Identify Ghostty tests, fixtures, generated tables, and behavior examples that
apply to Roastty. Port them directly where practical. Where direct porting is
not practical, create equivalent Roastty tests or document why the upstream test
does not apply.

### Stage 5: App Walkthrough

Build both the pinned Ghostty app and current Roastty app. Walk through the real
macOS app behavior and prove workflows with automation whenever possible.

The walkthrough must cover at least:

- launch, quit, reopen, and cleanup;
- new window, tab, split, focus movement, resize, zoom, and fullscreen;
- text entry, shortcuts, dead keys, IME/preedit, and keybindings;
- mouse click, drag selection, scrollback, shift-click/drag where applicable,
  and mouse reporting;
- copy, paste, clipboard formats, bracketed paste, and selection behavior;
- links, URL opening, hover/cursor behavior, and context menus;
- menus, command palette, titlebar, quick terminal, notifications, bell, and app
  lifecycle behavior;
- font/theme changes, renderer options, window padding, opacity, cursor style,
  and other visible config-driven behavior;
- shell integration, terminfo, title reporting, working directory reporting, and
  process lifecycle.

### Stage 6: A/B Behavioral Matrix

For workflows where visual or terminal behavior matters, run Ghostty and Roastty
with the same recipe and compare deterministic artifacts:

- terminal output;
- screenshots or cropped screenshots;
- pasteboard contents;
- accessibility state;
- process/window state;
- config output;
- logs or trace files.

### Stage 7: Divergence Review

Record every accepted difference in `divergences.md`. Each entry must include:

- upstream Ghostty behavior;
- Roastty behavior;
- why the divergence exists;
- user-visible impact;
- explicit acceptance rationale.

Unaccepted divergences remain gaps and block closure.

### Stage 8: Final Parity Certification

The issue can close only when:

- every matrix row is `Pass`, `Intentional divergence`, or `Not applicable`;
- there are zero unresolved `Gap` rows;
- required automated tests pass;
- the app walkthrough passes;
- source-code audit findings are fixed or accepted as divergences;
- config parity is complete;
- the conclusion names the exact Ghostty commit hash and summarizes the final
  evidence.

## Learnings

Record concrete, reusable findings here as experiments discover how to prove or
fix Roastty parity with Ghostty. Update this section whenever an experiment
learns something that may be useful to future experiments. Keep hypotheses in
experiment files until they are proven.

- **The upstream parity target is fixed.** All source, config, ABI, and app
  behavior comparisons in this issue target Ghostty commit
  `2c62d182cec246764ff725096a70b9ef44996f7f`.
- **Use the resolved Ghostty app build wrapper.** The pinned Ghostty app should
  be built with `scripts/ghostty-app/build-macos-app.sh Debug`, not by
  hand-running `build.zig` from `vendor/ghostty/macos`.
- **Use the current Roastty app bundle.** `scripts/roastty-app/start-app.sh` now
  honors `ROASTTY_APP` and otherwise prefers the newer debug app bundle when
  both Roastty debug layouts exist.
- **The VM can synthesize keyboard and mouse input.** Issue 804 proved System
  Events keyboard input, CGEvent mouse click/drag/scroll, full-window
  screenshots, and cleanup against the current Roastty app after the required
  permissions were granted to Ghostty, the responsible host app for this Codex
  session.
- **Passing behavior needs a durable but cheap guard.** Future experiments
  should record the cheapest sufficient regression guard for each passing parity
  row. Prefer static checks and unit tests when they prove the behavior; reserve
  GUI and full A/B tests for behavior that genuinely requires the real app
  surface.
- **Plain `zig` builds pinned Ghostty on this VM.** Homebrew `zig@0.15` is on
  `PATH` as `/opt/homebrew/bin/zig`, version `0.15.2`. With that toolchain,
  pinned Ghostty builds from a clean source checkout with
  `zig build -Demit-macos-app=false` and
  `nu macos/build.nu --configuration Debug`; the old build-only
  `macos-only-xcframework.patch` is no longer needed for the baseline.
- **The pinned A/B build/render rig works.** Experiment 1 proved the debug
  Ghostty and Roastty apps can both build, launch side by side, render the same
  startup recipe through the live A/B smoke harness, capture comparable
  screenshots, and clean up their debug process trees in this VM.
- **Keyboard injection must avoid duplicate Ghostty process targeting.** When
  Codex itself is running inside installed Ghostty, System Events can resolve a
  debug Ghostty PID activation attempt to the installed `ghostty` host process.
  This can send typed test commands into the Codex window instead of the debug
  Ghostty window. Future keyboard experiments must use a safer targeting method
  or a harness that avoids the duplicate-process-name collision.
- **PID-guarded System Events keyboard input works for both debug apps.**
  Experiment 2 proved the safe sequence: activate by exact Unix PID, immediately
  verify the global frontmost PID equals the target PID, and only then type. For
  Roastty, click the terminal window center after activation to give the
  terminal view first-responder focus before the final pre-type PID guard.
- **Parity rows need evidence and guard fields.** Experiment 3 created the
  required feature, config, source-audit, walkthrough, and divergence matrices.
  Every row must name status, verification, evidence, regression guard tier,
  guard command or checklist, cadence, guard sufficiency, and owner experiment;
  divergence rows are not exempt.
- **A/B app runs should use matched config files.** Ghostty loads user config
  from `~/.config/ghostty/config`. Roastty's analogous config path is
  `~/.config/roastty/config`. The user's Ghostty config has been cloned to the
  Roastty path so early A/B runs compare the same visual and behavioral config
  wherever possible. Until a later experiment intentionally tests custom
  Roastty-only config options, keep these files aligned so Ghostty and Roastty
  should look nearly identical except for app naming.
- **App-facing ABI parity must be scoped before diffing.** Roastty's C header is
  intentionally larger than Ghostty's header, so full symbol-count equality is
  the wrong oracle. Experiment 4 uses Swift app-source identifiers as the
  app-facing ABI slice, then separately records non-app header differences as
  follow-up source-audit rows.
- **Full-header ABI gaps must be split into symbol and semantic outcomes.**
  Experiment 5 proved the mapped Ghostty header can be closed at the
  declaration/export level while still recording honest semantic divergences for
  unsupported helpers. Do not hide unsupported behavior behind a passing symbol
  check; put it in `divergences.md` with a guard.
- **Config inventory needs bounded extraction.** Ghostty's `Config.zig` contains
  the real top-level config fields, compatibility aliases, private/internal
  fields, and many later nested enum/helper values in one file. Broad scans
  produce false positives. Use the Experiment 6 helper to inventory only the
  top-level fields before the first `pub fn`, the static compatibility map, and
  Roastty's `pub(crate) struct Config`.
- **Ghostty config compatibility entries are mixed.** Some compatibility map
  entries are true renamed keys (`background-blur-radius`, `adw-toolbar-style`),
  while others are legacy values or removed boolean shims on canonical keys
  (`gtk-tabs-location = hidden`, `macos-dock-drop-behavior = window`, etc.).
  Test the parser effect, not just whether the key name appears.
- **`key-remap` missing values intentionally reset.** Unlike many config
  options, pinned Ghostty's `RemapSet.parseCLI` treats `null`/missing input as
  an empty reset. For Roastty, `key-remap`, `key-remap =`, and `--key-remap`
  should clear the remap set; invalid non-empty remaps should report diagnostics
  without inventing a `ValueRequired` branch.
- **`theme` parser parity is macOS/non-Windows scoped.** Pinned Ghostty routes
  any comma, equals sign, or colon in `theme` through the light/dark pair parser
  on macOS and other non-Windows builds. Its Windows-only drive-letter exception
  for `C:\...` paths is outside Issue 805's copied macOS app parity oracle and
  should be tracked separately if Windows config parity work starts.
- **`keybind` parser parity is parser-surface only.** Experiment 48 proves
  pinned Ghostty `Keybinds.parseCLI` semantics for defaults, clear, root
  bindings, key sequences, chains, tables, slash disambiguation, trigger
  prefixes, diagnostics, CLI, formatting, equality, and clone behavior. Runtime
  shortcut dispatch remains covered by separate keybinding/runtime facets.
- **Default config formatter parity needs an A/B fixture.** Experiment 8 found
  and fixed non-repeatable formatter order drift by comparing pinned Ghostty
  `+show-config --default --no-pager` output to Roastty
  `Config::default().format_config(...)`. After app-name normalization, all 454
  comparable default lines now match exactly; the remaining default-format gaps
  are isolated to `keybind` and `command-palette-entry` repeatable surfaces.
- **Ghostty keybind formatting does not print `performable:` flags.** Experiment
  9 matched pinned Ghostty's default keybind output by preserving the runtime
  performable flags internally while omitting them from formatted config lines,
  using Ghostty's modifier order (`super`, `ctrl`, `alt`, `shift`), and adding
  the physical `digit_1` through `digit_8` aliases that pair with the unicode
  number shortcuts.
- **Default command-palette text actions store semantic UTF-8.** Experiment 10
  matched pinned Ghostty's default command-palette output by storing the
  built-in Ghostty text action as the semantic UTF-8 text payload, letting the
  shared action formatter emit Ghostty's `\xf0\x9f\x91\xbb` bytes instead of
  double-escaping a pre-escaped string.
- **Ghostty's default config output must also parse.** Experiment 11 proved all
  635 pinned Ghostty default config lines are accepted by Roastty's per-line
  parser. The default parser gaps were void codepoint-map reset lines,
  `background-image-opacity`, and keybind triggers where `=` or `+` is both
  syntax and the key.
- **Config parity rows are facet-based.** Experiment 12 decomposed the config
  matrix so canonical option rows prove inventory/default-surface coverage only.
  Remaining config work is tracked by explicit facet `Gap` rows for non-default
  parsers, non-default formatters, diagnostics, validation/finalization,
  precedence/load semantics, reload behavior, and runtime/UI effects.
- **The `link` config option is the first concrete parser dispatch gap.**
  Experiment 13 mapped all 203 canonical config options to Roastty parser rows.
  It found 202 options with identified `Config::set_from_source` dispatch and
  one missing dispatch row: upstream canonical `link`. Pinned Ghostty's
  `RepeatableLink.parseCLI` is itself `NotImplemented`, so the next parser
  experiment must decide and prove the exact equivalent Roastty behavior instead
  of treating `UnknownField` as parity by accident.
- **`link` is recognized-but-unsupported, not unknown.** Experiment 14 matched
  Ghostty's parser shape for canonical `link`: set-but-empty `link =` resets to
  the default link list before parser dispatch, while bare or non-empty `link`
  returns a recognized not-implemented parser error. CFG-217 still remains `Gap`
  because parser-family oracles are not complete, but there are no remaining
  canonical parser dispatch gaps.
- **Ordinary boolean parser rows share one upstream oracle.** Experiment 15
  proved the direct parser semantics for 39 ordinary `set_bool_field` rows:
  upstream true spellings, false spellings, bare true, set-but-empty default
  reset, and invalid values. `config-default-files` remains `Audit covered`
  because its direct parser and effective default-file load-order semantics must
  be proven together under CFG-221.
- **Integer scalar parser rows share one upstream oracle.** Experiment 16 proved
  the direct parser semantics for all 10 integer scalar rows using
  representative `u32`, `usize`, `u64`, `i16`, and `u8` fields: base-0 decimal,
  lowercase and uppercase prefixes, accepted signs, interior underscores,
  set-but-empty reset, missing values, invalid bare signs/prefixes,
  prefix-adjacent underscores, and overflow/range failures.
- **Float scalar parser rows need Zig syntax, not Rust syntax.** Experiment 17
  found and fixed concrete Rust/Zig parser differences for direct float fields:
  Zig accepts digit separators and hexadecimal float literals such as `0x1p4`,
  exponentless hex floats, mixed-case signed `nan`/`inf`/`infinity`, and
  overflow to infinities. Roastty now uses a shared Zig-compatible float helper
  for the 9 float scalar parser rows.
- **Direct string parser rows copy bytes exactly.** Experiment 18 matched
  Ghostty's `[]const u8` / `[:0]const u8` type-magic behavior for the 9 string
  scalar rows: missing values are required, explicit empty strings are accepted
  at the child parser level, `key =` resets through the surrounding dispatch
  helper, and embedded NUL bytes are preserved instead of rejected.
- **Duration parser rows share one upstream grammar.** Experiment 19 proved the
  4 duration parser rows against Ghostty's `Duration.parseCLI` shape: all units,
  longest-unit matching, adjacent and whitespace-separated segments, trailing
  whitespace, bare zero, malformed values, missing values, required/optional
  empty-reset behavior, product-overflow saturation, and over-wide decimal
  literal rejection.
- **Path parser rows must use option-boundary semantics.** Experiment 20 proved
  the 3 path parser rows against Ghostty's `?Path` plus `Path.parseCLI` and
  `RepeatablePath.parseCLI` behavior: leading `?` optional markers, quoted
  literal `?` paths, parsed-empty single-path no-ops, raw-empty resets,
  repeatable accumulation/clearing, formatter output, and embedded NUL
  acceptance at the parser layer.
- **`working-directory` parser rows are parser-only before finalization.**
  Experiment 21 proved the direct parser boundary for `working-directory`: ASCII
  whitespace trimming, surrounding quote stripping, exact lowercase
  `home`/`inherit` keywords, path fallback for all other strings, embedded NUL
  acceptance, raw-empty optional reset, and formatter output. Home expansion and
  probable-CLI defaults remain separate non-parser facets.
- **`command-palette-entry` parser parity is repeatable auto-struct parity.**
  Experiment 22 proved the direct parser boundary for `command-palette-entry`:
  missing/raw-empty values restore the default list, exact `clear` empties it,
  valid entries append through auto-struct parsing, quoted commas are preserved,
  duplicate fields use the last value, actions are canonicalized, invalid
  fields/actions/quotes/escapes are rejected, and formatter output repeats one
  line per entry.
- **Window padding parser rows share one pair parser.** Experiment 23 proved the
  direct parser boundary for `window-padding-x` and `window-padding-y`: one
  base-10 `u32` applies to both sides, comma-separated pairs set each side, only
  spaces and tabs are trimmed around values, raw-empty option values reset to
  defaults, invalid numeric/pair forms are rejected, and formatter output
  collapses equal sides to one value.
- **Packed flag parser rows share one packed-struct parser.** Experiment 24
  proved the direct parser boundary for the 9 packed-flags rows: standalone
  Ghostty bool spellings set every flag, comma lists start from struct defaults,
  `no-` prefixes disable named flags, only spaces and tabs are trimmed around
  comma parts, hyphenated field names are exact, duplicate flags use the later
  token, raw-empty option values reset to defaults, invalid tokens are rejected,
  diagnostics preserve earlier valid values, and formatter output follows
  upstream field order.
- **Unsupported parser rows can still be oracle-complete.** Experiment 25
  promoted canonical `link` by proving pinned Ghostty's current
  `RepeatableLink.parseCLI` boundary: `link` is recognized, bare and non-empty
  values return the not-implemented parser error, raw-empty `link =` resets to
  defaults before parser dispatch, diagnostics preserve that distinction, and
  truly unknown keys still report `UnknownField`.
- **`link` is an intentional no-output formatter row.** Experiment 50 found that
  pinned Ghostty's canonical `link` option has a `RepeatableLink.formatEntry`
  method that intentionally emits nothing because `link` cannot currently be
  set. Formatter inventories and oracles should count `link` as a canonical
  no-output row rather than forcing a nonexistent Roastty
  `Config::format_config` helper.
- **Primitive formatter rows are inventory-family scoped.** Experiment 51
  promoted only rows classified as `boolean`, `integer`, `float`, or `string` in
  `config-formatter-inventory.md`. Rows that happen to use primitive helpers but
  are classified under another family, such as `font`, remain out of scope for
  the primitive oracle and need their own family-specific formatter proof.
- **Metric modifier formatter rows are inventory-family scoped.** Experiment 52
  promoted the 12 non-font rows classified as `metric modifier`. Font-classified
  adjust rows such as `adjust-font-baseline` still need the broader font
  formatter oracle even though they use the same local `format_metric_modifier`
  helper.
- **Window padding formatter rows are a compact independent family.** Experiment
  53 promoted the four rows classified as `window padding`. The formatter oracle
  must cover both compact padding forms (`N` and `left,right`), every balance
  keyword, every padding color keyword, empty resets, and the local order of the
  four rows.
- **Repeatable path formatter classification must use exact options.**
  Experiment 54 found that `custom-shader-animation` was a classifier false
  positive because its Rust identifier contains `custom_shader`; the actual
  repeatable path formatter family is `config-file`, `custom-shader`, and
  `gtk-custom-css`.
- **Color formatter rows are keyword rows in this inventory.** Experiment 55
  promoted `osc-color-report-format` and `window-colorspace`; these are
  keyword/enum-style rows associated with color behavior, not arbitrary RGB
  color formatters. The actual `Config::format_config` order is
  `window-colorspace` before `osc-color-report-format`.
- **Key-remap formatter output is normalized and ordered by remap internals.**
  Experiment 56 promoted the single `key-remap` formatter row. Alias inputs such
  as `control`, `command`, and `option` format as concrete side-specific names,
  and one observed CLI normalized order is `right_ctrl=left_super`,
  `right_alt=left_ctrl`, then `left_ctrl=left_super`.
- **Canonical `link` is a proven no-output formatter row.** Experiment 57
  promoted the single `no-output` formatter row. Pinned Ghostty's
  `RepeatableLink.formatEntry` intentionally emits nothing because `link` cannot
  currently be set; Roastty now has an oracle proving `format_config` emits no
  `link = ` line while adjacent `link-url` still formats normally.
- **Command-palette formatter coverage reuses the parser-family oracle.**
  Experiment 58 promoted the single `command-palette-entry` formatter row using
  existing focused tests that already prove default entries, clear output,
  custom entries, quoted comma values, shorthand actions, reset behavior,
  diagnostics, and exact formatted output.
- **Cleared keybind tables are formatter-silent in pinned Ghostty.** Experiment
  59 found that `foo/` clears a key table, but Ghostty's formatter emits no
  empty `keybind = foo/` line afterward. Roastty now matches that behavior and
  the single `keybind` formatter row is promoted by a dedicated formatter
  oracle.
- **Optional scalar formatter rows are distinct from optional custom rows.**
  Experiment 60 split 11 `entry_optional` rows that recurse into `entry_bool`,
  `entry_int`, or `entry_str` into an `optional scalar` formatter family. These
  rows share void output for `None`, scalar output for `Some`, raw-empty reset
  behavior, and representative declaration-order checks. Optional custom
  `format_entry` rows and font rows remain unpromoted.
- **Optional single-color rows are distinct from optional color-list rows.**
  Experiment 61 promoted 10 optional rows backed by `Color`, `TerminalColor`, or
  `BoldColor`. These rows share void output for `None`, lowercase `#rrggbb`
  output for colors, sentinel keyword output, `bright` bold output, and
  raw-empty reset behavior. `macos-icon-screen-color` remains unpromoted because
  it formats an optional color list.
- **Enum parser rows share exact keyword semantics plus compatibility
  branches.** Experiment 26 proved the 52 enum rows: required and optional enum
  fields accept exact keywords only, missing values are required, raw-empty
  values reset to defaults, invalid
  numeric/uppercase/snake-case/whitespace-padded values are rejected,
  diagnostics preserve earlier valid values, and the pinned compatibility
  branches for `macos-dock-drop-behavior = window`,
  `gtk-single-instance = desktop`, and `gtk-tabs-location = hidden` are part of
  the enum-family oracle.
- **Color parser rows share color, terminal-color, and bold-color semantics.**
  Experiment 27 proved the 16 color rows: named colors and hex values parse
  through `Color`, required and optional `TerminalColor` rows accept
  `cell-foreground` and `cell-background`, plain `Color` rows reject those
  sentinels, `BoldColor` accepts `bright`, missing values are required,
  raw-empty values reset to defaults, invalid colors and padded sentinel
  keywords are rejected, diagnostics preserve earlier valid values, and
  formatter output canonicalizes colors to lowercase hex or sentinel keywords.
- **Metric modifier parser rows use Zig numeric syntax.** Experiment 28 proved
  the 13 `parse_metric_modifier` rows: absolute values use Ghostty's
  `std.fmt.parseInt(i32, input, 10)` shape, percentage bodies use
  `std.fmt.parseFloat(f64, ...)`, values at or below `-100%` clamp to `0`,
  special floats such as `nan`, `inf`, and `infinity` are accepted in the
  percentage branch, hexadecimal floats and interior underscores are accepted
  where Zig accepts them, malformed separators and payload NaNs are rejected,
  and formatter output intentionally preserves Zig-style floating precision
  artifacts such as `15.999999999999993%`.
- **Background blur parses bools before radii.** Experiment 29 proved the
  canonical `background-blur` parser row: a bare value sets `true`, `1` and `0`
  are bools rather than radii, exact glass keywords are accepted, non-bool
  numbers parse as base-0 `u8` radii, raw-empty config values reset to the
  default `false`, direct empty parser input is invalid, and malformed numeric
  boundaries such as leading or trailing underscores are rejected while interior
  underscores, including doubled interior underscores, are accepted like Zig.
- **Click repeat interval parser stays separate from finalization.** Experiment
  30 proved canonical `click-repeat-interval`: parser-level values use base-10
  `u32` syntax, raw-empty values reset to `0`, missing values are required,
  whitespace-padded integers and base prefixes are rejected, and `0` remains `0`
  until finalization later resolves the platform/default repeat interval.
- **Cursor style blink is optional bool dispatch.** Experiment 31 proved the
  canonical `cursor-style-blink` parser row: default `null` formats as blank,
  bare/missing values set `true`, raw-empty config values reset to `null`, exact
  Ghostty bool spellings set `true` or `false`, and uppercase words,
  whitespace-padded values, and numeric values outside `0`/`1` are rejected.
- **macOS icon screen colors use ColorList boundaries.** Experiment 32 proved
  canonical `macos-icon-screen-color`: default `null` formats as blank,
  comma-separated named and hex colors format as lowercase hex, spaces and tabs
  are trimmed per token, leading/trailing/doubled comma empty tokens are
  skipped, every parse resets the list, raw-empty config values reset to `null`,
  direct missing or empty child-parser values are required, all-empty or invalid
  lists are rejected, and the 65th color exceeds the upstream 64-color cap.
- **Selection word chars parse through Ghostty string escapes.** Experiment 33
  proved canonical `selection-word-chars`: parsed lists always begin with null,
  explicit empty values are valid and leave only that null boundary, missing
  values are required, literal characters plus `\t`, `\\`, and `\u{...}` escapes
  are accepted, invalid escapes preserve the previous valid list, formatter
  output skips the leading null, invalid Unicode codepoints are skipped, and
  output stops before the upstream 4096-byte buffer cap.
- **Window decoration parses bools before variants.** Experiment 34 proved
  canonical `window-decoration`: direct missing parser input is `auto`, bool
  tokens map true to `auto` and false to `none`, exact variants
  `auto`/`client`/`server`/`none` are accepted, empty strings, unknown values,
  whitespace-padded values, and case-changed values are rejected, and formatting
  emits the canonical keyword.
- **Mouse scroll multiplier uses auto-struct plus Zig floats.** Experiment 35
  proved canonical `mouse-scroll-multiplier`: bare values set both fields,
  auto-struct fields preserve unspecified current values, explicit empty values
  are no-ops, quoted field values are decoded before parsing, Zig float syntax
  such as hex floats, infinities, and NaNs is accepted, malformed structures and
  bad floats are rejected, and finalization/clamping remains a separate facet.
- **Quick terminal size uses Zig numbers in both units.** Experiment 36 proved
  canonical `quick-terminal-size`: pixel values use Zig-compatible base-10 `u32`
  syntax, percentage values use Zig-compatible `f32` syntax, comma-separated
  primary/secondary values trim CLI whitespace, empty config values reset to
  default, invalid units and malformed numbers are rejected, and formatter plus
  representative calculation behavior match the pinned helper.
- **Command parser rejects whole-empty input, not empty prefixed payloads.**
  Experiment 37 proved canonical `command` and `initial-command`: the shared
  parser trims ASCII spaces before prefix detection and rejects only missing,
  empty, or all-space whole inputs; `direct:` / `direct:   ` produce a direct
  command with one empty argument, `shell:` / `shell:   ` produce an empty shell
  command, exact `direct:` and `shell:` prefixes select parser mode, unknown
  colon prefixes remain shell commands, and direct payloads split naively on
  ASCII spaces.
- **Palette parser mutates only after key and color parse.** Experiment 38
  proved canonical `palette`: values require the first `=`, keys trim ASCII
  space/tab and parse with Zig base-0 `u8` syntax, color suffixes parse through
  `Color.parseCLI`, failed key or color parses leave prior palette values and
  mask bits unchanged, explicit empty config values reset to the default palette
  through optional dispatch, and formatting emits all 256 entries.
- **Env parser is RepeatableStringMap semantics.** Experiment 39 proved
  canonical `env`: missing values and whitespace-only direct values are
  required-value errors, an exactly empty value clears the whole map, the first
  `=` splits key from value, ASCII whitespace is trimmed around both sides,
  empty keys are accepted, empty parsed values delete a key, repeated keys
  overwrite, and equality ignores insertion order.
- **Repeatable paths distinguish raw-empty reset from parsed-empty no-op.**
  Experiment 40 proved canonical `custom-shader` and `gtk-custom-css`: missing
  values are required, raw empty values clear the list, required paths append,
  leading `?` paths append as optional, quoted leading `?` paths stay required,
  parsed-empty values `?`, `""`, and `?""` are ignored, formatting emits one
  entry per path, and file/CLI loading expands relative paths against the
  correct base.
- **Input parser validates before tag fallback.** Experiment 41 proved canonical
  `input`: missing values are required, an exactly empty repeatable value clears
  the list, non-empty entries validate with Zig string-literal syntax before any
  append, `raw:` and `path:` select explicit storage, unknown-tag values fall
  back to raw input, `raw:` may carry an empty payload, and invalid entries
  leave the existing list unchanged.
- **Repeatable font strings share one helper with a CLI exception.** Experiment
  42 proved canonical `font-family`, `font-family-bold`, `font-family-italic`,
  `font-family-bold-italic`, and `font-feature`: missing values are required,
  exact empty values clear the list, non-empty values append byte-for-byte,
  `overwrite_next` clears only before the next append and then resets,
  clones/equality ignore `overwrite_next`, CLI font-family values replace file
  values, and CLI font-feature values append normally.
- **Font style parsing treats most input as a name.** Experiment 43 proved
  canonical `font-style`, `font-style-bold`, `font-style-italic`, and
  `font-style-bold-italic`: missing values are required, exact `default` and
  `false` tokens select their special variants, every other supplied value is a
  named style without trimming or validation, direct empty input is an empty
  name, while set-but-empty config dispatch resets the field to `default`.
- **Font variation values use Zig float syntax.** Experiment 44 proved canonical
  `font-variation`, `font-variation-bold`, `font-variation-italic`, and
  `font-variation-bold-italic`: values split on the first `=`, axis ids and
  values trim ASCII space/tab, axis ids must be exactly four bytes, value
  parsing accepts the Zig `f64` space including underscores, hex floats,
  NaN/Inf, overflow, and underflow, invalid values leave the list unchanged, and
  set-but-empty config dispatch resets the repeatable list.
- **Codepoint maps split direct parsing from config reset.** Experiment 45
  proved canonical `font-codepoint-map` and `clipboard-codepoint-map`: direct
  empty input is invalid, set-but-empty config dispatch resets the map, range
  keys use Ghostty's `U+...`/range/comma grammar, font maps store descriptor
  family strings, and clipboard maps preserve pinned Ghostty `u21` behavior for
  non-scalar-but-in-range keys and replacement codepoints.

## Verification

This issue is complete when the matrices prove total accepted parity with
Ghostty commit `2c62d182cec246764ff725096a70b9ef44996f7f`.

The final conclusion must not claim parity from incomplete evidence. If a row
has no deterministic proof, it is not passing. If the behavior is not tested and
not explicitly accepted as a divergence or not-applicable item, the issue
remains open.

## Experiments

- [Experiment 1: Pinned A/B baseline](01-pinned-ab-baseline.md) — **Partial**
- [Experiment 2: Keyboard target isolation](02-keyboard-target-isolation.md) —
  **Pass**
- [Experiment 3: Parity matrix schema](03-parity-matrix-schema.md) — **Pass**
- [Experiment 4: Embedded ABI app bridge audit](04-embedded-abi-app-bridge-audit.md)
  — **Partial**
- [Experiment 5: Resolve non-app embedded ABI functions](05-resolve-non-app-embedded-abi-functions.md)
  — **Pass**
- [Experiment 6: Config option inventory](06-config-option-inventory.md) —
  **Pass**
- [Experiment 7: Config compatibility alias semantics](07-config-compatibility-alias-semantics.md)
  — **Pass**
- [Experiment 8: Default config format oracle](08-default-config-format-oracle.md)
  — **Pass**
- [Experiment 9: Default keybind format parity](09-default-keybind-format-parity.md)
  — **Pass**
- [Experiment 10: Command palette default format parity](10-command-palette-default-format-parity.md)
  — **Pass**
- [Experiment 11: Default config parser oracle](11-default-config-parser-oracle.md)
  — **Pass**
- [Experiment 12: Config matrix facet decomposition](12-config-matrix-facet-decomposition.md)
  — **Pass**
- [Experiment 13: Non-default parser facet audit](13-non-default-parser-facet-audit.md)
  — **Pass**
- [Experiment 14: Link parser recognition](14-link-parser-recognition.md) —
  **Pass**
- [Experiment 15: Boolean parser oracle](15-boolean-parser-oracle.md) — **Pass**
- [Experiment 16: Integer parser oracle](16-integer-parser-oracle.md) — **Pass**
- [Experiment 17: Float parser oracle](17-float-parser-oracle.md) — **Pass**
- [Experiment 18: String parser oracle](18-string-parser-oracle.md) — **Pass**
- [Experiment 19: Duration parser oracle](19-duration-parser-oracle.md) —
  **Pass**
- [Experiment 20: Path parser oracle](20-path-parser-oracle.md) — **Pass**
- [Experiment 21: Working directory parser oracle](21-working-directory-parser-oracle.md)
  — **Pass**
- [Experiment 22: Command palette parser oracle](22-command-palette-parser-oracle.md)
  — **Pass**
- [Experiment 23: Window padding parser oracle](23-window-padding-parser-oracle.md)
  — **Pass**
- [Experiment 24: Packed flags parser oracle](24-packed-flags-parser-oracle.md)
  — **Pass**
- [Experiment 25: Unsupported parser oracle](25-unsupported-parser-oracle.md) —
  **Pass**
- [Experiment 26: Enum parser oracle](26-enum-parser-oracle.md) — **Pass**
- [Experiment 27: Color parser oracle](27-color-parser-oracle.md) — **Pass**
- [Experiment 28: Metric modifier parser oracle](28-metric-modifier-parser-oracle.md)
  — **Pass**
- [Experiment 29: Background blur parser oracle](29-background-blur-parser-oracle.md)
  — **Pass**
- [Experiment 30: Click repeat interval parser oracle](30-click-repeat-interval-parser-oracle.md)
  — **Pass**
- [Experiment 31: Cursor style blink parser oracle](31-cursor-style-blink-parser-oracle.md)
  — **Pass**
- [Experiment 32: macOS icon screen color parser oracle](32-macos-icon-screen-color-parser-oracle.md)
  — **Pass**
- [Experiment 33: Selection word chars parser oracle](33-selection-word-chars-parser-oracle.md)
  — **Pass**
- [Experiment 34: Window decoration parser oracle](34-window-decoration-parser-oracle.md)
  — **Pass**
- [Experiment 35: Mouse scroll multiplier parser oracle](35-mouse-scroll-multiplier-parser-oracle.md)
  — **Pass**
- [Experiment 36: Quick terminal size parser oracle](36-quick-terminal-size-parser-oracle.md)
  — **Pass**
- [Experiment 37: Command parser oracle](37-command-parser-oracle.md) — **Pass**
- [Experiment 38: Palette parser oracle](38-palette-parser-oracle.md) — **Pass**
- [Experiment 39: Env parser oracle](39-env-parser-oracle.md) — **Pass**
- [Experiment 40: Repeatable path parser oracle](40-repeatable-path-parser-oracle.md)
  — **Pass**
- [Experiment 41: Input parser oracle](41-input-parser-oracle.md) — **Pass**
- [Experiment 42: Repeatable string font parser oracle](42-repeatable-string-font-parser-oracle.md)
  — **Pass**
- [Experiment 43: Font style parser oracle](43-font-style-parser-oracle.md) —
  **Pass**
- [Experiment 44: Font variation parser oracle](44-font-variation-parser-oracle.md)
  — **Pass**
- [Experiment 45: Codepoint map parser oracle](45-codepoint-map-parser-oracle.md)
  — **Pass**
- [Experiment 46: Key remap parser oracle](46-key-remap-parser-oracle.md) —
  **Pass**
- [Experiment 47: Theme parser oracle](47-theme-parser-oracle.md) — **Pass**
- [Experiment 48: Keybind parser oracle](48-keybind-parser-oracle.md) — **Pass**
- [Experiment 49: Config default files load oracle](49-config-default-files-load-oracle.md)
  — **Pass**
- [Experiment 50: Non-default formatter facet audit](50-non-default-formatter-facet-audit.md)
  — **Pass**
- [Experiment 51: Primitive formatter oracle](51-primitive-formatter-oracle.md)
  — **Pass**
- [Experiment 52: Metric modifier formatter oracle](52-metric-modifier-formatter-oracle.md)
  — **Pass**
- [Experiment 53: Window padding formatter oracle](53-window-padding-formatter-oracle.md)
  — **Pass**
- [Experiment 54: Repeatable path formatter oracle](54-repeatable-path-formatter-oracle.md)
  — **Pass**
- [Experiment 55: Color keyword formatter oracle](55-color-keyword-formatter-oracle.md)
  — **Pass**
- [Experiment 56: Key remap formatter oracle](56-key-remap-formatter-oracle.md)
  — **Pass**
- [Experiment 57: Link no-output formatter oracle](57-link-no-output-formatter-oracle.md)
  — **Pass**
- [Experiment 58: Command palette formatter oracle](58-command-palette-formatter-oracle.md)
  — **Pass**
- [Experiment 59: Keybind formatter oracle](59-keybind-formatter-oracle.md) —
  **Pass**
- [Experiment 60: Optional scalar formatter oracle](60-optional-scalar-formatter-oracle.md)
  — **Pass**
- [Experiment 61: Optional color formatter oracle](61-optional-color-formatter-oracle.md)
  — **Pass**
- [Experiment 62: Optional path formatter oracle](62-optional-path-formatter-oracle.md)
  — **Designed**
