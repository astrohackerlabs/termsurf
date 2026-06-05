+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 620: os args iterator (NSProcessInfo-backed command-line args)

## Description

Port `os/args.zig`'s macOS command-line argument iterator into
`roastty/src/os/args.rs`. Upstream avoids libc `argc`/`argv` for
`NSApplicationMain` launches and reads `NSProcessInfo.processInfo.arguments`
instead; Roastty should do the same through the existing `objc2-foundation`
dependency.

This is intentionally a narrow OS slice: expose the current process arguments as
owned UTF-8 `String`s and an iterator API suitable for config/app startup code.
It does not port the broader CLI parser.

## Upstream behavior (`os/args.zig`)

```zig
pub fn iterator(allocator: Allocator) ArgIterator.InitError!ArgIterator {
    return .initWithAllocator(allocator);
}

const IteratorMacOS = struct {
    alloc: Allocator,
    index: usize,
    count: usize,
    buf: [:0]u8,
    args: objc.Object,

    pub fn initWithAllocator(alloc: Allocator) InitError!IteratorMacOS {
        const NSProcessInfo = objc.getClass("NSProcessInfo").?;
        const info = NSProcessInfo.msgSend(objc.Object, objc.sel("processInfo"), .{});
        const args = info.getProperty(objc.Object, "arguments");
        // precompute max UTF-8 byte length and allocate a reusable NUL buffer
        return .{ .alloc = alloc, .index = 0, .count = count, .buf = buf, .args = args };
    }

    pub fn next(self: *IteratorMacOS) ?[:0]const u8 {
        if (self.index == self.count) return null;
        const nsstr = self.args.msgSend(objc.Object, objc.sel("objectAtIndex:"), .{self.index});
        self.index += 1;
        if (!nsstr.msgSend(bool, objc.sel("getCString:maxLength:encoding:"), .{...})) return "";
        return std.mem.sliceTo(self.buf, 0);
    }

    pub fn skip(self: *IteratorMacOS) bool {
        if (self.index == self.count) return false;
        self.index += 1;
        return true;
    }
};
```

## Rust mapping (`roastty/src/os/args.rs`)

```rust
/// Snapshot of process command-line arguments, matching upstream `os.args.iterator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Args {
    values: Vec<String>,
    index: usize,
}

impl Args {
    pub(crate) fn new(values: Vec<String>) -> Self { ... }
    pub(crate) fn from_process_info() -> Self { ... }
    pub(crate) fn next(&mut self) -> Option<&str> { ... }
    pub(crate) fn skip(&mut self) -> bool { ... }
    pub(crate) fn len(&self) -> usize { ... }
    pub(crate) fn remaining(&self) -> usize { ... }
    pub(crate) fn is_empty(&self) -> bool { ... }
}

pub(crate) fn iterator() -> Args {
    Args::from_process_info()
}

#[cfg(target_os = "macos")]
fn process_info_arguments() -> Vec<String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::NSProcessInfo;

    let info = NSProcessInfo::processInfo();
    let args = info.arguments();
    autoreleasepool(|pool| {
        (0..args.len())
            .map(|i| {
                let s = args.objectAtIndex(i);
                unsafe { s.to_str(pool) }.to_owned()
            })
            .collect()
    })
}

#[cfg(not(target_os = "macos"))]
fn process_info_arguments() -> Vec<String> {
    std::env::args().collect()
}
```

### Notes / deviations

- Roastty is macOS-only in product scope. The non-macOS `std::env::args()`
  fallback is test/build scaffolding only, so host-side `cargo test` can run
  without making a cross-platform abstraction part of the product design.
- Upstream returns borrowed slices into a reusable NUL-terminated buffer. Rust
  snapshots to owned `String`s because later startup/config code can hold the
  iterator without Objective-C lifetime coupling. This preserves the observable
  sequence, `next`, and `skip` behavior.
- `objc2-foundation` already provides typed `NSProcessInfo::processInfo()` and
  `arguments()`, gated by features. This experiment adds the minimal needed
  features to `roastty/Cargo.toml`: `NSArray` and `NSProcessInfo` alongside the
  existing `NSString`.
- The macOS bridge uses `args.len()` + `args.objectAtIndex(i)`, matching
  upstream's indexed `objectAtIndex:` loop and avoiding an `NSEnumerator`
  feature dependency.
- `NSString::to_str(pool)` may borrow an autoreleased UTF-8 buffer; each value
  is copied into a `String` inside the autorelease pool before it can escape.
  Upstream returns `""` if `getCString:maxLength:encoding:` fails; this is not
  expected for `NSProcessInfo.arguments` Foundation strings, so this slice uses
  the typed UTF-8 bridge rather than a lower-level `getCString` fallback.

## Changes

- `roastty/Cargo.toml` — enable `objc2-foundation` features `NSArray` and
  `NSProcessInfo`.
- `roastty/src/os/args.rs` — add the `Args` snapshot iterator, `iterator()`, and
  macOS `NSProcessInfo.arguments` bridge.
- `roastty/src/os/mod.rs` — expose the new `args` module.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — new tests cover:
  - `Args::new(vec!["app", "--flag", "value"])` yields the same sequence as
    upstream `next`;
  - `skip` advances by one and returns `false` at the end;
  - `remaining`, `len`, and `is_empty` reflect the snapshot and cursor;
  - `iterator()` smoke test returns at least one argument on the host process.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = Roastty has an `os::args` iterator backed by `NSProcessInfo.arguments` on
macOS with upstream-equivalent `next` and `skip` cursor behavior, ready for
later config/app startup slices.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found one Required issue: the first draft used `NSArray::iter()`
without enabling the `NSEnumerator` feature. The design was changed to use
`args.len()` plus `args.objectAtIndex(i)`, matching upstream's indexed
`objectAtIndex:` loop and avoiding the extra feature. The review also asked that
the rare upstream `getCString` failure behavior and non-macOS fallback be
documented; both notes were added.

Follow-up review approved the design with no Required findings. Codex confirmed
the Objective-C lifetime plan is sound: `NSProcessInfo::arguments()` is retained
by the typed binding, each `NSString` is converted inside an autorelease pool,
and the UTF-8 borrow is copied into an owned `String` before it can escape.
