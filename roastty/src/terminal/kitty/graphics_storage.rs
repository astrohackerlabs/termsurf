//! Kitty graphics image storage.

use std::collections::HashMap;

use super::graphics_image::{Image, ImageLoadError, LoadingImage, LoadingImageLimits};

pub(crate) const DEFAULT_NEXT_IMAGE_ID: u32 = 2_147_483_647;
pub(crate) const DEFAULT_NEXT_INTERNAL_PLACEMENT_ID: u32 = 0;
pub(crate) const DEFAULT_TOTAL_LIMIT: usize = 320 * 1000 * 1000;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ImageStorage {
    pub(crate) dirty: bool,
    pub(crate) next_image_id: u32,
    pub(crate) next_internal_placement_id: u32,
    pub(crate) loading: Option<Box<LoadingImage>>,
    pub(crate) image_limits: LoadingImageLimits,
    pub(crate) total_bytes: usize,
    pub(crate) total_limit: usize,
    images: HashMap<u32, Image>,
    placements: HashMap<PlacementKey, Placement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacementError {
    ImageNotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PlacementKey {
    pub(crate) image_id: u32,
    pub(crate) placement_id: PlacementId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PlacementId {
    Internal(u32),
    External(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacementLocation {
    Cell { x: u32, y: u32 },
    Virtual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) location: PlacementLocation,
    pub(crate) x_offset: u32,
    pub(crate) y_offset: u32,
    pub(crate) source_x: u32,
    pub(crate) source_y: u32,
    pub(crate) source_width: u32,
    pub(crate) source_height: u32,
    pub(crate) columns: u32,
    pub(crate) rows: u32,
    pub(crate) z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellMetrics {
    pub(crate) columns: u32,
    pub(crate) rows: u32,
    pub(crate) width_px: u32,
    pub(crate) height_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridSize {
    pub(crate) columns: u32,
    pub(crate) rows: u32,
}

impl Default for LoadingImageLimits {
    fn default() -> Self {
        Self::DIRECT
    }
}

impl Default for ImageStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageStorage {
    pub(crate) fn new() -> Self {
        Self {
            dirty: false,
            next_image_id: DEFAULT_NEXT_IMAGE_ID,
            next_internal_placement_id: DEFAULT_NEXT_INTERNAL_PLACEMENT_ID,
            loading: None,
            image_limits: LoadingImageLimits::DIRECT,
            total_bytes: 0,
            total_limit: DEFAULT_TOTAL_LIMIT,
            images: HashMap::new(),
            placements: HashMap::new(),
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.total_limit != 0
    }

    pub(crate) fn len(&self) -> usize {
        self.images.len()
    }

    pub(crate) fn set_limit(&mut self, limit: usize) {
        if limit == 0 {
            let image_limits = self.image_limits;
            self.images.clear();
            self.placements.clear();
            self.loading = None;
            self.next_internal_placement_id = DEFAULT_NEXT_INTERNAL_PLACEMENT_ID;
            self.total_bytes = 0;
            self.total_limit = 0;
            self.image_limits = image_limits;
            self.dirty = true;
            return;
        }

        if limit < self.total_bytes {
            let required_bytes = self.total_bytes - limit;
            let _ = self.evict_image(required_bytes);
        }

        self.total_limit = limit;
    }

    pub(crate) fn add_image(&mut self, image: Image) -> Result<(), ImageLoadError> {
        let image_bytes = image.data.len();
        if image_bytes > self.total_limit {
            return Err(ImageLoadError::OutOfMemory);
        }

        let existing_bytes = self
            .images
            .get(&image.id)
            .map(|stored| stored.data.len())
            .unwrap_or(0);
        let final_bytes_without_eviction = self
            .total_bytes
            .checked_sub(existing_bytes)
            .and_then(|bytes| bytes.checked_add(image_bytes))
            .ok_or(ImageLoadError::OutOfMemory)?;

        if final_bytes_without_eviction > self.total_limit {
            let required_bytes = final_bytes_without_eviction - self.total_limit;
            if !self.evict_image_excluding(required_bytes, image.id) {
                return Err(ImageLoadError::OutOfMemory);
            }
        }

        if let Some(old) = self.images.insert(image.id, image) {
            self.total_bytes -= old.data.len();
        }
        self.total_bytes += image_bytes;
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn image_by_id(&self, image_id: u32) -> Option<&Image> {
        self.images.get(&image_id)
    }

    pub(crate) fn image_by_number(&self, image_number: u32) -> Option<&Image> {
        self.images
            .values()
            .filter(|image| image.number == image_number)
            .max_by(|lhs, rhs| compare_newest(lhs, rhs))
    }

    pub(crate) fn placement_len(&self) -> usize {
        self.placements.len()
    }

    pub(crate) fn placement_by_key(&self, key: PlacementKey) -> Option<&Placement> {
        self.placements.get(&key)
    }

    pub(crate) fn placements_for_image(&self, image_id: u32) -> Vec<(PlacementKey, &Placement)> {
        self.placements
            .iter()
            .filter(|(key, _)| key.image_id == image_id)
            .map(|(key, placement)| (*key, placement))
            .collect()
    }

    pub(crate) fn add_placement(
        &mut self,
        image_id: u32,
        placement_id: u32,
        placement: Placement,
    ) -> Result<PlacementKey, PlacementError> {
        if !self.images.contains_key(&image_id) {
            return Err(PlacementError::ImageNotFound);
        }

        let key = PlacementKey {
            image_id,
            placement_id: if placement_id == 0 {
                let id = self.next_internal_placement_id;
                self.next_internal_placement_id = self.next_internal_placement_id.wrapping_add(1);
                PlacementId::Internal(id)
            } else {
                PlacementId::External(placement_id)
            },
        };
        self.placements.insert(key, placement);
        self.dirty = true;
        Ok(key)
    }

    pub(crate) fn evict_image(&mut self, required_bytes: usize) -> bool {
        self.evict_image_excluding(required_bytes, u32::MAX)
    }

    fn evict_image_excluding(&mut self, required_bytes: usize, excluded_id: u32) -> bool {
        if required_bytes == 0 {
            return true;
        }

        let mut candidates: Vec<(u32, Option<std::time::Instant>, bool)> = self
            .images
            .values()
            .filter(|image| image.id != excluded_id)
            .map(|image| {
                (
                    image.id,
                    image.transmit_time,
                    self.placements.contains_key_for_image(image.id),
                )
            })
            .collect();
        candidates.sort_by(
            |(lhs_id, lhs_time, lhs_used), (rhs_id, rhs_time, rhs_used)| {
                compare_eviction_candidate(
                    *lhs_time, *lhs_id, *lhs_used, *rhs_time, *rhs_id, *rhs_used,
                )
            },
        );

        let mut evicted = 0usize;
        for (id, _, _) in candidates {
            let Some(image) = self.images.remove(&id) else {
                continue;
            };
            evicted += image.data.len();
            self.total_bytes -= image.data.len();
            self.remove_placements_for_image(id);
            self.dirty = true;
            if evicted >= required_bytes {
                return true;
            }
        }

        false
    }

    fn remove_placements_for_image(&mut self, image_id: u32) {
        self.placements.retain(|key, _| key.image_id != image_id);
    }
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            location: PlacementLocation::Cell { x: 0, y: 0 },
            x_offset: 0,
            y_offset: 0,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            columns: 0,
            rows: 0,
            z: 0,
        }
    }
}

impl Placement {
    pub(crate) fn pixel_size(&self, image: &Image, metrics: CellMetrics) -> PixelSize {
        let image_width = if self.source_width > 0 {
            self.source_width
        } else {
            image.width
        };
        let image_height = if self.source_height > 0 {
            self.source_height
        } else {
            image.height
        };

        if self.columns == 0 && self.rows == 0 {
            return PixelSize {
                width: image_width,
                height: image_height,
            };
        }

        let cell_width = metrics.cell_width();
        let cell_height = metrics.cell_height();

        if self.columns > 0 && self.rows > 0 {
            return PixelSize {
                width: cell_width.saturating_mul(self.columns),
                height: cell_height.saturating_mul(self.rows),
            };
        }

        if self.columns > 0 {
            let width = cell_width.saturating_mul(self.columns);
            let height = if image_width == 0 {
                0
            } else {
                round_ratio(width, image_height, image_width)
            };
            return PixelSize { width, height };
        }

        let height = cell_height.saturating_mul(self.rows);
        let width = if image_height == 0 {
            0
        } else {
            round_ratio(height, image_width, image_height)
        };
        PixelSize { width, height }
    }

    pub(crate) fn grid_size(&self, image: &Image, metrics: CellMetrics) -> GridSize {
        if self.columns > 0 && self.rows > 0 {
            return GridSize {
                columns: self.columns,
                rows: self.rows,
            };
        }

        let pixel_size = self.pixel_size(image, metrics);
        GridSize {
            columns: div_ceil_axis(pixel_size.width, self.x_offset, metrics.cell_width()),
            rows: div_ceil_axis(pixel_size.height, self.y_offset, metrics.cell_height()),
        }
    }
}

impl CellMetrics {
    fn cell_width(self) -> u32 {
        if self.columns == 0 {
            0
        } else {
            self.width_px / self.columns
        }
    }

    fn cell_height(self) -> u32 {
        if self.rows == 0 {
            0
        } else {
            self.height_px / self.rows
        }
    }
}

trait PlacementMapExt {
    fn contains_key_for_image(&self, image_id: u32) -> bool;
}

impl PlacementMapExt for HashMap<PlacementKey, Placement> {
    fn contains_key_for_image(&self, image_id: u32) -> bool {
        self.keys().any(|key| key.image_id == image_id)
    }
}

fn compare_newest(lhs: &Image, rhs: &Image) -> std::cmp::Ordering {
    match (lhs.transmit_time, rhs.transmit_time) {
        (Some(lhs_time), Some(rhs_time)) => {
            lhs_time.cmp(&rhs_time).then_with(|| lhs.id.cmp(&rhs.id))
        }
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => lhs.id.cmp(&rhs.id),
    }
}

fn compare_oldest_parts(
    lhs_time: Option<std::time::Instant>,
    lhs_id: u32,
    rhs_time: Option<std::time::Instant>,
    rhs_id: u32,
) -> std::cmp::Ordering {
    match (lhs_time, rhs_time) {
        (Some(lhs_time), Some(rhs_time)) => {
            lhs_time.cmp(&rhs_time).then_with(|| lhs_id.cmp(&rhs_id))
        }
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => lhs_id.cmp(&rhs_id),
    }
}

fn compare_eviction_candidate(
    lhs_time: Option<std::time::Instant>,
    lhs_id: u32,
    lhs_used: bool,
    rhs_time: Option<std::time::Instant>,
    rhs_id: u32,
    rhs_used: bool,
) -> std::cmp::Ordering {
    if lhs_used == rhs_used {
        compare_oldest_parts(lhs_time, lhs_id, rhs_time, rhs_id)
    } else if lhs_used {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Less
    }
}

fn round_ratio(value: u32, numerator: u32, denominator: u32) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((value as f64) * (numerator as f64) / (denominator as f64)).round() as u32
}

fn div_ceil_axis(value: u32, offset: u32, divisor: u32) -> u32 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(offset).div_ceil(divisor)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::graphics_command::{TransmissionCompression, TransmissionFormat};
    use super::*;

    fn image(id: u32, number: u32, bytes: usize, transmit_time: Instant) -> Image {
        Image {
            id,
            number,
            width: bytes as u32,
            height: 1,
            format: TransmissionFormat::Rgb,
            compression: TransmissionCompression::None,
            data: vec![id as u8; bytes],
            transmit_time: Some(transmit_time),
            implicit_id: false,
        }
    }

    fn image_with_dimensions(id: u32, width: u32, height: u32) -> Image {
        Image {
            id,
            width,
            height,
            ..Image::default()
        }
    }

    fn placement() -> Placement {
        Placement {
            location: PlacementLocation::Cell { x: 4, y: 5 },
            x_offset: 1,
            y_offset: 2,
            source_x: 3,
            source_y: 4,
            source_width: 5,
            source_height: 6,
            columns: 7,
            rows: 8,
            z: 9,
        }
    }

    fn metrics() -> CellMetrics {
        CellMetrics {
            columns: 100,
            rows: 100,
            width_px: 1000,
            height_px: 2000,
        }
    }

    #[test]
    fn kitty_graphics_storage_defaults_and_enabled_state() {
        let storage = ImageStorage::new();
        let default_storage = ImageStorage::default();
        assert_eq!(default_storage.next_image_id, storage.next_image_id);
        assert_eq!(default_storage.total_limit, storage.total_limit);
        assert_eq!(default_storage.image_limits, storage.image_limits);
        assert_eq!(default_storage.enabled(), storage.enabled());

        assert!(!storage.dirty);
        assert_eq!(storage.next_image_id, DEFAULT_NEXT_IMAGE_ID);
        assert_eq!(
            storage.next_internal_placement_id,
            DEFAULT_NEXT_INTERNAL_PLACEMENT_ID
        );
        assert!(storage.loading.is_none());
        assert_eq!(storage.image_limits, LoadingImageLimits::DIRECT);
        assert_eq!(storage.total_bytes, 0);
        assert_eq!(storage.total_limit, DEFAULT_TOTAL_LIMIT);
        assert!(storage.enabled());
        assert_eq!(storage.len(), 0);
        assert_eq!(storage.placement_len(), 0);
    }

    #[test]
    fn kitty_graphics_storage_set_limit_zero_clears_images_and_preserves_limits() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.image_limits = LoadingImageLimits::ALL;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_placement(1, 0, Placement::default())
            .expect("placement");
        storage.next_internal_placement_id = 42;
        storage.dirty = false;

        storage.set_limit(0);

        assert!(!storage.enabled());
        assert_eq!(storage.image_limits, LoadingImageLimits::ALL);
        assert_eq!(storage.total_limit, 0);
        assert_eq!(storage.total_bytes, 0);
        assert_eq!(
            storage.next_internal_placement_id,
            DEFAULT_NEXT_INTERNAL_PLACEMENT_ID
        );
        assert_eq!(storage.len(), 0);
        assert_eq!(storage.placement_len(), 0);
        assert!(storage.image_by_id(1).is_none());
        assert!(storage.dirty);
    }

    #[test]
    fn kitty_graphics_storage_add_image_updates_bytes_lookup_and_dirty() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 9, 10, base)).unwrap();

        assert_eq!(storage.total_bytes, 10);
        assert_eq!(storage.len(), 1);
        assert!(storage.dirty);
        assert_eq!(storage.image_by_id(1).unwrap().number, 9);
    }

    #[test]
    fn kitty_graphics_storage_replace_same_id_updates_accounting_for_sizes() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 25;

        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(1, 0, 10, base + Duration::from_secs(1)))
            .unwrap();
        assert_eq!(storage.total_bytes, 10);
        assert_eq!(storage.len(), 1);

        storage
            .add_image(image(1, 0, 5, base + Duration::from_secs(2)))
            .unwrap();
        assert_eq!(storage.total_bytes, 5);
        assert_eq!(storage.len(), 1);

        storage
            .add_image(image(1, 0, 25, base + Duration::from_secs(3)))
            .unwrap();
        assert_eq!(storage.total_bytes, 25);
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn kitty_graphics_storage_same_id_replacement_does_not_over_evict() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 20;
        storage.add_image(image(1, 0, 15, base)).unwrap();
        storage
            .add_image(image(2, 0, 5, base + Duration::from_secs(1)))
            .unwrap();

        storage
            .add_image(image(1, 0, 15, base + Duration::from_secs(2)))
            .unwrap();

        assert_eq!(storage.total_bytes, 20);
        assert!(storage.image_by_id(1).is_some());
        assert!(storage.image_by_id(2).is_some());
    }

    #[test]
    fn kitty_graphics_storage_same_id_replacement_preserves_existing_placement() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 20;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 5, base + Duration::from_secs(1)))
            .unwrap();
        let placement_key = storage.add_placement(1, 5, placement()).unwrap();

        storage
            .add_image(image(1, 0, 15, base + Duration::from_secs(2)))
            .unwrap();

        assert_eq!(storage.total_bytes, 20);
        assert!(storage.image_by_id(1).is_some());
        assert!(storage.image_by_id(2).is_some());
        assert_eq!(storage.placement_len(), 1);
        assert_eq!(storage.placement_by_key(placement_key), Some(&placement()));
    }

    #[test]
    fn kitty_graphics_storage_zero_placement_ids_create_internal_keys() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 0, 10, base)).unwrap();

        let first = storage.add_placement(1, 0, Placement::default()).unwrap();
        let second = storage.add_placement(1, 0, placement()).unwrap();

        assert_eq!(first.image_id, 1);
        assert_eq!(first.placement_id, PlacementId::Internal(0));
        assert_eq!(second.placement_id, PlacementId::Internal(1));
        assert_eq!(storage.next_internal_placement_id, 2);
        assert_eq!(storage.placement_len(), 2);
        assert_eq!(storage.placements_for_image(1).len(), 2);
    }

    #[test]
    fn kitty_graphics_storage_external_placement_replaces_same_key() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 0, 10, base)).unwrap();

        let first = storage.add_placement(1, 9, Placement::default()).unwrap();
        let second = storage.add_placement(1, 9, placement()).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.placement_id, PlacementId::External(9));
        assert_eq!(storage.placement_len(), 1);
        assert_eq!(storage.placement_by_key(first), Some(&placement()));
    }

    #[test]
    fn kitty_graphics_storage_add_placement_missing_image_fails_without_mutation() {
        let mut storage = ImageStorage::new();
        storage.dirty = false;

        assert_eq!(
            storage.add_placement(99, 1, Placement::default()),
            Err(PlacementError::ImageNotFound)
        );
        assert_eq!(storage.placement_len(), 0);
        assert_eq!(
            storage.next_internal_placement_id,
            DEFAULT_NEXT_INTERNAL_PLACEMENT_ID
        );
        assert!(!storage.dirty);
    }

    #[test]
    fn kitty_graphics_storage_rejects_image_larger_than_limit_without_mutation() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 10;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage.dirty = false;

        assert_eq!(
            storage.add_image(image(2, 0, 11, base + Duration::from_secs(1))),
            Err(ImageLoadError::OutOfMemory)
        );
        assert_eq!(storage.total_bytes, 10);
        assert_eq!(storage.len(), 1);
        assert!(storage.image_by_id(1).is_some());
        assert!(storage.image_by_id(2).is_none());
        assert!(!storage.dirty);
    }

    #[test]
    fn kitty_graphics_storage_lowering_limit_evicts_oldest_images() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 10, base + Duration::from_secs(1)))
            .unwrap();
        storage
            .add_image(image(3, 0, 10, base + Duration::from_secs(2)))
            .unwrap();

        storage.set_limit(15);

        assert_eq!(storage.total_limit, 15);
        assert_eq!(storage.total_bytes, 10);
        assert!(storage.image_by_id(1).is_none());
        assert!(storage.image_by_id(2).is_none());
        assert!(storage.image_by_id(3).is_some());
    }

    #[test]
    fn kitty_graphics_storage_lowering_limit_exact_fit_succeeds() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 10, base + Duration::from_secs(1)))
            .unwrap();

        storage.set_limit(10);

        assert_eq!(storage.total_limit, 10);
        assert_eq!(storage.total_bytes, 10);
        assert!(storage.image_by_id(1).is_none());
        assert!(storage.image_by_id(2).is_some());
    }

    #[test]
    fn kitty_graphics_storage_eviction_removes_placements_for_evicted_images() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 20;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 10, base + Duration::from_secs(1)))
            .unwrap();
        let evicted_key = storage.add_placement(1, 1, Placement::default()).unwrap();
        let survivor_key = storage.add_placement(2, 1, Placement::default()).unwrap();

        storage
            .add_image(image(3, 0, 10, base + Duration::from_secs(2)))
            .unwrap();

        assert!(storage.image_by_id(1).is_none());
        assert!(storage.placement_by_key(evicted_key).is_none());
        assert!(storage.image_by_id(2).is_some());
        assert!(storage.placement_by_key(survivor_key).is_some());
    }

    #[test]
    fn kitty_graphics_storage_eviction_prefers_unused_images() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 20;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 10, base + Duration::from_secs(1)))
            .unwrap();
        let used_key = storage.add_placement(1, 1, Placement::default()).unwrap();

        storage
            .add_image(image(3, 0, 10, base + Duration::from_secs(2)))
            .unwrap();

        assert!(storage.image_by_id(1).is_some());
        assert!(storage.placement_by_key(used_key).is_some());
        assert!(storage.image_by_id(2).is_none());
        assert!(storage.image_by_id(3).is_some());
    }

    #[test]
    fn kitty_graphics_storage_add_image_evicts_enough_old_images_to_fit() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 25;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 10, base + Duration::from_secs(1)))
            .unwrap();

        storage
            .add_image(image(3, 0, 15, base + Duration::from_secs(2)))
            .unwrap();

        assert_eq!(storage.total_bytes, 25);
        assert!(storage.image_by_id(1).is_none());
        assert!(storage.image_by_id(2).is_some());
        assert!(storage.image_by_id(3).is_some());
    }

    #[test]
    fn kitty_graphics_storage_add_image_exact_fit_eviction_succeeds() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 20;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        storage
            .add_image(image(2, 0, 10, base + Duration::from_secs(1)))
            .unwrap();

        storage
            .add_image(image(3, 0, 10, base + Duration::from_secs(2)))
            .unwrap();

        assert_eq!(storage.total_bytes, 20);
        assert!(storage.image_by_id(1).is_none());
        assert!(storage.image_by_id(2).is_some());
        assert!(storage.image_by_id(3).is_some());
    }

    #[test]
    fn kitty_graphics_storage_image_by_id_borrows_stored_image() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 0, 10, base)).unwrap();

        let stored = storage.image_by_id(1).unwrap();
        assert_eq!(
            stored.data.as_ptr(),
            storage.image_by_id(1).unwrap().data.as_ptr()
        );
        assert_eq!(stored.data.len(), 10);
    }

    #[test]
    fn kitty_graphics_storage_image_by_number_picks_newest_with_id_tie_break() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.add_image(image(1, 7, 1, base)).unwrap();
        storage
            .add_image(image(3, 7, 1, base + Duration::from_secs(1)))
            .unwrap();
        storage
            .add_image(image(2, 7, 1, base + Duration::from_secs(1)))
            .unwrap();

        assert_eq!(storage.image_by_number(7).unwrap().id, 3);
    }

    #[test]
    fn kitty_graphics_storage_eviction_moves_images_without_payload_clones() {
        let base = Instant::now();
        let mut storage = ImageStorage::new();
        storage.total_limit = 20;
        storage.add_image(image(1, 0, 10, base)).unwrap();
        let survivor = image(2, 0, 10, base + Duration::from_secs(1));
        let survivor_ptr = survivor.data.as_ptr();
        storage.add_image(survivor).unwrap();

        assert!(storage.evict_image(10));

        let stored = storage.image_by_id(2).unwrap();
        assert_eq!(stored.data.as_ptr(), survivor_ptr);
        assert_eq!(storage.total_bytes, 10);
        assert!(storage.image_by_id(1).is_none());
    }

    #[test]
    fn kitty_graphics_storage_placement_pixel_size_native_and_source_rect() {
        let image = image_with_dimensions(1, 40, 20);
        assert_eq!(
            Placement::default().pixel_size(&image, metrics()),
            PixelSize {
                width: 40,
                height: 20
            }
        );

        assert_eq!(
            Placement {
                source_width: 12,
                source_height: 8,
                ..Placement::default()
            }
            .pixel_size(&image, metrics()),
            PixelSize {
                width: 12,
                height: 8
            }
        );
    }

    #[test]
    fn kitty_graphics_storage_placement_pixel_size_both_grid_axes() {
        let image = image_with_dimensions(1, 16, 9);
        let placement = Placement {
            columns: 10,
            rows: 5,
            ..Placement::default()
        };

        assert_eq!(
            placement.pixel_size(&image, metrics()),
            PixelSize {
                width: 100,
                height: 100
            }
        );
    }

    #[test]
    fn kitty_graphics_storage_placement_pixel_size_aspect_ratio_axes() {
        let image = image_with_dimensions(1, 16, 9);

        assert_eq!(
            Placement {
                columns: 10,
                ..Placement::default()
            }
            .pixel_size(&image, metrics()),
            PixelSize {
                width: 100,
                height: 56
            }
        );
        assert_eq!(
            Placement {
                rows: 5,
                ..Placement::default()
            }
            .pixel_size(&image, metrics()),
            PixelSize {
                width: 178,
                height: 100
            }
        );
    }

    #[test]
    fn kitty_graphics_storage_placement_pixel_size_zero_metrics_do_not_panic() {
        let image = image_with_dimensions(1, 16, 9);

        assert_eq!(
            Placement {
                columns: 10,
                ..Placement::default()
            }
            .pixel_size(
                &image,
                CellMetrics {
                    columns: 0,
                    rows: 100,
                    width_px: 1000,
                    height_px: 2000,
                },
            ),
            PixelSize {
                width: 0,
                height: 0
            }
        );
        assert_eq!(
            Placement {
                rows: 5,
                ..Placement::default()
            }
            .pixel_size(
                &image,
                CellMetrics {
                    columns: 100,
                    rows: 0,
                    width_px: 1000,
                    height_px: 2000,
                },
            ),
            PixelSize {
                width: 0,
                height: 0
            }
        );
    }

    #[test]
    fn kitty_graphics_storage_placement_grid_size_ceilings_and_zero_cells() {
        let image = image_with_dimensions(1, 16, 9);
        let placement = Placement {
            x_offset: 1,
            y_offset: 2,
            ..Placement::default()
        };

        assert_eq!(
            placement.grid_size(&image, metrics()),
            GridSize {
                columns: 2,
                rows: 1
            }
        );
        assert_eq!(
            placement.grid_size(
                &image,
                CellMetrics {
                    columns: 0,
                    rows: 0,
                    width_px: 1000,
                    height_px: 2000,
                },
            ),
            GridSize {
                columns: 0,
                rows: 0
            }
        );
    }
}
