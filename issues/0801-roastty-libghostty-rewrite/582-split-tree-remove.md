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

# Experiment 582: split tree remove (delete a node, collapsing its parent split)

## Description

This experiment ports `remove` from upstream `datastruct/split_tree.zig` — the
inverse of `split`. `remove` builds a **new** compacted tree with the node `at`
deleted: its parent split collapses into the surviving sibling, and any zoomed
node migrates to its new position. It uses two recursive helpers,
`countAfterRemoval` (sizing the new arena) and `removeNode` (copying nodes into
place); the view ref-counting comes from `Rc::clone`. It extends
`terminal::split_tree`.

## Upstream behavior

```zig
pub fn remove(self, gpa, at) !Self {
    assert(at.idx() < self.nodes.len);
    if (at == .root) return .empty;                      // removing the root clears the tree
    const nodes = try alloc.alloc(Node, self.countAfterRemoval(.root, at, 0));
    var result = .{ .arena = arena, .nodes = nodes, .zoomed = null };
    assert(self.removeNode(&result, 0, .root, at) != 0); // copy compacted nodes in
    try refNodes(gpa, nodes);                            // ref every view
    return result;
}

fn countAfterRemoval(self, current, target, acc) usize {
    return switch (self.nodes[current.idx()]) {
        .leaf => acc + 1,
        .split => |s|
            if (s.left == target) self.countAfterRemoval(s.right, target, acc)   // collapse
            else if (s.right == target) self.countAfterRemoval(s.left, target, acc)
            else self.countAfterRemoval(s.left, target, acc)
               + self.countAfterRemoval(s.right, target, acc) + 1,               // keep the split
    };
}

fn removeNode(old, new, new_offset, current, target) usize {       // returns nodes written
    assert(current != target);
    if (old.zoomed) |v| if (v == current) new.zoomed = @enumFromInt(new_offset); // migrate zoom
    switch (old.nodes[current.idx()]) {
        .leaf => |view| { new.nodes[new_offset] = .{ .leaf = view }; return 1; },
        .split => |s| {
            if (s.left == target)  return old.removeNode(new, new_offset, s.right, target); // collapse
            if (s.right == target) return old.removeNode(new, new_offset, s.left, target);
            const left  = old.removeNode(new, new_offset + 1, s.left, target);
            const right = old.removeNode(new, new_offset + left + 1, s.right, target);
            new.nodes[new_offset] = .{ .split = .{ .layout = s.layout, .ratio = s.ratio,
                .left = new_offset + 1, .right = new_offset + 1 + left } };
            return left + right + 1;
        },
    }
}
```

So: removing the root yields the empty tree. Otherwise the new tree is a
**compacted** copy where `at`'s parent split is dropped and replaced by `at`'s
surviving sibling subtree; every kept node is written contiguously (`removeNode`
returns its subtree size so the parent's child offsets can be computed), the
zoomed handle migrates to the surviving node's new index (or is dropped if the
zoomed node was removed), and all views are ref'd once. The `acc` parameter of
`countAfterRemoval` is vestigial — it is always called with `0` and never
accumulates (it's passed through unchanged), so the function just computes
leaf=1 / collapsed-split=sibling-count / kept-split=both+1.

## Rust mapping (`roastty/src/terminal/split_tree.rs`)

`removeNode` writes at computed offsets (the split parent before its children),
so the new arena is a pre-sized `Vec<Option<Node<V>>>` written by index, then
unwrapped (every slot is written exactly once). The vestigial `acc` is dropped.
`Rc::clone` at copy time _is_ upstream's deferred `refNodes`.

```rust
impl<V> SplitTree<V> {
    /// Build a new tree with the node `at` removed, collapsing its parent split into the surviving
    /// sibling (upstream `remove`). Removing the root yields the empty tree. Shares views with
    /// `self` (ref-counted); the zoomed node migrates to its new index (or is dropped if removed).
    pub(crate) fn remove(&self, at: Handle) -> SplitTree<V> {
        assert!(at.idx() < self.nodes.len(), "remove handle out of range");
        if at == Handle::ROOT {
            return SplitTree::empty();
        }

        let count = self.count_after_removal(Handle::ROOT, at);
        let mut new_nodes: Vec<Option<Node<V>>> = (0..count).map(|_| None).collect();
        let mut new_zoomed: Option<Handle> = None;
        let written = self.remove_node(&mut new_nodes, &mut new_zoomed, 0, Handle::ROOT, at);
        debug_assert_eq!(written, count);

        let nodes = new_nodes
            .into_iter()
            .map(|n| n.expect("every slot written"))
            .collect();
        SplitTree {
            nodes,
            zoomed: new_zoomed,
        }
    }

    /// The node count of the tree after removing `target` (upstream `countAfterRemoval`, without the
    /// vestigial `acc`).
    fn count_after_removal(&self, current: Handle, target: Handle) -> usize {
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => 1,
            Node::Split(s) => {
                if s.left == target {
                    self.count_after_removal(s.right, target)
                } else if s.right == target {
                    self.count_after_removal(s.left, target)
                } else {
                    self.count_after_removal(s.left, target)
                        + self.count_after_removal(s.right, target)
                        + 1
                }
            }
        }
    }

    /// Copy the subtree at `current` (with `target` removed) into `new_nodes` starting at
    /// `new_offset`, returning the number of nodes written (upstream `removeNode`).
    fn remove_node(
        &self,
        new_nodes: &mut [Option<Node<V>>],
        new_zoomed: &mut Option<Handle>,
        new_offset: usize,
        current: Handle,
        target: Handle,
    ) -> usize {
        assert!(current != target);

        // Migrate a zoomed node to its new index.
        if self.zoomed == Some(current) {
            *new_zoomed = Some(Handle::from_index(new_offset));
        }

        match &self.nodes[current.idx()] {
            Node::Leaf(view) => {
                new_nodes[new_offset] = Some(Node::Leaf(Rc::clone(view)));
                1
            }
            Node::Split(s) => {
                let s = *s;
                // If a child is the target, drop this split and keep only the other child.
                if s.left == target {
                    return self.remove_node(new_nodes, new_zoomed, new_offset, s.right, target);
                }
                if s.right == target {
                    return self.remove_node(new_nodes, new_zoomed, new_offset, s.left, target);
                }
                // Keep the split: copy its children (filling the slots after `new_offset`), then
                // write the split node itself into `new_offset` with the children's new offsets.
                let left = self.remove_node(new_nodes, new_zoomed, new_offset + 1, s.left, target);
                let right =
                    self.remove_node(new_nodes, new_zoomed, new_offset + left + 1, s.right, target);
                new_nodes[new_offset] = Some(Node::Split(Split {
                    layout: s.layout,
                    ratio: s.ratio,
                    left: Handle::from_index(new_offset + 1),
                    right: Handle::from_index(new_offset + 1 + left),
                }));
                left + right + 1
            }
        }
    }
}
```

## Scope / faithfulness notes

- **Ported**: `remove` / `countAfterRemoval` / `removeNode` →
  `SplitTree::remove` / `count_after_removal` / `remove_node`.
- **Faithful**: removing the root yields the empty tree; otherwise the new tree
  is the compacted copy with `at`'s parent split collapsed into the surviving
  sibling (a split whose child is the target is dropped, keeping only the other
  side); the contiguous layout (the split node occupies `new_offset`, its left
  subtree `+1`, its right subtree `+1+left_count` — the children are copied
  first so their sizes are known, then the parent is written into `new_offset`);
  the zoom migration (to the surviving node's new index, dropped if the zoomed
  node was removed); and the per-view ref-counting are all reproduced.
- **Faithful adaptation**: the pre-sized uninitialized arena becomes a
  `Vec<Option<Node<V>>>` written by index then unwrapped (every slot is written
  exactly once, as the exact count guarantees); `Rc::clone` at copy time
  replaces upstream's deferred single `refNodes` (same net: each leaf ref'd
  once); the vestigial `acc` parameter (always `0`) is dropped; the `@constCast`
  (to write into the const-but-owned `new.nodes`) is unnecessary in Rust (the
  new `Vec` is plainly mutable). `remove` returns `Self` directly (removal only
  shrinks, so there is no overflow/alloc error).
- **Deferred**: `equalize` / `resize` (the `f16`-ratio rebalancers) and the
  formatters.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::split_tree`.

## Changes

1. `roastty/src/terminal/split_tree.rs`: add `SplitTree::remove`,
   `count_after_removal`, and `remove_node`.
2. Tests (in `split_tree.rs`):
   - **remove a leaf from a 2-leaf split**: the split collapses to the surviving
     leaf (a single-leaf tree, not a split).
   - **remove a leaf from a 3-leaf tree**: the removed leaf's parent split
     collapses; the result has the expected leaves, structure (`is_split`), and
     `dimensions`.
   - **remove the root**: yields the empty tree.
   - **remove migrates the zoom**: zoom a surviving node, remove a different
     node, and the survivor's **new** handle is zoomed; zooming the removed node
     leaves the result un-zoomed.
   - **remove zoom on a collapsed parent split**: zoom the parent split that
     gets collapsed; the zoom migrates to the surviving sibling's new handle.
   - **ref-counting** (`remove` is immutable — it builds a new tree without
     touching `self`): after `remove`, each surviving view's `Rc::strong_count`
     **rises by one** (the new tree's reference), while the removed view's count
     is **unchanged** (the new tree does not reference it). Dropping the **old**
     tree then drops the removed view's reference (and one of each survivor's).
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

- `remove` builds the compacted tree with the parent split collapsed into the
  surviving sibling, the contiguous node layout, the zoom migration, and the
  shared-view ref-counting (and the empty-tree root case) — faithful to
  `datastruct/split_tree.zig`;
- the tests pass (collapse / 3-leaf / root / zoom-migration / ref-count), and
  the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the collapse logic, the node layout, the zoom
migration, or the ref-counting diverges from upstream, an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed the design and found **one Required** finding plus an Optional
and a Nit, all addressed:

- **Required (fixed)**: the ref-count test expectation was wrong. `remove` is
  **immutable** — it builds a new tree without mutating or dropping `self`, so
  the removed view's `Rc::strong_count` does **not** drop merely because
  `remove()` was called; it simply does **not increase** (survivors gain one ref
  from the new tree). The removed view's reference drops only when the **old**
  tree is later dropped. The test plan was corrected to match.
- **Optional (adopted)**: added a zoom test where the **collapsed parent split**
  is zoomed — upstream migrates zoom before collapse, so the zoom moves to the
  surviving sibling's new handle.
- **Nit (fixed)**: clarified that `removeNode` copies the children first (so
  their sizes are known) and then writes the parent split into `new_offset` —
  i.e. the parent _occupies_ `new_offset` but is written after the children.

Codex confirmed everything else: dropping the vestigial `acc` is safe,
`count_after_removal` matches the effective upstream recurrence, `remove_node`'s
contiguous offsets and collapse behavior match upstream, the
`Vec<Option<Node<V>>>` unwrap is justified by the count/write invariant,
`Rc::clone` is the right equivalent of the deferred ref-all, root removal to
empty is faithful, and the 3-leaf trace produces `H(b@1, c@2)` with
`dimensions == {2,1}`.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d582-prompt.md`
- Result: `logs/codex-review/20260604-d582-last-message.md`
