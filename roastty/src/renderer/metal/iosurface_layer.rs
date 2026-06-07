use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_io_surface::IOSurfaceRef;
use objc2_quartz_core::{kCAGravityTopLeft, CALayer};

pub(crate) struct MetalIOSurfaceLayer {
    layer: Retained<CALayer>,
}

impl MetalIOSurfaceLayer {
    pub(crate) fn new() -> Self {
        let layer = CALayer::layer();
        layer.setContentsGravity(unsafe { kCAGravityTopLeft });
        Self { layer }
    }

    pub(crate) fn layer(&self) -> &CALayer {
        &self.layer
    }

    pub(crate) fn set_bounds_pixels(&self, width: f64, height: f64, scale: f64) {
        self.layer
            .setBounds(CGRect::new(CGPoint::ZERO, CGSize::new(width, height)));
        self.layer.setContentsScale(scale);
    }

    pub(crate) fn expected_pixel_size(&self) -> (usize, usize) {
        let bounds = self.layer.bounds();
        let scale = self.layer.contentsScale();
        (
            (bounds.size.width * scale) as usize,
            (bounds.size.height * scale) as usize,
        )
    }

    pub(crate) fn set_surface_sync(&self, surface: &IOSurfaceRef) {
        unsafe {
            self.layer
                .setContents(Some(iosurface_as_any_object(surface)));
        }
    }

    pub(crate) fn set_surface_if_size_matches(&self, surface: &IOSurfaceRef) -> bool {
        let (width, height) = self.expected_pixel_size();
        if width != surface.width() || height != surface.height() {
            return false;
        }
        self.set_surface_sync(surface);
        true
    }
}

fn iosurface_identity(surface: &IOSurfaceRef) -> *const AnyObject {
    surface as *const IOSurfaceRef as *const AnyObject
}

unsafe fn iosurface_as_any_object(surface: &IOSurfaceRef) -> &AnyObject {
    &*iosurface_identity(surface)
}

#[cfg(test)]
mod tests {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_foundation::NSString;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};

    use super::*;
    use crate::renderer::metal::api::{MetalPixelFormat, MetalStorageMode};
    use crate::renderer::metal::target::{MetalTarget, MetalTargetOptions};

    fn metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("Roastty requires a Metal device")
    }

    fn target(width: usize, height: usize) -> MetalTarget {
        let device = metal_device();
        MetalTarget::new(MetalTargetOptions {
            device: &device,
            width,
            height,
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            storage_mode: MetalStorageMode::Shared,
        })
        .expect("target should be created")
    }

    fn contents_identity(layer: &MetalIOSurfaceLayer) -> Option<*const AnyObject> {
        unsafe { layer.layer().contents() }.map(|contents| Retained::as_ptr(&contents))
    }

    #[test]
    fn layer_initializes_with_top_left_gravity() {
        let layer = MetalIOSurfaceLayer::new();
        let gravity = layer.layer().contentsGravity();
        let gravity: &NSString = gravity.as_ref();
        assert_eq!(gravity, unsafe { kCAGravityTopLeft });
    }

    #[test]
    fn set_surface_sync_sets_layer_contents_to_iosurface() {
        let layer = MetalIOSurfaceLayer::new();
        let target = target(2, 2);

        layer.set_surface_sync(target.surface());

        assert_eq!(
            contents_identity(&layer),
            Some(iosurface_identity(target.surface()))
        );
    }

    #[test]
    fn matching_surface_sets_contents_and_mismatch_keeps_previous_contents() {
        let layer = MetalIOSurfaceLayer::new();
        let matching = target(3, 4);
        let mismatched = target(2, 4);
        layer.set_bounds_pixels(1.5, 2.0, 2.0);

        assert_eq!(layer.expected_pixel_size(), (3, 4));
        assert!(layer.set_surface_if_size_matches(matching.surface()));
        assert_eq!(
            contents_identity(&layer),
            Some(iosurface_identity(matching.surface()))
        );

        assert!(!layer.set_surface_if_size_matches(mismatched.surface()));
        assert_eq!(
            contents_identity(&layer),
            Some(iosurface_identity(matching.surface()))
        );
    }
}
