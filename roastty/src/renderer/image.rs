#![allow(dead_code)]
// This renderer foundation is consumed by later renderer slices.

use std::collections::{BTreeMap, BTreeSet};

use crate::renderer::shader::{ImageDrawCall, ImageVertex, PrimitiveType};
use crate::KittyGraphicsRenderPlacementSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ImageId {
    Kitty(u32),
    Overlay,
}

impl ImageId {
    pub(crate) fn z_less_than(self, other: Self) -> bool {
        match (self, other) {
            (ImageId::Kitty(lhs), ImageId::Kitty(rhs)) => lhs < rhs,
            (ImageId::Kitty(_), ImageId::Overlay) => true,
            (ImageId::Overlay, ImageId::Kitty(_)) => false,
            (ImageId::Overlay, ImageId::Overlay) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PixelFormat {
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
}

impl PixelFormat {
    pub(crate) fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Gray => 1,
            PixelFormat::GrayAlpha => 2,
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
        }
    }

    fn from_kitty_snapshot_format(format: i32) -> Option<Self> {
        match format {
            0 => Some(PixelFormat::Rgb),
            1 => Some(PixelFormat::Rgba),
            3 => Some(PixelFormat::GrayAlpha),
            4 => Some(PixelFormat::Gray),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixel_format: PixelFormat,
    pub(crate) data: Vec<u8>,
}

impl PendingImage {
    fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            pixel_format: PixelFormat::Rgba,
            data: Vec::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.width as usize * self.height as usize * self.pixel_format.bytes_per_pixel()
    }

    fn prepare_rgba(&mut self) -> Result<(), ImagePrepareError> {
        let expected_len = self.len();
        if self.data.len() != expected_len {
            return Err(ImagePrepareError::LengthMismatch {
                expected: expected_len,
                actual: self.data.len(),
            });
        }

        let rgba = match self.pixel_format {
            PixelFormat::Rgba => return Ok(()),
            PixelFormat::Gray => self
                .data
                .iter()
                .flat_map(|gray| [*gray, *gray, *gray, 255])
                .collect(),
            PixelFormat::GrayAlpha => self
                .data
                .chunks_exact(2)
                .flat_map(|chunk| [chunk[0], chunk[0], chunk[0], chunk[1]])
                .collect(),
            PixelFormat::Rgb => self
                .data
                .chunks_exact(3)
                .flat_map(|chunk| [chunk[0], chunk[1], chunk[2], 255])
                .collect(),
        };
        self.data = rgba;
        self.pixel_format = PixelFormat::Rgba;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImagePrepareError {
    LengthMismatch { expected: usize, actual: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RendererImage<Texture = ()> {
    Pending(PendingImage),
    Replace {
        texture: Texture,
        pending: PendingImage,
    },
    Ready {
        texture: Texture,
        source: PendingImage,
    },
    UnloadPending(PendingImage),
    UnloadReady {
        texture: Texture,
        source: PendingImage,
    },
    UnloadReplace {
        texture: Texture,
        pending: PendingImage,
    },
}

impl<Texture> RendererImage<Texture> {
    pub(crate) fn is_pending(&self) -> bool {
        self.pending_image().is_some()
    }

    pub(crate) fn has_texture(&self) -> bool {
        self.texture().is_some()
    }

    pub(crate) fn is_unloading(&self) -> bool {
        matches!(
            self,
            RendererImage::UnloadPending(_)
                | RendererImage::UnloadReady { .. }
                | RendererImage::UnloadReplace { .. }
        )
    }

    pub(crate) fn pending_image(&self) -> Option<&PendingImage> {
        match self {
            RendererImage::Pending(image)
            | RendererImage::UnloadPending(image)
            | RendererImage::Replace { pending: image, .. }
            | RendererImage::UnloadReplace { pending: image, .. } => Some(image),
            RendererImage::Ready { .. } | RendererImage::UnloadReady { .. } => None,
        }
    }

    fn pending_image_mut(&mut self) -> Option<&mut PendingImage> {
        match self {
            RendererImage::Pending(image)
            | RendererImage::UnloadPending(image)
            | RendererImage::Replace { pending: image, .. }
            | RendererImage::UnloadReplace { pending: image, .. } => Some(image),
            RendererImage::Ready { .. } | RendererImage::UnloadReady { .. } => None,
        }
    }

    pub(crate) fn mark_for_unload(&mut self) {
        let image = std::mem::replace(self, RendererImage::Pending(PendingImage::empty()));
        *self = match image {
            RendererImage::Pending(image) | RendererImage::UnloadPending(image) => {
                RendererImage::UnloadPending(image)
            }
            RendererImage::Ready { texture, source }
            | RendererImage::UnloadReady { texture, source } => {
                RendererImage::UnloadReady { texture, source }
            }
            RendererImage::Replace { texture, pending }
            | RendererImage::UnloadReplace { texture, pending } => {
                RendererImage::UnloadReplace { texture, pending }
            }
        };
    }

    pub(crate) fn mark_for_replace(&mut self, pending: PendingImage) {
        let image = std::mem::replace(self, RendererImage::Pending(PendingImage::empty()));
        *self = match image {
            RendererImage::Pending(_) | RendererImage::UnloadPending(_) => {
                RendererImage::Pending(pending)
            }
            RendererImage::Ready { texture, .. }
            | RendererImage::UnloadReady { texture, .. }
            | RendererImage::Replace { texture, .. }
            | RendererImage::UnloadReplace { texture, .. } => {
                RendererImage::Replace { texture, pending }
            }
        };
    }

    fn texture(&self) -> Option<&Texture> {
        match self {
            RendererImage::Ready { texture, .. }
            | RendererImage::UnloadReady { texture, .. }
            | RendererImage::Replace { texture, .. }
            | RendererImage::UnloadReplace { texture, .. } => Some(texture),
            RendererImage::Pending(_) | RendererImage::UnloadPending(_) => None,
        }
    }

    fn ready_texture(&self) -> Option<&Texture> {
        match self {
            RendererImage::Ready { texture, .. } | RendererImage::UnloadReady { texture, .. } => {
                Some(texture)
            }
            RendererImage::Pending(_)
            | RendererImage::Replace { .. }
            | RendererImage::UnloadPending(_)
            | RendererImage::UnloadReplace { .. } => None,
        }
    }

    fn source_image(&self) -> Option<&PendingImage> {
        match self {
            RendererImage::Ready { source, .. } | RendererImage::UnloadReady { source, .. } => {
                Some(source)
            }
            RendererImage::Pending(_)
            | RendererImage::Replace { .. }
            | RendererImage::UnloadPending(_)
            | RendererImage::UnloadReplace { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) image_id: ImageId,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) z: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) cell_offset_x: u32,
    pub(crate) cell_offset_y: u32,
    pub(crate) source_x: u32,
    pub(crate) source_y: u32,
    pub(crate) source_width: u32,
    pub(crate) source_height: u32,
}

impl Placement {
    pub(crate) fn to_image_draw_call(self) -> ImageDrawCall {
        ImageDrawCall {
            vertex: ImageVertex {
                grid_pos: [self.x as f32, self.y as f32],
                cell_offset: [self.cell_offset_x as f32, self.cell_offset_y as f32],
                source_rect: [
                    self.source_x as f32,
                    self.source_y as f32,
                    self.source_width as f32,
                    self.source_height as f32,
                ],
                dest_size: [self.width as f32, self.height as f32],
            },
            primitive: PrimitiveType::TriangleStrip,
            vertex_count: 4,
            instance_count: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageState<Texture = ()> {
    pub(crate) images: BTreeMap<ImageId, RendererImage<Texture>>,
    pub(crate) kitty_placements: Vec<Placement>,
    pub(crate) kitty_bg_end: u32,
    pub(crate) kitty_text_end: u32,
    pub(crate) kitty_virtual: bool,
    pub(crate) overlay_placements: Vec<Placement>,
}

impl<Texture> Default for ImageState<Texture> {
    fn default() -> Self {
        Self {
            images: BTreeMap::new(),
            kitty_placements: Vec::new(),
            kitty_bg_end: 0,
            kitty_text_end: 0,
            kitty_virtual: false,
            overlay_placements: Vec::new(),
        }
    }
}

impl<Texture> ImageState<Texture> {
    pub(crate) fn update_kitty_from_render_placements(
        &mut self,
        placements: &[KittyGraphicsRenderPlacementSnapshot],
    ) {
        self.kitty_placements.clear();
        self.kitty_virtual = false;

        let present = placements
            .iter()
            .map(|placement| ImageId::Kitty(placement.info.image_id))
            .collect::<BTreeSet<_>>();
        for (id, image) in self.images.iter_mut() {
            if matches!(id, ImageId::Kitty(_)) && !present.contains(id) {
                image.mark_for_unload();
            }
        }

        for placement in placements {
            let id = ImageId::Kitty(placement.info.image_id);
            let Some(pending) = pending_image_from_kitty_snapshot(placement) else {
                continue;
            };
            match self.images.get_mut(&id) {
                Some(image) if image.pending_image() == Some(&pending) => {
                    if image.is_unloading() {
                        let previous =
                            std::mem::replace(image, RendererImage::Pending(PendingImage::empty()));
                        *image = match previous {
                            RendererImage::UnloadReplace { texture, pending } => {
                                RendererImage::Replace { texture, pending }
                            }
                            RendererImage::UnloadPending(pending) => {
                                RendererImage::Pending(pending)
                            }
                            other => other,
                        };
                    }
                }
                Some(image) if image.source_image() == Some(&pending) => {
                    if image.is_unloading() {
                        let previous =
                            std::mem::replace(image, RendererImage::Pending(PendingImage::empty()));
                        *image = match previous {
                            RendererImage::UnloadReady { texture, source }
                            | RendererImage::Ready { texture, source } => {
                                RendererImage::Ready { texture, source }
                            }
                            other => other,
                        };
                    }
                }
                Some(image) => {
                    image.mark_for_replace(pending);
                }
                None => {
                    self.images.insert(id, RendererImage::Pending(pending));
                }
            }

            self.kitty_virtual |= placement.info.is_virtual;
            self.kitty_placements.push(Placement {
                image_id: id,
                x: placement.info.viewport_col,
                y: placement.info.viewport_row,
                z: placement.info.z,
                width: placement.info.pixel_width,
                height: placement.info.pixel_height,
                cell_offset_x: placement.info.x_offset,
                cell_offset_y: placement.info.y_offset,
                source_x: placement.info.source_x,
                source_y: placement.info.source_y,
                source_width: placement.info.source_width,
                source_height: placement.info.source_height,
            });
        }

        self.kitty_placements.sort_by(|lhs, rhs| {
            lhs.z.cmp(&rhs.z).then_with(|| {
                if lhs.image_id.z_less_than(rhs.image_id) {
                    std::cmp::Ordering::Less
                } else if rhs.image_id.z_less_than(lhs.image_id) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });

        let bg_limit = i32::MIN / 2;
        self.kitty_bg_end = first_index_at_or_after(&self.kitty_placements, bg_limit);
        self.kitty_text_end = first_index_at_or_after(&self.kitty_placements, 0);
    }

    pub(crate) fn upload<Backend>(&mut self, backend: &mut Backend) -> bool
    where
        Backend: ImageUploadBackend<Texture = Texture>,
    {
        let mut success = true;
        let image_ids = self.images.keys().copied().collect::<Vec<_>>();

        for image_id in image_ids {
            let Some(image) = self.images.get(&image_id) else {
                continue;
            };
            if image.is_unloading() {
                self.images.remove(&image_id);
                continue;
            }

            if !image.is_pending() {
                continue;
            }

            let upload = {
                let image = self.images.get_mut(&image_id).expect("image id exists");
                let pending = image.pending_image_mut().expect("pending image exists");
                let source = pending.clone();
                let mut upload_pending = source.clone();
                if upload_pending.prepare_rgba().is_err() {
                    success = false;
                    continue;
                }
                backend
                    .upload_image(&upload_pending)
                    .map(|texture| (texture, source))
            };

            match upload {
                Ok((texture, source)) => {
                    self.images
                        .insert(image_id, RendererImage::Ready { texture, source });
                }
                Err(_) => {
                    success = false;
                }
            }
        }

        success
    }

    pub(crate) fn draw<Backend>(
        &self,
        placement_type: DrawPlacements,
        backend: &mut Backend,
    ) -> DrawSummary
    where
        Backend: ImageDrawBackend<Texture>,
    {
        let mut summary = DrawSummary::default();
        for placement in self.placements_for_draw(placement_type) {
            let Some(image) = self.images.get(&placement.image_id) else {
                summary.skipped_missing += 1;
                continue;
            };
            let Some(texture) = image.ready_texture() else {
                summary.skipped_not_ready += 1;
                continue;
            };

            summary.attempted += 1;
            let call = placement.to_image_draw_call();
            match backend.draw_image(texture, *placement, call) {
                Ok(()) => summary.succeeded += 1,
                Err(_) => summary.failed += 1,
            }
        }
        summary
    }

    fn placements_for_draw(&self, placement_type: DrawPlacements) -> &[Placement] {
        match placement_type {
            DrawPlacements::KittyBelowBackground => {
                &self.kitty_placements[..self.kitty_bg_end as usize]
            }
            DrawPlacements::KittyBelowText => {
                &self.kitty_placements[self.kitty_bg_end as usize..self.kitty_text_end as usize]
            }
            DrawPlacements::KittyAboveText => {
                &self.kitty_placements[self.kitty_text_end as usize..]
            }
            DrawPlacements::Overlay => &self.overlay_placements,
        }
    }
}

pub(crate) trait ImageUploadBackend {
    type Texture;
    type Error;

    fn upload_image(&mut self, pending: &PendingImage) -> Result<Self::Texture, Self::Error>;
}

pub(crate) trait ImageDrawBackend<Texture> {
    type Error;

    fn draw_image(
        &mut self,
        texture: &Texture,
        placement: Placement,
        call: ImageDrawCall,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrawPlacements {
    KittyBelowBackground,
    KittyBelowText,
    KittyAboveText,
    Overlay,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DrawSummary {
    pub(crate) attempted: u32,
    pub(crate) succeeded: u32,
    pub(crate) skipped_missing: u32,
    pub(crate) skipped_not_ready: u32,
    pub(crate) failed: u32,
}

fn pending_image_from_kitty_snapshot(
    placement: &KittyGraphicsRenderPlacementSnapshot,
) -> Option<PendingImage> {
    Some(PendingImage {
        width: placement.image.width,
        height: placement.image.height,
        pixel_format: PixelFormat::from_kitty_snapshot_format(placement.image.format)?,
        data: placement.image.data.clone(),
    })
}

fn first_index_at_or_after(placements: &[Placement], z: i32) -> u32 {
    placements
        .iter()
        .position(|placement| placement.z >= z)
        .unwrap_or(placements.len()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        KittyGraphicsImageSnapshot, KittyGraphicsRenderPlacementSnapshot,
        RoasttyKittyGraphicsRenderPlacementInfo,
    };

    fn snapshot(
        image_id: u32,
        is_virtual: bool,
        z: i32,
        viewport_col: i32,
        viewport_row: i32,
        data: &[u8],
    ) -> KittyGraphicsRenderPlacementSnapshot {
        KittyGraphicsRenderPlacementSnapshot {
            image: KittyGraphicsImageSnapshot {
                id: image_id,
                number: 0,
                width: 1,
                height: data.len() as u32 / 4,
                format: 1,
                compression: 0,
                data: data.to_vec(),
            },
            info: RoasttyKittyGraphicsRenderPlacementInfo {
                size: std::mem::size_of::<RoasttyKittyGraphicsRenderPlacementInfo>(),
                image_id,
                placement_id: 1,
                is_virtual,
                x_offset: 2,
                y_offset: 3,
                pixel_width: 4,
                pixel_height: 5,
                grid_cols: 1,
                grid_rows: 1,
                viewport_col,
                viewport_row,
                viewport_visible: true,
                source_x: 6,
                source_y: 7,
                source_width: 8,
                source_height: 9,
                z,
            },
            virtual_row: 0,
            virtual_col: 0,
            source_group: crate::KittyGraphicsRenderPlacementSourceGroup::Pinned,
            discovery_index: 0,
        }
    }

    fn pending(width: u32, height: u32, pixel_format: PixelFormat, data: &[u8]) -> PendingImage {
        PendingImage {
            width,
            height,
            pixel_format,
            data: data.to_vec(),
        }
    }

    fn snapshot_with_format(
        image_id: u32,
        format: i32,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> KittyGraphicsRenderPlacementSnapshot {
        let mut snapshot = snapshot(image_id, false, 0, 0, 0, data);
        snapshot.image.format = format;
        snapshot.image.width = width;
        snapshot.image.height = height;
        snapshot
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct FakeTexture(u32);

    #[derive(Default)]
    struct FakeUploadBackend {
        next_texture: u32,
        fail_next: bool,
        uploaded: Vec<PendingImage>,
    }

    impl ImageUploadBackend for FakeUploadBackend {
        type Texture = FakeTexture;
        type Error = ();

        fn upload_image(&mut self, pending: &PendingImage) -> Result<Self::Texture, Self::Error> {
            self.uploaded.push(pending.clone());
            if self.fail_next {
                self.fail_next = false;
                return Err(());
            }

            self.next_texture += 1;
            Ok(FakeTexture(self.next_texture))
        }
    }

    #[derive(Default)]
    struct FakeDrawBackend {
        fail_textures: BTreeSet<FakeTexture>,
        calls: Vec<(FakeTexture, Placement, ImageDrawCall)>,
    }

    impl ImageDrawBackend<FakeTexture> for FakeDrawBackend {
        type Error = ();

        fn draw_image(
            &mut self,
            texture: &FakeTexture,
            placement: Placement,
            call: ImageDrawCall,
        ) -> Result<(), Self::Error> {
            self.calls.push((*texture, placement, call));
            if self.fail_textures.contains(texture) {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn renderer_image_id_z_tie_breaking_matches_upstream_shape() {
        assert!(ImageId::Kitty(1).z_less_than(ImageId::Kitty(2)));
        assert!(!ImageId::Kitty(2).z_less_than(ImageId::Kitty(1)));
        assert!(ImageId::Kitty(1).z_less_than(ImageId::Overlay));
        assert!(!ImageId::Overlay.z_less_than(ImageId::Kitty(1)));
        assert!(!ImageId::Overlay.z_less_than(ImageId::Overlay));
    }

    #[test]
    fn renderer_placement_converts_to_upstream_image_draw_call() {
        let placement = Placement {
            image_id: ImageId::Kitty(9),
            x: -3,
            y: 4,
            z: 0,
            width: 80,
            height: 24,
            cell_offset_x: 5,
            cell_offset_y: 6,
            source_x: 7,
            source_y: 8,
            source_width: 9,
            source_height: 10,
        };

        assert_eq!(
            placement.to_image_draw_call(),
            ImageDrawCall {
                vertex: ImageVertex {
                    grid_pos: [-3.0, 4.0],
                    cell_offset: [5.0, 6.0],
                    source_rect: [7.0, 8.0, 9.0, 10.0],
                    dest_size: [80.0, 24.0],
                },
                primitive: PrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 1,
            }
        );
    }

    #[test]
    fn renderer_pending_image_owns_bytes_and_reports_len() {
        let source = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let image = PendingImage {
            width: 1,
            height: 2,
            pixel_format: PixelFormat::Rgba,
            data: source.clone(),
        };
        assert_eq!(image.len(), 8);
        drop(source);
        assert_eq!(image.data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn renderer_pending_image_prepare_rgba_converts_supported_formats() {
        let mut gray = pending(1, 1, PixelFormat::Gray, &[9]);
        gray.prepare_rgba().unwrap();
        assert_eq!(gray.pixel_format, PixelFormat::Rgba);
        assert_eq!(gray.data, vec![9, 9, 9, 255]);

        let mut gray_alpha = pending(1, 1, PixelFormat::GrayAlpha, &[9, 7]);
        gray_alpha.prepare_rgba().unwrap();
        assert_eq!(gray_alpha.pixel_format, PixelFormat::Rgba);
        assert_eq!(gray_alpha.data, vec![9, 9, 9, 7]);

        let mut rgb = pending(1, 1, PixelFormat::Rgb, &[1, 2, 3]);
        rgb.prepare_rgba().unwrap();
        assert_eq!(rgb.pixel_format, PixelFormat::Rgba);
        assert_eq!(rgb.data, vec![1, 2, 3, 255]);

        let mut rgba = pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]);
        rgba.prepare_rgba().unwrap();
        assert_eq!(rgba.pixel_format, PixelFormat::Rgba);
        assert_eq!(rgba.data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn renderer_pending_image_prepare_rgba_rejects_length_mismatch() {
        let mut image = pending(2, 1, PixelFormat::Rgb, &[1, 2, 3]);
        assert_eq!(
            image.prepare_rgba(),
            Err(ImagePrepareError::LengthMismatch {
                expected: 6,
                actual: 3
            })
        );
        assert_eq!(image.pixel_format, PixelFormat::Rgb);
        assert_eq!(image.data, vec![1, 2, 3]);
    }

    #[test]
    fn renderer_image_mark_for_replace_preserves_ready_texture() {
        let mut image = RendererImage::Ready {
            texture: FakeTexture(1),
            source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
        };
        image.mark_for_replace(pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1]));

        assert_eq!(
            image,
            RendererImage::Replace {
                texture: FakeTexture(1),
                pending: pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1])
            }
        );
        assert!(image.has_texture());
        assert!(image.is_pending());
    }

    #[test]
    fn renderer_image_mark_for_unload_preserves_state_shape() {
        let pending_image = pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]);
        let mut pending_state = RendererImage::<FakeTexture>::Pending(pending_image.clone());
        pending_state.mark_for_unload();
        assert!(matches!(pending_state, RendererImage::UnloadPending(_)));

        let mut ready_state = RendererImage::Ready {
            texture: FakeTexture(1),
            source: pending_image.clone(),
        };
        ready_state.mark_for_unload();
        assert!(matches!(
            ready_state,
            RendererImage::UnloadReady {
                texture: FakeTexture(1),
                ..
            }
        ));

        let mut replace_state = RendererImage::Replace {
            texture: FakeTexture(2),
            pending: pending_image,
        };
        replace_state.mark_for_unload();
        assert!(matches!(
            replace_state,
            RendererImage::UnloadReplace {
                texture: FakeTexture(2),
                ..
            }
        ));
    }

    #[test]
    fn renderer_image_state_updates_pinned_kitty_placements() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[snapshot(7, false, 1, 10, 11, &[1, 2, 3, 4])]);

        assert_eq!(state.images.len(), 1);
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Pending(_))
        ));
        assert_eq!(
            state.kitty_placements,
            vec![Placement {
                image_id: ImageId::Kitty(7),
                x: 10,
                y: 11,
                z: 1,
                width: 4,
                height: 5,
                cell_offset_x: 2,
                cell_offset_y: 3,
                source_x: 6,
                source_y: 7,
                source_width: 8,
                source_height: 9,
            }]
        );
        assert!(!state.kitty_virtual);
    }

    #[test]
    fn renderer_image_state_tracks_virtual_placements() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[snapshot(7, true, -1, 0, 0, &[1, 2, 3, 4])]);
        assert!(state.kitty_virtual);
    }

    #[test]
    fn renderer_image_state_handles_duplicate_same_frame_image_ids() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[
            snapshot(7, false, 0, 0, 0, &[1, 2, 3, 4]),
            snapshot(7, false, 1, 1, 0, &[1, 2, 3, 4]),
        ]);

        assert_eq!(state.images.len(), 1);
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Pending(_))
        ));
        assert_eq!(state.kitty_placements.len(), 2);
    }

    #[test]
    fn renderer_image_state_sorts_and_splits_kitty_layers() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[
            snapshot(3, false, 2, 0, 0, &[1, 2, 3, 4]),
            snapshot(2, false, 2, 0, 0, &[1, 2, 3, 4]),
            snapshot(1, false, i32::MIN / 2 - 1, 0, 0, &[1, 2, 3, 4]),
            snapshot(4, false, -1, 0, 0, &[1, 2, 3, 4]),
        ]);

        assert_eq!(
            state
                .kitty_placements
                .iter()
                .map(|placement| placement.image_id)
                .collect::<Vec<_>>(),
            vec![
                ImageId::Kitty(1),
                ImageId::Kitty(4),
                ImageId::Kitty(2),
                ImageId::Kitty(3)
            ]
        );
        assert_eq!(state.kitty_bg_end, 1);
        assert_eq!(state.kitty_text_end, 2);
    }

    #[test]
    fn renderer_image_state_reuses_unchanged_image_records() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[snapshot(7, false, 0, 0, 0, &[1, 2, 3, 4])]);
        state.update_kitty_from_render_placements(&[snapshot(7, false, 0, 1, 0, &[1, 2, 3, 4])]);

        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Pending(_))
        ));
        assert_eq!(state.kitty_placements.len(), 1);
        assert_eq!(state.kitty_placements[0].x, 1);
    }

    #[test]
    fn renderer_image_state_changed_pending_image_stays_pending() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[snapshot(7, false, 0, 0, 0, &[1, 2, 3, 4])]);
        state.update_kitty_from_render_placements(&[snapshot(7, false, 0, 0, 0, &[4, 3, 2, 1])]);

        let Some(RendererImage::Pending(pending)) = state.images.get(&ImageId::Kitty(7)) else {
            panic!("changed image without a ready texture should stay pending");
        };
        assert_eq!(pending.data, vec![4, 3, 2, 1]);
    }

    #[test]
    fn renderer_image_state_marks_removed_images_for_unload() {
        let mut state = ImageState::<()>::default();
        state.update_kitty_from_render_placements(&[snapshot(7, false, 0, 0, 0, &[1, 2, 3, 4])]);
        state.update_kitty_from_render_placements(&[]);

        let image = state.images.get(&ImageId::Kitty(7)).unwrap();
        assert!(image.is_unloading());
        assert!(state.kitty_placements.is_empty());
    }

    #[test]
    fn renderer_image_state_reappearing_image_cancels_unload() {
        let mut state = ImageState::<()>::default();
        let placement = snapshot(7, false, 0, 0, 0, &[1, 2, 3, 4]);
        state.update_kitty_from_render_placements(std::slice::from_ref(&placement));
        state.update_kitty_from_render_placements(&[]);

        assert!(state.images.get(&ImageId::Kitty(7)).unwrap().is_unloading());

        state.update_kitty_from_render_placements(&[placement]);

        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Pending(_))
        ));
        assert_eq!(state.kitty_placements.len(), 1);
    }

    #[test]
    fn renderer_image_state_ready_absent_becomes_unload_ready() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Ready {
                texture: FakeTexture(1),
                source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
            },
        );

        state.update_kitty_from_render_placements(&[]);

        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::UnloadReady {
                texture: FakeTexture(1),
                ..
            })
        ));
    }

    #[test]
    fn renderer_image_state_reappearing_ready_image_cancels_unload() {
        let mut state = ImageState::<FakeTexture>::default();
        let placement = snapshot(7, false, 0, 0, 0, &[1, 2, 3, 4]);
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Ready {
                texture: FakeTexture(1),
                source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
            },
        );
        state.update_kitty_from_render_placements(&[]);
        state.update_kitty_from_render_placements(&[placement]);

        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Ready {
                texture: FakeTexture(1),
                ..
            })
        ));
    }

    #[test]
    fn renderer_image_state_uploaded_rgb_snapshot_reuses_ready_image() {
        let mut state = ImageState::<FakeTexture>::default();
        let placement = snapshot_with_format(7, 0, 1, 1, &[1, 2, 3]);
        state.update_kitty_from_render_placements(std::slice::from_ref(&placement));
        let mut backend = FakeUploadBackend::default();

        assert!(state.upload(&mut backend));
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Ready {
                texture: FakeTexture(1),
                ..
            })
        ));

        state.update_kitty_from_render_placements(&[placement]);

        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Ready {
                texture: FakeTexture(1),
                ..
            })
        ));
    }

    #[test]
    fn renderer_image_state_changed_ready_reappearance_marks_replace() {
        let mut state = ImageState::<FakeTexture>::default();
        let placement = snapshot(7, false, 0, 0, 0, &[4, 3, 2, 1]);
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Ready {
                texture: FakeTexture(1),
                source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
            },
        );
        state.update_kitty_from_render_placements(&[]);
        state.update_kitty_from_render_placements(&[placement]);

        assert_eq!(
            state.images.get(&ImageId::Kitty(7)),
            Some(&RendererImage::Replace {
                texture: FakeTexture(1),
                pending: pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1])
            })
        );
    }

    #[test]
    fn renderer_image_state_unload_replace_reappearance_retains_texture() {
        let mut state = ImageState::<FakeTexture>::default();
        let replacement = pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1]);
        let placement = snapshot(7, false, 0, 0, 0, &[4, 3, 2, 1]);
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Replace {
                texture: FakeTexture(1),
                pending: replacement.clone(),
            },
        );
        state.update_kitty_from_render_placements(&[]);
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::UnloadReplace {
                texture: FakeTexture(1),
                ..
            })
        ));

        state.update_kitty_from_render_placements(&[placement]);

        assert_eq!(
            state.images.get(&ImageId::Kitty(7)),
            Some(&RendererImage::Replace {
                texture: FakeTexture(1),
                pending: replacement
            })
        );
    }

    #[test]
    fn renderer_image_state_upload_removes_unloading_images() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::UnloadReady {
                texture: FakeTexture(1),
                source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
            },
        );
        let mut backend = FakeUploadBackend::default();

        assert!(state.upload(&mut backend));
        assert!(!state.images.contains_key(&ImageId::Kitty(7)));
        assert!(backend.uploaded.is_empty());
    }

    #[test]
    fn renderer_image_state_upload_pending_becomes_ready() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Pending(pending(1, 1, PixelFormat::Rgb, &[1, 2, 3])),
        );
        let mut backend = FakeUploadBackend::default();

        assert!(state.upload(&mut backend));
        assert_eq!(
            backend.uploaded,
            vec![pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 255])]
        );
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Ready {
                texture: FakeTexture(1),
                ..
            })
        ));
    }

    #[test]
    fn renderer_image_state_upload_replacement_becomes_new_ready_texture() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Replace {
                texture: FakeTexture(1),
                pending: pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1]),
            },
        );
        let mut backend = FakeUploadBackend {
            next_texture: 1,
            ..Default::default()
        };

        assert!(state.upload(&mut backend));
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Ready {
                texture: FakeTexture(2),
                ..
            })
        ));
    }

    #[test]
    fn renderer_image_state_upload_failure_leaves_pending_retryable() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Pending(pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4])),
        );
        let mut backend = FakeUploadBackend {
            fail_next: true,
            ..Default::default()
        };

        assert!(!state.upload(&mut backend));
        assert_eq!(
            state.images.get(&ImageId::Kitty(7)),
            Some(&RendererImage::Pending(pending(
                1,
                1,
                PixelFormat::Rgba,
                &[1, 2, 3, 4]
            )))
        );
    }

    #[test]
    fn renderer_image_state_replacement_failure_keeps_old_texture_and_pending() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Replace {
                texture: FakeTexture(1),
                pending: pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1]),
            },
        );
        let mut backend = FakeUploadBackend {
            fail_next: true,
            ..Default::default()
        };

        assert!(!state.upload(&mut backend));
        assert_eq!(
            state.images.get(&ImageId::Kitty(7)),
            Some(&RendererImage::Replace {
                texture: FakeTexture(1),
                pending: pending(1, 1, PixelFormat::Rgba, &[4, 3, 2, 1])
            })
        );
    }

    #[test]
    fn renderer_image_state_mixed_upload_applies_successes_and_reports_failure() {
        let mut state = ImageState::<FakeTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Pending(pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4])),
        );
        state.images.insert(
            ImageId::Kitty(8),
            RendererImage::Pending(pending(2, 1, PixelFormat::Rgba, &[1, 2, 3, 4])),
        );
        let mut backend = FakeUploadBackend::default();

        assert!(!state.upload(&mut backend));
        assert!(matches!(
            state.images.get(&ImageId::Kitty(7)),
            Some(RendererImage::Ready { .. })
        ));
        assert!(matches!(
            state.images.get(&ImageId::Kitty(8)),
            Some(RendererImage::Pending(_))
        ));
    }

    #[test]
    fn renderer_image_state_draw_uses_kitty_layer_buckets() {
        let mut state = ImageState::<FakeTexture>::default();
        state.kitty_placements = vec![
            Placement {
                image_id: ImageId::Kitty(1),
                x: 1,
                y: 0,
                z: i32::MIN / 2 - 1,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
            Placement {
                image_id: ImageId::Kitty(2),
                x: 2,
                y: 0,
                z: -1,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
            Placement {
                image_id: ImageId::Kitty(3),
                x: 3,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
        ];
        state.kitty_bg_end = 1;
        state.kitty_text_end = 2;
        for id in 1..=3 {
            state.images.insert(
                ImageId::Kitty(id),
                RendererImage::Ready {
                    texture: FakeTexture(id),
                    source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
                },
            );
        }

        let mut backend = FakeDrawBackend::default();
        assert_eq!(
            state.draw(DrawPlacements::KittyBelowBackground, &mut backend),
            DrawSummary {
                attempted: 1,
                succeeded: 1,
                skipped_missing: 0,
                skipped_not_ready: 0,
                failed: 0,
            }
        );
        assert_eq!(backend.calls[0].0, FakeTexture(1));
        assert_eq!(backend.calls[0].2.vertex.grid_pos, [1.0, 0.0]);
        assert_eq!(backend.calls[0].2.primitive, PrimitiveType::TriangleStrip);
        assert_eq!(backend.calls[0].2.vertex_count, 4);
        assert_eq!(backend.calls[0].2.instance_count, 1);

        backend.calls.clear();
        assert_eq!(
            state.draw(DrawPlacements::KittyBelowText, &mut backend),
            DrawSummary {
                attempted: 1,
                succeeded: 1,
                skipped_missing: 0,
                skipped_not_ready: 0,
                failed: 0,
            }
        );
        assert_eq!(backend.calls[0].0, FakeTexture(2));
        assert_eq!(backend.calls[0].2.vertex.grid_pos, [2.0, 0.0]);
        assert_eq!(backend.calls[0].2.primitive, PrimitiveType::TriangleStrip);
        assert_eq!(backend.calls[0].2.vertex_count, 4);
        assert_eq!(backend.calls[0].2.instance_count, 1);

        backend.calls.clear();
        assert_eq!(
            state.draw(DrawPlacements::KittyAboveText, &mut backend),
            DrawSummary {
                attempted: 1,
                succeeded: 1,
                skipped_missing: 0,
                skipped_not_ready: 0,
                failed: 0,
            }
        );
        assert_eq!(backend.calls[0].0, FakeTexture(3));
        assert_eq!(backend.calls[0].2.vertex.grid_pos, [3.0, 0.0]);
        assert_eq!(backend.calls[0].2.primitive, PrimitiveType::TriangleStrip);
        assert_eq!(backend.calls[0].2.vertex_count, 4);
        assert_eq!(backend.calls[0].2.instance_count, 1);
    }

    #[test]
    fn renderer_image_state_draw_skips_missing_and_not_ready_images() {
        let mut state = ImageState::<FakeTexture>::default();
        state.kitty_placements = vec![
            Placement {
                image_id: ImageId::Kitty(1),
                x: 0,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
            Placement {
                image_id: ImageId::Kitty(2),
                x: 0,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
            Placement {
                image_id: ImageId::Kitty(3),
                x: 0,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
        ];
        state.kitty_text_end = 0;
        state.images.insert(
            ImageId::Kitty(1),
            RendererImage::Ready {
                texture: FakeTexture(1),
                source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
            },
        );
        state.images.insert(
            ImageId::Kitty(2),
            RendererImage::Pending(pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4])),
        );
        let mut backend = FakeDrawBackend::default();

        assert_eq!(
            state.draw(DrawPlacements::KittyAboveText, &mut backend),
            DrawSummary {
                attempted: 1,
                succeeded: 1,
                skipped_missing: 1,
                skipped_not_ready: 1,
                failed: 0,
            }
        );
        assert_eq!(backend.calls.len(), 1);
    }

    #[test]
    fn renderer_image_state_draw_ignores_errors_and_continues() {
        let mut state = ImageState::<FakeTexture>::default();
        state.kitty_placements = vec![
            Placement {
                image_id: ImageId::Kitty(1),
                x: 0,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
            Placement {
                image_id: ImageId::Kitty(2),
                x: 0,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                cell_offset_x: 0,
                cell_offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: 1,
                source_height: 1,
            },
        ];
        state.kitty_text_end = 0;
        for id in 1..=2 {
            state.images.insert(
                ImageId::Kitty(id),
                RendererImage::Ready {
                    texture: FakeTexture(id),
                    source: pending(1, 1, PixelFormat::Rgba, &[1, 2, 3, 4]),
                },
            );
        }
        let mut backend = FakeDrawBackend::default();
        backend.fail_textures.insert(FakeTexture(1));

        assert_eq!(
            state.draw(DrawPlacements::KittyAboveText, &mut backend),
            DrawSummary {
                attempted: 2,
                succeeded: 1,
                skipped_missing: 0,
                skipped_not_ready: 0,
                failed: 1,
            }
        );
        assert_eq!(backend.calls.len(), 2);
    }

    #[test]
    fn renderer_image_state_overlay_draw_bucket_is_empty_until_overlay_update() {
        let state = ImageState::<FakeTexture>::default();
        let mut backend = FakeDrawBackend::default();

        assert_eq!(
            state.draw(DrawPlacements::Overlay, &mut backend),
            DrawSummary::default()
        );
        assert!(backend.calls.is_empty());
    }
}
