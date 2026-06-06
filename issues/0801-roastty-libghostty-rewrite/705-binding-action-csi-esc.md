+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 705: Binding Action CSI and ESC

## Description

Experiment 704 added byte-oriented binding-action parsing and raw termio writes
for `text:`. Upstream Ghostty's `performBindingAction` also supports:

- `csi:<bytes>` — send `ESC [` followed by the raw parameter bytes;
- `esc:<bytes>` — send `ESC` followed by the raw parameter bytes.

Unlike `text:`, upstream does not decode Zig string-literal escapes for these
two action parameters. `Binding.Action.parse` stores the parameter as a raw byte
slice and `performBindingAction` prepends the escape prefix before queueing one
write request. Both actions then scroll to bottom, which Roastty is deferring
until terminal scrolling / renderer state integration just like the `text:`
slice.

Upstream formats CSI/ESC writes through a fixed local buffer. Roastty will use
an owned `Vec<u8>` in this slice so the ABI can accept the full caller-provided
parameter length; preserving upstream's effective fixed-buffer failure behavior
is not useful until full binding-action error reporting exists.

Roastty now has the required raw write helper and byte-oriented parser. This
experiment extends the binding-action foundation to support `csi:` and `esc:`.

This does not implement cursor-key actions, terminal reset, clipboard actions,
scrolling actions, keybind storage/lookup, or app-scoped actions.

## Changes

- `roastty/src/lib.rs`
  - Extend the internal parsed binding-action enum with raw write variants for
    `csi` and `esc`.
  - Extend `parse_binding_action` to accept `csi:<bytes>` and `esc:<bytes>`
    while rejecting missing parameters.
  - Dispatch `csi:<bytes>` as one raw write of `ESC [` plus the parameter.
  - Dispatch `esc:<bytes>` as one raw write of `ESC` plus the parameter.
  - Return `true` for attached parsed actions even when the parameter is empty
    or no termio worker exists.
  - Return `false` for null/detached surfaces or missing parameters.
  - Keep split, close, and `text:` semantics unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage that `csi` / `esc` without parameters are rejected
    and `csi:` / `esc:` can be invoked through the public ABI.

- Tests in `roastty/src/lib.rs`
  - Cover `csi` and `esc` without parameters returning false.
  - Cover `csi:` and `esc:` returning true with no worker.
  - Cover null/detached surfaces returning false.
  - Cover child PTY delivery for `csi:A` as bytes `1b 5b 41`.
  - Cover child PTY delivery for `esc:d` as bytes `1b 64`.
  - Cover raw parameter bytes are not decoded, for example `csi:\x15` action
    bytes delivering `1b 5b 5c 78 31 35`.
  - Cover empty parameters writing only the prefix bytes.
  - Re-run existing binding-action tests to prove split, close, and `text:`
    semantics did not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty binding_action -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the initial Experiment 705 design and approved the technical
scope after two plan fixes. The review confirmed that `csi:` and `esc:` should
use raw `[]const u8` parameters, should not decode Zig string-literal escapes,
should reject missing parameters while accepting empty parameters after the
colon, and should return `true` for attached no-worker surfaces.

The review required adding an explicit no-decode PTY test so an implementation
that accidentally reused the `text:` parser would fail. It also noted upstream's
fixed local buffer for formatting CSI/ESC writes; this design records that
Roastty will use an owned `Vec<u8>` and not preserve that fixed-buffer limit in
this slice. The review also required updating the README provenance tuple before
the plan commit.

## Result

**Result:** Pass

Implemented `csi:` and `esc:` support in the binding-action ABI:

- Extended parsed binding actions with `Csi` and `Esc`.
- Parsed `csi:<bytes>` and `esc:<bytes>` as raw byte parameters without
  string-literal decoding.
- Rejected missing `csi` / `esc` parameters while accepting empty parameters
  after the colon.
- Dispatched `csi:<bytes>` as `ESC [` plus the parameter bytes through the raw
  termio write helper.
- Dispatched `esc:<bytes>` as `ESC` plus the parameter bytes through the raw
  termio write helper.
- Kept null and detached surfaces returning `false`.
- Added Rust tests for no-worker behavior, null/detached behavior, CSI and ESC
  child-PTY byte delivery, empty-parameter prefix writes, and the no-decode
  `csi:\x15` case.
- Added C ABI harness smoke coverage for missing and empty CSI/ESC parameters.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty binding_action -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Roastty binding-action invocation now supports split actions, `close_surface`,
raw `text:`, raw `csi:`, and raw `esc:` input. Remaining binding-action parity
still requires cursor-key actions, terminal reset and scrolling actions,
clipboard actions, complete keybind storage/lookup, app-scoped actions, and
frontend split/tab/window mutation.

## Completion Review

Codex reviewed the staged completed Experiment 705 result. The review found no
implementation blockers: `csi:` and `esc:` parse as raw byte parameters, reject
missing parameters, accept empty parameters after the colon, dispatch `ESC [` /
`ESC` plus raw bytes, and preserve split, close, and `text:` semantics.

The review accepted the PTY tests, including the no-decode `csi:\x15` case. It
initially blocked the result commit only because result-review provenance had
not yet been recorded. This section, the `[review.result]` frontmatter, and the
README tuple update resolve that workflow finding.
