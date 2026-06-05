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

# Experiment 557: the CircBuf iterator

## Description

Continuing the `CircBuf` port (Experiment 556 did the core ring), this
experiment ports its **iterator** — a forward/reverse traversal over the ring's
logical elements, with `seek_by` and `reset`. It's how callers (and eventually
the search subsystem) walk the buffer oldest→newest or newest→oldest. The
auto-growing (`resize`) and the two-span mutable `getPtrSlice` accessor remain
deferred.

## Upstream behavior

`datastruct/circ_buf.zig` — `CircBuf.Iterator`:

```zig
pub const Iterator = struct {
    buf: Self,
    idx: usize,
    direction: Direction,
    pub const Direction = enum { forward, reverse };

    pub fn next(self: *Iterator) ?*T {
        if (self.idx >= self.buf.len()) return null;
        const tail_idx = switch (self.direction) {
            .forward => self.idx,
            .reverse => self.buf.len() - self.idx - 1,
        };
        const storage_idx = (self.buf.tail + tail_idx) % self.buf.capacity();
        self.idx += 1;
        return &self.buf.storage[storage_idx];
    }

    pub fn seekBy(self: *Iterator, amount: isize) void {
        if (amount > 0) self.idx +|= @intCast(amount)            // saturating
        else self.idx -|= @intCast(@abs(amount));                // saturating
    }

    pub fn reset(self: *Iterator) void { self.idx = 0; }
};

pub fn iterator(self: Self, direction: Iterator.Direction) Iterator {
    return .{ .buf = self, .idx = 0, .direction = direction };
}
```

- `iterator(direction)` starts at logical index `0`. `next()` returns the
  element at the current logical index — `forward` counts from the oldest
  (`tail`), `reverse` from the newest (`len - idx - 1`) — translated to the
  storage index `(tail + tail_idx) % capacity`, advancing `idx`; it returns
  `null` once `idx >= len()`.
- `seekBy(amount)` moves the logical index by a (signed) amount, **saturating**
  (no under/overflow). `reset()` returns to index `0`.
- Upstream's iterator holds a **copy** of the (slice-backed) buffer — a
  read-only view that doesn't affect the original.

## Rust mapping (`roastty/src/terminal/circ_buf.rs`)

A borrowing `Iter<'a, T>` (the read-only-view equivalent of upstream's by-value
copy, since a Rust `Vec` copy would deep-clone). `next` / `seek_by` / `reset`
are inherent methods matching upstream (not the `std::iter::Iterator` trait, to
keep the explicit `seek_by` / `reset` shape):

```rust
/// Traversal direction for a `CircBuf` iterator (upstream `Iterator.Direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Forward,
    Reverse,
}

/// A read-only forward/reverse iterator over a `CircBuf` (upstream `CircBuf.Iterator`).
pub(crate) struct Iter<'a, T: Copy> {
    buf: &'a CircBuf<T>,
    idx: usize,
    direction: Direction,
}

impl<T: Copy> CircBuf<T> {
    /// Iterate over the logical elements, oldest-first (`Forward`) or newest-first
    /// (`Reverse`) (upstream `iterator`).
    pub(crate) fn iterator(&self, direction: Direction) -> Iter<'_, T> {
        Iter {
            buf: self,
            idx: 0,
            direction,
        }
    }
}

impl<'a, T: Copy> Iter<'a, T> {
    /// The next element, or `None` once the iterator is past the end (upstream `next`).
    pub(crate) fn next(&mut self) -> Option<&'a T> {
        if self.idx >= self.buf.len() {
            return None;
        }
        let tail_idx = match self.direction {
            Direction::Forward => self.idx,
            Direction::Reverse => self.buf.len() - self.idx - 1,
        };
        let storage_idx = (self.buf.tail + tail_idx) % self.buf.capacity();
        self.idx += 1;
        Some(&self.buf.storage[storage_idx])
    }

    /// Move the logical index by a signed amount, saturating at the bounds (upstream
    /// `seekBy`).
    pub(crate) fn seek_by(&mut self, amount: isize) {
        if amount > 0 {
            self.idx = self.idx.saturating_add(amount as usize);
        } else {
            self.idx = self.idx.saturating_sub(amount.unsigned_abs());
        }
    }

    /// Reset back to the first element (upstream `reset`).
    pub(crate) fn reset(&mut self) {
        self.idx = 0;
    }
}
```

`next` reproduces upstream's logical→storage index translation exactly (forward
from `tail`, reverse from `len - idx - 1`, modulo `capacity`); `seek_by` uses
`saturating_add` / `saturating_sub` (the `+|` / `-|` saturating ops); `reset`
zeroes the index. `Iter` borrows the buffer (`&'a CircBuf`) — the read-only view
that mirrors upstream's by-value slice copy without deep-cloning the `Vec`.

## Scope / faithfulness notes

- **Ported (bridged)**: `CircBuf.Iterator` (+ `Direction`) and `iterator()` →
  `terminal::circ_buf::Iter` / `Direction` and `CircBuf::iterator`.
- **Faithful**: forward (oldest-first) / reverse (newest-first) traversal; the
  `(tail + tail_idx) % capacity` storage translation; the `idx >= len` stop;
  `seek_by`'s saturating move; `reset`.
- **Faithful adaptation**: upstream's by-value `buf: Self` (a slice copy =
  read-only view) → a borrowing `Iter<'a, T>` (a Rust `Vec` copy would
  deep-clone, so a borrow is the faithful read-only view); the `+|` / `-|`
  saturating ops → `saturating_add` / `saturating_sub`; inherent `next` /
  `seek_by` / `reset` (not `std::iter::Iterator`, to keep the explicit upstream
  shape).
- **Deferred**: auto-growing (`resize` / `ensureUnusedCapacity`); the two-span
  mutable `getPtrSlice` (and `appendSliceAssumeCapacity`) — the next CircBuf
  slices.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::circ_buf`.

## Changes

1. `roastty/src/terminal/circ_buf.rs`: add `Direction`, `Iter`, and
   `CircBuf::iterator`.
2. Tests (in `circ_buf.rs`):
   - **forward / reverse**: over `[1, 2, 3]`, `iterator(Forward)` yields
     `1, 2, 3, None` (oldest-first); `iterator(Reverse)` yields `3, 2, 1, None`
     (newest-first).
   - **wrapped**: from `[1, 2, 3]`, `delete_oldest(1)` then `append(4)` (head
     wrapped), `iterator(Forward)` yields `2, 3, 4` and `iterator(Reverse)`
     yields `4, 3, 2` — verifying the storage translation across the wrap.
   - **seek_by / reset**: a forward iterator after `seek_by(1)` skips the first
     element; `seek_by(-100)` saturates to index `0` (so `next` is the first
     element again); `reset` restarts.
   - **empty**: `iterator(Forward).next()` on an empty buffer is `None`.
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

- `CircBuf::iterator` walks the ring forward (oldest-first) and reverse
  (newest-first) with the correct storage translation (incl. across the wrap),
  stops at `len`, and supports saturating `seek_by` / `reset` — faithful to
  `datastruct/circ_buf.zig`'s `Iterator`;
- the tests pass (forward/reverse + wrapped + seek/reset + empty), and the
  existing tests still pass;
- the resize/grow and `getPtrSlice` pieces stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the traversal order, the storage translation, or the
seek/reset behavior diverges from upstream, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. Codex confirmed the iterator translation is faithful — `next` stops
on `idx >= len`, maps the forward/reverse logical indices exactly like upstream,
and handles wrapped storage through `(tail + tail_idx) % capacity`; `seek_by`
with `saturating_add` / `saturating_sub(amount.unsigned_abs())` matches Zig's
`+|` / `-|` (including `0` and large negative moves); borrowing `&CircBuf` is
the right Rust equivalent of upstream's slice-backed read-only copy; and keeping
inherent `next` / `seek_by` / `reset` preserves the upstream API shape. The
proposed tests cover the traversal, wrap, seek/reset, and empty cases.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d557-prompt.md` (design)
- Result: `logs/codex-review/20260604-d557-last-message.md` (design)

## Result

**Result:** Pass

`terminal::circ_buf` gained `Direction` (`Forward` / `Reverse`), the borrowing
`Iter<'a, T>`, and `CircBuf::iterator`. `Iter::next` translates the logical
index to storage (`Forward` from `tail`, `Reverse` from `len - idx - 1`, modulo
`capacity`) and stops at `len`; `seek_by` moves the index saturatingly; `reset`
zeroes it. Four new tests: forward (oldest-first) / reverse (newest-first) over
`[1, 2, 3]`; iteration across the wrap (after a `delete_oldest` + wrapping
`append`, forward ⇒ `2, 3, 4`, reverse ⇒ `4, 3, 2`); `seek_by` (skip +
saturating negative) and `reset`; and an empty-buffer `next() == None`.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3093 passed, 0 failed (four new tests; no
  regressions, up from 3089).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/circ_buf.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **one Nit** (no
Required or Optional findings): the doc had `## Result` but no `## Conclusion` —
fixed by adding the conclusion below. Codex confirmed the implementation matches
upstream — the forward/reverse index translation is exact, wrapped iteration is
covered, `idx >= len` stops before any underflow/modulo edge, `seek_by` uses the
right saturating behavior, and `reset` restores index `0` — and that the
borrowing iterator is a sound Rust equivalent for this read-only view.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r557-prompt.md` (result)
- Result: `logs/codex-review/20260604-r557-last-message.md` (result)

## Conclusion

`terminal::circ_buf` now has its forward/reverse `Iter` (with `seek_by` /
`reset`), faithfully ported from `CircBuf.Iterator` — the read-only traversal
callers (and the search subsystem) use to walk the ring oldest→newest or
newest→oldest. Combined with Experiment 556's core, the CircBuf can now be
built, appended/wrapped, queried, deleted-from, and iterated. The remaining
CircBuf slices are auto-growing (`resize` / `ensureUnusedCapacity`) and the
two-span mutable `getPtrSlice` (plus `appendSliceAssumeCapacity`) — the last
pieces before the search subsystem itself can be ported. The objc/bundle-id
helpers, the `home()` resolver, and config `loadDefaultFiles` remain deferred
pending roastty's naming decision; `background-image-opacity` stays
float-blocked.
