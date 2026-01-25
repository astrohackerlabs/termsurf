//! IOSurface utilities for cross-process GPU texture sharing.
//!
//! IOSurface is Apple's mechanism for sharing GPU textures between processes.
//! On macOS, this is the standard way to share rendered content.
//!
//! # Cross-Process Sharing
//!
//! To share an IOSurface between processes:
//!
//! 1. Creator calls `IOSurfaceCreateMachPort()` to get a Mach port
//! 2. Creator sends the port via XPC using `dict.set_mach_send()`
//! 3. Receiver gets the port via `dict.copy_mach_send()`
//! 4. Receiver calls `IOSurfaceLookupFromMachPort()` to reconstruct the handle

use crate::error::{Result, XpcError};
use crate::ffi;
use std::ptr;

// Re-export types
pub use ffi::IOSurfaceRef;

/// Create a new IOSurface with the given dimensions.
///
/// The surface uses BGRA pixel format (4 bytes per pixel).
/// The returned handle must be properly managed (it's reference-counted).
///
/// # Example
///
/// ```ignore
/// let surface = create_iosurface(800, 600)?;
/// fill_with_color(surface, 0xFF, 0x69, 0xB4, 0xFF); // Hot pink
/// ```
pub fn create_iosurface(width: u32, height: u32) -> Result<IOSurfaceRef> {
    unsafe {
        // Create mutable dictionary for properties
        let dict = ffi::CFDictionaryCreateMutable(
            ffi::kCFAllocatorDefault,
            0,
            &ffi::kCFTypeDictionaryKeyCallBacks,
            &ffi::kCFTypeDictionaryValueCallBacks,
        );

        if dict.is_null() {
            return Err(XpcError::NullPointer("CFDictionaryCreateMutable"));
        }

        // Set width
        let width_val = width as i32;
        let width_num =
            ffi::CFNumberCreate(ffi::kCFAllocatorDefault, ffi::kCFNumberIntType, &width_val as *const _ as *const _);
        ffi::CFDictionarySetValue(dict, ffi::kIOSurfaceWidth, width_num as *const _);
        ffi::CFRelease(width_num as ffi::CFTypeRef);

        // Set height
        let height_val = height as i32;
        let height_num =
            ffi::CFNumberCreate(ffi::kCFAllocatorDefault, ffi::kCFNumberIntType, &height_val as *const _ as *const _);
        ffi::CFDictionarySetValue(dict, ffi::kIOSurfaceHeight, height_num as *const _);
        ffi::CFRelease(height_num as ffi::CFTypeRef);

        // Set bytes per element (4 for BGRA)
        let bpe: i32 = 4;
        let bpe_num =
            ffi::CFNumberCreate(ffi::kCFAllocatorDefault, ffi::kCFNumberIntType, &bpe as *const _ as *const _);
        ffi::CFDictionarySetValue(dict, ffi::kIOSurfaceBytesPerElement, bpe_num as *const _);
        ffi::CFRelease(bpe_num as ffi::CFTypeRef);

        // Set pixel format ('BGRA' = 0x42475241)
        let pixel_format: i32 = 0x42475241_u32 as i32;
        let pf_num = ffi::CFNumberCreate(
            ffi::kCFAllocatorDefault,
            ffi::kCFNumberIntType,
            &pixel_format as *const _ as *const _,
        );
        ffi::CFDictionarySetValue(dict, ffi::kIOSurfacePixelFormat, pf_num as *const _);
        ffi::CFRelease(pf_num as ffi::CFTypeRef);

        // Create the IOSurface
        let surface = ffi::IOSurfaceCreate(dict as *const _);
        ffi::CFRelease(dict as ffi::CFTypeRef);

        if surface.is_null() {
            return Err(XpcError::NullPointer("IOSurfaceCreate"));
        }

        Ok(surface)
    }
}

/// Fill an IOSurface with a solid color.
///
/// The color is specified as RGBA components (0-255 each).
pub fn fill_with_color(surface: IOSurfaceRef, r: u8, g: u8, b: u8, a: u8) {
    unsafe {
        // Lock for writing
        ffi::IOSurfaceLock(surface, 0, ptr::null_mut());

        let base = ffi::IOSurfaceGetBaseAddress(surface) as *mut u8;
        let width = ffi::IOSurfaceGetWidth(surface);
        let height = ffi::IOSurfaceGetHeight(surface);
        let stride = ffi::IOSurfaceGetBytesPerRow(surface);

        for y in 0..height {
            for x in 0..width {
                let offset = y * stride + x * 4;
                // BGRA format
                *base.add(offset) = b;
                *base.add(offset + 1) = g;
                *base.add(offset + 2) = r;
                *base.add(offset + 3) = a;
            }
        }

        ffi::IOSurfaceUnlock(surface, 0, ptr::null_mut());
    }
}

/// Read a pixel from an IOSurface.
///
/// Returns the pixel as a u32 in RGBA format (R in high byte).
pub fn read_pixel(surface: IOSurfaceRef, x: usize, y: usize) -> u32 {
    unsafe {
        // Lock for reading (option 1 = read-only)
        ffi::IOSurfaceLock(surface, 1, ptr::null_mut());

        let base = ffi::IOSurfaceGetBaseAddress(surface) as *const u8;
        let stride = ffi::IOSurfaceGetBytesPerRow(surface);
        let offset = y * stride + x * 4;

        // Read BGRA, return as RGBA
        let b = *base.add(offset);
        let g = *base.add(offset + 1);
        let r = *base.add(offset + 2);
        let a = *base.add(offset + 3);

        ffi::IOSurfaceUnlock(surface, 0, ptr::null_mut());

        ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32)
    }
}

/// Get the width of an IOSurface.
pub fn get_width(surface: IOSurfaceRef) -> usize {
    unsafe { ffi::IOSurfaceGetWidth(surface) }
}

/// Get the height of an IOSurface.
pub fn get_height(surface: IOSurfaceRef) -> usize {
    unsafe { ffi::IOSurfaceGetHeight(surface) }
}

/// Create a Mach port for transferring this IOSurface to another process.
///
/// The returned port can be sent via XPC using `dict.set_mach_send()`.
/// The receiving process calls `lookup_from_mach_port()` to reconstruct
/// the IOSurface handle.
pub fn create_mach_port(surface: IOSurfaceRef) -> ffi::mach_port_t {
    unsafe { ffi::IOSurfaceCreateMachPort(surface) }
}

/// Reconstruct an IOSurface from a Mach port received from another process.
///
/// The port should have been received via `dict.copy_mach_send()`.
/// Returns None if the port is invalid.
pub fn lookup_from_mach_port(port: ffi::mach_port_t) -> Option<IOSurfaceRef> {
    let surface = unsafe { ffi::IOSurfaceLookupFromMachPort(port) };
    if surface.is_null() {
        None
    } else {
        Some(surface)
    }
}
