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

# Experiment 578: split tree Spatial container (spatial / fillSpatialSlots)

## Description

This experiment ports the **`Spatial` representation** of upstream
`datastruct/split_tree.zig` — `spatial` and `fillSpatialSlots`, which compute
each node's normalized 2D rectangle in a 1×1 space. It combines the `Node<V>`
arena (Experiment 576) with the `Slot` `f16` geometry (Experiments 573 / 574),
both now available. With this and the geometry helpers, only the arena-coupled
`nearest` / `nearestWrapped` (then `goto`) and the formatters remain for the
spatial side. It extends `terminal::split_tree`.

## Upstream behavior

```zig
pub fn spatial(self, alloc) !Spatial {
    if (self.nodes.len == 0) return .empty;
    const dim = self.dimensions(.root);                 // relative size in leaf units
    const slots = try alloc.alloc(Spatial.Slot, self.nodes.len);
    slots[0] = .{ .x = 0, .y = 0,
                  .width = @floatFromInt(dim.width), .height = @floatFromInt(dim.height) };
    self.fillSpatialSlots(slots, .root);
    for (slots) |*slot| {                               // normalize to a 1x1 grid
        slot.x /= @floatFromInt(dim.width);   slot.y /= @floatFromInt(dim.height);
        slot.width /= @floatFromInt(dim.width); slot.height /= @floatFromInt(dim.height);
    }
    return .{ .slots = slots };
}

fn fillSpatialSlots(self, slots, current) void {
    assert(slots[current].width >= 0 and slots[current].height >= 0);
    switch (self.nodes[current]) {
        .leaf => {},                                     // already filled by the caller
        .split => |s| {
            // horizontal: split the WIDTH by ratio; vertical: split the HEIGHT.
            // left gets `width * ratio` (or `height * ratio`); right the remainder `* (1 - ratio)`,
            // offset by the left child's extent. Then recurse into both children.
        },
    }
}
```

`spatial` sizes a slot for every node (matching the arena exactly): the root
spans the full relative dimensions, each split divides its slot between its two
children by `ratio` along the split axis (the other axis is inherited), and
finally every slot is divided by the total dimensions to normalize into a 1×1
space (top-left `(0, 0)`). An empty tree yields an empty `Spatial`.

## Rust mapping (`roastty/src/terminal/split_tree.rs`)

```rust
/// The normalized 2D layout of every node, in a 1×1 space (upstream `Spatial`).
pub(crate) struct Spatial {
    slots: Vec<Slot>,
}

impl Spatial {
    /// The per-node slots, in the same order as the tree's nodes.
    pub(crate) fn slots(&self) -> &[Slot] {
        &self.slots
    }
}

impl<V> SplitTree<V> {
    /// The normalized spatial representation: each node's rectangle in a 1×1 space (upstream
    /// `spatial`). An empty tree yields an empty `Spatial`.
    pub(crate) fn spatial(&self) -> Spatial {
        if self.nodes.is_empty() {
            return Spatial { slots: Vec::new() };
        }

        let dim = self.dimensions(Handle::ROOT);
        let width = f16::from_f32(dim.width as f32);
        let height = f16::from_f32(dim.height as f32);
        let zero = f16::from_f32(0.0);

        // One slot per node; the root spans the full relative dimensions, the rest are filled by
        // `fill_spatial_slots`. The zero init is just a placeholder (every node is reached).
        let mut slots = vec![
            Slot { x: zero, y: zero, width: zero, height: zero };
            self.nodes.len()
        ];
        slots[0] = Slot {
            x: zero,
            y: zero,
            width,
            height,
        };
        self.fill_spatial_slots(&mut slots, Handle::ROOT);

        // Normalize to a 1×1 grid.
        for slot in &mut slots {
            slot.x /= width;
            slot.y /= height;
            slot.width /= width;
            slot.height /= height;
        }

        Spatial { slots }
    }

    /// Recursively fill each child's slot from its parent split's slot (upstream `fillSpatialSlots`).
    fn fill_spatial_slots(&self, slots: &mut [Slot], current: Handle) {
        let cur = slots[current.idx()];
        let zero = f16::from_f32(0.0);
        assert!(cur.width >= zero && cur.height >= zero);

        if let Node::Split(s) = &self.nodes[current.idx()] {
            let s = *s; // copy (drops the borrow of `self.nodes`)
            let one = f16::from_f32(1.0);
            match s.layout {
                Layout::Horizontal => {
                    slots[s.left.idx()] = Slot {
                        x: cur.x,
                        y: cur.y,
                        width: cur.width * s.ratio,
                        height: cur.height,
                    };
                    slots[s.right.idx()] = Slot {
                        x: cur.x + cur.width * s.ratio,
                        y: cur.y,
                        width: cur.width * (one - s.ratio),
                        height: cur.height,
                    };
                }
                Layout::Vertical => {
                    slots[s.left.idx()] = Slot {
                        x: cur.x,
                        y: cur.y,
                        width: cur.width,
                        height: cur.height * s.ratio,
                    };
                    slots[s.right.idx()] = Slot {
                        x: cur.x,
                        y: cur.y + cur.height * s.ratio,
                        width: cur.width,
                        height: cur.height * (one - s.ratio),
                    };
                }
            }
            self.fill_spatial_slots(slots, s.left);
            self.fill_spatial_slots(slots, s.right);
        }
        // Leaf: the slot was already filled by the caller (upstream's `.leaf => {}`).
    }
}
```

## Scope / faithfulness notes

- **Ported**: `spatial` / `fillSpatialSlots` → `SplitTree::spatial` /
  `fill_spatial_slots`, plus the `Spatial` container (`slots` accessor).
- **Faithful**: the per-node slot sizing (root = full relative dimensions; each
  split divides its slot between its children by `ratio` along the split axis,
  inheriting the other axis; right child offset by the left's extent), the `f16`
  arithmetic (`*`, `+`, `/`, `1 - ratio`), the 1×1 normalization, the
  `width >= 0 && height >= 0` assertion, and the empty-tree → empty-`Spatial`
  case are reproduced exactly.
- **Faithful adaptation**: upstream's `alloc`-allocated, uninitialized slot
  array becomes a `Vec<Slot>` zero-initialized as a placeholder (every node is
  reached and overwritten before normalization, so the placeholder is never
  observed); `@floatFromInt(dim.width)` becomes
  `f16::from_f32(dim.width as f32)` (exact for the tiny `u16` leaf counts). The
  `Split` is copied out of the borrow (`let s = *s;`) before recursing, since
  `Split` is `Copy`. The `deinit` (free slots) is automatic (`Vec` drop).
- **Deferred**: `nearest` / `nearestWrapped` (they consume a `Spatial`, iterate
  its slots, and skip non-leaf nodes via the arena — using the
  `Slot::is_in_direction` / `distance_to` / `wrapped_for` helpers from
  Experiment 574), the `goto` method, and the formatters.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::split_tree`.

## Changes

1. `roastty/src/terminal/split_tree.rs`: add the `Spatial` struct (+ `slots`
   accessor), `SplitTree::spatial`, and the private `fill_spatial_slots`.
2. Tests (in `split_tree.rs`), using binary-exact `f16` values (ratio `0.5`,
   power-of-two dims):
   - **single leaf**: one slot, `{0, 0, 1, 1}`.
   - **empty tree**: zero slots.
   - **horizontal split (ratio 0.5)**: root `{0, 0, 1, 1}`; left leaf
     `{0, 0, 0.5, 1}`; right leaf `{0.5, 0, 0.5, 1}`.
   - **horizontal split (ratio 0.25)** — an asymmetric, binary-exact ratio so
     the `1 - ratio` right child can't be confused with `ratio`: left leaf
     `{0, 0, 0.25, 1}`; right leaf `{0.25, 0, 0.75, 1}`.
   - **vertical split (ratio 0.5)**: left leaf `{0, 0, 1, 0.5}`; right leaf
     `{0, 0.5, 1, 0.5}`.
   - **nested tree**: a horizontal split of a vertical 1×2 column and a single
     leaf — the column's two leaves get the left half stacked vertically, the
     right leaf the right half.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::split_tree
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/split_tree.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `spatial` / `fill_spatial_slots` reproduce upstream's per-node slot sizing,
  the ratio-based split along each axis, and the 1×1 normalization (with the
  empty-tree case) — faithful to `datastruct/split_tree.zig`;
- the tests pass (single / empty / horizontal / vertical / nested), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the slot sizing, the ratio split, the normalization,
or the empty-tree handling diverges from upstream, an unrelated item changes, or
any public C API/ABI changes.

## Design Review

Codex reviewed the design and found **no Required findings**, with one Optional
— adopted:

- **Optional (adopted)**: add a non-`0.5` ratio case (binary-exact `0.25` /
  `0.75`) for a split, because with `0.5` the `ratio == 1 - ratio` symmetry
  would not catch an implementation that used `ratio` for both children instead
  of `1 - ratio` on the right/bottom child. Added a horizontal `ratio 0.25` test
  (left `{0, 0, 0.25, 1}`, right `{0.25, 0, 0.75, 1}`).

Codex confirmed the design otherwise matches upstream — empty spatial is empty,
the root starts at the relative dimensions, `fill_spatial_slots` splits width
for horizontal and height for vertical with the right/bottom child offset by the
left/top extent, and normalization divides all slots into the 1×1 space — and
that the zero-initialized `Vec<Slot>` placeholder is safe under the same
valid-tree invariant upstream relies on (every node is reachable from root and
assigned before normalization reads it).

Review artifacts:

- Prompt: `logs/codex-review/20260604-d578-prompt.md`
- Result: `logs/codex-review/20260604-d578-last-message.md`
