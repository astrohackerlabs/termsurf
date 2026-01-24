//! Cross-process IOSurface sharing utilities
//!
//! IOSurface supports sharing textures between processes via a numeric ID.
//! This module provides the bindings for:
//! - `IOSurfaceGetID`: Get numeric ID from an IOSurface handle
//! - `IOSurfaceLookupByID`: Reconstruct an IOSurface handle from ID in another process
//!
//! Note: Cross-process IOSurface sharing via global IDs does not work for
//! IOSurfaces created by CEF's GPU process. These functions are preserved
//! for potential future use with different approaches (e.g., Mach ports).

use std::os::raw::c_void;

#[cfg(target_os = "macos")]
#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    /// Returns the unique ID of an IOSurface.
    /// This ID can be passed to another process and used with IOSurfaceLookupByID
    /// to get a handle to the same IOSurface.
    fn IOSurfaceGetID(buffer: *const c_void) -> u32;

    /// Looks up an IOSurface by its global ID.
    /// This is the correct function for cross-process IOSurface sharing.
    #[link_name = "IOSurfaceLookup"]
    fn IOSurfaceLookup(csid: u32) -> *mut c_void;
}

/// Get the unique global ID of an IOSurface handle.
/// This ID can be passed to another process and used to reconstruct
/// the IOSurface handle.
#[cfg(target_os = "macos")]
pub fn get_iosurface_id(handle: *const c_void) -> Option<u32> {
    if handle.is_null() {
        return None;
    }
    let id = unsafe { IOSurfaceGetID(handle) };
    if id == 0 {
        None
    } else {
        Some(id)
    }
}

/// Look up an IOSurface by its global ID.
/// Returns the IOSurface handle if found, or None if the surface
/// no longer exists or the ID is invalid.
///
/// The returned handle is retained by the IOSurface framework and will
/// remain valid as long as the original IOSurface exists.
#[cfg(target_os = "macos")]
pub fn lookup_iosurface_by_id(id: u32) -> Option<*mut c_void> {
    if id == 0 {
        return None;
    }
    let handle = unsafe { IOSurfaceLookup(id) };
    if handle.is_null() {
        None
    } else {
        Some(handle)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_iosurface_id(_handle: *const c_void) -> Option<u32> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn lookup_iosurface_by_id(_id: u32) -> Option<*mut c_void> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_handle_returns_none() {
        assert!(get_iosurface_id(std::ptr::null()).is_none());
    }

    #[test]
    fn test_zero_id_returns_none() {
        assert!(lookup_iosurface_by_id(0).is_none());
    }

    #[test]
    fn test_invalid_id_returns_none() {
        // An arbitrary large ID that shouldn't exist
        assert!(lookup_iosurface_by_id(0xDEADBEEF).is_none());
    }
}
