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

# Experiment 553: Mach VM memory tags (os::mach)

## Description

Continuing the `os` module (Experiments 541–552), this experiment opens
`os::mach` with the **Mach VM memory tags** from upstream `os/mach.zig`: the
`VMTag` enum (the macOS application-specific virtual-memory tags) and its
`make()` — the `VM_MAKE_TAG` value passed to `mmap` / `mach_vm_*` so a memory
region is labeled in tools like `vmmap` and Instruments. roastty will use this
to tag its large allocations (e.g. GPU / glyph atlases) for memory profiling.
The `taggedPageAllocator` built on top of it is Zig-allocator machinery (a port
of Zig's `PageAllocator`) and does not map onto Rust's allocator model, so it is
deferred (see Deferred).

## Upstream behavior

`os/mach.zig`:

```zig
/// macOS virtual memory tags for use with mach_vm_map/mach_vm_allocate. These identify
/// memory regions in tools like vmmap and Instruments.
pub const VMTag = enum(u8) {
    application_specific_1 = 240,
    application_specific_2 = 241,
    // … _3 … _15 …
    application_specific_16 = 255,

    // We ignore the rest because we never realistically set them.
    _,

    /// Converts the tag to the format expected by mach_vm_map/mach_vm_allocate.
    /// Equivalent to C macro: VM_MAKE_TAG(tag)
    pub fn make(self: VMTag) i32 {
        return @bitCast(@as(u32, @intFromEnum(self)) << 24);
    }
};
```

- `VMTag` names the 16 application-specific VM tags `240` … `255`. It is
  **non-exhaustive** (`_`) so the allocator can round-trip an arbitrary `u8` tag
  through `@enumFromInt`, but upstream only ever sets the named
  application-specific values.
- `make()` is the `VM_MAKE_TAG(tag)` macro: the tag byte shifted left 24 bits,
  reinterpreted as the signed `i32` the `mmap` / `mach_vm_*` tag argument
  expects (e.g. tag `240` ⇒ `0xF0000000` as `i32` = `-268435456`).

The upstream test:
`VMTag.application_specific_1.make() == @bitCast(@as(u32, 240) << 24)`.

## Rust mapping (`roastty/src/os/mach.rs`)

A `#[repr(u8)]` enum mirroring the named tags, with `make()` computing the
shifted, bit-cast `i32`:

```rust
//! Mach VM helpers (port of upstream `os/mach`).

/// macOS virtual-memory tags for use with `mmap` / `mach_vm_*` (upstream `os.mach.VMTag`).
/// These identify memory regions in tools like `vmmap` and Instruments. Only the
/// application-specific tags (`240`–`255`) are named — the only ones realistically set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum VMTag {
    ApplicationSpecific1 = 240,
    ApplicationSpecific2 = 241,
    ApplicationSpecific3 = 242,
    ApplicationSpecific4 = 243,
    ApplicationSpecific5 = 244,
    ApplicationSpecific6 = 245,
    ApplicationSpecific7 = 246,
    ApplicationSpecific8 = 247,
    ApplicationSpecific9 = 248,
    ApplicationSpecific10 = 249,
    ApplicationSpecific11 = 250,
    ApplicationSpecific12 = 251,
    ApplicationSpecific13 = 252,
    ApplicationSpecific14 = 253,
    ApplicationSpecific15 = 254,
    ApplicationSpecific16 = 255,
}

impl VMTag {
    /// The tag in the format `mmap` / `mach_vm_*` expects — the tag byte shifted left 24 bits,
    /// reinterpreted as a signed `i32` (the C macro `VM_MAKE_TAG(tag)`; upstream `make`).
    pub(crate) fn make(self) -> i32 {
        ((self as u32) << 24) as i32
    }
}
```

`self as u32` reads the `#[repr(u8)]` discriminant (the tag byte); `<< 24` then
`as i32` reinterprets the bits — the faithful equivalent of
`@bitCast(@as(u32, @intFromEnum(self)) << 24)` (Rust's `as i32` is a
bit-preserving reinterpret for an out-of-range `u32`, so
`240 << 24 = 0xF0000000` ⇒ `-268435456`).

## Scope / faithfulness notes

- **Ported (bridged)**: `os.mach.VMTag` (the named application-specific tags)
  and `VMTag.make` → `os::mach::VMTag` / `VMTag::make`.
- **Faithful**: the 16 application-specific tag values (`240` … `255`); `make` =
  the tag byte `<< 24` bit-cast to `i32` (the `VM_MAKE_TAG` macro).
- **Faithful adaptation**: `enum(u8)` (non-exhaustive) → `#[repr(u8)]` enum of
  the named tags (the `_` round-trip is only used by the deferred allocator —
  see Deferred); `@bitCast(@as(u32, …) << 24)` → `((self as u32) << 24) as i32`.
- **Deferred**: `taggedPageAllocator` and the `TaggedPageAllocator` (a port of
  Zig's `PageAllocator` that `mmap`s tagged memory through a Zig `Allocator`
  vtable, smuggling the tag through the context pointer via the non-exhaustive
  `@enumFromInt` round-trip) — this is Zig-allocator machinery that does not map
  onto Rust's allocator model, so the tag value (`make`) is ported now and the
  allocator integration left for when roastty wires tagged `mmap`s.
- No C ABI/header/ABI-inventory change (internal Rust). New `os::mach` module.

## Changes

1. `roastty/src/os/mach.rs` (new): `VMTag` (+ `make`).
2. `roastty/src/os/mod.rs`: add `pub(crate) mod mach;`.
3. Tests (in `mach.rs`):
   - **discriminants**: each named tag's `as u8` equals its value
     (`ApplicationSpecific1` `240` … `ApplicationSpecific16` `255`).
   - **make = VM_MAKE_TAG**: for every named tag, `make()` equals
     `((value as u32) << 24) as i32`; spot-check
     `ApplicationSpecific1.make() == -268435456` (`0xF0000000` as `i32`) and
     `ApplicationSpecific16.make() == ((255u32 << 24) as i32)`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty os::mach
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/os/mach.rs roastty/src/os/mod.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `VMTag` has the 16 application-specific values (`240`–`255`) and `make()`
  returns the tag byte shifted left 24 bits, bit-cast to `i32` — faithful to
  `os/mach.zig`'s `VMTag` / `VM_MAKE_TAG`;
- the tests pass (discriminants + `make`), and the existing tests still pass;
- the tagged page allocator stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a tag value or the `make` computation diverges from
upstream, an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. Codex confirmed `((self as u32) << 24) as i32` is the faithful Rust
equivalent of upstream's `@bitCast(@as(u32, @intFromEnum(self)) << 24)` (the tag
byte is shifted into the high byte and the final cast preserves the
two's-complement bit pattern as `i32`); porting only the named `240..=255`
application-specific tags is appropriate for this slice (the non-exhaustive `_`
only matters for the deferred allocator's arbitrary `@enumFromInt` round-trip);
and deferring `TaggedPageAllocator` is reasonable since the reusable building
block is the Mach tag value, not Zig's allocator vtable machinery.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d553-prompt.md` (design)
- Result: `logs/codex-review/20260604-d553-last-message.md` (design)

## Result

**Result:** Pass

`os::mach` was opened with `VMTag` (the 16 application-specific tags
`240`–`255`) and `make` (`((self as u32) << 24) as i32` — the `VM_MAKE_TAG`
macro). The module is registered in `os/mod.rs` (`cargo fmt` ordered `mach`
before `macos`, accepted). Two tests, both driven by an `ALL` table of the 16
(tag, value) pairs: the discriminants (`tag as u8 == value`), and `make`
(`tag.make() == ((value as u32) << 24) as i32` for every tag, plus the boundary
spot-checks `ApplicationSpecific1.make() == -268435456` and
`ApplicationSpecific16.make() == ((255u32 << 24) as i32)`).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3073 passed, 0 failed (two new tests; no regressions,
  up from 3071).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + os/mach.rs + os/mod.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **one Nit** (no
Required or Optional findings): the doc had `## Result` but no `## Conclusion` —
fixed by adding the conclusion below. Codex confirmed the implementation matches
upstream: all 16 tag values are exact, and `make()` correctly implements
`VM_MAKE_TAG` by shifting the tag byte into the high byte and preserving the
resulting bit pattern as `i32`; the table-driven tests are sound.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r553-prompt.md` (result)
- Result: `logs/codex-review/20260604-r553-last-message.md` (result)

## Conclusion

`os::mach::VMTag` + `make` — the macOS application-specific VM memory tags and
the `VM_MAKE_TAG` value — are faithfully ported from `os/mach.zig`, opening the
`os::mach` module. This is the reusable building block roastty will pass to a
tagged `mmap` so its large allocations show up labeled in `vmmap` / Instruments
(the `TaggedPageAllocator` that wires it into a Zig `Allocator` is deferred —
that machinery doesn't map onto Rust's allocator model). The OS-utility frontier
still has self-contained slices (`locale`, `i18n_locales`). The objc/bundle-id
helpers, the `home()` resolver, and config `loadDefaultFiles` remain deferred
pending roastty's naming decision; `background-image-opacity` stays
float-blocked.
