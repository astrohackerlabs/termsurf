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

# Experiment 562: LRU resize (free-slot reuse + shrink eviction)

## Description

This experiment ports the last remaining `Lru` operation from upstream
`datastruct/lru.zig` — `resize` — **completing the `Lru` port** (Experiment 561
did `init` / `get` / `get_or_put`). `resize` changes the cache's capacity;
shrinking below the current count evicts the least-recently-used entries and
returns their values. Because `resize` is the first operation that **frees** an
arena slot (`get_or_put` only ever recycles), this experiment also adds the
arena's free-slot handling: a free list, with `get_or_put`'s allocation path
reusing a freed slot when one is available.

## Upstream behavior

`datastruct/lru.zig`:

```zig
pub fn resize(self, alloc, capacity) !?[]V {
    if (capacity >= self.capacity) { self.capacity = capacity; return null; }     // grow
    if (self.map.count() <= capacity) { self.capacity = capacity; return null; }  // shrink, no removal
    const delta = self.map.count() - capacity;
    var evicted = try alloc.alloc(V, delta);
    var i = 0;
    while (i < delta) : (i += 1) {
        const node = self.queue.popFirst().?;     // the least-recently-used
        const entry: *Entry = .fromNode(node);
        evicted[i] = entry.data.value;
        self.map.remove(entry.data.key);
        alloc.destroy(entry);                     // free the entry's storage
    }
    self.capacity = capacity;
    return evicted;
}
```

- Growing (or shrinking to ≥ the current count): just update `capacity`, return
  `null`.
- Shrinking below the current count: evict the `delta = count - capacity`
  **least-recently-used** entries (popped from the LRU end, oldest first),
  collecting their **values** into a freshly allocated slice (returned to the
  caller, who frees it), removing each from the map and freeing its entry.
  `evicted[0]` is the most-LRU.

The upstream tests: a shrink to a capacity ≥ the count returns `null` (no
removal); a shrink below the count returns a 1-element slice (the evicted
value), after which the next `getOrPut` evicts the (now-LRU) survivor.

## Rust mapping (`roastty/src/terminal/lru.rs`)

Slots become `Option<Node>` so a freed slot can hold no value (the only way to
move a value out of an arena slot in safe Rust). A `free: Vec<usize>` tracks
freed indices; `get_or_put`'s allocation reuses one when present. `resize`
returns `Option<Vec<V>>`:

```rust
// `nodes: Vec<Option<Node<K, V>>>` (a freed slot is `None`); `free: Vec<usize>` of freed indices.

/// Resize the cache. Growing (or shrinking to >= the current count) just updates the capacity;
/// shrinking below the count evicts the least-recently-used entries and returns their values
/// (upstream `resize`).
pub(crate) fn resize(&mut self, capacity: usize) -> Option<Vec<V>> {
    if capacity >= self.capacity || self.map.len() <= capacity {
        self.capacity = capacity;
        return None;
    }

    let delta = self.map.len() - capacity;
    let mut evicted = Vec::with_capacity(delta);
    for _ in 0..delta {
        let lru = self.head; // the least-recently-used
        self.unlink(lru);
        let node = self.nodes[lru].take().expect("occupied slot");
        self.map.remove(&node.key);
        evicted.push(node.value);
        self.free.push(lru);
    }
    self.capacity = capacity;
    Some(evicted)
}
```

The arena accessors become
`self.nodes[idx].as_ref()/.as_mut().expect("occupied")` (occupied by the
map/list invariant). `get_or_put`'s allocation path reuses a freed slot:

```rust
// (miss, not at capacity) — reuse a freed slot if available, else push a new one:
let idx = match self.free.pop() {
    Some(idx) => {
        self.nodes[idx] = Some(Node { key, value, prev: NIL, next: NIL });
        idx
    }
    None => {
        self.nodes.push(Some(Node { key, value, prev: NIL, next: NIL }));
        self.nodes.len() - 1
    }
};
```

The eviction (at-capacity) path is unchanged except it writes `Some(Node { … })`
via `replace`. `resize` pops `delta` nodes from the LRU end, moving each value
into `evicted` (returned `Vec`), freeing the slot (`take()` ⇒ `None`, index
pushed to `free`). `Vec<V>` (caller-owned) replaces the allocator-returned
slice; `Option<Vec<V>>` replaces the `?[]V`.

## Scope / faithfulness notes

- **Ported (bridged)**: `datastruct.lru.HashMap.resize` → `Lru::resize`. With
  this, **`Lru` is fully ported** (`get`, `get_or_put`, `resize`).
- **Faithful**: grow / shrink-no-removal ⇒ update capacity, return `None`;
  shrink-below-count ⇒ evict the `delta` least-recently-used entries (oldest
  first), return their values; update capacity.
- **Faithful adaptation**: `Vec<Node>` → `Vec<Option<Node>>` + a
  `free: Vec<usize>` list (so a freed arena slot is representable in safe Rust —
  Zig's `alloc.destroy(entry)` ⇒ `take()` the slot and record it free;
  `get_or_put` reuses a freed slot); the allocator-returned `?[]V` →
  `Option<Vec<V>>` (caller-owned, no manual free).
- **Deferred**: nothing — `lru.zig` is fully ported (only the
  `Context`-parameterized variants remain, which Rust's `Hash + Eq` bound
  subsumes).
- No C ABI/header/ABI-inventory change (internal Rust). Extends `terminal::lru`.

## Changes

1. `roastty/src/terminal/lru.rs`: change `nodes` to `Vec<Option<Node>>`, add
   `free: Vec<usize>`, reuse freed slots in `get_or_put_with`'s allocation, add
   `resize`, and adapt the arena accessors.
2. Tests (in `lru.rs`):
   - **grow**: `resize(5)` on a capacity-2 cache returns `None` and sets
     `capacity() == 5`.
   - **shrink without removal**: cache with one entry (capacity 2), `resize(1)`
     returns `None`; the entry is still present.
   - **shrink with removal**: cache `[1, 2]` (capacity 2), `resize(1)` returns
     `Some(vec![1])` (the LRU value); then `get_or_put_with(1, …)` is a miss
     that evicts the survivor `2`.
   - **free-slot reuse**: after a shrink-with-removal, a subsequent
     `get_or_put_with` (miss) succeeds and the cache behaves correctly (the
     freed slot is reused without corrupting the LRU order).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::lru
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/lru.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `resize` grows / shrinks-without-removal by updating `capacity` (returning
  `None`), and shrinks-below-count by evicting the `delta` least-recently-used
  entries and returning their values; freed slots are reused by later
  `get_or_put` allocations — faithful to `datastruct/lru.zig`, completing the
  `Lru` port;
- the tests pass (grow / shrink-no-removal / shrink-removal / free-slot reuse),
  and the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the resize thresholds, the eviction order, or the
free-slot reuse diverges from upstream, an unrelated item changes, or any public
C API/ABI changes.

## Design Review

Codex reviewed the design and **approved it with no findings**. It confirmed the
`resize` thresholds and ordering are faithful (grow / shrink-to-≥-count ⇒ update
capacity, return `None`; shrink-below-count ⇒ evict `count − capacity` nodes
from the LRU end, oldest first, returning their values in that order);
`Option<Vec<V>>` is the right Rust shape for upstream's optional caller-owned
slice; and the arena refactor is sound — `Vec<Option<Node>>` gives a safe empty
state for freed slots, `take()` moves the evicted node/value out cleanly, and
`free: Vec<usize>` lets later non-capacity misses reuse freed indices without
corrupting the list, with `head`/`tail` correct through partial and full
eviction so long as `unlink` is adapted through the occupied-slot accessors. The
planned tests cover the important resize paths, including free-slot reuse.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d562-prompt.md`
- Result: `logs/codex-review/20260604-d562-last-message.md`

## Result

**Result:** Pass

`Lru::resize` was added, **completing the `Lru` port**. The arena was refactored
from `Vec<Node>` to `Vec<Option<Node>>` plus a `free: Vec<usize>` list, with
private `node` / `node_mut` accessors (`as_ref()` /
`as_mut().expect("occupied slot")`, justified by the map/list invariant). `get`,
the at-capacity eviction path (now `nodes[lru].replace(…)`), and `unlink` /
`link_tail` use the accessors; `get_or_put_with`'s miss-allocation reuses a
freed slot (`free.pop()`) when available, else pushes. `resize(capacity)`
returns `None` after just updating the capacity when growing or shrinking to ≥
the current count, and otherwise evicts the `delta = count − capacity`
least-recently-used entries (oldest first — popping `head`, `unlink`, `take()`,
free the slot), returning their values as `Some(Vec<V>)`.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3117 passed, 0 failed (five new tests; no
  regressions, up from 3112).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/lru.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The five new tests: grow (keeps entries, `capacity()` updated),
shrink-without-removal, shrink-with-removal (`resize(1)` on `[1, 2]` ⇒
`Some(vec![1])`; survivor `2` becomes the LRU and the next miss evicts
`(2, 2)`), oldest-first eviction (`resize(1)` on `[1, 2, 3]` ⇒
`Some(vec![1, 2])`), and free-slot reuse (a shrink frees a slot, a
grow-then-insert reuses it with the LRU order intact).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet in the saved file — added here as part of result recording). Codex
confirmed the implementation matches upstream `resize` semantics (grow /
shrink-without-removal update capacity and return `None`; shrink-below-count
evicts from the LRU end, oldest first, returning values in order), the
`Vec<Option<Node>>` + `free` refactor is sound, the occupied-slot `expect()`s
are justified by the map/list invariant, and `unlink` / `link_tail` preserve
`head` / `tail` behavior through partial and full eviction; the tests cover the
key paths including oldest-first eviction and free-slot reuse.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r562-prompt.md` (result)
- Result: `logs/codex-review/20260604-r562-last-message.md` (result)

## Conclusion

`terminal::lru::Lru` is now **fully ported** from `datastruct/lru.zig` (`init`,
`get`, `get_or_put`, `resize`). Completing `resize` forced the arena's free-slot
story: upstream's `alloc.destroy(entry)` became `take()`-ing the slot to `None`
and recording its index in a `free` list that `get_or_put` reuses —
`Vec<Option<Node>>` being the safe Rust representation of an arena slot whose
value has been moved out. roastty's fourth `datastruct/` type is complete. The
remaining `datastruct/` work is `intrusive_linked_list` (the raw-pointer
machinery), `segmented_pool` (libuv-shaped), `blocking_queue` (channel-like),
and the large `split_tree`. The terminal **search subsystem** (now that
`CircBuf` and the cache datastructs are in place) is the other natural target.
The objc/bundle-id helpers, the `home()` resolver, and config `loadDefaultFiles`
remain deferred pending roastty's naming decision; `background-image-opacity`
stays float-blocked.
