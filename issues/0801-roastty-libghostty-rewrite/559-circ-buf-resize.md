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

# Experiment 559: CircBuf auto-grow (resize + ensureUnusedCapacity)

## Description

This experiment ports the last remaining `CircBuf` operations from upstream
`datastruct/circ_buf.zig` — `resize`, `ensureUnusedCapacity`, and the private
`rotateToZero` — **completing the `CircBuf` port** (Experiments 556–558 did the
core ring, iterator, and two-span access). `resize` changes the buffer's
capacity (rotating the data to be zero-aligned first, so the new space is
contiguous at the end); `ensureUnusedCapacity` grows it when an append wouldn't
fit. With these, `CircBuf` is fully ported and the terminal search subsystem
(which builds on it) becomes portable.

## Upstream behavior

`datastruct/circ_buf.zig`:

```zig
pub fn ensureUnusedCapacity(self: *Self, alloc, amount: usize) !void {
    const new_cap = self.len() + amount;
    if (new_cap <= self.capacity()) return;
    try self.resize(alloc, new_cap);
}

pub fn resize(self: *Self, alloc, size: usize) !void {
    try self.rotateToZero();                                   // align data to index 0
    const prev_len = self.len();
    const prev_cap = self.storage.len;
    self.storage = try alloc.realloc(self.storage, size);
    if (size > prev_cap) {                                     // grew
        @memset(self.storage[prev_cap..], default);
        if (self.full) { self.head = prev_len; self.full = false; }
    }
}

fn rotateToZero(self: *Self) !void {
    if (self.tail == 0) return;
    std.mem.rotate(T, self.storage, self.tail);               // move storage[tail] to index 0
    self.head = self.len() % self.storage.len;
    self.tail = 0;
}
```

- `rotateToZero`: if `tail != 0`, rotate `storage` left by `tail` (so the oldest
  element lands at index 0), then `head = len % capacity`, `tail = 0`. The
  element count is unchanged.
- `resize(size)`: rotate to zero, then reallocate to `size`. If it grew, set the
  new tail region to `default`; and if the buffer was `full`, move `head` to the
  old length and clear `full` (the data now occupies `[0, prev_len)` with free
  space after).
- `ensureUnusedCapacity(amount)`: if `len + amount` exceeds capacity, `resize`
  to `len + amount`.

The upstream tests exercise growing a full buffer (data preserved, new capacity
appendable) and `ensureUnusedCapacity`.

## Rust mapping (`roastty/src/terminal/circ_buf.rs`)

`Vec::resize(size, default)` subsumes upstream's `realloc` + `@memset` (it grows
filling with `default`, or truncates); `slice::rotate_left(tail)` is
`std.mem.rotate(.., tail)`. Rust `Vec` allocation failure aborts rather than
returning an error, so these return `()` (not a `Result`):

```rust
/// Ensure there is room to append `amount` more items, growing if needed (upstream
/// `ensureUnusedCapacity`).
pub(crate) fn ensure_unused_capacity(&mut self, amount: usize) {
    let new_cap = self.len() + amount;
    if new_cap <= self.capacity() {
        return;
    }
    self.resize(new_cap);
}

/// Resize the buffer to `size` (larger or smaller). New slots (when growing) are `default`
/// (upstream `resize`).
pub(crate) fn resize(&mut self, size: usize) {
    // Rotate the data to be zero-aligned so the reallocation's new space is contiguous.
    self.rotate_to_zero();

    let prev_len = self.len();
    let prev_cap = self.storage.len();
    // `Vec::resize` both grows (filling new slots with `default`) and shrinks (truncating) —
    // the equivalent of `realloc` + the grow-time `@memset` to `default`.
    self.storage.resize(size, self.default);

    if size > prev_cap && self.full {
        // We grew a full buffer: the data now occupies `[0, prev_len)`, with free space after.
        self.head = prev_len;
        self.full = false;
    }
}

/// Rotate the data so the oldest element is at index 0 (upstream `rotateToZero`).
fn rotate_to_zero(&mut self) {
    if self.tail == 0 {
        return;
    }
    self.storage.rotate_left(self.tail);
    self.head = self.len() % self.storage.len();
    self.tail = 0;
}
```

`rotate_to_zero`'s `self.len()` reads the (unchanged-by-rotate) old
`head`/`tail`/`full`, so it is the old length before `head` is reassigned —
matching upstream's `head = len() % capacity` ordering. `resize` captures
`prev_len` / `prev_cap` before the `Vec::resize`, then applies the same
full-buffer `head`/`full` fixup when it grew. `Vec::resize(size, default)` fills
new slots with a clone of `default` (`T: Copy`), the equivalent of the
`@memset`.

## Scope / faithfulness notes

- **Ported (bridged)**: `resize`, `ensureUnusedCapacity`, and the private
  `rotateToZero` → `resize`, `ensure_unused_capacity`, `rotate_to_zero`. **This
  completes the `CircBuf` port.**
- **Faithful**: `rotateToZero`'s `tail != 0` guard, the left-rotate by `tail`,
  and the `head = len % cap`, `tail = 0` fixup; `resize`'s rotate-then-grow, the
  new-slot `default` fill, and the full-buffer `head = prev_len` /
  `full = false` fixup on growth; `ensureUnusedCapacity`'s
  `len + amount > capacity` ⇒ resize.
- **Faithful adaptation**: `alloc.realloc` + grow-time `@memset(default)` →
  `Vec::resize(size, default)` (which both grows-with-default and truncates);
  `std.mem.rotate(.., tail)` → `slice::rotate_left(tail)`; the `Allocator.Error`
  return → `()` (Rust `Vec` allocation failure aborts, not returns).
- **Deferred**: nothing — `CircBuf` is fully ported (the search subsystem that
  consumes it is the next, separate target).
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::circ_buf`.

## Changes

1. `roastty/src/terminal/circ_buf.rs`: add `resize`, `ensure_unused_capacity`,
   `rotate_to_zero`.
2. Tests (in `circ_buf.rs`):
   - **grow a full buffer**: fill `new(3, 0)` to `[1, 2, 3]` (full), `resize(5)`
     ⇒ `capacity` 5, `len` 3, the data preserved (`iterator(Forward)` ⇒
     `1, 2, 3`), and two more appends fit (`iterator` ⇒ `1, 2, 3, 4, 5`).
   - **grow a wrapped buffer**: from a wrapped/non-full buffer (e.g.
     `new(4, 0)`, append `1, 2, 3`, `delete_oldest(1)` leaving `2, 3` with
     `tail != 0`), `resize(6)` rotates to zero and preserves the data
     (`iterator(Forward)` ⇒ `2, 3`), with room to append.
   - **ensure_unused_capacity**: on a full `[1, 2, 3]`,
     `ensure_unused_capacity(2)` grows (`capacity >= 5`, data preserved, two
     appends fit); when there is already room, `ensure_unused_capacity` is a
     no-op (capacity unchanged).
   - **grow from zero capacity** (Codex design review): `new(0, 0)` (which is
     `full`), `resize(3)` ⇒ `capacity` 3, `len` 0 (empty), and three appends fit
     (`iterator` ⇒ `1, 2, 3`).
   - **shrink a full buffer** (Codex design review): from a full `[1, 2, 3, 4]`,
     `resize(3)` keeps `full == true`, `len() == 3`, `capacity() == 3`, and
     preserves `[1, 2, 3]` (the upstream "smaller" path).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty circ_buf
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/circ_buf.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `resize` rotates the data to zero, grows/shrinks the storage (filling new
  slots with `default`), and applies the full-buffer `head`/`full` fixup on
  growth; `ensure_unused_capacity` grows only when `len + amount` exceeds
  capacity — faithful to `datastruct/circ_buf.zig`, completing the `CircBuf`
  port;
- the tests pass (grow-full / grow-wrapped / ensure_unused_capacity), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the rotate-to-zero alignment, the grow fixup, or the
`ensure_unused_capacity` threshold diverges from upstream, an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it (no
Required findings), with **one Optional** suggestion, adopted:

- **(Optional, adopted)**: add the two upstream resize-edge tests — grow from
  zero capacity and shrink a full buffer — since `resize` claims "larger or
  smaller" and completes `CircBuf`. The shrink test (per Codex) verifies
  `resize(3)` from a full `[1, 2, 3, 4]` keeps `full == true`, `len() == 3`,
  `capacity() == 3`, and preserves `[1, 2, 3]`.

Codex confirmed the core design is faithful: `Vec::resize(size, self.default)`
matches `realloc` plus the grow-time default fill for this `T: Copy` buffer;
`rotate_left(tail)` is the correct direction for `std.mem.rotate(.., tail)`;
`rotate_to_zero` computes the length before changing `head` / `tail`; the
full-buffer grow fixup is correct; `ensure_unused_capacity` matches the upstream
threshold; and the allocator-error-to-abort adaptation is acceptable for Rust
`Vec`.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d559-prompt.md` (design)
- Result: `logs/codex-review/20260604-d559-last-message.md` (design)
