+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 444: the osc-color-report-format config enum and its reports predicate (OscColorReportFormat, reports)

## Description

This experiment ports the `osc-color-report-format` config enum —
`OscColorReportFormat { None, Bits8, Bits16 }` — **and the predicate** the
termio stream handler uses to decide whether an OSC color query is answered at
all. Upstream's handler short-circuits on `osc_color_report_format == .none`
before emitting any report; this experiment captures that as an
`OscColorReportFormat::reports` method. It diversifies the config-type family
into the terminal-OSC / termio subsystem (upstream `termio/stream_handler.zig`);
the 8-bit / 16-bit report _formatting_ (the imperative `writer.print` with the
channel scaling) and the handler call site stay deferred.

## Upstream behavior

In `config/Config.zig`, the enum and its `Config` field (default `.16-bit`):

```zig
@"osc-color-report-format": OSCColorReportFormat = .@"16-bit",

pub const OSCColorReportFormat = enum {
    none,
    @"8-bit",
    @"16-bit",
};
```

In `termio/stream_handler.zig`, an OSC color `query` is gated on the format,
then formatted per the bit depth:

```zig
.query => |kind| report: {
    if (self.osc_color_report_format == .none) break :report;

    const color = ...; // resolve the queried color

    switch (self.osc_color_report_format) {
        .@"16-bit" => ... writer.print("...rgb:{x:0>4}/{x:0>4}/{x:0>4}", .{ color.r * 257, ... }),
        .@"8-bit"  => ... writer.print("...rgb:{x:0>2}/{x:0>2}/{x:0>2}", .{ color.r, ... }),
        .none => unreachable, // handled above
    }
},
```

`none` disables color reports entirely (early break); `8-bit` and `16-bit` both
emit a report, differing only in the channel precision (`16-bit` scales each
8-bit channel by `257`, i.e. `0xAB → 0xABAB`; `8-bit` writes the byte as-is).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `osc-color-report-format` config (upstream `OSCColorReportFormat`): the
/// precision of OSC color query reports. The `Config` default is `Bits16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OscColorReportFormat {
    /// Color reports disabled.
    None,
    /// Report at 8-bit channel precision (upstream `8-bit`).
    Bits8,
    /// Report at 16-bit channel precision (upstream `16-bit`).
    Bits16,
}

impl OscColorReportFormat {
    /// Whether OSC color queries are answered at all (upstream's
    /// `osc_color_report_format == .none` guard): `None` disables reports;
    /// `Bits8` and `Bits16` enable them.
    pub(crate) fn reports(self) -> bool {
        !matches!(self, OscColorReportFormat::None)
    }
}
```

`reports` is the `!= .none` guard: `false` for `None`, `true` for `Bits8` and
`Bits16` — exactly the upstream short-circuit. The `match` is exhaustive (no
wildcard).

## Scope / faithfulness notes

- **Ported (bridged)**: the `OscColorReportFormat` config enum
  (`config/Config.zig`) and its reports predicate
  (`OscColorReportFormat::reports`, upstream's
  `osc_color_report_format == .none` guard).
- **Faithful**: the enum has the three upstream variants (`none`, `8-bit`,
  `16-bit`); `reports` returns `false` only for `None`, `true` for `Bits8` and
  `Bits16` — exactly the upstream disable check.
- **Faithful adaptation**: the upstream tag names `8-bit` / `16-bit` (not valid
  Rust identifiers) map to `Bits8` / `Bits16` (documented). The `Config` field
  default (`.16-bit`) is documented on the enum but kept off it (the other
  config types keep defaults on the deferred `Config` struct). No formatting
  method is extracted — the `8-bit` / `16-bit` report bodies are imperative
  `writer.print` with channel scaling, so they port with the handler call site.
- **Deferred**: the `Config` struct / parsing (and the `.16-bit` field default),
  the 8-bit / 16-bit report formatting (the `writer.print` and the `× 257`
  channel scaling), and the termio stream-handler call site. (Consumed by a
  later slice; this experiment lands the enum and the reports predicate.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum OscColorReportFormat { None, Bits8, Bits16 }` (derive
     `Debug, Clone, Copy, PartialEq, Eq`) and
     `OscColorReportFormat::reports(self) -> bool`
     (`!matches!(self, OscColorReportFormat::None)`).
2. Tests (in `config/mod.rs`):
   - `reports`: `None.reports() == false`, `Bits8.reports() == true`,
     `Bits16.reports() == true`; the three variants distinct (via an array with
     `assert_eq!(len, 3)` and a representative `assert_ne!`) and a `Copy`/`Eq`
     round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty osc_color_report
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `OscColorReportFormat` has the three upstream variants and `reports` returns
  `false` only for `None` (`true` for `Bits8` / `Bits16`) via an exhaustive
  `match` — faithful to upstream's enum and the `!= .none` guard;
- the tests pass (the predicate; the exact variant set), and the existing tests
  still pass;
- the `Config` struct, the report formatting, and the stream-handler call site
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, `reports` treats `Bits8`
or `Bits16` as disabled (or `None` as enabled), an unrelated item changes, or
any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the variant set is exact
(`none`, `8-bit`, `16-bit`, `Config.zig:8966`), and `Bits8` / `Bits16` are a
reasonable Rust-identifier mapping of the non-identifier Zig tags; the default
`.@"16-bit"` is correctly documented as a deferred Config-field default
(`Config.zig:2920`); `reports()` correctly extracts the query-site guard
(`stream_handler.zig:1341`, `.none` suppresses reporting, the other two
proceed); and deferring the formatting is the right scope — the `8-bit` /
`16-bit` branches are writer-side behavior (the `× 257` scaling and
width-specific formatting) that belongs with the stream-handler port
(`stream_handler.zig:1371`). It judged the tests adequate for this enum +
predicate slice.

Review artifacts:

- Prompt: `logs/codex-review/20260604-105217-d444-prompt.md` (design)
- Result: `logs/codex-review/20260604-105217-d444-last-message.md` (design)

## Result

**Result:** Pass

The osc-color-report-format config enum and its reports predicate are now live.

- `roastty/src/config/mod.rs`:
  `pub(crate) enum OscColorReportFormat { None, Bits8, Bits16 }` (upstream
  `OSCColorReportFormat`; the non-identifier tags `8-bit` / `16-bit` map to
  `Bits8` / `Bits16`) and `OscColorReportFormat::reports(self) -> bool`
  (`!matches!(self, OscColorReportFormat::None)`), the extraction of upstream's
  `osc_color_report_format == .none` query guard.

Test (in `config/mod.rs`): `osc_color_report_format_reports_unless_none` — the
exact variant set (array, `assert_eq!(len, 3)`); `None.reports() == false`,
`Bits8.reports() == true`, `Bits16.reports() == true`;
`assert_ne!(Bits8, Bits16)`; `Copy`/`Eq`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2932 passed, 0 failed (+1, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The config layer now carries `OscColorReportFormat` and its reports predicate —
the seventh config slice in a row to land its consumer logic alongside the type,
and the first to reach the terminal-OSC / termio subsystem. The `Config` struct
/ parsing, the 8-bit / 16-bit report formatting (the `writer.print` and the
`× 257` channel scaling), and the stream-handler call site stay deferred. The
config-type family — now spanning renderer, font, terminal-mode, input,
clipboard, and terminal-OSC consumers — remains a clean, gated way to advance
the rewrite while the larger coupled subsystems stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed `OscColorReportFormat { None, Bits8, Bits16 }`
faithfully maps upstream `none`/`8-bit`/`16-bit`; `reports()` correctly captures
the upstream `!= .none` query guard; deferring the byte formatting and `× 257`
scaling to the stream-handler call-site slice is the right boundary; and the
test covers all variants and the predicate behavior. No public C ABI/header
impact; nothing needed to change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-105415-r444-prompt.md` (result)
- Result: `logs/codex-review/20260604-105415-r444-last-message.md` (result)
