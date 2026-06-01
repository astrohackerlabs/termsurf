use super::size::CellCountInt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tag {
    Active,
    Viewport,
    Screen,
    History,
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub(crate) struct Coordinate {
    pub(crate) x: CellCountInt,
    pub(crate) y: u32,
}

impl Coordinate {
    pub(crate) const fn new(x: CellCountInt, y: u32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    Active(Coordinate),
    Viewport(Coordinate),
    Screen(Coordinate),
    History(Coordinate),
}

impl Point {
    pub(super) const fn active(coord: Coordinate) -> Self {
        Self::Active(coord)
    }

    pub(super) const fn viewport(coord: Coordinate) -> Self {
        Self::Viewport(coord)
    }

    pub(super) const fn screen(coord: Coordinate) -> Self {
        Self::Screen(coord)
    }

    pub(super) const fn history(coord: Coordinate) -> Self {
        Self::History(coord)
    }

    pub(super) const fn coord(self) -> Coordinate {
        match self {
            Self::Active(coord)
            | Self::Viewport(coord)
            | Self::Screen(coord)
            | Self::History(coord) => coord,
        }
    }

    pub(super) const fn tag(self) -> Tag {
        match self {
            Self::Active(_) => Tag::Active,
            Self::Viewport(_) => Tag::Viewport,
            Self::Screen(_) => Tag::Screen,
            Self::History(_) => Tag::History,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_equality() {
        assert_eq!(Coordinate::new(4, 9), Coordinate::new(4, 9));
        assert_ne!(Coordinate::new(4, 9), Coordinate::new(5, 9));
        assert_ne!(Coordinate::new(4, 9), Coordinate::new(4, 10));
    }

    #[test]
    fn coordinate_y_can_exceed_cell_count() {
        let coord = Coordinate::new(1, CellCountInt::MAX as u32 + 1);
        assert_eq!(coord.x, 1);
        assert_eq!(coord.y, CellCountInt::MAX as u32 + 1);
    }

    #[test]
    fn point_active_preserves_coordinate_and_tag() {
        let coord = Coordinate::new(1, 2);
        let point = Point::active(coord);

        assert_eq!(point.coord(), coord);
        assert_eq!(point.tag(), Tag::Active);
    }

    #[test]
    fn point_viewport_preserves_coordinate_and_tag() {
        let coord = Coordinate::new(3, 4);
        let point = Point::viewport(coord);

        assert_eq!(point.coord(), coord);
        assert_eq!(point.tag(), Tag::Viewport);
    }

    #[test]
    fn point_screen_preserves_coordinate_and_tag() {
        let coord = Coordinate::new(5, 6);
        let point = Point::screen(coord);

        assert_eq!(point.coord(), coord);
        assert_eq!(point.tag(), Tag::Screen);
    }

    #[test]
    fn point_history_preserves_coordinate_and_tag() {
        let coord = Coordinate::new(7, 8);
        let point = Point::history(coord);

        assert_eq!(point.coord(), coord);
        assert_eq!(point.tag(), Tag::History);
    }
}
