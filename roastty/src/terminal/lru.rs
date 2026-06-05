//! A least-recently-used cache (port of the core of upstream `datastruct/lru` `HashMap`).

use std::collections::HashMap;
use std::hash::Hash;

const NIL: usize = usize::MAX;

struct Node<K, V> {
    key: K,
    value: V,
    prev: usize, // NIL = none
    next: usize, // NIL = none
}

/// The result of `get_or_put_with` (upstream `GetOrPutResult`).
pub(crate) struct GetOrPut<'a, K, V> {
    /// The entry's value (the existing one on a hit, the freshly-inserted one on a miss).
    pub(crate) value: &'a mut V,
    /// Whether the key already existed.
    pub(crate) found_existing: bool,
    /// The entry evicted to make room, if any.
    pub(crate) evicted: Option<(K, V)>,
}

/// A capacity-bounded LRU cache (upstream `datastruct.lru.HashMap`). `head` is the
/// least-recently-used end, `tail` the most-recently-used. A slot is `None` once freed
/// (by `resize`); `free` holds those indices for reuse by a later allocation.
pub(crate) struct Lru<K: Copy + Eq + Hash, V> {
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    map: HashMap<K, usize>,
    head: usize,
    tail: usize,
    capacity: usize,
}

impl<K: Copy + Eq + Hash, V> Lru<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            nodes: Vec::new(),
            free: Vec::new(),
            map: HashMap::new(),
            head: NIL,
            tail: NIL,
            capacity,
        }
    }

    /// Borrow an occupied node (occupied by the map/list invariant).
    fn node(&self, idx: usize) -> &Node<K, V> {
        self.nodes[idx].as_ref().expect("occupied slot")
    }

    /// Mutably borrow an occupied node (occupied by the map/list invariant).
    fn node_mut(&mut self, idx: usize) -> &mut Node<K, V> {
        self.nodes[idx].as_mut().expect("occupied slot")
    }

    /// Get a value without changing recency (upstream `get`).
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key).map(|&idx| &self.node(idx).value)
    }

    /// Get the entry for `key`, or insert it (computing the value with `make_value` only on a
    /// miss), evicting the least-recently-used entry if at capacity (upstream `getOrPut`).
    pub(crate) fn get_or_put_with<F: FnOnce() -> V>(
        &mut self,
        key: K,
        make_value: F,
    ) -> GetOrPut<'_, K, V> {
        if let Some(&idx) = self.map.get(&key) {
            self.bump(idx); // move to most-recently-used
            return GetOrPut {
                value: &mut self.node_mut(idx).value,
                found_existing: true,
                evicted: None,
            };
        }

        // Compute the value up front (before mutating the cache) so a panic in `make_value`
        // can't leave the structure half-evicted; upstream similarly only writes the value
        // after `getOrPut` returns.
        let value = make_value();
        let at_capacity = self.map.len() >= self.capacity;
        let (idx, evicted) = if at_capacity {
            // Reuse the least-recently-used node's slot.
            let lru = self.head;
            self.unlink(lru);
            let old_key = self.node(lru).key;
            self.map.remove(&old_key);
            let old = self.nodes[lru]
                .replace(Node {
                    key,
                    value,
                    prev: NIL,
                    next: NIL,
                })
                .expect("occupied slot");
            // Return the genuinely-evicted entry `(old.key, old.value)`. NOTE: upstream's
            // `evicted.key` is actually the *new* key (it assigns `entry.data.key = key` before
            // forming `evicted`), so only `evicted.value` is meaningful upstream — and that's all
            // its tests/consumers use. roastty intentionally corrects the key to the evicted one;
            // the value (the meaningful field) is identical.
            (lru, Some((old.key, old.value)))
        } else {
            // Reuse a freed slot if one is available, else allocate a new node.
            let node = Node {
                key,
                value,
                prev: NIL,
                next: NIL,
            };
            let idx = match self.free.pop() {
                Some(idx) => {
                    self.nodes[idx] = Some(node);
                    idx
                }
                None => {
                    self.nodes.push(Some(node));
                    self.nodes.len() - 1
                }
            };
            (idx, None)
        };

        self.link_tail(idx);
        self.map.insert(key, idx);
        GetOrPut {
            value: &mut self.node_mut(idx).value,
            found_existing: false,
            evicted,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    /// Resize the cache. Growing (or shrinking to >= the current count) just updates the
    /// capacity and returns `None`; shrinking below the count evicts the least-recently-used
    /// entries (oldest first) and returns their values (upstream `resize`).
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

    /// Unlink a node from the LRU list (helper).
    fn unlink(&mut self, idx: usize) {
        let prev = self.node(idx).prev;
        let next = self.node(idx).next;
        if prev != NIL {
            self.node_mut(prev).next = next;
        } else {
            self.head = next;
        }
        if next != NIL {
            self.node_mut(next).prev = prev;
        } else {
            self.tail = prev;
        }
        let node = self.node_mut(idx);
        node.prev = NIL;
        node.next = NIL;
    }

    /// Append a node at the most-recently-used end (helper).
    fn link_tail(&mut self, idx: usize) {
        let tail = self.tail;
        let node = self.node_mut(idx);
        node.prev = tail;
        node.next = NIL;
        if tail != NIL {
            self.node_mut(tail).next = idx;
        } else {
            self.head = idx;
        }
        self.tail = idx;
    }

    /// Move a node to the most-recently-used end (helper).
    fn bump(&mut self, idx: usize) {
        self.unlink(idx);
        self.link_tail(idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_put_evicts_least_recently_used() {
        let mut lru: Lru<u32, u8> = Lru::new(2);

        // Two misses fill the cache.
        let r = lru.get_or_put_with(1, || 1);
        assert!(!r.found_existing);
        assert!(r.evicted.is_none());
        let r = lru.get_or_put_with(2, || 2);
        assert!(!r.found_existing);
        assert!(r.evicted.is_none());

        // Hits bump recency: now order is 1 (LRU), 2 -> get 1 -> 2 (LRU), 1; get 2 -> 1, 2.
        assert!(
            lru.get_or_put_with(1, || panic!("hit must not compute"))
                .found_existing
        );
        assert!(
            lru.get_or_put_with(2, || panic!("hit must not compute"))
                .found_existing
        );

        // A miss at capacity evicts the LRU (key 1, value 1).
        let r = lru.get_or_put_with(3, || 3);
        assert!(!r.found_existing);
        assert_eq!(r.evicted, Some((1, 1)));

        // Order is now 2 (LRU), 3; bump 2 -> 3 (LRU), 2.
        assert!(
            lru.get_or_put_with(2, || panic!("hit must not compute"))
                .found_existing
        );

        // Next miss evicts 3.
        let r = lru.get_or_put_with(4, || 4);
        assert!(!r.found_existing);
        assert_eq!(r.evicted, Some((3, 3)));
    }

    #[test]
    fn get_reads_without_changing_recency() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        lru.get_or_put_with(1, || 1);
        lru.get_or_put_with(2, || 2);

        assert_eq!(lru.get(&1), Some(&1));
        assert_eq!(lru.get(&2), Some(&2));
        assert_eq!(lru.get(&3), None);

        // `get` did not bump 1, so 1 is still the LRU and is the one evicted.
        let r = lru.get_or_put_with(3, || 3);
        assert_eq!(r.evicted, Some((1, 1)));
        assert_eq!(lru.get(&1), None);
    }

    #[test]
    fn returned_value_is_writable() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        let r = lru.get_or_put_with(1, || 10);
        assert_eq!(*r.value, 10);
        *r.value = 42;
        assert_eq!(lru.get(&1), Some(&42));
    }

    #[test]
    fn len_and_capacity() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        assert_eq!(lru.capacity(), 2);
        assert_eq!(lru.len(), 0);
        lru.get_or_put_with(1, || 1);
        assert_eq!(lru.len(), 1);
        lru.get_or_put_with(2, || 2);
        lru.get_or_put_with(3, || 3); // evicts, len stays at capacity
        assert_eq!(lru.len(), 2);
    }

    #[test]
    fn resize_grow_keeps_entries() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        lru.get_or_put_with(1, || 1);
        lru.get_or_put_with(2, || 2);

        assert!(lru.resize(5).is_none());
        assert_eq!(lru.capacity(), 5);
        assert_eq!(lru.get(&1), Some(&1));
        assert_eq!(lru.get(&2), Some(&2));
        assert_eq!(lru.len(), 2);
    }

    #[test]
    fn resize_shrink_without_removal() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        lru.get_or_put_with(1, || 1);

        // Shrinking to a capacity >= the current count removes nothing.
        assert!(lru.resize(1).is_none());
        assert_eq!(lru.capacity(), 1);
        assert_eq!(lru.get(&1), Some(&1));
        assert_eq!(lru.len(), 1);
    }

    #[test]
    fn resize_shrink_evicts_least_recently_used() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        lru.get_or_put_with(1, || 1); // 1 is LRU
        lru.get_or_put_with(2, || 2); // 2 is MRU

        // Shrinking below the count evicts the LRU (key 1) and returns its value.
        assert_eq!(lru.resize(1), Some(vec![1]));
        assert_eq!(lru.capacity(), 1);
        assert_eq!(lru.len(), 1);
        assert_eq!(lru.get(&1), None);
        assert_eq!(lru.get(&2), Some(&2));

        // The survivor 2 is now the LRU; the next miss evicts it.
        let r = lru.get_or_put_with(3, || 3);
        assert_eq!(r.evicted, Some((2, 2)));
    }

    #[test]
    fn resize_evicts_oldest_first() {
        let mut lru: Lru<u32, u8> = Lru::new(3);
        lru.get_or_put_with(1, || 1); // LRU
        lru.get_or_put_with(2, || 2);
        lru.get_or_put_with(3, || 3); // MRU

        // Evict the two least-recently-used, oldest first: [1, 2].
        assert_eq!(lru.resize(1), Some(vec![1, 2]));
        assert_eq!(lru.len(), 1);
        assert_eq!(lru.get(&3), Some(&3));
    }

    #[test]
    fn free_slot_is_reused_after_resize() {
        let mut lru: Lru<u32, u8> = Lru::new(2);
        lru.get_or_put_with(1, || 1);
        lru.get_or_put_with(2, || 2);

        // Free a slot by shrinking, then grow and insert: the freed slot is reused without
        // corrupting the LRU order.
        assert_eq!(lru.resize(1), Some(vec![1]));
        assert!(lru.resize(2).is_none());

        let r = lru.get_or_put_with(3, || 3);
        assert!(!r.found_existing);
        assert!(r.evicted.is_none());
        assert_eq!(lru.get(&2), Some(&2));
        assert_eq!(lru.get(&3), Some(&3));
        assert_eq!(lru.len(), 2);

        // Order is 2 (LRU), 3 (MRU); the next miss at capacity evicts 2.
        let r = lru.get_or_put_with(4, || 4);
        assert_eq!(r.evicted, Some((2, 2)));
    }
}
