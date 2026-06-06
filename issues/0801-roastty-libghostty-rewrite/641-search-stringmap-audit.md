+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 641: Search And StringMap Audit

## Description

Audit the Issue 801 terminal checklist line for scrollback `search` and
`StringMap`.

The README currently says scrollback search plus `StringMap` are missing and
need `oniguruma`. Current code suggests that line is stale: `StringMap` exists
using Rust's `regex` byte engine in place of Oniguruma, and the search subsystem
contains `SlidingWindow`, active/page-list/screen/viewport searchers, the
multi-screen `Search` aggregator, and a std-concurrency adaptation of upstream's
search `Thread`.

This experiment should verify that evidence against vendored Ghostty and update
the checklist wording only if the current state supports it. It is intended as a
documentation-only audit unless the verification uncovers a small missing test
that should be added immediately.

## Audit Targets

1. `vendor/ghostty/src/terminal/StringMap.zig` vs.
   `roastty/src/terminal/string_map.rs` and `roastty/src/terminal/screen.rs`:
   - per-byte string-to-pin mapping;
   - search iterator match-to-selection conversion;
   - URL-like regex matching;
   - multibyte byte-map invariants;
   - Rust `regex` substitution for Oniguruma and the removed retry-budget path.
2. `vendor/ghostty/src/terminal/search.zig` and
   `vendor/ghostty/src/terminal/search/*.zig` vs.
   `roastty/src/terminal/search/*.rs`:
   - `SlidingWindow` page encoding, forward/reverse matching, overlap handling,
     and highlight construction;
   - active, page-list, screen, and viewport searchers;
   - selected-match indexing, next/prev selection, search-all, feed/tick, and
     pruning behavior;
   - `Search` multi-screen aggregator and search `Thread` message handling /
     spawn loop.
3. Surface/app boundaries:
   - confirm that any remaining search work belongs to Surface/App integration
     or UI event plumbing, not the terminal-core checklist line.

## Changes

1. Update `issues/0801-roastty-libghostty-rewrite/README.md`:
   - if verification supports it, mark the terminal-core search/StringMap line
     complete and mention the Rust `regex` substitution;
   - otherwise refine the open item to name the specific missing terminal-core
     behavior.
2. If the audit uncovers a small missing test that should be added immediately,
   update the relevant `roastty/src/terminal/*.rs` or
   `roastty/src/terminal/search/*.rs` test module.
3. Update this experiment file with the result and review records.

## Verification

- `cargo test -p roastty terminal::string_map`
- `cargo test -p roastty terminal::search`
- `cargo test -p roastty page_list::tests::search_encode`
- `cargo test -p roastty terminal::search::thread::tests::spawn`
- `cargo test -p roastty terminal::search::thread::tests::select`
- compare/read audited Rust files against:
  - `vendor/ghostty/src/terminal/StringMap.zig`
  - `vendor/ghostty/src/terminal/search.zig`
  - `vendor/ghostty/src/terminal/search/sliding_window.zig`
  - `vendor/ghostty/src/terminal/search/active.zig`
  - `vendor/ghostty/src/terminal/search/pagelist.zig`
  - `vendor/ghostty/src/terminal/search/screen.zig`
  - `vendor/ghostty/src/terminal/search/viewport.zig`
  - `vendor/ghostty/src/terminal/search/Thread.zig`
  - `vendor/ghostty/src/Surface.zig` search integration sections
- `cargo fmt -p roastty` if Rust tests are added
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/641-search-stringmap-audit.md`
- `git diff --name-only` shows only issue docs unless the audit uncovers a small
  missing test
- `git diff --check`

Pass = the checklist accurately reflects the current terminal-core search and
StringMap state, completed items are checked only with direct test evidence, and
any remaining Surface/App integration work is not mislabeled as missing
terminal-core behavior.

Fail = the audit relies on vague coverage, marks unverified search-thread or
StringMap behavior complete, or discovers a behavioral gap that needs a
dedicated implementation experiment before the checklist can be closed.

## Design Review

Codex design review session `019e9a9a-ee48-7ec2-bb17-ea152a97b42d` approved the
design with no blocking findings. The reviewer confirmed that the upstream/local
audit scope is coherent and that the verification commands match real test
modules or prefixes. The only nit was for the result to state explicitly that
Rust `regex` is a deliberate terminal-core substitution for Oniguruma, not
evidence that the broader `oniguruma` dependency is implemented, and to keep
Surface link/search UI plumbing classified outside terminal core unless direct
local evidence says otherwise.

## Result

**Result:** Pass

The audit found that the README's "missing (needs `oniguruma`)" terminal-core
search wording was stale. No Rust code changes were needed.

`StringMap` is present and tested. Roastty maps one output byte to one screen
pin, converts regex matches back into selections, preserves multibyte byte-map
invariants, and covers URL-like matching. The local implementation deliberately
uses the Rust `regex` byte engine instead of upstream Oniguruma. That is a
terminal-core substitution only: it does not mean the broader `oniguruma`
dependency is ported for Surface link detection or other regex-driven UI
integration.

The search subsystem is present and tested:

- `SlidingWindow` covers page encoding, forward/reverse data, overlap buffers,
  case-insensitive matching, successive matches, and highlight construction;
- active, page-list, screen, and viewport searchers cover active-area search,
  scrollback/history feed, selected-match indexing, next/prev wrapping,
  search-all, pruning, viewport dirty tracking, and viewport match reporting;
- the multi-screen `Search` aggregator covers screen reconciliation, alternate
  removal, feed/tick/notify, selection events, totals, completion, and dropped
  stale screens;
- the outer search `Thread` has a std-concurrency adaptation of upstream's
  libxev loop with message handling, spawn/stop/join, change-needle, select, and
  quit/complete events.

The remaining search-related work is Surface/App integration: start/end/navigate
search commands, renderer/UI event plumbing, and link-detection regex behavior.
That belongs under the existing App/Surface/IO and `oniguruma` dependency
checklist lines, not the terminal-core search line.

Verification passed:

- `cargo test -p roastty terminal::string_map` — 9 passed
- `cargo test -p roastty terminal::search` — 96 passed
- `cargo test -p roastty page_list::tests::search_encode` — 4 passed
- `cargo test -p roastty terminal::search::thread::tests::spawn` — 2 passed
- `cargo test -p roastty terminal::search::thread::tests::select` — 4 passed
- `cargo fmt -p roastty -- --check` — passed

Source comparison was performed against:

- `vendor/ghostty/src/terminal/StringMap.zig`
- `vendor/ghostty/src/terminal/search.zig`
- `vendor/ghostty/src/terminal/search/sliding_window.zig`
- `vendor/ghostty/src/terminal/search/active.zig`
- `vendor/ghostty/src/terminal/search/pagelist.zig`
- `vendor/ghostty/src/terminal/search/screen.zig`
- `vendor/ghostty/src/terminal/search/viewport.zig`
- `vendor/ghostty/src/terminal/search/Thread.zig`
- `vendor/ghostty/src/Surface.zig` search integration sections

Completion review in Codex session `019e9a9a-ee48-7ec2-bb17-ea152a97b42d`
approved the result with no blocking findings. The reviewer agreed that marking
terminal-core search + `StringMap` complete is justified by the recorded test
coverage and source comparison, and that the Oniguruma dependency note correctly
keeps broader Surface link/search UI regex work open. The only nit was to record
this completion review before the result commit.

## Conclusion

The terminal-core `search` + `StringMap` checklist item can be marked complete.
Roastty has the core terminal search stack, StringMap, and search thread with
direct tests. The broader Oniguruma dependency remains open for Surface/App link
and search UI integration, but it no longer gates terminal-core search.
