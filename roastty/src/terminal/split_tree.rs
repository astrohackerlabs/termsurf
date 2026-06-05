//! The split-pane tree (port of upstream `datastruct/split_tree`). Lands the vocabulary
//! (`Handle` / `Layout` / `Direction`), the `Split` / `Slot` payloads, the spatial geometry, the
//! `Node<V>` arena (`SplitTree`) with the structural queries (`is_empty` / `is_split` / `deepest` /
//! `dimensions`) and `Rc<V>`-based view ref-counting, the leaf `iter`ator, `zoom`, and the `Goto`
//! enum, the `Spatial` container's normalization (`spatial` / `fillSpatialSlots`), and the full
//! navigation (the spatial `nearest` / `nearest_wrapped`, the in-order `previous` / `next`, and the
//! `goto` dispatch), and the `split` / `remove` tree-shaping operations. Still deferred: the
//! remaining tree-shaping operations (`equalize` / `resize`) and the formatters.

use half::f16;
use std::rc::Rc;

/// A handle into the tree's `nodes` array (upstream `Node.Handle`): a `u16`-backed index, so nodes
/// are referenced by 16-bit handles rather than pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Handle(u16);

impl Handle {
    /// The root node's handle (index 0) (upstream `.root`).
    pub(crate) const ROOT: Handle = Handle(0);

    /// Build a handle from an index (upstream `@enumFromInt`). The full `u16` range is valid —
    /// upstream's `enum(u16)` can represent `u16::MAX`, which the tree iterator uses as an
    /// end sentinel (`@enumFromInt(handle.idx() + 1)`).
    pub(crate) fn from_index(index: usize) -> Handle {
        assert!(index <= u16::MAX as usize, "split tree handle out of range");
        Handle(index as u16)
    }

    /// The index this handle refers to (upstream `idx`).
    pub(crate) fn idx(self) -> usize {
        self.0 as usize
    }

    /// Offset the handle by `v` (upstream `offset`), asserting the result stays below `u16::MAX`
    /// (matching upstream's `final < maxInt(Backing)`).
    pub(crate) fn offset(self, v: usize) -> Handle {
        let result = (self.0 as usize)
            .checked_add(v)
            .expect("split tree handle offset overflow");
        assert!(result < u16::MAX as usize, "split tree handle overflow");
        Handle(result as u16)
    }
}

/// The orientation of a split (upstream `Split.Layout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Horizontal,
    Vertical,
}

/// The direction a new view is split off in (upstream `Split.Direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Left,
    Right,
    Down,
    Up,
}

impl Direction {
    /// The split layout and whether the new view goes on the first (left / top) side, for a split
    /// in this direction (upstream `split`'s direction switch).
    pub(crate) fn split_layout(self) -> (Layout, bool) {
        match self {
            Direction::Left => (Layout::Horizontal, true),
            Direction::Right => (Layout::Horizontal, false),
            Direction::Up => (Layout::Vertical, true),
            Direction::Down => (Layout::Vertical, false),
        }
    }
}

/// The payload of a split node (upstream `Split`): two child handles, the split orientation, and
/// the fraction of space given to the first (left / top) child.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Split {
    pub(crate) layout: Layout,
    pub(crate) ratio: f16,
    pub(crate) left: Handle,
    pub(crate) right: Handle,
}

/// A node's normalized 2D rectangle in the spatial representation (upstream `Spatial.Slot`); all
/// coordinates are in a 1×1 space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slot {
    pub(crate) x: f16,
    pub(crate) y: f16,
    pub(crate) width: f16,
    pub(crate) height: f16,
}

impl Slot {
    /// The right edge, `x + width` (upstream `maxX`).
    pub(crate) fn max_x(self) -> f16 {
        self.x + self.width
    }

    /// The bottom edge, `y + height` (upstream `maxY`).
    pub(crate) fn max_y(self) -> f16 {
        self.y + self.height
    }

    /// Whether `self` (a candidate slot) lies in `direction` relative to `target` (upstream
    /// `nearest`'s direction switch).
    pub(crate) fn is_in_direction(self, target: Slot, direction: SpatialDirection) -> bool {
        match direction {
            SpatialDirection::Left => self.max_x() <= target.x,
            SpatialDirection::Right => self.x >= target.max_x(),
            SpatialDirection::Up => self.max_y() <= target.y,
            SpatialDirection::Down => self.y >= target.max_y(),
        }
    }

    /// The euclidean distance from `self` to `target` (upstream `nearest`'s
    /// `@sqrt(dx*dx + dy*dy)`). The `dx`/`dy`/products/sum are computed in `f16` (matching
    /// upstream's per-op binary16 arithmetic); the square root widens the `f16` sum to `f64`, takes
    /// the root there, and rounds back to `f16` (Rust's `half` has no `f16` sqrt). The wide `f64`
    /// intermediate makes this a single effective rounding, matching Zig's `@sqrt` on `f16`.
    pub(crate) fn distance_to(self, target: Slot) -> f16 {
        let dx = self.x - target.x;
        let dy = self.y - target.y;
        let sum = dx * dx + dy * dy;
        f16::from_f64(sum.to_f64().sqrt())
    }

    /// `self` shifted by one full normalized (1×1) grid for wrap-around in `direction` (upstream
    /// `nearestWrapped`'s target shift). Shifts in the opposite sense of travel so the nearest
    /// search re-finds across the wrap boundary.
    pub(crate) fn wrapped_for(self, direction: SpatialDirection) -> Slot {
        let one = f16::from_f32(1.0);
        match direction {
            SpatialDirection::Left => Slot {
                x: self.x + one,
                ..self
            },
            SpatialDirection::Right => Slot {
                x: self.x - one,
                ..self
            },
            SpatialDirection::Up => Slot {
                y: self.y + one,
                ..self
            },
            SpatialDirection::Down => Slot {
                y: self.y - one,
                ..self
            },
        }
    }
}

/// A spatial navigation direction — the nearest surface visually in this direction (upstream
/// `Spatial.Direction`; a separate type from `Direction`, with the same variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpatialDirection {
    Left,
    Right,
    Down,
    Up,
}

/// A node in the split tree (upstream `Node`): a leaf holding a (ref-counted) view, or an internal
/// split.
#[derive(Debug)]
pub(crate) enum Node<V> {
    Leaf(Rc<V>),
    Split(Split),
}

// Manual `Clone` (not derived) so it does not require `V: Clone` — `Rc<V>` is `Clone` for any `V`.
impl<V> Clone for Node<V> {
    fn clone(&self) -> Self {
        match self {
            Node::Leaf(v) => Node::Leaf(Rc::clone(v)),
            Node::Split(s) => Node::Split(*s),
        }
    }
}

/// Which child to descend into (upstream `Side`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// The result of the backtracking previous/next search (upstream `Backtrack`).
enum Backtrack {
    Deadend,
    Backtrack,
    Result(Handle),
}

/// The node count of a `split` result would exceed the `u16` handle range (upstream's
/// `error.OutOfMemory` on the handle-limit check).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TooManyNodes;

/// Relative tree dimensions in leaf units (upstream `dimensions`' anonymous return).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dimensions {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

/// An immutable binary tree of split panes, stored as a flat node arena (upstream `SplitTree(V)`).
/// Index 0 is the root. Cloning the tree clones each leaf's `Rc<V>` (upstream `clone` / `refNodes`);
/// dropping it drops them (upstream `deinit` / `viewUnref`).
#[derive(Debug)]
pub(crate) struct SplitTree<V> {
    nodes: Vec<Node<V>>,
    zoomed: Option<Handle>,
}

// Manual `Clone` (not derived) so it does not require `V: Clone` — cloning the node arena clones
// each leaf's `Rc<V>` (upstream `refNodes`), which works for any `V`.
impl<V> Clone for SplitTree<V> {
    fn clone(&self) -> Self {
        SplitTree {
            nodes: self.nodes.clone(),
            zoomed: self.zoomed,
        }
    }
}

impl<V> SplitTree<V> {
    /// An empty tree with no nodes (upstream `empty`).
    pub(crate) fn empty() -> Self {
        SplitTree {
            nodes: Vec::new(),
            zoomed: None,
        }
    }

    /// A single-leaf tree holding `view` (upstream `init`). The caller's `Rc` is stored (its
    /// refcount is the view's ref).
    pub(crate) fn new(view: Rc<V>) -> Self {
        SplitTree {
            nodes: vec![Node::Leaf(view)],
            zoomed: None,
        }
    }

    /// Whether the tree has no nodes (upstream `isEmpty`).
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether the root is a split — i.e. the tree has more than one view (upstream `isSplit`).
    pub(crate) fn is_split(&self) -> bool {
        matches!(self.nodes.first(), Some(Node::Split(_)))
    }

    /// The deepest leaf reached by always descending into the `side` child from `from` (upstream
    /// `deepest`).
    pub(crate) fn deepest(&self, side: Side, from: Handle) -> Handle {
        let mut current = from;
        loop {
            match &self.nodes[current.idx()] {
                Node::Leaf(_) => return current,
                Node::Split(s) => {
                    current = match side {
                        Side::Left => s.left,
                        Side::Right => s.right,
                    }
                }
            }
        }
    }

    /// Iterate the tree's leaf views in node-arena order (upstream `iterator`).
    pub(crate) fn iter(&self) -> Iter<'_, V> {
        Iter {
            nodes: &self.nodes,
            i: 0,
        }
    }

    /// Set (or clear) the zoomed node (upstream `zoom`). Asserts the handle is in range.
    pub(crate) fn zoom(&mut self, handle: Option<Handle>) {
        if let Some(h) = handle {
            assert!(h.idx() < self.nodes.len(), "zoom handle out of range");
        }
        self.zoomed = handle;
    }

    /// The currently-zoomed node, if any (upstream's `zoomed` field).
    pub(crate) fn zoomed(&self) -> Option<Handle> {
        self.zoomed
    }

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
            Slot {
                x: zero,
                y: zero,
                width: zero,
                height: zero,
            };
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

    /// Build a new tree by splitting `at` in `direction` with `ratio`, inserting `insert` on the
    /// new side (upstream `split`). The result shares all views with `self` and `insert`
    /// (ref-counted) and resets the zoom state.
    pub(crate) fn split(
        &self,
        at: Handle,
        direction: Direction,
        ratio: f16,
        insert: &SplitTree<V>,
    ) -> Result<SplitTree<V>, TooManyNodes> {
        let self_len = self.nodes.len();
        let total = self_len + insert.nodes.len() + 1;
        if total > u16::MAX as usize {
            return Err(TooManyNodes);
        }

        let mut nodes: Vec<Node<V>> = Vec::with_capacity(total);
        // self's nodes, unchanged (cloning refs each view).
        nodes.extend(self.nodes.iter().cloned());
        // insert's nodes, with split handles shifted by `self_len`.
        for node in &insert.nodes {
            nodes.push(match node {
                Node::Leaf(v) => Node::Leaf(Rc::clone(v)),
                Node::Split(s) => Node::Split(Split {
                    left: s.left.offset(self_len),
                    right: s.right.offset(self_len),
                    ..*s
                }),
            });
        }

        // Relocate the `at` node to the end, then replace its slot with the new split.
        let relocated_at = total - 1;
        let inserted_root = self_len;
        nodes.push(nodes[at.idx()].clone());

        let (layout, left) = direction.split_layout();
        nodes[at.idx()] = Node::Split(Split {
            layout,
            ratio,
            left: Handle::from_index(if left { inserted_root } else { relocated_at }),
            right: Handle::from_index(if left { relocated_at } else { inserted_root }),
        });

        Ok(SplitTree {
            nodes,
            zoomed: None, // split always resets zoom
        })
    }

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
                let right = self.remove_node(
                    new_nodes,
                    new_zoomed,
                    new_offset + left + 1,
                    s.right,
                    target,
                );
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

    /// The in-order previous view of `from`, or `None` if it is the first (upstream `previous`).
    fn previous(&self, from: Handle) -> Option<Handle> {
        match self.previous_backtrack(from, Handle::ROOT) {
            Backtrack::Result(v) => Some(v),
            Backtrack::Backtrack | Backtrack::Deadend => None,
        }
    }

    /// The in-order next view of `from`, or `None` if it is the last (upstream `next`).
    fn next(&self, from: Handle) -> Option<Handle> {
        match self.next_backtrack(from, Handle::ROOT) {
            Backtrack::Result(v) => Some(v),
            Backtrack::Backtrack | Backtrack::Deadend => None,
        }
    }

    fn previous_backtrack(&self, from: Handle, current: Handle) -> Backtrack {
        if from == current {
            return Backtrack::Backtrack;
        }
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => Backtrack::Deadend,
            Node::Split(s) => {
                let s = *s;
                match self.previous_backtrack(from, s.left) {
                    Backtrack::Result(v) => Backtrack::Result(v),
                    Backtrack::Backtrack => Backtrack::Backtrack,
                    Backtrack::Deadend => match self.previous_backtrack(from, s.right) {
                        Backtrack::Result(v) => Backtrack::Result(v),
                        Backtrack::Deadend => Backtrack::Deadend,
                        Backtrack::Backtrack => {
                            Backtrack::Result(self.deepest(Side::Right, s.left))
                        }
                    },
                }
            }
        }
    }

    fn next_backtrack(&self, from: Handle, current: Handle) -> Backtrack {
        if from == current {
            return Backtrack::Backtrack;
        }
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => Backtrack::Deadend,
            Node::Split(s) => {
                let s = *s;
                match self.next_backtrack(from, s.right) {
                    Backtrack::Result(v) => Backtrack::Result(v),
                    Backtrack::Backtrack => Backtrack::Backtrack,
                    Backtrack::Deadend => match self.next_backtrack(from, s.left) {
                        Backtrack::Result(v) => Backtrack::Result(v),
                        Backtrack::Deadend => Backtrack::Deadend,
                        Backtrack::Backtrack => {
                            Backtrack::Result(self.deepest(Side::Left, s.right))
                        }
                    },
                }
            }
        }
    }

    /// Resolve a `Goto` to a target handle, or `None` (upstream `goto`).
    pub(crate) fn goto(&self, from: Handle, to: Goto) -> Option<Handle> {
        match to {
            Goto::Previous => self.previous(from),
            Goto::Next => self.next(from),
            Goto::PreviousWrapped => self
                .previous(from)
                .or_else(|| Some(self.deepest(Side::Right, Handle::ROOT))),
            Goto::NextWrapped => self
                .next(from)
                .or_else(|| Some(self.deepest(Side::Left, Handle::ROOT))),
            Goto::Spatial(d) => {
                let sp = self.spatial();
                self.nearest_wrapped(&sp, from, d)
            }
        }
    }

    /// The nearest leaf to `target` in `direction`, or `None` if there is none (upstream
    /// `nearest`). Scans the spatial slots, skipping `from` and non-leaf nodes, and returns the
    /// closest in-direction leaf (ties to the first found).
    fn nearest(
        &self,
        sp: &Spatial,
        from: Handle,
        direction: SpatialDirection,
        target: Slot,
    ) -> Option<Handle> {
        let mut result: Option<(Handle, f16)> = None;
        for (handle, &slot) in sp.slots().iter().enumerate() {
            if handle == from.idx() {
                continue; // never match ourself
            }
            if !matches!(self.nodes[handle], Node::Leaf(_)) {
                continue; // only leaves
            }
            if !slot.is_in_direction(target, direction) {
                continue; // must be in the proper direction
            }
            let distance = slot.distance_to(target);
            if let Some((_, best)) = result {
                if distance >= best {
                    continue; // an existing nearest must be strictly closer
                }
            }
            result = Some((Handle::from_index(handle), distance));
        }
        result.map(|(handle, _)| handle)
    }

    /// Like `nearest`, but wraps around the 1×1 grid if there is no in-direction leaf (upstream
    /// `nearestWrapped`).
    fn nearest_wrapped(
        &self,
        sp: &Spatial,
        from: Handle,
        direction: SpatialDirection,
    ) -> Option<Handle> {
        let target = sp.slots()[from.idx()];
        if let Some(v) = self.nearest(sp, from, direction, target) {
            return Some(v);
        }

        // No in-direction leaf: shift the target one full grid (the wrap) and retry. The grid is
        // normalized to 1×1, so this models wrapping without modifying the representation.
        let zero = f16::from_f32(0.0);
        let one = f16::from_f32(1.0);
        assert!(target.x >= zero && target.y >= zero);
        assert!(target.max_x() <= one && target.max_y() <= one);
        let wrapped = target.wrapped_for(direction);
        self.nearest(sp, from, direction, wrapped)
    }

    /// Recursively fill each child's slot from its parent split's slot (upstream
    /// `fillSpatialSlots`).
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

    /// Relative dimensions of the subtree at `from`, in leaf units (upstream `dimensions`).
    pub(crate) fn dimensions(&self, from: Handle) -> Dimensions {
        match &self.nodes[from.idx()] {
            Node::Leaf(_) => Dimensions {
                width: 1,
                height: 1,
            },
            Node::Split(s) => {
                let left = self.dimensions(s.left);
                let right = self.dimensions(s.right);
                match s.layout {
                    Layout::Horizontal => Dimensions {
                        width: left.width + right.width,
                        height: left.height.max(right.height),
                    },
                    Layout::Vertical => Dimensions {
                        width: left.width.max(right.width),
                        height: left.height + right.height,
                    },
                }
            }
        }
    }
}

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

/// A leaf visited by the tree iterator (upstream `ViewEntry`).
pub(crate) struct ViewEntry<'a, V> {
    pub(crate) handle: Handle,
    pub(crate) view: &'a Rc<V>,
}

/// An iterator over the tree's leaf views, in node-arena order (upstream `Iterator`).
pub(crate) struct Iter<'a, V> {
    nodes: &'a [Node<V>],
    i: usize,
}

impl<'a, V> Iterator for Iter<'a, V> {
    type Item = ViewEntry<'a, V>;

    fn next(&mut self) -> Option<ViewEntry<'a, V>> {
        while self.i < self.nodes.len() {
            let handle = Handle::from_index(self.i);
            self.i += 1;
            if let Node::Leaf(view) = &self.nodes[handle.idx()] {
                return Some(ViewEntry { handle, view });
            }
            // split → skip, advance to the next node (upstream's `self.next()` tail recursion).
        }
        None
    }
}

/// A navigation target for `goto` (upstream `Goto`): the previous / next view (optionally wrapped),
/// or a spatial direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Goto {
    Previous,
    Next,
    PreviousWrapped,
    NextWrapped,
    Spatial(SpatialDirection),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_root_idx_offset() {
        assert_eq!(Handle::ROOT.idx(), 0);
        assert_eq!(Handle::from_index(5).idx(), 5);
        assert_eq!(Handle::ROOT.offset(3).idx(), 3);
        assert_eq!(Handle::from_index(2).offset(4).idx(), 6);
    }

    #[test]
    fn from_index_allows_u16_max() {
        // Upstream's enum can represent u16::MAX (the iterator's end sentinel).
        assert_eq!(
            Handle::from_index(u16::MAX as usize).idx(),
            u16::MAX as usize
        );
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn from_index_above_u16_max_panics() {
        let _ = Handle::from_index(u16::MAX as usize + 1);
    }

    #[test]
    fn offset_just_below_u16_max_succeeds() {
        assert_eq!(
            Handle::ROOT.offset(u16::MAX as usize - 1).idx(),
            u16::MAX as usize - 1
        );
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn offset_reaching_u16_max_panics() {
        let _ = Handle::ROOT.offset(u16::MAX as usize);
    }

    #[test]
    fn split_layout_mapping() {
        assert_eq!(Direction::Left.split_layout(), (Layout::Horizontal, true));
        assert_eq!(Direction::Right.split_layout(), (Layout::Horizontal, false));
        assert_eq!(Direction::Up.split_layout(), (Layout::Vertical, true));
        assert_eq!(Direction::Down.split_layout(), (Layout::Vertical, false));
    }

    #[test]
    fn enum_variants_are_distinct() {
        assert_ne!(Layout::Horizontal, Layout::Vertical);
        assert_ne!(Direction::Left, Direction::Right);
        assert_ne!(Direction::Up, Direction::Down);
        assert_ne!(Direction::Left, Direction::Up);
    }

    #[test]
    fn split_fields_and_equality() {
        let split = Split {
            layout: Layout::Horizontal,
            ratio: f16::from_f32(0.5),
            left: Handle::from_index(1),
            right: Handle::from_index(2),
        };
        assert_eq!(split.layout, Layout::Horizontal);
        assert_eq!(split.left, Handle::from_index(1));
        assert_eq!(split.right, Handle::from_index(2));

        let same = split;
        assert_eq!(split, same);

        let different = Split {
            ratio: f16::from_f32(0.25),
            ..split
        };
        assert_ne!(split, different);
    }

    #[test]
    fn ratio_round_trips_through_f16() {
        let split = Split {
            layout: Layout::Vertical,
            ratio: f16::from_f32(0.5),
            left: Handle::ROOT,
            right: Handle::from_index(1),
        };
        assert_eq!(split.ratio.to_f32(), 0.5);
    }

    #[test]
    fn slot_max_x_and_max_y() {
        // Binary-exact half fractions, so there is no decimal-rounding ambiguity.
        let slot = Slot {
            x: f16::from_f32(0.25),
            y: f16::from_f32(0.125),
            width: f16::from_f32(0.5),
            height: f16::from_f32(0.25),
        };
        assert_eq!(slot.max_x(), f16::from_f32(0.75));
        assert_eq!(slot.max_y(), f16::from_f32(0.375));
        // Compare like-for-like against the explicit half addition.
        assert_eq!(slot.max_y(), f16::from_f32(0.125) + f16::from_f32(0.25));
    }

    /// A square slot of the given size at the given position.
    fn slot(x: f32, y: f32, w: f32, h: f32) -> Slot {
        Slot {
            x: f16::from_f32(x),
            y: f16::from_f32(y),
            width: f16::from_f32(w),
            height: f16::from_f32(h),
        }
    }

    #[test]
    fn is_in_direction_basic() {
        // Target occupies [0.5, 0.75] x [0.5, 0.75].
        let target = slot(0.5, 0.5, 0.25, 0.25);

        let left = slot(0.0, 0.5, 0.25, 0.25); // max_x 0.25 <= 0.5
        assert!(left.is_in_direction(target, SpatialDirection::Left));
        assert!(!left.is_in_direction(target, SpatialDirection::Right));

        let right = slot(0.75, 0.5, 0.25, 0.25); // x 0.75 >= max_x 0.75
        assert!(right.is_in_direction(target, SpatialDirection::Right));
        assert!(!right.is_in_direction(target, SpatialDirection::Left));

        let up = slot(0.5, 0.0, 0.25, 0.25); // max_y 0.25 <= 0.5
        assert!(up.is_in_direction(target, SpatialDirection::Up));
        assert!(!up.is_in_direction(target, SpatialDirection::Down));

        let down = slot(0.5, 0.75, 0.25, 0.25); // y 0.75 >= max_y 0.75
        assert!(down.is_in_direction(target, SpatialDirection::Down));
        assert!(!down.is_in_direction(target, SpatialDirection::Up));

        // The target itself overlaps and is in no direction.
        assert!(!target.is_in_direction(target, SpatialDirection::Left));
        assert!(!target.is_in_direction(target, SpatialDirection::Right));
        assert!(!target.is_in_direction(target, SpatialDirection::Up));
        assert!(!target.is_in_direction(target, SpatialDirection::Down));
    }

    #[test]
    fn is_in_direction_boundary_touch_is_inclusive() {
        let target = slot(0.5, 0.5, 0.25, 0.25);

        // candidate.max_x() == target.x → Left (inclusive `<=`).
        let touch_left = slot(0.25, 0.5, 0.25, 0.25);
        assert_eq!(touch_left.max_x(), target.x);
        assert!(touch_left.is_in_direction(target, SpatialDirection::Left));

        // candidate.x == target.max_x() → Right (inclusive `>=`).
        let touch_right = slot(0.75, 0.5, 0.25, 0.25);
        assert_eq!(touch_right.x, target.max_x());
        assert!(touch_right.is_in_direction(target, SpatialDirection::Right));

        // candidate.max_y() == target.y → Up.
        let touch_up = slot(0.5, 0.25, 0.25, 0.25);
        assert_eq!(touch_up.max_y(), target.y);
        assert!(touch_up.is_in_direction(target, SpatialDirection::Up));

        // candidate.y == target.max_y() → Down.
        let touch_down = slot(0.5, 0.75, 0.25, 0.25);
        assert_eq!(touch_down.y, target.max_y());
        assert!(touch_down.is_in_direction(target, SpatialDirection::Down));
    }

    #[test]
    fn distance_to_uses_euclidean_geometry() {
        let target = slot(0.0, 0.0, 0.0, 0.0);

        // Zero separation.
        assert_eq!(target.distance_to(target), f16::from_f32(0.0));

        // Axis-aligned: dy = 0.5.
        let axis = slot(0.0, 0.5, 0.0, 0.0);
        assert_eq!(axis.distance_to(target), f16::from_f32(0.5));

        // Binary-exact 3-4-5 triangle: 0.75² + 1.0² = 1.5625 = 1.25².
        let diag = slot(0.75, 1.0, 0.0, 0.0);
        assert_eq!(diag.distance_to(target), f16::from_f32(1.25));
    }

    #[test]
    fn wrapped_for_shifts_one_grid() {
        let s = slot(0.25, 0.5, 0.1, 0.2);
        let one = f16::from_f32(1.0);

        let left = s.wrapped_for(SpatialDirection::Left);
        assert_eq!(left.x, s.x + one);
        assert_eq!((left.y, left.width, left.height), (s.y, s.width, s.height));

        let right = s.wrapped_for(SpatialDirection::Right);
        assert_eq!(right.x, s.x - one);
        assert_eq!(right.y, s.y);

        let up = s.wrapped_for(SpatialDirection::Up);
        assert_eq!(up.y, s.y + one);
        assert_eq!(up.x, s.x);

        let down = s.wrapped_for(SpatialDirection::Down);
        assert_eq!(down.y, s.y - one);
        assert_eq!(down.x, s.x);
    }

    // A split node payload for hand-built test trees.
    fn split(layout: Layout, left: usize, right: usize) -> Split {
        Split {
            layout,
            ratio: f16::from_f32(0.5),
            left: Handle::from_index(left),
            right: Handle::from_index(right),
        }
    }

    #[test]
    fn single_leaf_tree_queries() {
        let tree = SplitTree::new(Rc::new("view"));
        assert!(!tree.is_empty());
        assert!(!tree.is_split());
        assert_eq!(tree.deepest(Side::Left, Handle::ROOT), Handle::ROOT);
        assert_eq!(tree.deepest(Side::Right, Handle::ROOT), Handle::ROOT);
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 1,
                height: 1
            }
        );
    }

    #[test]
    fn empty_tree_queries() {
        let tree: SplitTree<&str> = SplitTree::empty();
        assert!(tree.is_empty());
        assert!(!tree.is_split());
    }

    #[test]
    fn horizontal_split_of_two_leaves() {
        // root = H split(left=1, right=2); nodes[1]=leaf, nodes[2]=leaf.
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split(Layout::Horizontal, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        assert!(tree.is_split());
        assert_eq!(
            tree.deepest(Side::Left, Handle::ROOT),
            Handle::from_index(1)
        );
        assert_eq!(
            tree.deepest(Side::Right, Handle::ROOT),
            Handle::from_index(2)
        );
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 2,
                height: 1
            }
        );
    }

    #[test]
    fn vertical_split_dimensions() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split(Layout::Vertical, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 1,
                height: 2
            }
        );
    }

    #[test]
    fn nested_tree_deepest_and_dimensions() {
        // root = H split(left=1, right=4); node1 = V split(left=2, right=3) of two leaves; node4 =
        // leaf. Layout: a 1x2 column on the left, a single leaf on the right → width 2, height 2.
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split(Layout::Horizontal, 1, 4)),
                Node::Split(split(Layout::Vertical, 2, 3)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
                Node::Leaf(Rc::new("c")),
            ],
            zoomed: None,
        };
        assert!(tree.is_split());
        // deepest-left descends root→node1→node2.
        assert_eq!(
            tree.deepest(Side::Left, Handle::ROOT),
            Handle::from_index(2)
        );
        // deepest-right descends root→node4 (a leaf).
        assert_eq!(
            tree.deepest(Side::Right, Handle::ROOT),
            Handle::from_index(4)
        );
        // left column is 1 wide, 2 tall; right leaf is 1x1. Horizontal: width 1+1=2, height
        // max(2,1)=2.
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 2,
                height: 2
            }
        );
    }

    #[test]
    fn clone_ref_counts_the_views() {
        let view = Rc::new("shared");
        let tree = SplitTree::new(Rc::clone(&view));
        // `view` + the leaf's Rc.
        assert_eq!(Rc::strong_count(&view), 2);

        let cloned = tree.clone();
        // Cloning the tree refs the view again (upstream `refNodes`).
        assert_eq!(Rc::strong_count(&view), 3);

        drop(cloned);
        // Dropping the clone unrefs it (upstream `deinit` / `viewUnref`).
        assert_eq!(Rc::strong_count(&view), 2);

        drop(tree);
        assert_eq!(Rc::strong_count(&view), 1);
    }

    /// Collect `(handle index, view)` pairs from an iterator.
    fn collect_views<V: Copy>(tree: &SplitTree<V>) -> Vec<(usize, V)> {
        tree.iter().map(|e| (e.handle.idx(), **e.view)).collect()
    }

    #[test]
    fn iterate_single_leaf() {
        let tree = SplitTree::new(Rc::new("v"));
        assert_eq!(collect_views(&tree), vec![(0, "v")]);
    }

    #[test]
    fn iterate_empty_tree() {
        let tree: SplitTree<&str> = SplitTree::empty();
        assert_eq!(collect_views(&tree), vec![]);
    }

    #[test]
    fn iterate_horizontal_split_skips_the_split() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split(Layout::Horizontal, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        // The split at index 0 is skipped; leaves at 1, 2 visited in order.
        assert_eq!(collect_views(&tree), vec![(1, "a"), (2, "b")]);
    }

    #[test]
    fn iterate_nested_tree_visits_all_leaves_in_arena_order() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split(Layout::Horizontal, 1, 4)),
                Node::Split(split(Layout::Vertical, 2, 3)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
                Node::Leaf(Rc::new("c")),
            ],
            zoomed: None,
        };
        // Splits at 0, 1 skipped; leaves at 2, 3, 4 in arena order.
        assert_eq!(collect_views(&tree), vec![(2, "a"), (3, "b"), (4, "c")]);
    }

    #[test]
    fn zoom_sets_and_clears() {
        let mut tree = SplitTree {
            nodes: vec![
                Node::Split(split(Layout::Horizontal, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        assert_eq!(tree.zoomed(), None);
        tree.zoom(Some(Handle::from_index(2)));
        assert_eq!(tree.zoomed(), Some(Handle::from_index(2)));
        tree.zoom(None);
        assert_eq!(tree.zoomed(), None);
    }

    #[test]
    #[should_panic(expected = "zoom handle out of range")]
    fn zoom_out_of_range_panics() {
        let mut tree = SplitTree::new(Rc::new("v"));
        tree.zoom(Some(Handle::from_index(5)));
    }

    #[test]
    fn goto_variants_are_distinct() {
        assert_ne!(Goto::Previous, Goto::Next);
        assert_ne!(Goto::PreviousWrapped, Goto::NextWrapped);
        assert_ne!(Goto::Next, Goto::Spatial(SpatialDirection::Left));
        assert_ne!(
            Goto::Spatial(SpatialDirection::Left),
            Goto::Spatial(SpatialDirection::Right)
        );
        assert_eq!(
            Goto::Spatial(SpatialDirection::Up),
            Goto::Spatial(SpatialDirection::Up)
        );
    }

    /// A split node payload with an explicit ratio.
    fn split_ratio(layout: Layout, ratio: f32, left: usize, right: usize) -> Split {
        Split {
            layout,
            ratio: f16::from_f32(ratio),
            left: Handle::from_index(left),
            right: Handle::from_index(right),
        }
    }

    fn slot_eq(s: Slot, x: f32, y: f32, w: f32, h: f32) -> bool {
        s.x == f16::from_f32(x)
            && s.y == f16::from_f32(y)
            && s.width == f16::from_f32(w)
            && s.height == f16::from_f32(h)
    }

    #[test]
    fn spatial_single_leaf() {
        let tree = SplitTree::new(Rc::new("v"));
        let sp = tree.spatial();
        assert_eq!(sp.slots().len(), 1);
        assert!(slot_eq(sp.slots()[0], 0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn spatial_empty_tree() {
        let tree: SplitTree<&str> = SplitTree::empty();
        assert!(tree.spatial().slots().is_empty());
    }

    #[test]
    fn spatial_horizontal_split_half() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        let sp = tree.spatial();
        assert!(slot_eq(sp.slots()[0], 0.0, 0.0, 1.0, 1.0)); // root
        assert!(slot_eq(sp.slots()[1], 0.0, 0.0, 0.5, 1.0)); // left leaf
        assert!(slot_eq(sp.slots()[2], 0.5, 0.0, 0.5, 1.0)); // right leaf
    }

    #[test]
    fn spatial_horizontal_split_quarter_is_asymmetric() {
        // ratio 0.25: left gets 0.25 width, right gets 1 - 0.25 = 0.75 (catches a `1 - ratio` bug).
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.25, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        let sp = tree.spatial();
        assert!(slot_eq(sp.slots()[1], 0.0, 0.0, 0.25, 1.0));
        assert!(slot_eq(sp.slots()[2], 0.25, 0.0, 0.75, 1.0));
    }

    #[test]
    fn spatial_vertical_split_half() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Vertical, 0.5, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        let sp = tree.spatial();
        assert!(slot_eq(sp.slots()[1], 0.0, 0.0, 1.0, 0.5)); // top leaf
        assert!(slot_eq(sp.slots()[2], 0.0, 0.5, 1.0, 0.5)); // bottom leaf
    }

    /// A 2x2 grid tree: root is a horizontal split of two vertical columns. Leaves: TL=2, BL=3,
    /// TR=5, BR=6.
    fn grid_2x2() -> SplitTree<&'static str> {
        SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 4)), // 0: root
                Node::Split(split_ratio(Layout::Vertical, 0.5, 2, 3)),   // 1: left column
                Node::Leaf(Rc::new("TL")),                               // 2
                Node::Leaf(Rc::new("BL")),                               // 3
                Node::Split(split_ratio(Layout::Vertical, 0.5, 5, 6)),   // 4: right column
                Node::Leaf(Rc::new("TR")),                               // 5
                Node::Leaf(Rc::new("BR")),                               // 6
            ],
            zoomed: None,
        }
    }

    #[test]
    fn nearest_horizontal_no_wrap() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        let sp = tree.spatial();
        let left = Handle::from_index(1);
        let right = Handle::from_index(2);

        // From the left leaf, Right finds the right leaf; symmetric.
        assert_eq!(
            tree.nearest(&sp, left, SpatialDirection::Right, sp.slots()[1]),
            Some(right)
        );
        assert_eq!(
            tree.nearest(&sp, right, SpatialDirection::Left, sp.slots()[2]),
            Some(left)
        );
        // From the left leaf, Left has no in-direction leaf (no wrap).
        assert_eq!(
            tree.nearest(&sp, left, SpatialDirection::Left, sp.slots()[1]),
            None
        );
    }

    #[test]
    fn nearest_wrapped_horizontal() {
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        let sp = tree.spatial();
        let left = Handle::from_index(1);
        let right = Handle::from_index(2);

        // Left from the leftmost wraps around to the rightmost, and vice versa.
        assert_eq!(
            tree.nearest_wrapped(&sp, left, SpatialDirection::Left),
            Some(right)
        );
        assert_eq!(
            tree.nearest_wrapped(&sp, right, SpatialDirection::Right),
            Some(left)
        );
    }

    #[test]
    fn nearest_wrapped_2x2_grid() {
        let tree = grid_2x2();
        let sp = tree.spatial();
        let tl = Handle::from_index(2);
        let bl = Handle::from_index(3);
        let tr = Handle::from_index(5);
        let br = Handle::from_index(6);

        // Adjacents from the top-left corner.
        assert_eq!(
            tree.nearest_wrapped(&sp, tl, SpatialDirection::Right),
            Some(tr)
        );
        assert_eq!(
            tree.nearest_wrapped(&sp, tl, SpatialDirection::Down),
            Some(bl)
        );
        // Adjacents from the bottom-right corner.
        assert_eq!(
            tree.nearest_wrapped(&sp, br, SpatialDirection::Left),
            Some(bl)
        );
        assert_eq!(
            tree.nearest_wrapped(&sp, br, SpatialDirection::Up),
            Some(tr)
        );
        // Left from the left column wraps to the same row on the right.
        assert_eq!(
            tree.nearest_wrapped(&sp, tl, SpatialDirection::Left),
            Some(tr)
        );
    }

    #[test]
    fn next_and_previous_in_order_traversal() {
        let tree = grid_2x2();
        // In-order leaves: TL=2, BL=3, TR=5, BR=6.
        assert_eq!(
            tree.next(Handle::from_index(2)),
            Some(Handle::from_index(3))
        );
        assert_eq!(
            tree.next(Handle::from_index(3)),
            Some(Handle::from_index(5))
        );
        assert_eq!(
            tree.next(Handle::from_index(5)),
            Some(Handle::from_index(6))
        );
        assert_eq!(tree.next(Handle::from_index(6)), None);

        // previous is the mirror.
        assert_eq!(
            tree.previous(Handle::from_index(6)),
            Some(Handle::from_index(5))
        );
        assert_eq!(
            tree.previous(Handle::from_index(5)),
            Some(Handle::from_index(3))
        );
        assert_eq!(
            tree.previous(Handle::from_index(3)),
            Some(Handle::from_index(2))
        );
        assert_eq!(tree.previous(Handle::from_index(2)), None);
    }

    #[test]
    fn goto_previous_next_and_wrapped() {
        let tree = grid_2x2();
        let first = Handle::from_index(2); // TL
        let last = Handle::from_index(6); // BR

        assert_eq!(tree.goto(first, Goto::Next), Some(Handle::from_index(3)));
        assert_eq!(tree.goto(last, Goto::Next), None);
        // NextWrapped from the last wraps to the first (deepest-left of root).
        assert_eq!(tree.goto(last, Goto::NextWrapped), Some(first));
        // PreviousWrapped from the first wraps to the last (deepest-right of root).
        assert_eq!(tree.goto(first, Goto::PreviousWrapped), Some(last));
    }

    #[test]
    fn goto_spatial_uses_nearest_wrapped() {
        let tree = grid_2x2();
        let tl = Handle::from_index(2);
        assert_eq!(
            tree.goto(tl, Goto::Spatial(SpatialDirection::Right)),
            Some(Handle::from_index(5)) // TR
        );
        assert_eq!(
            tree.goto(tl, Goto::Spatial(SpatialDirection::Down)),
            Some(Handle::from_index(3)) // BL
        );
    }

    fn half() -> f16 {
        f16::from_f32(0.5)
    }

    #[test]
    fn split_single_right_puts_inserted_on_the_right() {
        let a = Rc::new("a");
        let b = Rc::new("b");
        let original = SplitTree::new(Rc::clone(&a));
        let insert = SplitTree::new(Rc::clone(&b));

        let tree = original
            .split(Handle::ROOT, Direction::Right, half(), &insert)
            .unwrap();

        assert!(tree.is_split());
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 2,
                height: 1
            }
        );
        // Arena: split@0, b@1 (inserted), a@2 (relocated). a is the left child, b the right.
        assert_eq!(collect_views(&tree), vec![(1, "b"), (2, "a")]);
        let sp = tree.spatial();
        assert!(slot_eq(sp.slots()[2], 0.0, 0.0, 0.5, 1.0)); // a on the left
        assert!(slot_eq(sp.slots()[1], 0.5, 0.0, 0.5, 1.0)); // b on the right
    }

    #[test]
    fn split_single_left_puts_inserted_on_the_left() {
        let a = Rc::new("a");
        let b = Rc::new("b");
        let original = SplitTree::new(Rc::clone(&a));
        let insert = SplitTree::new(Rc::clone(&b));

        let tree = original
            .split(Handle::ROOT, Direction::Left, half(), &insert)
            .unwrap();

        let sp = tree.spatial();
        assert!(slot_eq(sp.slots()[1], 0.0, 0.0, 0.5, 1.0)); // b on the left
        assert!(slot_eq(sp.slots()[2], 0.5, 0.0, 0.5, 1.0)); // a on the right
    }

    #[test]
    fn split_vertical_down_puts_inserted_below() {
        let a = Rc::new("a");
        let b = Rc::new("b");
        let original = SplitTree::new(Rc::clone(&a));
        let insert = SplitTree::new(Rc::clone(&b));

        let tree = original
            .split(Handle::ROOT, Direction::Down, half(), &insert)
            .unwrap();

        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 1,
                height: 2
            }
        );
        let sp = tree.spatial();
        assert!(slot_eq(sp.slots()[2], 0.0, 0.0, 1.0, 0.5)); // a on top
        assert!(slot_eq(sp.slots()[1], 0.0, 0.5, 1.0, 0.5)); // b below
    }

    #[test]
    fn split_inserting_a_subtree_offsets_its_handles() {
        let a = Rc::new("a");
        let original = SplitTree::new(Rc::clone(&a));
        // A two-leaf horizontal subtree to insert.
        let insert = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::new("b")),
                Node::Leaf(Rc::new("c")),
            ],
            zoomed: None,
        };

        let tree = original
            .split(Handle::ROOT, Direction::Right, half(), &insert)
            .unwrap();

        // a={1,1}; inserted split (b|c)={2,1}; root H={3,1}.
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 3,
                height: 1
            }
        );
        // The inserted split's children were offset by self_len=1: b@2, c@3; a relocated to @4.
        assert_eq!(collect_views(&tree), vec![(2, "b"), (3, "c"), (4, "a")]);
    }

    #[test]
    fn split_at_a_split_node_relocates_the_subtree() {
        // self is a 2-leaf horizontal split.
        let original = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        };
        let insert = SplitTree::new(Rc::new("c"));

        // Split at the root split node, Right.
        let tree = original
            .split(Handle::ROOT, Direction::Right, half(), &insert)
            .unwrap();

        // Relocated original split (a|b) on the left, c on the right.
        // a={1,1},b={1,1} → relocated split {2,1}; c={1,1}; root H = {3,1}.
        assert_eq!(
            tree.dimensions(Handle::ROOT),
            Dimensions {
                width: 3,
                height: 1
            }
        );
        // Arena: new split@0, a@1, b@2 (preserved front), c@3 (inserted), relocated split@4.
        assert_eq!(collect_views(&tree), vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn split_overflowing_the_handle_range_errors() {
        // A tree at the u16 node limit; splitting pushes the count past the handle range.
        let big: SplitTree<u8> = SplitTree {
            nodes: (0..u16::MAX as usize)
                .map(|_| Node::Leaf(Rc::new(0u8)))
                .collect(),
            zoomed: None,
        };
        let insert = SplitTree::new(Rc::new(0u8));
        assert_eq!(
            big.split(Handle::ROOT, Direction::Right, half(), &insert)
                .err(),
            Some(TooManyNodes)
        );
    }

    #[test]
    fn split_ref_counts_both_trees_views() {
        let a = Rc::new("a");
        let b = Rc::new("b");
        let original = SplitTree::new(Rc::clone(&a));
        let insert = SplitTree::new(Rc::clone(&b));
        // Each view: the local Rc + its tree's leaf.
        assert_eq!(Rc::strong_count(&a), 2);
        assert_eq!(Rc::strong_count(&b), 2);

        let tree = original
            .split(Handle::ROOT, Direction::Right, half(), &insert)
            .unwrap();
        // The new tree references each view once more.
        assert_eq!(Rc::strong_count(&a), 3);
        assert_eq!(Rc::strong_count(&b), 3);

        drop(tree);
        assert_eq!(Rc::strong_count(&a), 2);
        assert_eq!(Rc::strong_count(&b), 2);
    }

    /// A two-leaf horizontal split of `a` and `b` (root@0, a@1, b@2).
    fn two_leaf() -> SplitTree<&'static str> {
        SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
            ],
            zoomed: None,
        }
    }

    /// A three-leaf tree: root H split of a vertical `a|b` column and leaf `c`
    /// (root@0, col@1, a@2, b@3, c@4).
    fn three_leaf() -> SplitTree<&'static str> {
        SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 4)),
                Node::Split(split_ratio(Layout::Vertical, 0.5, 2, 3)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
                Node::Leaf(Rc::new("c")),
            ],
            zoomed: None,
        }
    }

    #[test]
    fn remove_leaf_from_two_leaf_split_collapses() {
        let tree = two_leaf();
        // Remove `a` (handle 1): the split collapses to just `b`.
        let result = tree.remove(Handle::from_index(1));
        assert!(!result.is_split());
        assert_eq!(collect_views(&result), vec![(0, "b")]);
    }

    #[test]
    fn remove_leaf_from_three_leaf_tree() {
        let tree = three_leaf();
        // Remove `a` (handle 2): the `a|b` column collapses to `b`; root becomes H(b, c).
        let result = tree.remove(Handle::from_index(2));
        assert!(result.is_split());
        assert_eq!(collect_views(&result), vec![(1, "b"), (2, "c")]);
        assert_eq!(
            result.dimensions(Handle::ROOT),
            Dimensions {
                width: 2,
                height: 1
            }
        );
    }

    #[test]
    fn remove_root_yields_empty() {
        let tree = two_leaf();
        let result = tree.remove(Handle::ROOT);
        assert!(result.is_empty());
    }

    /// The view at a leaf handle (test helper; reads the node arena directly).
    fn view_at<V: Copy>(tree: &SplitTree<V>, handle: Handle) -> V {
        match &tree.nodes[handle.idx()] {
            Node::Leaf(v) => **v,
            Node::Split(_) => panic!("not a leaf"),
        }
    }

    #[test]
    fn remove_migrates_a_surviving_zoom() {
        let mut tree = three_leaf();
        tree.zoom(Some(Handle::from_index(4))); // zoom `c`
                                                // Remove `a` (handle 2): `c` survives, relocating to handle 2.
        let result = tree.remove(Handle::from_index(2));
        assert_eq!(result.zoomed(), Some(Handle::from_index(2)));
        assert_eq!(view_at(&result, result.zoomed().unwrap()), "c");
    }

    #[test]
    fn remove_drops_zoom_when_the_zoomed_node_is_removed() {
        let mut tree = three_leaf();
        tree.zoom(Some(Handle::from_index(2))); // zoom `a`
        let result = tree.remove(Handle::from_index(2)); // remove `a`
        assert_eq!(result.zoomed(), None);
    }

    #[test]
    fn remove_migrates_zoom_on_a_collapsed_parent_split() {
        let mut tree = three_leaf();
        tree.zoom(Some(Handle::from_index(1))); // zoom the `a|b` column (which collapses)
                                                // Remove `a`: the column collapses to `b`; the zoom migrates to `b`'s new handle.
        let result = tree.remove(Handle::from_index(2));
        assert_eq!(result.zoomed(), Some(Handle::from_index(1)));
        assert_eq!(view_at(&result, result.zoomed().unwrap()), "b");
    }

    #[test]
    fn remove_ref_counts_are_immutable_on_self() {
        let a = Rc::new("a");
        let b = Rc::new("b");
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 2)),
                Node::Leaf(Rc::clone(&a)),
                Node::Leaf(Rc::clone(&b)),
            ],
            zoomed: None,
        };
        // Each view: local Rc + the tree's leaf.
        assert_eq!(Rc::strong_count(&a), 2);
        assert_eq!(Rc::strong_count(&b), 2);

        // Remove `a` (handle 1) → new tree holds only `b`.
        let result = tree.remove(Handle::from_index(1));
        // `remove` is immutable: the surviving `b` gains a ref; the removed `a` is unchanged.
        assert_eq!(Rc::strong_count(&b), 3);
        assert_eq!(Rc::strong_count(&a), 2);

        // Dropping the old tree releases its references (including the removed `a`).
        drop(tree);
        assert_eq!(Rc::strong_count(&a), 1);
        assert_eq!(Rc::strong_count(&b), 2);
        drop(result);
        assert_eq!(Rc::strong_count(&b), 1);
    }

    #[test]
    fn spatial_nested_tree() {
        // root = H split(left=1, right=4); node1 = V split(left=2, right=3); 2,3,4 leaves.
        // Left column (1 wide, 2 tall) at the left half; right leaf at the right half.
        let tree = SplitTree {
            nodes: vec![
                Node::Split(split_ratio(Layout::Horizontal, 0.5, 1, 4)),
                Node::Split(split_ratio(Layout::Vertical, 0.5, 2, 3)),
                Node::Leaf(Rc::new("a")),
                Node::Leaf(Rc::new("b")),
                Node::Leaf(Rc::new("c")),
            ],
            zoomed: None,
        };
        let sp = tree.spatial();
        // Left column occupies x∈[0,0.5]; its two leaves stack vertically.
        assert!(slot_eq(sp.slots()[2], 0.0, 0.0, 0.5, 0.5)); // top-left leaf
        assert!(slot_eq(sp.slots()[3], 0.0, 0.5, 0.5, 0.5)); // bottom-left leaf
                                                             // Right leaf occupies x∈[0.5,1], full height.
        assert!(slot_eq(sp.slots()[4], 0.5, 0.0, 0.5, 1.0));
    }
}
