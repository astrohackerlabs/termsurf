//! Safe block-based event handlers for XPC connections.
//!
//! This module provides Rust closure wrappers for XPC event handlers,
//! using the `block2` crate to create Objective-C blocks.

use crate::connection::XpcConnection;
use crate::dictionary::XpcDictionary;
use crate::error::Result;
use crate::ffi;
use crate::listener::XpcListener;
use block2::RcBlock;
use std::sync::Arc;

/// Set an event handler on a connection using a Rust closure.
///
/// The closure receives parsed events: `Ok(XpcDictionary)` for messages,
/// or `Err(XpcError)` for connection errors.
///
/// # Example
///
/// ```ignore
/// set_event_handler(&conn, |event| {
///     match event {
///         Ok(msg) => println!("Received: {:?}", msg.get_string("action")),
///         Err(e) => eprintln!("Error: {}", e),
///     }
/// });
/// ```
///
/// # Safety Note
///
/// The handler block is leaked to ensure it lives as long as the connection.
/// This is acceptable for long-lived connections but will leak memory if
/// connections are created and destroyed frequently.
pub fn set_event_handler<F>(conn: &XpcConnection, handler: F)
where
    F: Fn(Result<XpcDictionary>) + Send + 'static,
{
    let handler = Arc::new(handler);

    let block = RcBlock::new(move |event: ffi::xpc_object_t| {
        let result = unsafe { XpcConnection::parse_event(event) };
        handler(result);
    });

    // Leak the block to ensure it lives forever.
    // This is necessary because XPC holds a reference to the block
    // and we have no way to know when XPC is done with it.
    let block_ptr = RcBlock::into_raw(block);

    unsafe {
        ffi::xpc_connection_set_event_handler(conn.as_raw(), block_ptr as *mut _);
    }
}

/// Set a handler for new incoming connections on a listener.
///
/// For XPC service listeners, each incoming client gets its own connection.
/// For anonymous listeners, this receives peer connections.
///
/// # Example
///
/// ```ignore
/// set_new_connection_handler(&listener, |peer| {
///     println!("New connection!");
///     set_event_handler(&peer, |event| {
///         // Handle messages from this peer
///     });
///     peer.resume();
/// });
/// ```
pub fn set_new_connection_handler<F>(listener: &XpcListener, handler: F)
where
    F: Fn(XpcConnection) + Send + 'static,
{
    let handler = Arc::new(handler);

    let block = RcBlock::new(move |peer: ffi::xpc_connection_t| {
        if peer.is_null() {
            return;
        }

        // Retain the peer connection since XPC passes it without retaining
        unsafe {
            ffi::xpc_retain(peer as ffi::xpc_object_t);
        }

        // Wrap in XpcConnection (takes ownership)
        let conn = unsafe { XpcConnection::from_raw(peer) };
        if let Ok(conn) = conn {
            handler(conn);
        }
    });

    // Leak the block
    let block_ptr = RcBlock::into_raw(block);

    unsafe {
        ffi::xpc_connection_set_event_handler(listener.as_raw(), block_ptr as *mut _);
    }
}
