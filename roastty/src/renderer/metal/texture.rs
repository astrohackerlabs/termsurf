use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLOrigin, MTLRegion, MTLSize, MTLTexture, MTLTextureDescriptor};

use crate::font::atlas::{Atlas, Format};
use crate::renderer::image::{ImageUploadBackend, PendingImage, PixelFormat};
use crate::renderer::metal::api::{
    MetalPixelFormat, MetalResourceOptions, MetalStorageMode, MetalTextureUsage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageTextureFormat {
    Gray,
    Rgba,
    Bgra,
}

impl ImageTextureFormat {
    pub(crate) fn to_pixel_format(self, srgb: bool) -> MetalPixelFormat {
        match (self, srgb) {
            (ImageTextureFormat::Gray, false) => MetalPixelFormat::R8Unorm,
            (ImageTextureFormat::Gray, true) => MetalPixelFormat::R8UnormSrgb,
            (ImageTextureFormat::Rgba, false) => MetalPixelFormat::Rgba8Unorm,
            (ImageTextureFormat::Rgba, true) => MetalPixelFormat::Rgba8UnormSrgb,
            (ImageTextureFormat::Bgra, false) => MetalPixelFormat::Bgra8Unorm,
            (ImageTextureFormat::Bgra, true) => MetalPixelFormat::Bgra8UnormSrgb,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImageTextureOptions {
    pub(crate) pixel_format: MetalPixelFormat,
    pub(crate) resource_options: MetalResourceOptions,
    pub(crate) usage: MetalTextureUsage,
}

pub(crate) fn image_texture_options(
    format: ImageTextureFormat,
    srgb: bool,
    storage_mode: MetalStorageMode,
) -> ImageTextureOptions {
    ImageTextureOptions {
        pixel_format: format.to_pixel_format(srgb),
        resource_options: MetalResourceOptions::image(storage_mode),
        usage: MetalTextureUsage::shader_read(),
    }
}

pub(crate) fn render_target_texture_options(
    pixel_format: MetalPixelFormat,
    storage_mode: MetalStorageMode,
) -> ImageTextureOptions {
    ImageTextureOptions {
        pixel_format,
        resource_options: MetalResourceOptions::image(storage_mode),
        usage: MetalTextureUsage {
            shader_read: false,
            render_target: true,
        },
    }
}

pub(crate) fn image_texture_format_for_upload_pixel_format(
    format: PixelFormat,
) -> Option<ImageTextureFormat> {
    match format {
        PixelFormat::Rgba => Some(ImageTextureFormat::Rgba),
        PixelFormat::Gray | PixelFormat::GrayAlpha | PixelFormat::Rgb => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MetalTextureError {
    InvalidPixelFormat(MetalPixelFormat),
    ByteLengthMismatch {
        expected: usize,
        actual: usize,
    },
    RegionOutOfBounds {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        texture_width: usize,
        texture_height: usize,
    },
    TextureCreationFailed,
    UnsupportedUploadPixelFormat(PixelFormat),
    UnsupportedAtlasFormat(Format),
}

pub(crate) struct MetalTexture {
    texture: Retained<ProtocolObject<dyn MTLTexture>>,
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
}

impl MetalTexture {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        options: ImageTextureOptions,
        width: usize,
        height: usize,
        data: Option<&[u8]>,
    ) -> Result<Self, MetalTextureError> {
        let bytes_per_pixel = options
            .pixel_format
            .bytes_per_pixel()
            .ok_or(MetalTextureError::InvalidPixelFormat(options.pixel_format))?;

        if let Some(data) = data {
            let expected = texture_byte_len(width, height, bytes_per_pixel)?;
            if data.len() != expected {
                return Err(MetalTextureError::ByteLengthMismatch {
                    expected,
                    actual: data.len(),
                });
            }
        }

        let descriptor = MTLTextureDescriptor::new();
        descriptor.setPixelFormat(options.pixel_format.to_objc());
        unsafe {
            descriptor.setWidth(width);
            descriptor.setHeight(height);
        }
        descriptor.setResourceOptions(options.resource_options.to_objc());
        descriptor.setUsage(options.usage.to_objc());

        let texture = device
            .newTextureWithDescriptor(&descriptor)
            .ok_or(MetalTextureError::TextureCreationFailed)?;

        if let Some(data) = data {
            if !data.is_empty() {
                let region = full_region(width, height);
                let bytes = NonNull::new(data.as_ptr().cast_mut().cast()).expect("non-empty data");
                unsafe {
                    texture.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                        region,
                        0,
                        bytes,
                        width * bytes_per_pixel,
                    );
                }
            }
        }

        Ok(Self {
            texture,
            width,
            height,
            bytes_per_pixel,
        })
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }

    pub(crate) fn height(&self) -> usize {
        self.height
    }

    pub(crate) fn bytes_per_pixel(&self) -> usize {
        self.bytes_per_pixel
    }

    pub(crate) fn texture(&self) -> &ProtocolObject<dyn MTLTexture> {
        &self.texture
    }

    pub(crate) fn read_bytes(&self) -> Vec<u8> {
        let len = texture_byte_len(self.width, self.height, self.bytes_per_pixel)
            .expect("test texture dimensions fit in usize");
        let mut bytes = vec![0; len];
        if !bytes.is_empty() {
            let ptr = NonNull::new(bytes.as_mut_ptr().cast()).expect("non-empty bytes");
            unsafe {
                self.texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                    ptr,
                    self.width * self.bytes_per_pixel,
                    full_region(self.width, self.height),
                    0,
                );
            }
        }
        bytes
    }

    /// Replace the pixels of the `[x, x+width) × [y, y+height)` region with
    /// `data` (tightly packed, `width × height × bytes_per_pixel` bytes). The GPU
    /// operation behind upstream's `syncAtlasTexture` re-upload
    /// (`replaceRegion:mipmapLevel:withBytes:bytesPerRow:`); the full-region form
    /// (`replace_region(0, 0, w, h, data)`) matches upstream's
    /// `replaceRegion(0, 0, size, size, data)`.
    pub(crate) fn replace_region(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        data: &[u8],
    ) -> Result<(), MetalTextureError> {
        // The region must fit inside the texture. Subtraction-based to avoid the
        // overflow an `x + width` check would risk.
        if x > self.width || width > self.width - x || y > self.height || height > self.height - y {
            return Err(MetalTextureError::RegionOutOfBounds {
                x,
                y,
                width,
                height,
                texture_width: self.width,
                texture_height: self.height,
            });
        }

        // The data must be exactly `width × height × bytes_per_pixel`.
        let expected = texture_byte_len(width, height, self.bytes_per_pixel)?;
        if data.len() != expected {
            return Err(MetalTextureError::ByteLengthMismatch {
                expected,
                actual: data.len(),
            });
        }

        if !data.is_empty() {
            let region = MTLRegion {
                origin: MTLOrigin { x, y, z: 0 },
                size: MTLSize {
                    width,
                    height,
                    depth: 1,
                },
            };
            let bytes = NonNull::new(data.as_ptr().cast_mut().cast()).expect("non-empty data");
            unsafe {
                self.texture
                    .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                        region,
                        0,
                        bytes,
                        width * self.bytes_per_pixel,
                    );
            }
        }

        Ok(())
    }
}

/// Create a GPU texture sized to and matching the format of `atlas` (upstream
/// `initAtlasTexture`). The font-atlas pixel formats map to Metal as
/// `Grayscale → R8Unorm` and `Bgra → Bgra8UnormSrgb`; `Bgr` has no Metal pixel
/// format and is rejected (where upstream `@panic`s). The texture is square
/// (`atlas.size × atlas.size`), shader-read, with no initial data.
pub(crate) fn init_atlas_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    storage_mode: MetalStorageMode,
    atlas: &Atlas,
) -> Result<MetalTexture, MetalTextureError> {
    let (format, srgb) = match atlas.format() {
        Format::Grayscale => (ImageTextureFormat::Gray, false),
        Format::Bgra => (ImageTextureFormat::Bgra, true),
        Format::Bgr => return Err(MetalTextureError::UnsupportedAtlasFormat(atlas.format())),
    };
    let size = atlas.size() as usize;
    MetalTexture::new(
        device,
        image_texture_options(format, srgb, storage_mode),
        size,
        size,
        None,
    )
}

/// Sync `atlas`'s pixels into `texture`, reallocating it first if the atlas has
/// grown past the texture (upstream `syncAtlasTexture`): reallocate when
/// `atlas.size > texture.width`, then re-upload the whole atlas via
/// `replace_region(0, 0, size, size, atlas.data())`.
pub(crate) fn sync_atlas_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    storage_mode: MetalStorageMode,
    texture: &mut MetalTexture,
    atlas: &Atlas,
) -> Result<(), MetalTextureError> {
    let size = atlas.size() as usize;
    if size > texture.width() {
        *texture = init_atlas_texture(device, storage_mode, atlas)?;
    }
    texture.replace_region(0, 0, size, size, atlas.data())
}

/// A frame's atlas texture plus the last atlas `modified` value it was synced
/// to. Mirrors upstream's per-frame `grayscale` / `color` texture + the
/// `grayscale_modified` / `color_modified` counters: the texture is re-uploaded
/// only when the atlas's `modified` counter has advanced (upstream's `drawFrame`
/// `texture:` gate).
pub(crate) struct FrameAtlasTexture {
    texture: MetalTexture,
    last_modified: usize,
}

impl FrameAtlasTexture {
    /// Create the frame's atlas texture, sized/formatted to `atlas` but not yet
    /// uploaded (`last_modified = 0`, so the first `sync_if_modified` runs).
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        storage_mode: MetalStorageMode,
        atlas: &Atlas,
    ) -> Result<Self, MetalTextureError> {
        Ok(Self {
            texture: init_atlas_texture(device, storage_mode, atlas)?,
            last_modified: 0,
        })
    }

    /// Upload `atlas` only if its `modified` counter advanced past the last sync.
    /// Returns whether a sync happened (upstream's `texture:` gate). The atlas
    /// counter is read once and recorded before the sync (upstream's
    /// store-before-sync order); the live `font_grid` shared lock is deferred.
    pub(crate) fn sync_if_modified(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        storage_mode: MetalStorageMode,
        atlas: &Atlas,
    ) -> Result<bool, MetalTextureError> {
        let modified = atlas.modified();
        if modified <= self.last_modified {
            return Ok(false);
        }
        self.last_modified = modified;
        sync_atlas_texture(device, storage_mode, &mut self.texture, atlas)?;
        Ok(true)
    }

    /// The GPU texture (bound at the cell-text draw step).
    pub(crate) fn texture(&self) -> &MetalTexture {
        &self.texture
    }
}

pub(crate) struct MetalImageUploadBackend<'a> {
    device: &'a ProtocolObject<dyn MTLDevice>,
    storage_mode: MetalStorageMode,
    srgb: bool,
}

impl<'a> MetalImageUploadBackend<'a> {
    pub(crate) fn new(
        device: &'a ProtocolObject<dyn MTLDevice>,
        storage_mode: MetalStorageMode,
        srgb: bool,
    ) -> Self {
        Self {
            device,
            storage_mode,
            srgb,
        }
    }
}

impl ImageUploadBackend for MetalImageUploadBackend<'_> {
    type Texture = MetalTexture;
    type Error = MetalTextureError;

    fn upload_image(&mut self, pending: &PendingImage) -> Result<Self::Texture, Self::Error> {
        let format = image_texture_format_for_upload_pixel_format(pending.pixel_format).ok_or(
            MetalTextureError::UnsupportedUploadPixelFormat(pending.pixel_format),
        )?;
        let options = image_texture_options(format, self.srgb, self.storage_mode);
        MetalTexture::new(
            self.device,
            options,
            pending.width as usize,
            pending.height as usize,
            Some(&pending.data),
        )
    }
}

fn texture_byte_len(
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<usize, MetalTextureError> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or(MetalTextureError::ByteLengthMismatch {
            expected: usize::MAX,
            actual: 0,
        })
}

fn full_region(width: usize, height: usize) -> MTLRegion {
    MTLRegion {
        origin: MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width,
            height,
            depth: 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::image::{ImageId, ImageState, RendererImage};
    use crate::renderer::metal::api::{
        MetalCpuCacheMode, MetalHazardTrackingMode, MetalStorageMode,
    };
    use objc2_metal::MTLCreateSystemDefaultDevice;

    fn metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("Roastty requires a Metal device")
    }

    fn texture_error(result: Result<MetalTexture, MetalTextureError>) -> MetalTextureError {
        match result {
            Ok(_) => panic!("expected texture creation to fail"),
            Err(error) => error,
        }
    }

    #[test]
    fn image_texture_format_maps_to_metal_pixel_formats() {
        assert_eq!(
            ImageTextureFormat::Gray.to_pixel_format(false),
            MetalPixelFormat::R8Unorm
        );
        assert_eq!(
            ImageTextureFormat::Gray.to_pixel_format(true),
            MetalPixelFormat::R8UnormSrgb
        );
        assert_eq!(
            ImageTextureFormat::Rgba.to_pixel_format(false),
            MetalPixelFormat::Rgba8Unorm
        );
        assert_eq!(
            ImageTextureFormat::Rgba.to_pixel_format(true),
            MetalPixelFormat::Rgba8UnormSrgb
        );
        assert_eq!(
            ImageTextureFormat::Bgra.to_pixel_format(false),
            MetalPixelFormat::Bgra8Unorm
        );
        assert_eq!(
            ImageTextureFormat::Bgra.to_pixel_format(true),
            MetalPixelFormat::Bgra8UnormSrgb
        );
    }

    #[test]
    fn image_texture_options_match_upstream_intent() {
        let options =
            image_texture_options(ImageTextureFormat::Rgba, true, MetalStorageMode::Managed);
        assert_eq!(options.pixel_format, MetalPixelFormat::Rgba8UnormSrgb);
        assert_eq!(
            options.resource_options,
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::WriteCombined,
                storage_mode: MetalStorageMode::Managed,
                hazard_tracking_mode: MetalHazardTrackingMode::Default,
            }
        );
        assert_eq!(options.resource_options.raw(), 0x11);
        assert_eq!(options.usage, MetalTextureUsage::shader_read());
        assert_eq!(options.usage.raw(), 0x1);
    }

    #[test]
    fn render_target_texture_options_match_offscreen_readback_intent() {
        let options =
            render_target_texture_options(MetalPixelFormat::Bgra8Unorm, MetalStorageMode::Shared);

        assert_eq!(options.pixel_format, MetalPixelFormat::Bgra8Unorm);
        assert_eq!(
            options.resource_options,
            MetalResourceOptions::image(MetalStorageMode::Shared)
        );
        assert_eq!(
            options.usage,
            MetalTextureUsage {
                shader_read: false,
                render_target: true,
            }
        );
    }

    #[test]
    fn upload_pixel_format_bridge_accepts_only_prepared_rgba() {
        assert_eq!(
            image_texture_format_for_upload_pixel_format(PixelFormat::Rgba),
            Some(ImageTextureFormat::Rgba)
        );
        assert_eq!(
            image_texture_format_for_upload_pixel_format(PixelFormat::Gray),
            None
        );
        assert_eq!(
            image_texture_format_for_upload_pixel_format(PixelFormat::GrayAlpha),
            None
        );
        assert_eq!(
            image_texture_format_for_upload_pixel_format(PixelFormat::Rgb),
            None
        );
    }

    #[test]
    fn invalid_pixel_format_fails_before_texture_creation() {
        let device = metal_device();
        let options = ImageTextureOptions {
            pixel_format: MetalPixelFormat::Invalid,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            usage: MetalTextureUsage::shader_read(),
        };

        assert_eq!(
            texture_error(MetalTexture::new(
                &device,
                options,
                1,
                1,
                Some(&[1, 2, 3, 4])
            )),
            MetalTextureError::InvalidPixelFormat(MetalPixelFormat::Invalid)
        );
    }

    #[test]
    fn byte_length_mismatch_fails_before_texture_creation() {
        let device = metal_device();
        let options =
            image_texture_options(ImageTextureFormat::Rgba, false, MetalStorageMode::Shared);

        assert_eq!(
            texture_error(MetalTexture::new(&device, options, 1, 1, Some(&[1, 2, 3]))),
            MetalTextureError::ByteLengthMismatch {
                expected: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn live_texture_creation_writes_initial_rgba_bytes() {
        let device = metal_device();
        let options =
            image_texture_options(ImageTextureFormat::Rgba, false, MetalStorageMode::Shared);
        let texture = MetalTexture::new(&device, options, 1, 1, Some(&[10, 20, 30, 40]))
            .expect("create initialized texture");

        assert_eq!(texture.width(), 1);
        assert_eq!(texture.height(), 1);
        assert_eq!(texture.bytes_per_pixel(), 4);
        assert_eq!(texture.read_bytes(), [10, 20, 30, 40]);
    }

    #[test]
    fn metal_upload_backend_uploads_prepared_rgba_image() {
        let device = metal_device();
        let mut backend = MetalImageUploadBackend::new(&device, MetalStorageMode::Shared, false);
        let pending = PendingImage {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::Rgba,
            data: vec![1, 2, 3, 4],
        };

        let texture = backend
            .upload_image(&pending)
            .expect("upload prepared RGBA image");

        assert_eq!(texture.width(), 1);
        assert_eq!(texture.height(), 1);
        assert_eq!(texture.bytes_per_pixel(), 4);
        assert_eq!(texture.read_bytes(), [1, 2, 3, 4]);
    }

    #[test]
    fn metal_upload_backend_rejects_unprepared_image() {
        let device = metal_device();
        let mut backend = MetalImageUploadBackend::new(&device, MetalStorageMode::Shared, false);
        let pending = PendingImage {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::Rgb,
            data: vec![1, 2, 3],
        };

        assert_eq!(
            texture_error(backend.upload_image(&pending)),
            MetalTextureError::UnsupportedUploadPixelFormat(PixelFormat::Rgb)
        );
    }

    #[test]
    fn image_state_upload_moves_pending_image_to_ready_with_metal_backend() {
        let device = metal_device();
        let mut backend = MetalImageUploadBackend::new(&device, MetalStorageMode::Shared, false);
        let mut state = ImageState::<MetalTexture>::default();
        state.images.insert(
            ImageId::Kitty(7),
            RendererImage::Pending(PendingImage {
                width: 1,
                height: 1,
                pixel_format: PixelFormat::Rgb,
                data: vec![5, 6, 7],
            }),
        );

        assert!(state.upload(&mut backend));
        let image = state
            .images
            .get(&ImageId::Kitty(7))
            .expect("uploaded image remains tracked");
        let RendererImage::Ready { texture, source } = image else {
            panic!("image should be ready after upload");
        };
        assert_eq!(source.pixel_format, PixelFormat::Rgb);
        assert_eq!(texture.width(), 1);
        assert_eq!(texture.height(), 1);
        assert_eq!(texture.bytes_per_pixel(), 4);
        assert_eq!(texture.read_bytes(), [5, 6, 7, 255]);
    }

    fn gray_texture(
        device: &ProtocolObject<dyn MTLDevice>,
        width: usize,
        height: usize,
        data: &[u8],
    ) -> MetalTexture {
        MetalTexture::new(
            device,
            image_texture_options(ImageTextureFormat::Gray, false, MetalStorageMode::Shared),
            width,
            height,
            Some(data),
        )
        .expect("grayscale texture should be created")
    }

    #[test]
    fn replace_region_full_region_overwrites_all_pixels() {
        let device = metal_device();
        let texture = gray_texture(&device, 2, 2, &[1, 2, 3, 4]);

        texture
            .replace_region(0, 0, 2, 2, &[10, 20, 30, 40])
            .expect("full-region replace should succeed");

        assert_eq!(texture.read_bytes(), [10, 20, 30, 40]);
    }

    #[test]
    fn replace_region_sub_region_writes_at_offset() {
        let device = metal_device();
        let texture = gray_texture(&device, 4, 4, &[0; 16]);

        // Write a 2×2 block at origin (1, 1). Row-major index is `y * 4 + x`, so
        // the block lands at (1,1)=1, (2,1)=2, (1,2)=3, (2,2)=4.
        texture
            .replace_region(1, 1, 2, 2, &[1, 2, 3, 4])
            .expect("sub-region replace should succeed");

        assert_eq!(
            texture.read_bytes(),
            [0, 0, 0, 0, 0, 1, 2, 0, 0, 3, 4, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn replace_region_rejects_wrong_data_length() {
        let device = metal_device();
        let texture = gray_texture(&device, 2, 2, &[0; 4]);

        // A 2×2 region needs 4 bytes; 3 is short.
        assert_eq!(
            texture.replace_region(0, 0, 2, 2, &[1, 2, 3]),
            Err(MetalTextureError::ByteLengthMismatch {
                expected: 4,
                actual: 3,
            })
        );
    }

    #[test]
    fn replace_region_rejects_out_of_bounds_region() {
        let device = metal_device();
        let texture = gray_texture(&device, 4, 4, &[0; 16]);

        // Origin (3, 3) with a 2×2 region exceeds the 4×4 texture.
        assert_eq!(
            texture.replace_region(3, 3, 2, 2, &[1, 2, 3, 4]),
            Err(MetalTextureError::RegionOutOfBounds {
                x: 3,
                y: 3,
                width: 2,
                height: 2,
                texture_width: 4,
                texture_height: 4,
            })
        );
    }

    fn grayscale_atlas_with_pixel(size: u32, value: u8) -> Atlas {
        let mut atlas = Atlas::new(size, Format::Grayscale);
        let region = atlas.reserve(1, 1).expect("reserve a 1×1 region");
        atlas.set(region, &[value]);
        atlas
    }

    #[test]
    fn sync_atlas_texture_reallocates_when_atlas_grew() {
        let device = metal_device();
        let atlas = grayscale_atlas_with_pixel(4, 200);
        // The initial texture (2×2) is smaller than the 4×4 atlas.
        let mut texture = gray_texture(&device, 2, 2, &[0; 4]);

        sync_atlas_texture(&device, MetalStorageMode::Shared, &mut texture, &atlas)
            .expect("atlas sync should succeed");

        // Reallocated to the atlas size, holding the atlas pixels verbatim.
        assert_eq!(texture.width(), 4);
        assert_eq!(texture.height(), 4);
        assert_eq!(texture.read_bytes(), atlas.data());
    }

    #[test]
    fn sync_atlas_texture_uploads_sub_region_without_realloc() {
        let device = metal_device();
        let atlas = grayscale_atlas_with_pixel(4, 200);
        // The initial texture (6×6) already fits the 4×4 atlas.
        let mut texture = gray_texture(&device, 6, 6, &[0; 36]);

        sync_atlas_texture(&device, MetalStorageMode::Shared, &mut texture, &atlas)
            .expect("atlas sync should succeed");

        // No reallocation: the texture stays 6×6 and the atlas pixels land in the
        // top-left 4×4 block, zero elsewhere.
        assert_eq!(texture.width(), 6);
        let atlas_data = atlas.data();
        let mut expected = vec![0u8; 36];
        for y in 0..4 {
            for x in 0..4 {
                expected[y * 6 + x] = atlas_data[y * 4 + x];
            }
        }
        assert_eq!(texture.read_bytes(), expected);
    }

    #[test]
    fn init_atlas_texture_maps_formats_and_rejects_bgr() {
        use objc2_metal::MTLTexture;

        let device = metal_device();

        let grayscale = Atlas::new(4, Format::Grayscale);
        let gray_texture = init_atlas_texture(&device, MetalStorageMode::Shared, &grayscale)
            .expect("grayscale atlas texture");
        assert_eq!(
            gray_texture.texture().pixelFormat(),
            MetalPixelFormat::R8Unorm.to_objc()
        );
        assert_eq!(gray_texture.bytes_per_pixel(), 1);
        assert_eq!(gray_texture.width(), 4);

        let bgra = Atlas::new(4, Format::Bgra);
        let bgra_texture = init_atlas_texture(&device, MetalStorageMode::Shared, &bgra)
            .expect("bgra atlas texture");
        assert_eq!(
            bgra_texture.texture().pixelFormat(),
            MetalPixelFormat::Bgra8UnormSrgb.to_objc()
        );
        assert_eq!(bgra_texture.bytes_per_pixel(), 4);

        let bgr = Atlas::new(4, Format::Bgr);
        assert_eq!(
            init_atlas_texture(&device, MetalStorageMode::Shared, &bgr).err(),
            Some(MetalTextureError::UnsupportedAtlasFormat(Format::Bgr))
        );
    }

    #[test]
    fn frame_atlas_texture_syncs_first_then_skips_unchanged() {
        let device = metal_device();

        let mut atlas = Atlas::new(4, Format::Grayscale);
        let region = atlas.reserve(1, 1).expect("reserve a 1×1 region");
        atlas.set(region, &[200]);

        let mut frame_tex = FrameAtlasTexture::new(&device, MetalStorageMode::Shared, &atlas)
            .expect("frame atlas texture should be created");

        // The atlas changed since `last_modified == 0`, so the first sync runs.
        assert!(frame_tex
            .sync_if_modified(&device, MetalStorageMode::Shared, &atlas)
            .expect("first sync should succeed"));
        assert_eq!(frame_tex.texture().read_bytes(), atlas.data());

        // No further change → the second sync is skipped.
        assert!(!frame_tex
            .sync_if_modified(&device, MetalStorageMode::Shared, &atlas)
            .expect("second sync should succeed"));
    }

    #[test]
    fn frame_atlas_texture_resyncs_after_change() {
        let device = metal_device();

        let mut atlas = Atlas::new(4, Format::Grayscale);
        let region = atlas.reserve(1, 1).expect("reserve a 1×1 region");
        atlas.set(region, &[200]);

        let mut frame_tex = FrameAtlasTexture::new(&device, MetalStorageMode::Shared, &atlas)
            .expect("frame atlas texture should be created");
        assert!(frame_tex
            .sync_if_modified(&device, MetalStorageMode::Shared, &atlas)
            .expect("first sync should succeed"));

        // A further change advances the atlas `modified` counter → resync.
        let region2 = atlas.reserve(1, 1).expect("reserve a second 1×1 region");
        atlas.set(region2, &[100]);

        assert!(frame_tex
            .sync_if_modified(&device, MetalStorageMode::Shared, &atlas)
            .expect("resync should succeed"));
        assert_eq!(frame_tex.texture().read_bytes(), atlas.data());
    }
}
