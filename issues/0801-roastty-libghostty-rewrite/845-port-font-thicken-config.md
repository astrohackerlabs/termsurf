+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.result]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 845: Port the font-thicken config options

## Description

Exps 842–844 derived the render input's colors, palette, cursor, and
`row_never_extend` from the live terminal. The remaining
`FramePreparedRebuildInput` gaps are (a) the **dynamic buffers**
`highlights`/`link_ranges` and (b) the **config knobs** in `FrameRenderKnobs`.

**Architectural finding (why this experiment pivots to config):** the
`highlights`/`link_ranges` buffers (`frame_rebuild.rs:480-481`) are **not** a
simple `from_terminal` derivation. They carry search-match `Highlight`s and
hyperlink column ranges — outputs of the **search** and **hyperlink** subsystems
(the link `Highlight` enum lives in `input/link.rs:18`), produced as part of the
render loop after terminal-state/search/link updates run, not read off the
terminal grid. Wiring those subsystems is a later integration arc. So the
input-derivation arc's tractable, unblocked continuation is the **configuration
sub-arc**: porting the config options that source `FrameRenderKnobs`, and
(later) sourcing the knobs from `Config`. (The knobs already exist and are
consumed by the rebuild — `frame_rebuild.rs:489-490` — so porting their config
source is real incremental progress.)

This experiment ports the first pair — `font-thicken` and
`font-thicken-strength` — into roastty's `Config`. These are the config sources
for the knobs `FrameRenderKnobs::thicken` / `thicken_strength` (a later slice
sources the knob from `Config`). roastty's `Config` does not have them yet (Exp
842 noted them absent); upstream defines them with defaults
`font-thicken = false` and `font-thicken-strength = 255`
(`vendor/ghostty/src/config/Config.zig:337,347`).

## Changes

`roastty/src/config/mod.rs` (production code + tests). This mirrors the existing
end-to-end shape of a bool option (`background-image-repeat`) and an int option
(`window-position-x`).

- **Struct fields** on `Config` (with the upstream-key doc comment):

  ```rust
  /// `font-thicken`.
  pub font_thicken: bool,
  /// `font-thicken-strength`.
  pub font_thicken_strength: u8,
  ```

- **Defaults** in the `Default` impl: `font_thicken: false`,
  `font_thicken_strength: 255`.

- **Parse arms** in `set_cli`'s `match key`:
  - `"font-thicken" => { self.font_thicken = set_bool_field(value, default.font_thicken)? }`
    (identical to `background-image-repeat`).
  - `"font-thicken-strength" => { self.font_thicken_strength = set_value_field(value, default.font_thicken_strength, parse_u8_field)? }`,
    where `parse_u8_field` is a small helper mirroring `parse_i16_field` (or, if
    a u8 set-field helper already exists, that one). The exact helper is
    confirmed against the existing int-field plumbing during implementation.

- **Serialization** in the formatter:
  `EntryFormatter::new("font-thicken", out).entry_bool(self.font_thicken);` and
  `EntryFormatter::new("font-thicken-strength", out).entry_int(self.font_thicken_strength);`.

No render-side change yet; these are config-surface additions that a later slice
sources into `FrameRenderKnobs`.

## Verification

Per the bounded-run convention (15-min cap, Central-stamped, single tracked
task, no poll-watcher). Fast config unit tests in `config/mod.rs`:

- **Defaults:** a default `Config` has `font_thicken == false` and
  `font_thicken_strength == 255` (matching upstream).
- **Parse:** `cfg.set("font-thicken", Some("true"))` sets it true; a bare key
  (`None`) ⇒ true (the bool convention);
  `cfg.set("font-thicken-strength", Some("128"))` sets `128`; an
  out-of-range/invalid strength is rejected with `InvalidValue`.
- **Round-trip / format:** after setting, the formatted config output contains
  `font-thicken = true` and `font-thicken-strength = 128` (mirroring the
  existing `background-image-repeat` / int round-trip tests).
- **Base-0 parse fidelity:** `cfg.set("font-thicken-strength", Some("0xff"))` →
  `255` (proving `parse_u8_field` mirrors upstream `parseInt(u8, _, 0)`, like
  the `window-position-x` base-0 cases).
- **Ordered-keys formatter test:** the existing exact-ordered `keys` formatter
  test must gain `font-thicken` / `font-thicken-strength` at the positions
  matching where the two new `EntryFormatter` entries are inserted (struct
  fields, defaults, parse arms, and formatter entries all placed consistently —
  in the font-options group near `font-style`).
- `cargo build -p roastty` — no warnings. `cargo fmt -p roastty -- --check` —
  clean. Full suite via `scripts/bounded-run.sh` (default parallelism) stays
  green. No-ghostty grep on changed lines — clean (the upstream `Config.zig`
  citation is in this doc, not the code). `git diff --check` — clean.

**Pass** = the new config tests pass and the full suite stays green.
**Partial/Fail** = any test fails or the suite regresses.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED, no Required findings.** Confirmed: the upstream
defaults match (`Config.zig:337,347` → false/255); `set_bool_field` mirrors
`background-image-repeat` exactly; `set_value_field<T, E: Into<ConfigSetError>>`
composes with a `parse_u8_field` returning `MagicParseError` (which has the
`From` impl); `entry_int(impl Display)` accepts `u8` (and correctly not
`entry_optional`, since strength is a plain `u8`); no existing u8 setter, so a
base-0 `parse_u8_field` mirroring `parse_i16_field`/`parseInt(u8,_,0)` is the
faithful choice (and `"256"` overflows → `InvalidValue`); the pivot is justified
(the buffers are search/hyperlink-subsystem outputs, the knobs already exist and
are consumed, so porting their config source is real progress). Three
Optionals/Nit, all adopted:

- **Optional — imprecise citation.** `frame_rebuild.rs:46` documents
  `row_dirty`, not the highlights/links fields. **Fixed:** cite the field defs
  (`frame_rebuild.rs:480-481`) and `input/link.rs:18` instead.
- **Optional — ordered-keys formatter test.** Adding two `EntryFormatter`
  entries breaks the exact-ordered `keys` test unless the new keys are inserted
  at matching positions. **Fixed:** added to the verification (and handled in
  implementation).
- **Nit — base-0 fidelity.** **Fixed:** added a `"0xff"` → 255 parse case.

## Conclusion

_(to be written after the run)_
