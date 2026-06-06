+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 639: Highlight And Hyperlink Audit

## Description

Audit the Issue 801 terminal checklist line for `highlight` and `hyperlink`.

The README currently says `highlight`, `hyperlink` are "ported but untested
(finish + add tests)." Current code and prior experiments show a more nuanced
state: `terminal/highlight.rs` has value/lifecycle tests, semantic highlight
behavior is heavily tested in `page_list.rs`, hyperlink storage and propagation
are heavily tested through `page.rs`, `page_list.rs`, `screen.rs`,
`terminal.rs`, `osc.rs`, and the C ABI grid-ref accessor tests. This experiment
should verify that evidence against vendored Ghostty and update the checklist
wording only if the current state supports it.

This is intended as a documentation-only audit unless the verification uncovers
a small missing test that should be added immediately. If the audit finds a real
behavioral gap, record it in the result and leave the checklist item open with
precise remaining work.

## Audit Targets

1. `vendor/ghostty/src/terminal/highlight.zig` vs.
   `roastty/src/terminal/highlight.rs` and `page_list.rs`:
   - `Untracked`, `Tracked`, and `Flattened` value shape;
   - tracking lifecycle and rollback on failed second pin;
   - flattened start/end/untracked pin behavior;
   - semantic prompt/input/output highlight behavior and cross-page cases.
2. `vendor/ghostty/src/terminal/hyperlink.zig` vs.
   `roastty/src/terminal/hyperlink.rs`, `osc.rs`, `page.rs`, `page_list.rs`,
   `screen.rs`, `terminal.rs`, and C ABI accessors:
   - implicit and explicit hyperlink IDs;
   - page entry value shape and equality/hash context;
   - page-level insert, lookup, refcount, clone/move/free behavior;
   - OSC8 parser coverage for URI, params, id, and empty end payloads;
   - OSC8 start/end cursor state and written-cell propagation;
   - alternate-screen reset, pending-wrap overwrite/clear, insert-mode shifts,
     and scroll propagation;
   - grid-ref hyperlink URI readout.

## Changes

1. Update `issues/0801-roastty-libghostty-rewrite/README.md`:
   - if verification supports it, mark the `highlight`/`hyperlink` line complete
     and note that coverage exists across highlight/page/page_list/osc/screen/
     terminal/C ABI tests;
   - otherwise refine the open item to name the specific missing behavior.
2. If the audit uncovers a small missing test that should be added immediately,
   update the relevant `roastty/src/terminal/*.rs` test module.
3. Update this experiment file with the result and review records.

## Verification

- `cargo test -p roastty terminal::highlight`
- `cargo test -p roastty terminal::page::tests::page_hyperlink`
- `cargo test -p roastty terminal::page_list::tests::page_list_highlight`
- `cargo test -p roastty terminal::page_list::tests::page_list_hyperlink`
- `cargo test -p roastty terminal::osc::tests::osc_parser_hyperlinks`
- `cargo test -p roastty terminal::screen::tests::screen_formatter_vt_hyperlink_extra`
- `cargo test -p roastty terminal::screen::tests::screen_formatter_no_content_can_emit_only_hyperlink_extra`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_osc`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_alt_screen_switch_clears_hyperlink_and_carries_charset`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_print_repeat_uses_current_style_and_hyperlink`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_pending_wrap_overwrites_hyperlink_destination`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_insert_mode_shifts_existing_hyperlinks`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_scroll_up_preserves_printed_hyperlinks`
- `cargo test -p roastty grid_ref_accessor_c_abi_reads_graphemes_and_hyperlinks`
- compare/read audited Rust files against:
  - `vendor/ghostty/src/terminal/highlight.zig`
  - `vendor/ghostty/src/terminal/hyperlink.zig`
  - `vendor/ghostty/src/terminal/osc/parsers/hyperlink.zig`
  - relevant `Screen.zig` OSC8 hyperlink methods
- `cargo fmt -p roastty` if Rust tests are added
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/639-highlight-hyperlink-audit.md`
- `git diff --name-only` shows only issue docs unless the audit uncovers a small
  missing test
- `git diff --check`

Pass = the checklist accurately reflects the current highlight/hyperlink state,
completed items are checked only with direct test evidence, and any remaining
gap is named precisely.

Fail = the audit relies on vague coverage, marks an unverified behavior
complete, or discovers a behavioral gap that needs a dedicated implementation
experiment before the checklist can be closed.

## Design Review

Codex design review session `019e9a9a-ee48-7ec2-bb17-ea152a97b42d` initially
requested revisions:

- replace the nonexistent `screen_stream_hyperlink` filter with real screen
  formatter hyperlink filters;
- add explicit OSC8 parser coverage;
- include `cargo fmt` verification if Rust tests are added;
- narrow the clone/move/free claim to the page-level behavior that current tests
  can actually cover, or name lifecycle filters precisely.

The plan was revised to address those findings.

Follow-up review in the same session requested two process fixes:

- use `cargo fmt -p roastty` if a Rust test is added, not only
  `cargo fmt -- --check`;
- make the conditional small-test edit path explicit in the Changes section.

The plan was revised again to address both process findings.

Final follow-up review approved the design for the plan commit with no blocking
findings. The only nit was to clean up the checklist wording about coverage
areas, which this plan now does.
