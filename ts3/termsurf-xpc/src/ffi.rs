//! Raw FFI bindings to libxpc.
//!
//! These are minimal bindings covering only what TermSurf needs.
//! Reference: https://developer.apple.com/documentation/xpc

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_void};

// Opaque XPC types
pub type xpc_object_t = *mut c_void;
pub type xpc_connection_t = *mut c_void;
pub type xpc_endpoint_t = *mut c_void;
pub type xpc_handler_t = *mut c_void;

// Dispatch types (from libdispatch)
pub type dispatch_queue_t = *mut c_void;

// Mach types
pub type mach_port_t = u32;

// XPC connection flags
pub const XPC_CONNECTION_MACH_SERVICE_LISTENER: u64 = 1 << 0;
pub const XPC_CONNECTION_MACH_SERVICE_PRIVILEGED: u64 = 1 << 1;

// XPC type constants
pub const XPC_TYPE_DICTIONARY: *const c_void = unsafe { &_xpc_type_dictionary as *const _ as *const c_void };
pub const XPC_TYPE_STRING: *const c_void = unsafe { &_xpc_type_string as *const _ as *const c_void };
pub const XPC_TYPE_INT64: *const c_void = unsafe { &_xpc_type_int64 as *const _ as *const c_void };
pub const XPC_TYPE_UINT64: *const c_void = unsafe { &_xpc_type_uint64 as *const _ as *const c_void };
pub const XPC_TYPE_ENDPOINT: *const c_void = unsafe { &_xpc_type_endpoint as *const _ as *const c_void };
pub const XPC_TYPE_ERROR: *const c_void = unsafe { &_xpc_type_error as *const _ as *const c_void };

// Error constants
pub const XPC_ERROR_CONNECTION_INTERRUPTED: xpc_object_t = unsafe {
    &_xpc_error_connection_interrupted as *const _ as xpc_object_t
};
pub const XPC_ERROR_CONNECTION_INVALID: xpc_object_t = unsafe {
    &_xpc_error_connection_invalid as *const _ as xpc_object_t
};
pub const XPC_ERROR_TERMINATION_IMMINENT: xpc_object_t = unsafe {
    &_xpc_error_termination_imminent as *const _ as xpc_object_t
};

#[link(name = "System")]
extern "C" {
    // Type symbols (these are extern constants, not functions)
    static _xpc_type_dictionary: c_void;
    static _xpc_type_string: c_void;
    static _xpc_type_int64: c_void;
    static _xpc_type_uint64: c_void;
    static _xpc_type_endpoint: c_void;
    static _xpc_type_error: c_void;

    // Error symbols
    static _xpc_error_connection_interrupted: c_void;
    static _xpc_error_connection_invalid: c_void;
    static _xpc_error_termination_imminent: c_void;

    // === Object lifecycle ===

    pub fn xpc_retain(object: xpc_object_t) -> xpc_object_t;
    pub fn xpc_release(object: xpc_object_t);
    pub fn xpc_get_type(object: xpc_object_t) -> *const c_void;
    pub fn xpc_copy_description(object: xpc_object_t) -> *mut c_char;

    // === Connection ===

    /// Create an anonymous connection/listener.
    /// The name parameter should be NULL for anonymous connections.
    /// Pass NULL for targetq to use the default target queue.
    pub fn xpc_connection_create(
        name: *const c_char,
        targetq: dispatch_queue_t,
    ) -> xpc_connection_t;

    /// Connect to an XPC Mach service by name.
    /// Pass XPC_CONNECTION_MACH_SERVICE_LISTENER to create a listener.
    pub fn xpc_connection_create_mach_service(
        name: *const c_char,
        targetq: dispatch_queue_t,
        flags: u64,
    ) -> xpc_connection_t;

    /// Create a connection from an endpoint received from another process.
    pub fn xpc_connection_create_from_endpoint(
        endpoint: xpc_endpoint_t,
    ) -> xpc_connection_t;

    /// Set the event handler block for a connection.
    /// The block receives xpc_object_t messages (dictionaries or errors).
    pub fn xpc_connection_set_event_handler(
        connection: xpc_connection_t,
        handler: *mut c_void,  // Block pointer
    );

    /// Resume a connection (connections start suspended).
    pub fn xpc_connection_resume(connection: xpc_connection_t);

    /// Suspend a connection.
    pub fn xpc_connection_suspend(connection: xpc_connection_t);

    /// Cancel a connection.
    pub fn xpc_connection_cancel(connection: xpc_connection_t);

    /// Send a message (fire-and-forget).
    pub fn xpc_connection_send_message(
        connection: xpc_connection_t,
        message: xpc_object_t,
    );

    /// Send a message and receive a reply.
    pub fn xpc_connection_send_message_with_reply(
        connection: xpc_connection_t,
        message: xpc_object_t,
        replyq: dispatch_queue_t,
        handler: *mut c_void,  // Block pointer
    );

    /// Send a message and wait synchronously for reply.
    pub fn xpc_connection_send_message_with_reply_sync(
        connection: xpc_connection_t,
        message: xpc_object_t,
    ) -> xpc_object_t;

    // === Endpoint ===

    /// Create an endpoint from a connection (for passing to other processes).
    pub fn xpc_endpoint_create(connection: xpc_connection_t) -> xpc_endpoint_t;

    // === Dictionary ===

    /// Create an empty dictionary.
    pub fn xpc_dictionary_create(
        keys: *const *const c_char,
        values: *const xpc_object_t,
        count: usize,
    ) -> xpc_object_t;

    /// Create a reply dictionary for a received message.
    pub fn xpc_dictionary_create_reply(original: xpc_object_t) -> xpc_object_t;

    /// Get the remote connection from a received message.
    pub fn xpc_dictionary_get_remote_connection(dict: xpc_object_t) -> xpc_connection_t;

    // Dictionary setters
    pub fn xpc_dictionary_set_string(dict: xpc_object_t, key: *const c_char, value: *const c_char);
    pub fn xpc_dictionary_set_int64(dict: xpc_object_t, key: *const c_char, value: i64);
    pub fn xpc_dictionary_set_uint64(dict: xpc_object_t, key: *const c_char, value: u64);
    pub fn xpc_dictionary_set_bool(dict: xpc_object_t, key: *const c_char, value: bool);
    pub fn xpc_dictionary_set_value(dict: xpc_object_t, key: *const c_char, value: xpc_object_t);

    /// Set a Mach send right in the dictionary.
    /// The port is moved (not copied) into the dictionary.
    pub fn xpc_dictionary_set_mach_send(dict: xpc_object_t, key: *const c_char, port: mach_port_t);

    // Dictionary getters
    pub fn xpc_dictionary_get_string(dict: xpc_object_t, key: *const c_char) -> *const c_char;
    pub fn xpc_dictionary_get_int64(dict: xpc_object_t, key: *const c_char) -> i64;
    pub fn xpc_dictionary_get_uint64(dict: xpc_object_t, key: *const c_char) -> u64;
    pub fn xpc_dictionary_get_bool(dict: xpc_object_t, key: *const c_char) -> bool;
    pub fn xpc_dictionary_get_value(dict: xpc_object_t, key: *const c_char) -> xpc_object_t;

    /// Get a Mach send right from the dictionary.
    /// Returns a new send right (caller must deallocate).
    pub fn xpc_dictionary_copy_mach_send(dict: xpc_object_t, key: *const c_char) -> mach_port_t;

    // === String ===

    pub fn xpc_string_create(string: *const c_char) -> xpc_object_t;
    pub fn xpc_string_get_string_ptr(string: xpc_object_t) -> *const c_char;

    // === Dispatch (minimal, for queue creation) ===

    // Note: dispatch_get_main_queue() is a macro that returns &_dispatch_main_q
    // We expose the actual symbol and provide a wrapper function below.
    pub static _dispatch_main_q: c_void;

    pub fn dispatch_queue_create(
        label: *const c_char,
        attr: *const c_void,
    ) -> dispatch_queue_t;

    /// Run the main dispatch queue (blocks forever).
    pub fn dispatch_main() -> !;
}

/// Get the main dispatch queue.
///
/// This is equivalent to the `dispatch_get_main_queue()` macro in C.
#[inline]
pub fn dispatch_get_main_queue() -> dispatch_queue_t {
    unsafe { &_dispatch_main_q as *const _ as dispatch_queue_t }
}

// === CoreFoundation (for run loop) ===

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    /// Run the current thread's run loop (blocks until stopped).
    pub fn CFRunLoopRun();
}

// === IOSurface ===

pub type IOSurfaceRef = *mut c_void;

// IOSurface property keys
#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    pub static kIOSurfaceWidth: *const c_void;
    pub static kIOSurfaceHeight: *const c_void;
    pub static kIOSurfaceBytesPerElement: *const c_void;
    pub static kIOSurfacePixelFormat: *const c_void;
    pub static kIOSurfaceBytesPerRow: *const c_void;

    pub fn IOSurfaceCreate(properties: *const c_void) -> IOSurfaceRef;
    pub fn IOSurfaceGetWidth(buffer: IOSurfaceRef) -> usize;
    pub fn IOSurfaceGetHeight(buffer: IOSurfaceRef) -> usize;
    pub fn IOSurfaceGetBytesPerRow(buffer: IOSurfaceRef) -> usize;
    pub fn IOSurfaceLock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    pub fn IOSurfaceUnlock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    pub fn IOSurfaceGetBaseAddress(buffer: IOSurfaceRef) -> *mut c_void;
    pub fn IOSurfaceIncrementUseCount(buffer: IOSurfaceRef);
    pub fn IOSurfaceDecrementUseCount(buffer: IOSurfaceRef);

    /// Create a Mach port that can be used to transfer this IOSurface to another process.
    pub fn IOSurfaceCreateMachPort(buffer: IOSurfaceRef) -> mach_port_t;

    /// Reconstruct an IOSurface from a Mach port received from another process.
    pub fn IOSurfaceLookupFromMachPort(port: mach_port_t) -> IOSurfaceRef;
}

// === CoreFoundation types for IOSurface property dictionary ===

pub type CFAllocatorRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMutableDictionaryRef = *mut c_void;
pub type CFStringRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFTypeRef = *const c_void;

pub const kCFNumberIntType: i32 = 9; // CFNumberType for int

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;
    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;

    pub fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFMutableDictionaryRef;

    pub fn CFDictionarySetValue(
        dict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );

    pub fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: i32,
        value_ptr: *const c_void,
    ) -> CFNumberRef;

    pub fn CFRelease(cf: CFTypeRef);
}

// === Helper functions ===

/// Check if an XPC object is a dictionary.
#[inline]
pub unsafe fn xpc_is_dictionary(obj: xpc_object_t) -> bool {
    !obj.is_null() && xpc_get_type(obj) == XPC_TYPE_DICTIONARY
}

/// Check if an XPC object is an error.
#[inline]
pub unsafe fn xpc_is_error(obj: xpc_object_t) -> bool {
    !obj.is_null() && xpc_get_type(obj) == XPC_TYPE_ERROR
}

/// Check if an XPC object is an endpoint.
#[inline]
pub unsafe fn xpc_is_endpoint(obj: xpc_object_t) -> bool {
    !obj.is_null() && xpc_get_type(obj) == XPC_TYPE_ENDPOINT
}
