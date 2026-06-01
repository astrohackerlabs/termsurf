//! Terminal selection value types.

use std::ptr::NonNull;

use super::page_list::Pin;

#[derive(Debug, Clone, Copy)]
pub(super) struct Selection {
    bounds: Bounds,
    rectangle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Order {
    Forward,
    Reverse,
    MirroredForward,
    MirroredReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Adjustment {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    BeginningOfLine,
    EndOfLine,
}

#[derive(Debug, Clone, Copy)]
enum Bounds {
    Untracked {
        start: Pin,
        end: Pin,
    },
    Tracked {
        start: NonNull<Pin>,
        end: NonNull<Pin>,
    },
}

impl Selection {
    pub(super) fn new(start: Pin, end: Pin, rectangle: bool) -> Self {
        Self {
            bounds: Bounds::Untracked { start, end },
            rectangle,
        }
    }

    pub(super) fn tracked(start: NonNull<Pin>, end: NonNull<Pin>, rectangle: bool) -> Self {
        Self {
            bounds: Bounds::Tracked { start, end },
            rectangle,
        }
    }

    pub(super) fn start(&self) -> Pin {
        match self.bounds {
            Bounds::Untracked { start, .. } => start,
            Bounds::Tracked { start, .. } => unsafe {
                // Safety: tracked selections are non-owning wrappers. Callers
                // must ensure the pointed-to pins outlive this access.
                *start.as_ref()
            },
        }
    }

    pub(super) fn end(&self) -> Pin {
        match self.bounds {
            Bounds::Untracked { end, .. } => end,
            Bounds::Tracked { end, .. } => unsafe {
                // Safety: tracked selections are non-owning wrappers. Callers
                // must ensure the pointed-to pins outlive this access.
                *end.as_ref()
            },
        }
    }

    pub(super) fn start_mut(&mut self) -> &mut Pin {
        match self.bounds {
            Bounds::Untracked { ref mut start, .. } => start,
            Bounds::Tracked { mut start, .. } => unsafe {
                // Safety: tracked selections are non-owning wrappers. Callers
                // must ensure the pointed-to pins outlive this mutable access
                // and that no other mutable access aliases it.
                start.as_mut()
            },
        }
    }

    pub(super) fn end_mut(&mut self) -> &mut Pin {
        match self.bounds {
            Bounds::Untracked { ref mut end, .. } => end,
            Bounds::Tracked { mut end, .. } => unsafe {
                // Safety: tracked selections are non-owning wrappers. Callers
                // must ensure the pointed-to pins outlive this mutable access
                // and that no other mutable access aliases it.
                end.as_mut()
            },
        }
    }

    pub(super) fn is_tracked(&self) -> bool {
        matches!(self.bounds, Bounds::Tracked { .. })
    }

    pub(super) fn rectangle(&self) -> bool {
        self.rectangle
    }
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.start() == other.start()
            && self.end() == other.end()
            && self.rectangle == other.rectangle
    }
}

impl Eq for Selection {}

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;

    use super::*;
    use crate::terminal::page_list::Node;

    fn node_ptr() -> NonNull<Node> {
        NonNull::dangling()
    }

    fn pin(y: u16, x: u16) -> Pin {
        Pin::new(node_ptr(), y, x)
    }

    #[test]
    fn selection_new_stores_untracked_bounds_and_rectangle() {
        let start = pin(1, 2);
        let end = pin(3, 4);
        let selection = Selection::new(start, end, true);

        assert!(!selection.is_tracked());
        assert_eq!(selection.start(), start);
        assert_eq!(selection.end(), end);
        assert!(selection.rectangle());
    }

    #[test]
    fn selection_tracked_reads_pointed_to_bounds() {
        let mut start = Box::new(pin(1, 2));
        let mut end = Box::new(pin(3, 4));
        let selection = Selection::tracked(
            NonNull::from(start.as_mut()),
            NonNull::from(end.as_mut()),
            false,
        );

        assert!(selection.is_tracked());
        assert_eq!(selection.start(), *start);
        assert_eq!(selection.end(), *end);
        assert!(!selection.rectangle());
    }

    #[test]
    fn selection_mutates_untracked_bounds_in_place() {
        let start = pin(1, 2);
        let end = pin(3, 4);
        let new_start = pin(5, 6);
        let new_end = pin(7, 8);
        let mut selection = Selection::new(start, end, false);

        *selection.start_mut() = new_start;
        *selection.end_mut() = new_end;

        assert_eq!(selection.start(), new_start);
        assert_eq!(selection.end(), new_end);
    }

    #[test]
    fn selection_mutates_tracked_pin_storage() {
        let mut start = Box::new(pin(1, 2));
        let mut end = Box::new(pin(3, 4));
        let new_start = pin(5, 6);
        let new_end = pin(7, 8);
        let mut selection = Selection::tracked(
            NonNull::from(start.as_mut()),
            NonNull::from(end.as_mut()),
            false,
        );

        *selection.start_mut() = new_start;
        *selection.end_mut() = new_end;

        assert_eq!(*start, new_start);
        assert_eq!(*end, new_end);
        assert_eq!(selection.start(), new_start);
        assert_eq!(selection.end(), new_end);
    }

    #[test]
    fn selection_equality_uses_bounds_and_rectangle() {
        let start = pin(1, 2);
        let end = pin(3, 4);

        assert_eq!(
            Selection::new(start, end, false),
            Selection::new(start, end, false)
        );
        assert_ne!(
            Selection::new(start, end, false),
            Selection::new(start, end, true)
        );
        assert_ne!(
            Selection::new(start, end, false),
            Selection::new(end, start, false)
        );
    }

    #[test]
    fn selection_equality_matches_untracked_and_tracked_pin_values() {
        let start = pin(1, 2);
        let end = pin(3, 4);
        let mut tracked_start = Box::new(start);
        let mut tracked_end = Box::new(end);

        let untracked = Selection::new(start, end, true);
        let tracked = Selection::tracked(
            NonNull::from(tracked_start.as_mut()),
            NonNull::from(tracked_end.as_mut()),
            true,
        );

        assert_eq!(untracked, tracked);
    }

    #[test]
    fn selection_equality_uses_tracked_pin_values_not_pointer_identity() {
        let start = pin(1, 2);
        let end = pin(3, 4);
        let mut tracked_start_a = Box::new(start);
        let mut tracked_end_a = Box::new(end);
        let mut tracked_start_b = Box::new(start);
        let mut tracked_end_b = Box::new(end);
        let selection_a = Selection::tracked(
            NonNull::from(tracked_start_a.as_mut()),
            NonNull::from(tracked_end_a.as_mut()),
            false,
        );
        let selection_b = Selection::tracked(
            NonNull::from(tracked_start_b.as_mut()),
            NonNull::from(tracked_end_b.as_mut()),
            false,
        );

        assert_ne!(
            selection_a.start_mut_ptr_for_test(),
            selection_b.start_mut_ptr_for_test()
        );
        assert_ne!(
            selection_a.end_mut_ptr_for_test(),
            selection_b.end_mut_ptr_for_test()
        );
        assert_eq!(selection_a, selection_b);

        *tracked_end_b = pin(9, 9);
        assert_ne!(selection_a, selection_b);
    }

    #[test]
    fn selection_preserves_reversed_unordered_bounds() {
        let start = pin(5, 6);
        let end = pin(1, 2);
        let selection = Selection::new(start, end, false);

        assert_eq!(selection.start(), start);
        assert_eq!(selection.end(), end);
    }

    trait SelectionTestPointers {
        fn start_mut_ptr_for_test(&self) -> NonNull<Pin>;
        fn end_mut_ptr_for_test(&self) -> NonNull<Pin>;
    }

    impl SelectionTestPointers for Selection {
        fn start_mut_ptr_for_test(&self) -> NonNull<Pin> {
            match self.bounds {
                Bounds::Tracked { start, .. } => start,
                Bounds::Untracked { .. } => panic!("selection must be tracked"),
            }
        }

        fn end_mut_ptr_for_test(&self) -> NonNull<Pin> {
            match self.bounds {
                Bounds::Tracked { end, .. } => end,
                Bounds::Untracked { .. } => panic!("selection must be tracked"),
            }
        }
    }
}
