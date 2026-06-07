#![allow(dead_code)]
// Metal samplers are bound by later render-pass integration slices.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLSamplerDescriptor, MTLSamplerState};

use crate::renderer::metal::api::{MetalSamplerAddressMode, MetalSamplerMinMagFilter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalSamplerDescriptorOptions {
    pub(crate) min_filter: MetalSamplerMinMagFilter,
    pub(crate) mag_filter: MetalSamplerMinMagFilter,
    pub(crate) s_address_mode: MetalSamplerAddressMode,
    pub(crate) t_address_mode: MetalSamplerAddressMode,
}

impl Default for MetalSamplerDescriptorOptions {
    fn default() -> Self {
        Self {
            min_filter: MetalSamplerMinMagFilter::Nearest,
            mag_filter: MetalSamplerMinMagFilter::Nearest,
            s_address_mode: MetalSamplerAddressMode::ClampToEdge,
            t_address_mode: MetalSamplerAddressMode::ClampToEdge,
        }
    }
}

pub(crate) struct MetalSamplerOptions<'a> {
    pub(crate) device: &'a ProtocolObject<dyn MTLDevice>,
    pub(crate) descriptor: MetalSamplerDescriptorOptions,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MetalSamplerError {
    SamplerCreationFailed,
}

pub(crate) struct MetalSampler {
    sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
}

impl MetalSampler {
    pub(crate) fn new(options: MetalSamplerOptions<'_>) -> Result<Self, MetalSamplerError> {
        let descriptor = MTLSamplerDescriptor::new();
        descriptor.setMinFilter(options.descriptor.min_filter.to_objc());
        descriptor.setMagFilter(options.descriptor.mag_filter.to_objc());
        descriptor.setSAddressMode(options.descriptor.s_address_mode.to_objc());
        descriptor.setTAddressMode(options.descriptor.t_address_mode.to_objc());

        let sampler = options
            .device
            .newSamplerStateWithDescriptor(&descriptor)
            .ok_or(MetalSamplerError::SamplerCreationFailed)?;

        Ok(Self { sampler })
    }

    pub(crate) fn state(&self) -> &ProtocolObject<dyn MTLSamplerState> {
        &self.sampler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_descriptor_options_default_to_metal_defaults() {
        let options = MetalSamplerDescriptorOptions::default();

        assert_eq!(options.min_filter, MetalSamplerMinMagFilter::Nearest);
        assert_eq!(options.mag_filter, MetalSamplerMinMagFilter::Nearest);
        assert_eq!(options.s_address_mode, MetalSamplerAddressMode::ClampToEdge);
        assert_eq!(options.t_address_mode, MetalSamplerAddressMode::ClampToEdge);
    }

    #[test]
    fn sampler_descriptor_options_preserve_values() {
        let options = MetalSamplerDescriptorOptions {
            min_filter: MetalSamplerMinMagFilter::Linear,
            mag_filter: MetalSamplerMinMagFilter::Nearest,
            s_address_mode: MetalSamplerAddressMode::Repeat,
            t_address_mode: MetalSamplerAddressMode::MirrorRepeat,
        };

        assert_eq!(options.min_filter.raw(), 1);
        assert_eq!(options.mag_filter.raw(), 0);
        assert_eq!(options.s_address_mode.raw(), 2);
        assert_eq!(options.t_address_mode.raw(), 3);
    }

    #[test]
    fn sampler_descriptor_applies_options_to_objc_descriptor() {
        let options = MetalSamplerDescriptorOptions {
            min_filter: MetalSamplerMinMagFilter::Linear,
            mag_filter: MetalSamplerMinMagFilter::Nearest,
            s_address_mode: MetalSamplerAddressMode::ClampToZero,
            t_address_mode: MetalSamplerAddressMode::ClampToBorderColor,
        };

        let descriptor = MTLSamplerDescriptor::new();
        descriptor.setMinFilter(options.min_filter.to_objc());
        descriptor.setMagFilter(options.mag_filter.to_objc());
        descriptor.setSAddressMode(options.s_address_mode.to_objc());
        descriptor.setTAddressMode(options.t_address_mode.to_objc());

        assert_eq!(descriptor.minFilter(), options.min_filter.to_objc());
        assert_eq!(descriptor.magFilter(), options.mag_filter.to_objc());
        assert_eq!(descriptor.sAddressMode(), options.s_address_mode.to_objc());
        assert_eq!(descriptor.tAddressMode(), options.t_address_mode.to_objc());
    }

    #[test]
    fn sampler_creation_smoke_test_when_metal_device_exists() {
        let Some(device) = objc2_metal::MTLCreateSystemDefaultDevice() else {
            return;
        };

        let sampler = MetalSampler::new(MetalSamplerOptions {
            device: &device,
            descriptor: MetalSamplerDescriptorOptions {
                min_filter: MetalSamplerMinMagFilter::Linear,
                mag_filter: MetalSamplerMinMagFilter::Linear,
                s_address_mode: MetalSamplerAddressMode::ClampToEdge,
                t_address_mode: MetalSamplerAddressMode::ClampToEdge,
            },
        })
        .expect("sampler should be created");

        let _ = sampler.state();
    }
}
