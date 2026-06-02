use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLOrigin, MTLRegion, MTLSize, MTLTexture, MTLTextureDescriptor};

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
    ByteLengthMismatch { expected: usize, actual: usize },
    TextureCreationFailed,
    UnsupportedUploadPixelFormat(PixelFormat),
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
}
