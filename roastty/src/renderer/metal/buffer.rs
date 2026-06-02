#![allow(dead_code)]
// This Metal buffer layer is consumed by later renderer slices.

use std::marker::PhantomData;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSRange;
use objc2_metal::{MTLBuffer, MTLDevice};

use crate::renderer::metal::api::{MetalResourceOptions, MetalStorageMode};
use crate::renderer::shader::{BgImageVertex, CellBg, CellTextVertex, ImageVertex};

pub(crate) unsafe trait MetalBufferElement: Copy {}

unsafe impl MetalBufferElement for ImageVertex {}
unsafe impl MetalBufferElement for CellTextVertex {}
unsafe impl MetalBufferElement for CellBg {}
unsafe impl MetalBufferElement for BgImageVertex {}

#[derive(Clone, Copy)]
pub(crate) struct MetalBufferOptions<'a> {
    pub(crate) device: &'a ProtocolObject<dyn MTLDevice>,
    pub(crate) resource_options: MetalResourceOptions,
}

pub(crate) struct MetalBuffer<T> {
    buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    resource_options: MetalResourceOptions,
    capacity_items: usize,
    capacity_bytes: usize,
    _marker: PhantomData<T>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetalBufferError {
    ByteLengthOverflow,
    ZeroLengthBuffer,
    ZeroSizedElement,
    BufferCreationFailed,
}

impl<T: MetalBufferElement> MetalBuffer<T> {
    pub(crate) fn new(
        options: MetalBufferOptions<'_>,
        len: usize,
    ) -> Result<Self, MetalBufferError> {
        if len == 0 {
            return Err(MetalBufferError::ZeroLengthBuffer);
        }

        let capacity_bytes = byte_len::<T>(len)?;
        let buffer = options
            .device
            .newBufferWithLength_options(capacity_bytes, options.resource_options.to_objc())
            .ok_or(MetalBufferError::BufferCreationFailed)?;

        Ok(Self {
            buffer,
            resource_options: options.resource_options,
            capacity_items: len,
            capacity_bytes,
            _marker: PhantomData,
        })
    }

    pub(crate) fn init_fill(
        options: MetalBufferOptions<'_>,
        data: &[T],
    ) -> Result<Self, MetalBufferError> {
        if data.is_empty() {
            return Err(MetalBufferError::ZeroLengthBuffer);
        }

        let capacity_bytes = byte_len::<T>(data.len())?;
        let bytes = data_as_non_null_bytes(data)?;
        let buffer = unsafe {
            options.device.newBufferWithBytes_length_options(
                bytes,
                capacity_bytes,
                options.resource_options.to_objc(),
            )
        }
        .ok_or(MetalBufferError::BufferCreationFailed)?;

        Ok(Self {
            buffer,
            resource_options: options.resource_options,
            capacity_items: data.len(),
            capacity_bytes,
            _marker: PhantomData,
        })
    }

    pub(crate) fn sync(
        &mut self,
        options: MetalBufferOptions<'_>,
        data: &[T],
    ) -> Result<(), MetalBufferError> {
        let required_bytes = byte_len::<T>(data.len())?;
        if required_bytes > self.capacity_bytes {
            let new_capacity_items = data
                .len()
                .checked_mul(2)
                .ok_or(MetalBufferError::ByteLengthOverflow)?;
            let new_capacity_bytes = byte_len::<T>(new_capacity_items)?;
            let new_buffer = options
                .device
                .newBufferWithLength_options(new_capacity_bytes, options.resource_options.to_objc())
                .ok_or(MetalBufferError::BufferCreationFailed)?;

            self.buffer = new_buffer;
            self.resource_options = options.resource_options;
            self.capacity_items = new_capacity_items;
            self.capacity_bytes = new_capacity_bytes;
        }

        if required_bytes > 0 {
            let dst = self.buffer.contents().as_ptr().cast::<u8>();
            let src = data_as_bytes(data);
            unsafe {
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst, required_bytes);
            }

            if requires_did_modify(self.resource_options, required_bytes) {
                self.buffer.didModifyRange(NSRange::new(0, required_bytes));
            }
        }

        Ok(())
    }

    pub(crate) fn capacity_items(&self) -> usize {
        self.capacity_items
    }

    pub(crate) fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }

    pub(crate) fn buffer(&self) -> &ProtocolObject<dyn MTLBuffer> {
        &self.buffer
    }

    #[cfg(test)]
    fn read_bytes(&self, len: usize) -> Vec<u8> {
        let byte_count = byte_len::<T>(len).expect("test byte count fits");
        let src = self.buffer.contents().as_ptr().cast::<u8>();
        let mut bytes = vec![0; byte_count];
        if byte_count > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(src, bytes.as_mut_ptr(), byte_count);
            }
        }
        bytes
    }
}

fn byte_len<T>(len: usize) -> Result<usize, MetalBufferError> {
    let element_size = std::mem::size_of::<T>();
    if element_size == 0 {
        return Err(MetalBufferError::ZeroSizedElement);
    }

    len.checked_mul(element_size)
        .ok_or(MetalBufferError::ByteLengthOverflow)
}

fn data_as_non_null_bytes<T: MetalBufferElement>(
    data: &[T],
) -> Result<NonNull<std::ffi::c_void>, MetalBufferError> {
    NonNull::new(data.as_ptr().cast_mut().cast()).ok_or(MetalBufferError::BufferCreationFailed)
}

fn data_as_bytes<T: MetalBufferElement>(data: &[T]) -> &[u8] {
    let byte_count = std::mem::size_of_val(data);
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), byte_count) }
}

fn requires_did_modify(resource_options: MetalResourceOptions, modified_bytes: usize) -> bool {
    resource_options.storage_mode == MetalStorageMode::Managed && modified_bytes > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::metal::api::MetalResourceOptions;
    use objc2_metal::MTLCreateSystemDefaultDevice;

    unsafe impl MetalBufferElement for u32 {}

    fn metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("Roastty requires a Metal device")
    }

    fn shared_options(device: &ProtocolObject<dyn MTLDevice>) -> MetalBufferOptions<'_> {
        MetalBufferOptions {
            device,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
        }
    }

    fn image_vertex() -> ImageVertex {
        ImageVertex {
            grid_pos: [1.0, 2.0],
            cell_offset: [3.0, 4.0],
            source_rect: [5.0, 6.0, 7.0, 8.0],
            dest_size: [9.0, 10.0],
        }
    }

    fn u32_bytes(values: &[u32]) -> Vec<u8> {
        data_as_bytes(values).to_vec()
    }

    fn assert_buffer_element<T: MetalBufferElement>() {}

    #[test]
    fn shader_payloads_satisfy_buffer_element_bound() {
        assert_buffer_element::<CellTextVertex>();
        assert_buffer_element::<CellBg>();
        assert_buffer_element::<BgImageVertex>();
    }

    #[test]
    fn live_init_fill_uploads_image_vertex_bytes() {
        let device = metal_device();
        let vertex = image_vertex();
        let buffer = MetalBuffer::init_fill(shared_options(&device), &[vertex])
            .expect("create initialized image vertex buffer");

        assert_eq!(buffer.capacity_items(), 1);
        assert_eq!(buffer.capacity_bytes(), std::mem::size_of::<ImageVertex>());
        assert_eq!(buffer.read_bytes(1), data_as_bytes(&[vertex]));
    }

    #[test]
    fn live_new_allocates_requested_item_capacity() {
        let device = metal_device();
        let buffer =
            MetalBuffer::<u32>::new(shared_options(&device), 3).expect("create empty buffer");

        assert_eq!(buffer.capacity_items(), 3);
        assert_eq!(buffer.capacity_bytes(), 12);
    }

    #[test]
    fn zero_length_buffers_are_rejected_explicitly() {
        let device = metal_device();

        assert_eq!(
            MetalBuffer::<u32>::new(shared_options(&device), 0).err(),
            Some(MetalBufferError::ZeroLengthBuffer)
        );
        assert_eq!(
            MetalBuffer::<u32>::init_fill(shared_options(&device), &[]).err(),
            Some(MetalBufferError::ZeroLengthBuffer)
        );
    }

    #[test]
    fn sync_with_fitting_data_updates_bytes_without_reallocating() {
        let device = metal_device();
        let mut buffer =
            MetalBuffer::<u32>::new(shared_options(&device), 3).expect("create empty buffer");

        buffer
            .sync(shared_options(&device), &[1, 2, 3])
            .expect("sync fitting data");

        assert_eq!(buffer.capacity_items(), 3);
        assert_eq!(buffer.capacity_bytes(), 12);
        assert_eq!(buffer.read_bytes(3), u32_bytes(&[1, 2, 3]));
    }

    #[test]
    fn shorter_sync_preserves_capacity_and_trailing_bytes() {
        let device = metal_device();
        let mut buffer = MetalBuffer::init_fill(shared_options(&device), &[1_u32, 2, 3])
            .expect("create initialized buffer");

        buffer
            .sync(shared_options(&device), &[9])
            .expect("sync shorter data");

        assert_eq!(buffer.capacity_items(), 3);
        assert_eq!(buffer.capacity_bytes(), 12);
        assert_eq!(buffer.read_bytes(3), u32_bytes(&[9, 2, 3]));
    }

    #[test]
    fn larger_sync_reallocates_to_double_required_capacity() {
        let device = metal_device();
        let mut buffer =
            MetalBuffer::init_fill(shared_options(&device), &[1_u32]).expect("create buffer");

        buffer
            .sync(shared_options(&device), &[4, 5, 6])
            .expect("sync larger data");

        assert_eq!(buffer.capacity_items(), 6);
        assert_eq!(buffer.capacity_bytes(), 24);
        assert_eq!(buffer.read_bytes(3), u32_bytes(&[4, 5, 6]));
    }

    #[test]
    fn byte_length_overflow_returns_error() {
        assert_eq!(
            byte_len::<u32>(usize::MAX),
            Err(MetalBufferError::ByteLengthOverflow)
        );
    }

    #[test]
    fn zero_sized_elements_are_rejected() {
        assert_eq!(byte_len::<()>(1), Err(MetalBufferError::ZeroSizedElement));
    }

    #[test]
    fn did_modify_is_required_only_for_non_empty_managed_writes() {
        assert!(requires_did_modify(
            MetalResourceOptions::image(MetalStorageMode::Managed),
            4
        ));
        assert!(!requires_did_modify(
            MetalResourceOptions::image(MetalStorageMode::Managed),
            0
        ));
        assert!(!requires_did_modify(
            MetalResourceOptions::image(MetalStorageMode::Shared),
            4
        ));
    }
}
