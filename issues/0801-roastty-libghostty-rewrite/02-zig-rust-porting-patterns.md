# Experiment 2: Define Zig-to-Rust Porting Patterns

## Description

Before porting a real `libghostty` subsystem into `libroastty`, define the code
translation patterns Roastty will use when adapting Ghostty's Zig code to Rust.

This experiment is diagnostic and architectural. It should study representative
upstream Ghostty Zig code and record practical Rust translation rules. It should
not port a subsystem yet. The goal is to prevent every later experiment from
re-deciding the same questions about Zig allocators, `comptime`, tagged unions,
error unions, packed structs, pointer-heavy terminal data structures, C ABI
handles, tests, and `unsafe` Rust.

Roastty is macOS-only. Non-macOS Zig branches should be classified as omitted
unless they reveal a reusable pattern needed by the macOS implementation.

## Questions

Answer these questions in the result:

1. What recurring Zig language patterns appear in the Ghostty subsystems Roastty
   will port?
2. What Rust pattern should Roastty use for each recurring Zig pattern?
3. When is `unsafe` Rust acceptable during the initial faithful port?
4. Which upstream patterns should be preserved closely for behavior parity, and
   which should be simplified because Roastty is macOS-only?
5. How should Roastty translate upstream tests so behavior parity is proven?
6. What pattern decisions should the first real subsystem port follow?

## Changes

1. Inspect representative upstream Ghostty code.
   - Use `vendor/ghostty/` as the source of truth.
   - Inspect at least:
     - `vendor/ghostty/src/main_c.zig`
     - `vendor/ghostty/include/ghostty.h`
     - `vendor/ghostty/src/config/Config.zig`
     - `vendor/ghostty/src/Command.zig`
     - `vendor/ghostty/src/pty.zig`
     - `vendor/ghostty/src/termio/Exec.zig`
     - `vendor/ghostty/src/terminal/Tabstops.zig`
     - `vendor/ghostty/src/terminal/Screen.zig`
     - `vendor/ghostty/src/terminal/PageList.zig`
     - `vendor/ghostty/src/terminal/page.zig`
     - `vendor/ghostty/src/terminal/ref_counted_set.zig`
     - `vendor/ghostty/src/datastruct/split_tree.zig`
     - `vendor/ghostty/src/datastruct/intrusive_linked_list.zig`
     - `vendor/ghostty/src/datastruct/segmented_pool.zig`
     - `vendor/ghostty/src/App.zig`
     - `vendor/ghostty/src/Surface.zig`
     - `vendor/ghostty/src/renderer/Thread.zig`
     - `vendor/ghostty/src/termio/mailbox.zig`
     - `vendor/ghostty/src/font/backend.zig`
     - `vendor/ghostty/src/renderer/Metal.zig`
     - `vendor/ghostty/src/apprt/surface.zig`
   - Do not modify `vendor/ghostty/`.

2. Inspect current Roastty code.
   - Inspect at least:
     - `roastty/src/lib.rs`
     - `roastty/include/roastty.h`
     - `roastty/ABI_INVENTORY.md`
     - `roastty/tests/`
   - Record how current ABI/lifecycle patterns should evolve or remain stable.

3. Produce a Zig-to-Rust translation table.
   - Include at least these rows:
     - `comptime` build/config switches
     - `switch (builtin.os.tag)` platform gates
     - Zig tagged unions
     - Zig error unions and error sets
     - optional pointers and nullable values
     - allocators and arenas
     - `defer`, `errdefer`, transactional rollback, and allocation failure
     - `ArrayList`, slices, sentinel slices, and null-terminated strings
     - packed structs and bitfields
     - integer widths, casts, overflow behavior, bitsets, and packed storage
     - `extern struct` / C ABI layout
     - opaque C handles
     - callbacks/userdata
     - manual `deinit` patterns
     - pointer-heavy page/grid structures
     - intrusive/reference-counted state
     - threads, mailboxes, mutexes, atomics, and event delivery
     - tests embedded beside implementation
     - `@compileError` and unreachable platform paths

4. Define the unsafe Rust policy for Issue 801.
   - `unsafe` is allowed for the initial faithful port when it is the clearest
     way to preserve behavior, layout, or ABI semantics.
   - `unsafe` must be localized to small modules/functions.
   - Each `unsafe` block must have a short safety comment explaining the
     invariant.
   - ABI-facing and packed-memory ports must explicitly choose `repr(C)`,
     `repr(transparent)`, or ordinary Rust layout and explain why.
   - Layout-sensitive ports must include `size_of` / `align_of` assertions where
     layout parity matters.
   - Pointer-heavy ports must document ownership, lifetime, aliasing, and
     pointer provenance at the unsafe boundary.
   - Safe public APIs should not expose unsafe requirements unless an experiment
     explicitly justifies that shape.
   - Tests must cover the behavior or layout invariant that justifies the unsafe
     code.
   - Do not use `unsafe` to bypass ownership thinking when safe Rust is equally
     direct.
   - Do not start a broad unsafe cleanup effort inside Issue 801. Cleanup can be
     a later sweep once behavior parity exists.

5. Define behavior-parity rules.
   - Preserve upstream behavior unless an experiment explicitly records a
     Roastty-specific divergence.
   - Prefer local Rust idioms only when they do not alter observable behavior.
   - Keep macOS-only simplifications direct: remove non-macOS branches rather
     than preserving cross-platform abstraction layers by habit.
   - If exact behavior is uncertain, port the upstream test or create an
     equivalent test before changing the implementation shape.

6. Define test translation rules.
   - Record how to translate Zig `test` blocks into Rust unit or integration
     tests.
   - Record when upstream C examples should become Rust/C ABI integration tests.
   - Record when Swift/UI tests should be deferred to the app integration phase.
   - Require each future subsystem experiment to name the upstream tests it is
     porting or intentionally deferring.

7. Verify the diagnostic-only boundary.
   - Before recording the result, run:

     ```bash
     git status --short
     ```

   - Expected changed files are limited to Issue 801 documentation and
     gitignored review logs under `logs/`.
   - This experiment must not modify `roastty/`, `vendor/ghostty/`,
     `Cargo.toml`, `Cargo.lock`, scripts, build configuration, or source code.

8. Record the result inside this experiment file.
   - Append `## Result` and `## Conclusion` to this file.
   - Include these tables:
     - `Representative Source Patterns`
     - `Zig-to-Rust Translation Rules`
     - `Unsafe Rust Policy`
     - `Test Translation Rules`
     - `Patterns for the First Real Port`
     - `Open Pattern Questions`
   - Update the Issue 801 README experiment index status from `Designed` to
     `Pass`, `Partial`, or `Fail` after the result is recorded.

## Verification

The experiment passes if:

- the result cites concrete upstream Ghostty files for each major pattern;
- every required pattern category has a Rust translation rule;
- the unsafe Rust policy is explicit and actionable;
- behavior-parity and macOS-only simplification rules are explicit;
- test translation rules are explicit;
- the result recommends a concrete next implementation slice;
- `git status --short` confirms the diagnostic-only boundary was preserved;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- most patterns are classified, but one or two major patterns need a follow-up
  before the first real port can safely begin.

The experiment fails if:

- it starts porting production code instead of defining translation patterns;
- it leaves `unsafe` policy ambiguous;
- it fails to provide a concrete next implementation slice;
- it preserves non-macOS branches as live Roastty requirements without
  justification.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or running the audit.
