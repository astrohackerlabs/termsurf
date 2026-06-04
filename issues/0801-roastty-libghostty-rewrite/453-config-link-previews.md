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

# Experiment 453: the link-previews config enum and its preview predicates (LinkPreviews, previews_regular_link / previews_osc8_link)

## Description

This experiment ports the `link-previews` config enum —
`LinkPreviews { False, True, Osc8 }` — **and the two predicates** the surface
uses to decide whether to show a preview for a link, depending on the link's
kind. Upstream's `Surface` shows a preview for a regular (detected) link only
when `link_previews == .true`, and for an OSC8 hyperlink when
`link_previews != .false` (i.e. `true` or `osc8`). This experiment captures
those two distinct checks as `previews_regular_link` and `previews_osc8_link`
methods. The surface preview call sites (resolving the URL, updating the status
bar) stay deferred.

## Upstream behavior

In `config/Config.zig`, the enum and its `Config` field (default `.true`):

```zig
@"link-previews": LinkPreviews = .true,

pub const LinkPreviews = enum {
    false,
    true,
    osc8,
};
```

In `Surface.zig`, the preview decision depends on the link kind:

```zig
.open => { // a regular (detected) link
    break :link .{ .{ .url = str }, self.config.link_previews == .true };
},
._open_osc8 => { // an explicit OSC8 hyperlink
    break :link .{ .{ .url = ... }, self.config.link_previews != .false };
},
```

`false` disables previews entirely; `true` previews every link; `osc8` previews
only OSC8 hyperlinks. So a **regular** link is previewed only when the config is
`true` (`== .true`), and an **OSC8** hyperlink is previewed when the config is
`true` or `osc8` (`!= .false`).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `link-previews` config (upstream `LinkPreviews`): when to show a preview
/// for a link. The `Config` default is `True`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkPreviews {
    /// No link previews.
    False,
    /// Preview every link.
    True,
    /// Preview only OSC8 hyperlinks.
    Osc8,
}

impl LinkPreviews {
    /// Whether to preview a regular (detected) link (upstream's `link_previews ==
    /// .true` check): only when `True`.
    pub(crate) fn previews_regular_link(self) -> bool {
        matches!(self, LinkPreviews::True)
    }

    /// Whether to preview an OSC8 hyperlink (upstream's `link_previews != .false`
    /// check): when `True` or `Osc8`.
    pub(crate) fn previews_osc8_link(self) -> bool {
        !matches!(self, LinkPreviews::False)
    }
}
```

`previews_regular_link` is the `== .true` check (`true` only for `True`);
`previews_osc8_link` is the `!= .false` check (`true` for `True` and `Osc8`) —
exactly the two upstream conditions. Both `match`/`matches!` are exhaustive.

## Scope / faithfulness notes

- **Ported (bridged)**: the `LinkPreviews` config enum (`config/Config.zig`) and
  its two preview predicates (`previews_regular_link` / `previews_osc8_link`,
  upstream's `Surface` `== .true` / `!= .false` checks).
- **Faithful**: the enum has the three upstream variants (`false`, `true`,
  `osc8`); `previews_regular_link` returns `true` only for `True` (the
  `== .true` check) and `previews_osc8_link` returns `true` for `True` and
  `Osc8` (the `!= .false` check) — exactly upstream's two conditions.
- **Faithful adaptation**: the consumer is modeled as two methods (upstream
  inlines the two comparisons at the regular-link and OSC8 branches); each
  returns the positive "show a preview" decision.
- **Deferred**: the `Config` struct / parsing (and the `.true` field default),
  and the surface preview call sites (resolving the link / OSC8 URI, updating
  the status bar) that consume the decision. (Consumed by a later slice; this
  experiment lands the enum and the two predicates.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum LinkPreviews { False, True, Osc8 }` (derive
     `Debug, Clone, Copy, PartialEq, Eq`) and
     `LinkPreviews::previews_regular_link(self) -> bool`
     (`matches!(self, LinkPreviews::True)`) and
     `LinkPreviews::previews_osc8_link(self) -> bool`
     (`!matches!(self, LinkPreviews::False)`).
2. Tests (in `config/mod.rs`):
   - the two predicates over the three variants: `False` → `false`/`false`,
     `True` → `true`/`true`, `Osc8` → `false`/`true` (regular / osc8); the
     variants distinct and a `Copy`/`Eq` round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty link_previews
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `LinkPreviews` has the three upstream variants; `previews_regular_link`
  returns `true` only for `True` and `previews_osc8_link` returns `true` for
  `True` / `Osc8` — faithful to upstream's two checks;
- the tests pass (the two predicates over the three variants; the distinct
  variants), and the existing tests still pass;
- the `Config` struct and the surface preview call sites stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, a predicate maps a
variant the wrong way (e.g. `Osc8` previewing a regular link, or `Osc8` not
previewing an OSC8 link), an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the variants match
exactly (`false`, `true`, `osc8`, `Config.zig:5282`); the default `.true` is
correctly documented as deferred to the future `Config` field
(`Config.zig:1436`); the two predicates correctly preserve the different
upstream checks (regular detected links preview only for `.true`,
`Surface.zig:1608`; OSC8 hyperlinks preview for anything except `.false`,
`Surface.zig:1619`); splitting into two methods is the right modeling (a single
`enabled()` would lose the `osc8` distinction); and the 3-variant truth table
covers the semantics fully.

Review artifacts:

- Prompt: `logs/codex-review/20260604-112926-d453-prompt.md` (design)
- Result: `logs/codex-review/20260604-112926-d453-last-message.md` (design)
