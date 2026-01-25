//! Test IOSurface transfer via XPC anonymous connections.
//!
//! This combines XPC communication with IOSurface Mach port transfer,
//! which is the core mechanism needed for cross-process GPU texture sharing.
//!
//! Run with: cargo run --example test_xpc_iosurface

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use termsurf_xpc::*;

fn main() {
    println!("=== Testing IOSurface Transfer via XPC ===\n");

    let success = Arc::new(AtomicBool::new(false));
    let success_clone = success.clone();

    // Store peer connection
    let peer_storage: Arc<Mutex<Option<XpcConnection>>> = Arc::new(Mutex::new(None));
    let peer_storage_clone = peer_storage.clone();

    // Step 1: Create IOSurface on "server" side (simulating CEF renderer)
    println!("1. Creating IOSurface on server side...");
    let surface = match iosurface::create_iosurface(200, 150) {
        Ok(s) => {
            println!("   SUCCESS: Created 200x150 IOSurface");
            s
        }
        Err(e) => {
            eprintln!("   FAILED: {}", e);
            std::process::exit(1);
        }
    };

    // Fill with a recognizable pattern (cyan)
    println!("   Filling with cyan (0, 255, 255, 255)...");
    iosurface::fill_with_color(surface, 0, 255, 255, 255);

    // Create Mach port for transfer
    let mach_port = iosurface::create_mach_port(surface);
    println!("   Mach port: {}", mach_port);

    // Step 2: Create anonymous XPC connection
    println!("2. Creating anonymous XPC connection...");
    let server = match XpcListener::new_anonymous() {
        Ok(l) => {
            println!("   SUCCESS: Created anonymous connection");
            l
        }
        Err(e) => {
            eprintln!("   FAILED: {}", e);
            std::process::exit(1);
        }
    };

    // Step 3: Get endpoint
    println!("3. Getting endpoint...");
    let endpoint = match server.get_endpoint() {
        Ok(ep) => {
            println!("   SUCCESS: Got endpoint");
            ep
        }
        Err(e) => {
            eprintln!("   FAILED: {}", e);
            std::process::exit(1);
        }
    };

    // Step 4: Set up server handler
    println!("4. Setting up server handler...");
    set_new_connection_handler(&server, move |peer| {
        println!("   Server: Received peer connection!");

        set_event_handler(&peer, |event| {
            match event {
                Ok(msg) => {
                    println!("   Server: Received message from client");
                    if let Some(text) = msg.get_string("request") {
                        println!("   Server: Client requests: {}", text);
                    }
                }
                Err(e) => {
                    println!("   Server peer event: {}", e);
                }
            }
        });
        peer.resume();

        // Send IOSurface Mach port to client
        println!("   Server: Sending IOSurface Mach port to client...");
        let msg = XpcDictionary::new();
        msg.set_string("type", "iosurface");
        msg.set_mach_send("surface_port", mach_port);
        msg.set_u64("width", 200);
        msg.set_u64("height", 150);
        peer.send(&msg);
        println!("   Server: IOSurface info sent!");

        // Store peer
        *peer_storage_clone.lock().unwrap() = Some(peer);
    });
    server.resume();
    println!("   Server handler set and resumed");

    // Step 5: Create client connection
    println!("5. Creating client connection from endpoint...");
    let client = match XpcConnection::from_endpoint(endpoint) {
        Ok(c) => {
            println!("   SUCCESS: Created client connection");
            c
        }
        Err(e) => {
            eprintln!("   FAILED: {}", e);
            std::process::exit(1);
        }
    };

    // Step 6: Set up client handler to receive IOSurface
    println!("6. Setting up client handler...");
    set_event_handler(&client, move |event| {
        match event {
            Ok(msg) => {
                println!("   Client: Received message!");

                if let Some(msg_type) = msg.get_string("type") {
                    if msg_type == "iosurface" {
                        println!("   Client: Message contains IOSurface!");

                        // Get dimensions
                        let width = msg.get_u64("width");
                        let height = msg.get_u64("height");
                        println!("   Client: Expected dimensions: {}x{}", width, height);

                        // Get Mach port
                        let port = msg.copy_mach_send("surface_port");
                        println!("   Client: Received Mach port: {}", port);

                        if port == 0 {
                            eprintln!("   FAILED: Got null Mach port");
                            std::process::exit(1);
                        }

                        // Reconstruct IOSurface
                        println!("   Client: Reconstructing IOSurface from Mach port...");
                        match iosurface::lookup_from_mach_port(port) {
                            Some(surface) => {
                                println!("   Client: Successfully reconstructed IOSurface!");

                                // Verify dimensions
                                let w = iosurface::get_width(surface);
                                let h = iosurface::get_height(surface);
                                println!("   Client: Actual dimensions: {}x{}", w, h);

                                if w != 200 || h != 150 {
                                    eprintln!("   FAILED: Dimensions mismatch");
                                    std::process::exit(1);
                                }

                                // Verify pixel color
                                let pixel = iosurface::read_pixel(surface, 100, 75);
                                let r = ((pixel >> 24) & 0xFF) as u8;
                                let g = ((pixel >> 16) & 0xFF) as u8;
                                let b = ((pixel >> 8) & 0xFF) as u8;
                                let a = (pixel & 0xFF) as u8;
                                println!("   Client: Pixel at (100,75): RGBA({}, {}, {}, {})", r, g, b, a);

                                if r == 0 && g == 255 && b == 255 && a == 255 {
                                    println!("\n=== TEST PASSED ===");
                                    println!("   - IOSurface transferred via XPC");
                                    println!("   - Mach port correctly received");
                                    println!("   - Surface reconstructed with correct content");
                                    success_clone.store(true, Ordering::SeqCst);
                                    std::process::exit(0);
                                } else {
                                    eprintln!("   FAILED: Pixel color mismatch (expected cyan)");
                                    std::process::exit(1);
                                }
                            }
                            None => {
                                eprintln!("   FAILED: Could not reconstruct IOSurface");
                                std::process::exit(1);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("   Client event: {}", e);
            }
        }
    });
    client.resume();
    println!("   Client handler set and resumed");

    // Step 7: Client requests the IOSurface
    println!("7. Client requesting IOSurface...");
    std::thread::sleep(std::time::Duration::from_millis(100));
    let msg = XpcDictionary::new();
    msg.set_string("request", "get_surface");
    client.send(&msg);
    println!("   Request sent!");

    // Run event loop with timeout
    println!("\n   Running event loop (timeout in 5 seconds)...\n");

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if !success.load(Ordering::SeqCst) {
            eprintln!("\n=== TEST FAILED: Timeout ===");
            std::process::exit(1);
        }
    });

    let _keep_alive = peer_storage;
    run_loop();
}
