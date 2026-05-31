use std::mem::{align_of, size_of};

use super::size::{BaseAddress, CellCountInt, Offset, OffsetBuf};

pub(super) type Id = CellCountInt;
pub(super) type RefCount = CellCountInt;

const LOAD_FACTOR_NUMERATOR: usize = 13;
const LOAD_FACTOR_DENOMINATOR: usize = 16;
const PSL_STATS_LEN: usize = 32;

pub(super) trait Context<T: Copy> {
    fn hash(&self, value: T) -> u64;

    fn eql(&self, candidate: T, resident: T) -> bool;

    fn deleted(&mut self, _value: T) {}
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Metadata {
    bucket: Id,
    psl: Id,
    ref_count: RefCount,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            bucket: Id::MAX,
            psl: 0,
            ref_count: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Item<T: Copy> {
    value: T,
    meta: Metadata,
}

impl<T: Copy + Default> Default for Item<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            meta: Metadata::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) cap: usize,
    pub(super) table_cap: usize,
    pub(super) table_mask: Id,
    pub(super) table_start: usize,
    pub(super) items_start: usize,
    pub(super) total_size: usize,
}

impl Layout {
    pub(super) fn init<T: Copy>(capacity: usize) -> Self {
        assert!(capacity <= Id::MAX as usize + 1);

        if capacity == 0 {
            return Self {
                cap: 0,
                table_cap: 0,
                table_mask: 0,
                table_start: 0,
                items_start: 0,
                total_size: 0,
            };
        }

        let table_cap = capacity.next_power_of_two();
        let items_cap = table_cap * LOAD_FACTOR_NUMERATOR / LOAD_FACTOR_DENOMINATOR;
        let table_mask = Id::try_from(table_cap - 1).expect("table mask must fit Id");
        let table_start = 0;
        let table_end = table_start + table_cap * size_of::<Id>();
        let items_start = align_forward(table_end, align_of::<Item<T>>());
        let items_end = items_start + items_cap * size_of::<Item<T>>();

        Self {
            cap: items_cap,
            table_cap,
            table_mask,
            table_start,
            items_start,
            total_size: items_end,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AddError {
    OutOfMemory,
    NeedsRehash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RefCountedSet<T: Copy> {
    table: Offset<Id>,
    max_psl: Id,
    psl_stats: [Id; PSL_STATS_LEN],
    items: Offset<Item<T>>,
    living: usize,
    next_id: Id,
    layout: Layout,
}

impl<T: Copy + Default> RefCountedSet<T> {
    pub(super) const LOAD_FACTOR_NUMERATOR: usize = LOAD_FACTOR_NUMERATOR;
    pub(super) const LOAD_FACTOR_DENOMINATOR: usize = LOAD_FACTOR_DENOMINATOR;
    pub(super) const BASE_ALIGN: usize = {
        let layout_align = align_of::<Layout>();
        let item_align = align_of::<Item<T>>();
        let id_align = align_of::<Id>();
        if layout_align >= item_align && layout_align >= id_align {
            layout_align
        } else if item_align >= id_align {
            item_align
        } else {
            id_align
        }
    };

    pub(super) fn capacity_for_count(count: usize) -> usize {
        if count == 0 {
            return 0;
        }

        ((count + 1) * LOAD_FACTOR_DENOMINATOR).div_ceil(LOAD_FACTOR_NUMERATOR)
    }

    pub(super) fn layout(capacity: usize) -> Layout {
        Layout::init::<T>(capacity)
    }

    pub(super) fn init<B>(base: B, layout: Layout) -> Self
    where
        B: BaseAddress + Copy,
    {
        let table = OffsetBuf::init(base).member::<Id>(layout.table_start);
        let items = OffsetBuf::init(base).member::<Item<T>>(layout.items_start);
        let set = Self {
            table,
            max_psl: 0,
            psl_stats: [0; PSL_STATS_LEN],
            items,
            living: 0,
            next_id: 1,
            layout,
        };

        if layout.table_cap > 0 {
            set.table_mut(base).fill(0);
        }
        if layout.cap > 0 {
            set.items_mut(base).fill(Item::default());
        }

        set
    }

    pub(super) fn add<B, C>(&mut self, base: B, value: T, context: &mut C) -> Result<Id, AddError>
    where
        B: BaseAddress + Copy,
        C: Context<T>,
    {
        while self.next_id > 1 && self.item_ref_count(base, self.next_id - 1) == 0 {
            self.next_id -= 1;
            self.delete_item(base, self.next_id, context);
        }

        if let Some(id) = self.lookup(base, value, context) {
            context.deleted(value);
            self.add_ref(base, id, 1);
            return Ok(id);
        }

        if self.psl_stats[PSL_STATS_LEN - 1] > 0 {
            return Err(AddError::OutOfMemory);
        }

        if self.next_id as usize >= self.layout.cap {
            let rehash_threshold = self.layout.cap * 9 / 10;
            if self.living < rehash_threshold {
                return Err(AddError::NeedsRehash);
            }

            return Err(AddError::OutOfMemory);
        }

        let id = self.insert(base, value, self.next_id, context);
        self.add_ref(base, id, 1);
        assert_eq!(self.item_ref_count(base, id), 1);
        self.living += 1;

        if id == self.next_id {
            self.next_id += 1;
        }

        Ok(id)
    }

    pub(super) fn add_with_id<B, C>(
        &mut self,
        base: B,
        value: T,
        id: Id,
        context: &mut C,
    ) -> Result<Option<Id>, AddError>
    where
        B: BaseAddress + Copy,
        C: Context<T>,
    {
        assert!(id > 0);

        if id < self.next_id {
            if self.item_ref_count(base, id) == 0 {
                if self.psl_stats[PSL_STATS_LEN - 1] > 0 {
                    return Err(AddError::OutOfMemory);
                }

                self.delete_item(base, id, context);
                let added_id = self.upsert(base, value, id, context);
                self.add_ref(base, added_id, 1);
                self.living += 1;

                return Ok(if added_id == id { None } else { Some(added_id) });
            }

            let resident = self.item_value(base, id);
            if context.eql(value, resident) {
                context.deleted(value);
                self.add_ref(base, id, 1);
                return Ok(None);
            }
        }

        self.add(base, value, context).map(Some)
    }

    pub(super) fn use_one<B>(&self, base: B, id: Id)
    where
        B: BaseAddress + Copy,
    {
        assert!(id > 0);
        assert!((id as usize) < self.layout.cap);
        assert!(self.item_ref_count(base, id) > 0);
        self.add_ref(base, id, 1);
    }

    pub(super) fn use_multiple<B>(&self, base: B, id: Id, count: RefCount)
    where
        B: BaseAddress + Copy,
    {
        assert!(id > 0);
        assert!((id as usize) < self.layout.cap);
        assert!(self.item_ref_count(base, id) > 0);
        self.add_ref(base, id, count);
    }

    pub(super) fn get<B>(&self, base: B, id: Id) -> &T
    where
        B: BaseAddress + Copy,
    {
        assert!(id > 0);
        assert!((id as usize) < self.layout.cap);
        assert!(self.item_ref_count(base, id) > 0);
        &self.items(base)[id as usize].value
    }

    pub(super) fn release<B>(&mut self, base: B, id: Id)
    where
        B: BaseAddress + Copy,
    {
        self.release_multiple(base, id, 1);
    }

    pub(super) fn release_multiple<B>(&mut self, base: B, id: Id, count: RefCount)
    where
        B: BaseAddress + Copy,
    {
        assert!(id > 0);
        assert!((id as usize) < self.layout.cap);
        assert!(self.item_ref_count(base, id) >= count);

        let item = &mut self.items_mut(base)[id as usize];
        item.meta.ref_count -= count;
        if item.meta.ref_count == 0 {
            self.living -= 1;
        }
    }

    pub(super) fn ref_count<B>(&self, base: B, id: Id) -> RefCount
    where
        B: BaseAddress + Copy,
    {
        assert!(id > 0);
        assert!((id as usize) < self.layout.cap);
        self.item_ref_count(base, id)
    }

    pub(super) const fn count(&self) -> usize {
        self.living
    }

    pub(super) fn lookup<B, C>(&self, base: B, value: T, context: &C) -> Option<Id>
    where
        B: BaseAddress + Copy,
        C: Context<T>,
    {
        if self.layout.table_cap == 0 {
            return None;
        }

        let hash = context.hash(value);
        let table = self.table(base);
        let items = self.items(base);

        for i in 0..=self.max_psl {
            let bucket = self.bucket(hash, i);
            let id = table[bucket];

            if id == 0 {
                return None;
            }

            let item = items[id as usize];
            if item.meta.psl < i {
                return None;
            }

            if item.meta.psl == i && item.meta.ref_count > 0 && context.eql(value, item.value) {
                return Some(id);
            }
        }

        None
    }

    fn upsert<B, C>(&mut self, base: B, value: T, new_id: Id, context: &mut C) -> Id
    where
        B: BaseAddress + Copy,
        C: Context<T>,
    {
        if let Some(id) = self.lookup(base, value, context) {
            context.deleted(value);
            return id;
        }

        self.insert(base, value, new_id, context)
    }

    fn insert<B, C>(&mut self, base: B, value: T, new_id: Id, context: &mut C) -> Id
    where
        B: BaseAddress + Copy,
        C: Context<T>,
    {
        debug_assert_eq!(self.lookup(base, value, context), None);

        let hash = context.hash(value);
        let mut new_item = Item {
            value,
            meta: Metadata {
                psl: 0,
                ref_count: 0,
                ..Metadata::default()
            },
        };
        let mut held_id = new_id;
        let mut held_item = new_item;
        let mut chosen_id = new_id;

        for i in 0..self.layout.table_cap.saturating_sub(1) {
            let bucket = self.bucket(hash, i as Id);
            let id = self.table(base)[bucket];

            if id == 0 {
                self.place_held_item(base, bucket, held_id, &mut held_item, new_id, &mut new_item);
                break;
            }

            let resident = self.items(base)[id as usize];
            if resident.meta.ref_count == 0 {
                context.deleted(resident.value);

                self.psl_stats[resident.meta.psl as usize] -= 1;
                self.items_mut(base)[id as usize] = Item::default();

                if id < new_id {
                    chosen_id = id;
                }

                self.place_held_item(base, bucket, held_id, &mut held_item, new_id, &mut new_item);
                break;
            }

            if resident.meta.psl < held_item.meta.psl
                || (resident.meta.psl == held_item.meta.psl
                    && resident.meta.ref_count < held_item.meta.ref_count)
            {
                self.place_held_item(base, bucket, held_id, &mut held_item, new_id, &mut new_item);
                held_id = id;
                held_item = resident;
                self.psl_stats[resident.meta.psl as usize] -= 1;
            }

            held_item.meta.psl += 1;
        }

        self.table_mut(base)[new_item.meta.bucket as usize] = chosen_id;
        self.items_mut(base)[chosen_id as usize] = new_item;

        chosen_id
    }

    fn place_held_item<B>(
        &mut self,
        base: B,
        bucket: usize,
        held_id: Id,
        held_item: &mut Item<T>,
        new_id: Id,
        new_item: &mut Item<T>,
    ) where
        B: BaseAddress + Copy,
    {
        self.table_mut(base)[bucket] = held_id;
        held_item.meta.bucket = bucket as Id;
        self.psl_stats[held_item.meta.psl as usize] += 1;
        self.max_psl = self.max_psl.max(held_item.meta.psl);

        if held_id == new_id {
            *new_item = *held_item;
        } else {
            self.items_mut(base)[held_id as usize] = *held_item;
        }
    }

    fn delete_item<B, C>(&mut self, base: B, id: Id, context: &mut C)
    where
        B: BaseAddress + Copy,
        C: Context<T>,
    {
        let item = self.items(base)[id as usize];
        if item.meta.bucket as usize >= self.layout.table_cap {
            return;
        }

        assert_eq!(self.table(base)[item.meta.bucket as usize], id);
        context.deleted(item.value);

        self.psl_stats[item.meta.psl as usize] -= 1;
        self.table_mut(base)[item.meta.bucket as usize] = 0;
        self.items_mut(base)[id as usize] = Item::default();

        let mut previous = item.meta.bucket;
        let mut next = (previous.wrapping_add(1)) & self.layout.table_mask;

        while self.table(base)[next as usize] != 0
            && self.items(base)[self.table(base)[next as usize] as usize]
                .meta
                .psl
                > 0
        {
            let shifted_id = self.table(base)[next as usize];
            let shifted_psl = self.items(base)[shifted_id as usize].meta.psl;

            self.items_mut(base)[shifted_id as usize].meta.bucket = previous;
            self.psl_stats[shifted_psl as usize] -= 1;
            self.items_mut(base)[shifted_id as usize].meta.psl -= 1;
            let new_psl = self.items(base)[shifted_id as usize].meta.psl;
            self.psl_stats[new_psl as usize] += 1;
            self.table_mut(base)[previous as usize] = shifted_id;

            previous = next;
            next = (previous.wrapping_add(1)) & self.layout.table_mask;
        }

        while self.max_psl > 0 && self.psl_stats[self.max_psl as usize] == 0 {
            self.max_psl -= 1;
        }

        self.table_mut(base)[previous as usize] = 0;
    }

    fn add_ref<B>(&self, base: B, id: Id, count: RefCount)
    where
        B: BaseAddress + Copy,
    {
        let item = &mut self.items_mut(base)[id as usize];
        item.meta.ref_count += count;
    }

    fn item_ref_count<B>(&self, base: B, id: Id) -> RefCount
    where
        B: BaseAddress + Copy,
    {
        self.items(base)[id as usize].meta.ref_count
    }

    fn item_value<B>(&self, base: B, id: Id) -> T
    where
        B: BaseAddress + Copy,
    {
        self.items(base)[id as usize].value
    }

    fn bucket(&self, hash: u64, offset: Id) -> usize {
        ((hash.wrapping_add(offset as u64) as Id) & self.layout.table_mask) as usize
    }

    fn table<B>(&self, base: B) -> &[Id]
    where
        B: BaseAddress + Copy,
    {
        unsafe {
            // Safety: `init` stores an offset into a caller-provided backing
            // buffer whose valid extent is described by `layout.total_size`.
            std::slice::from_raw_parts(self.table.ptr(base), self.layout.table_cap)
        }
    }

    fn table_mut<B>(&self, base: B) -> &mut [Id]
    where
        B: BaseAddress + Copy,
    {
        unsafe {
            // Safety: mutable operations require unique access to the set and
            // its backing memory. The table offset and capacity come from the
            // layout used to initialize the set.
            std::slice::from_raw_parts_mut(self.table.ptr_mut(base), self.layout.table_cap)
        }
    }

    fn items<B>(&self, base: B) -> &[Item<T>]
    where
        B: BaseAddress + Copy,
    {
        unsafe {
            // Safety: `items` points inside the initialized backing memory, and
            // `layout.cap` is the number of initialized item slots.
            std::slice::from_raw_parts(self.items.ptr(base), self.layout.cap)
        }
    }

    fn items_mut<B>(&self, base: B) -> &mut [Item<T>]
    where
        B: BaseAddress + Copy,
    {
        unsafe {
            // Safety: mutable operations require unique access to the set and
            // its backing memory. The item offset and capacity come from the
            // layout used to initialize the set.
            std::slice::from_raw_parts_mut(self.items.ptr_mut(base), self.layout.cap)
        }
    }
}

const fn align_forward(value: usize, alignment: usize) -> usize {
    assert!(alignment.is_power_of_two());
    (value + alignment - 1) & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    struct TestValue {
        value: u32,
    }

    #[derive(Debug, Default)]
    struct TestContext {
        deleted: Rc<RefCell<Vec<TestValue>>>,
        constant_hash: bool,
    }

    impl TestContext {
        fn new() -> Self {
            Self::default()
        }

        fn with_constant_hash() -> Self {
            Self {
                constant_hash: true,
                ..Self::default()
            }
        }

        fn deleted_values(&self) -> Vec<TestValue> {
            self.deleted.borrow().clone()
        }
    }

    impl Context<TestValue> for TestContext {
        fn hash(&self, value: TestValue) -> u64 {
            if self.constant_hash {
                0
            } else {
                value.value as u64
            }
        }

        fn eql(&self, candidate: TestValue, resident: TestValue) -> bool {
            candidate == resident
        }

        fn deleted(&mut self, value: TestValue) {
            self.deleted.borrow_mut().push(value);
        }
    }

    fn value(value: u32) -> TestValue {
        TestValue { value }
    }

    fn init_set(capacity: usize) -> (Vec<u8>, RefCountedSet<TestValue>) {
        let layout = RefCountedSet::<TestValue>::layout(capacity);
        let mut backing = vec![0_u8; layout.total_size];
        let set = RefCountedSet::init(backing.as_mut_ptr(), layout);
        (backing, set)
    }

    #[test]
    fn layout_zero_capacity() {
        let layout = RefCountedSet::<TestValue>::layout(0);

        assert_eq!(layout.cap, 0);
        assert_eq!(layout.table_cap, 0);
        assert_eq!(layout.table_mask, 0);
        assert_eq!(layout.table_start, 0);
        assert_eq!(layout.items_start, 0);
        assert_eq!(layout.total_size, 0);
    }

    #[test]
    fn layout_normal_capacity() {
        let layout = RefCountedSet::<TestValue>::layout(16);

        assert_eq!(RefCountedSet::<TestValue>::BASE_ALIGN, 8);
        assert_eq!(size_of::<Metadata>(), 6);
        assert_eq!(align_of::<Metadata>(), 2);
        assert_eq!(size_of::<Item<TestValue>>(), 12);
        assert_eq!(align_of::<Item<TestValue>>(), 4);
        assert_eq!(layout.cap, 13);
        assert_eq!(layout.table_cap, 16);
        assert_eq!(layout.table_mask, 15);
        assert_eq!(layout.table_start, 0);
        assert_eq!(layout.items_start, 32);
        assert_eq!(layout.total_size, 188);
    }

    #[test]
    fn capacity_for_count() {
        assert_eq!(RefCountedSet::<TestValue>::capacity_for_count(0), 0);
        assert_eq!(RefCountedSet::<TestValue>::capacity_for_count(1), 3);
        assert_eq!(RefCountedSet::<TestValue>::capacity_for_count(12), 16);
    }

    #[test]
    fn add_new_item() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let id = set
            .add(backing.as_mut_ptr(), value(10), &mut context)
            .unwrap();

        assert_ne!(id, 0);
        assert_eq!(set.count(), 1);
        assert_eq!(set.ref_count(backing.as_mut_ptr(), id), 1);
        assert_eq!(*set.get(backing.as_mut_ptr(), id), value(10));
        assert_eq!(
            set.lookup(backing.as_mut_ptr(), value(10), &context),
            Some(id)
        );
    }

    #[test]
    fn add_duplicate_reuses_id_and_deletes_incoming() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let id1 = set
            .add(backing.as_mut_ptr(), value(10), &mut context)
            .unwrap();
        let id2 = set
            .add(backing.as_mut_ptr(), value(10), &mut context)
            .unwrap();

        assert_eq!(id1, id2);
        assert_eq!(set.count(), 1);
        assert_eq!(set.ref_count(backing.as_mut_ptr(), id1), 2);
        assert_eq!(context.deleted_values(), vec![value(10)]);
    }

    #[test]
    fn lookup_returns_only_living_items() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let id = set
            .add(backing.as_mut_ptr(), value(10), &mut context)
            .unwrap();
        set.release(backing.as_mut_ptr(), id);

        assert_eq!(set.count(), 0);
        assert_eq!(set.lookup(backing.as_mut_ptr(), value(10), &context), None);
    }

    #[test]
    fn use_and_release_multiple_update_ref_counts() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let id = set
            .add(backing.as_mut_ptr(), value(10), &mut context)
            .unwrap();

        set.use_one(backing.as_mut_ptr(), id);
        set.use_multiple(backing.as_mut_ptr(), id, 3);
        assert_eq!(set.ref_count(backing.as_mut_ptr(), id), 5);

        set.release_multiple(backing.as_mut_ptr(), id, 4);
        assert_eq!(set.ref_count(backing.as_mut_ptr(), id), 1);
        assert_eq!(set.count(), 1);

        set.release(backing.as_mut_ptr(), id);
        assert_eq!(set.ref_count(backing.as_mut_ptr(), id), 0);
        assert_eq!(set.count(), 0);
    }

    #[test]
    fn dead_item_can_be_reused() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::with_constant_hash();
        let id1 = set
            .add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();
        let _id2 = set
            .add(backing.as_mut_ptr(), value(2), &mut context)
            .unwrap();
        set.release(backing.as_mut_ptr(), id1);

        let id3 = set
            .add(backing.as_mut_ptr(), value(3), &mut context)
            .unwrap();

        assert_eq!(id3, id1);
        assert_eq!(*set.get(backing.as_mut_ptr(), id3), value(3));
        assert_eq!(set.count(), 2);
        assert!(context.deleted_values().contains(&value(1)));
    }

    #[test]
    fn add_with_id_uses_requested_dead_id() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let id = set
            .add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();
        set.release(backing.as_mut_ptr(), id);

        let result = set
            .add_with_id(backing.as_mut_ptr(), value(2), id, &mut context)
            .unwrap();

        assert_eq!(result, None);
        assert_eq!(*set.get(backing.as_mut_ptr(), id), value(2));
    }

    #[test]
    fn add_with_id_returns_alternate_id() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::with_constant_hash();
        let id1 = set
            .add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();
        let _id2 = set
            .add(backing.as_mut_ptr(), value(2), &mut context)
            .unwrap();
        let id3 = set
            .add(backing.as_mut_ptr(), value(3), &mut context)
            .unwrap();
        set.release(backing.as_mut_ptr(), id1);
        set.release(backing.as_mut_ptr(), id3);

        let result = set
            .add_with_id(backing.as_mut_ptr(), value(4), id3, &mut context)
            .unwrap();

        assert_eq!(result, Some(id1));
        assert_eq!(*set.get(backing.as_mut_ptr(), id1), value(4));
    }

    #[test]
    fn add_with_id_existing_value_reuses_requested_id() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let id = set
            .add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();

        let result = set
            .add_with_id(backing.as_mut_ptr(), value(1), id, &mut context)
            .unwrap();

        assert_eq!(result, None);
        assert_eq!(set.ref_count(backing.as_mut_ptr(), id), 2);
        assert_eq!(context.deleted_values(), vec![value(1)]);
    }

    #[test]
    fn add_returns_out_of_memory_when_full() {
        let (mut backing, mut set) = init_set(4);
        let mut context = TestContext::new();
        set.add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();
        set.add(backing.as_mut_ptr(), value(2), &mut context)
            .unwrap();

        assert_eq!(
            set.add(backing.as_mut_ptr(), value(3), &mut context),
            Err(AddError::OutOfMemory)
        );
    }

    #[test]
    fn add_returns_needs_rehash_with_many_dead_ids() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let ids: Vec<_> = (0..12)
            .map(|idx| {
                set.add(backing.as_mut_ptr(), value(idx), &mut context)
                    .unwrap()
            })
            .collect();

        for id in ids.iter().take(5) {
            set.release(backing.as_mut_ptr(), *id);
        }

        assert_eq!(
            set.add(backing.as_mut_ptr(), value(99), &mut context),
            Err(AddError::NeedsRehash)
        );
    }

    #[test]
    fn dead_resident_delete_callback_fires_when_overwritten() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::with_constant_hash();
        let id1 = set
            .add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();
        let _id2 = set
            .add(backing.as_mut_ptr(), value(2), &mut context)
            .unwrap();
        set.release(backing.as_mut_ptr(), id1);

        set.add(backing.as_mut_ptr(), value(3), &mut context)
            .unwrap();

        assert!(context.deleted_values().contains(&value(1)));
    }

    #[test]
    fn dead_trailing_delete_callback_fires_when_trimmed() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::new();
        let _id1 = set
            .add(backing.as_mut_ptr(), value(1), &mut context)
            .unwrap();
        let id2 = set
            .add(backing.as_mut_ptr(), value(2), &mut context)
            .unwrap();
        set.release(backing.as_mut_ptr(), id2);

        set.add(backing.as_mut_ptr(), value(3), &mut context)
            .unwrap();

        assert!(context.deleted_values().contains(&value(2)));
    }

    #[test]
    fn collision_heavy_lookup() {
        let (mut backing, mut set) = init_set(16);
        let mut context = TestContext::with_constant_hash();
        let ids: Vec<_> = (0..8)
            .map(|idx| {
                set.add(backing.as_mut_ptr(), value(idx), &mut context)
                    .unwrap()
            })
            .collect();

        for (idx, id) in ids.into_iter().enumerate() {
            assert_eq!(
                set.lookup(backing.as_mut_ptr(), value(idx as u32), &context),
                Some(id)
            );
        }
        assert_eq!(set.lookup(backing.as_mut_ptr(), value(99), &context), None);
    }
}
