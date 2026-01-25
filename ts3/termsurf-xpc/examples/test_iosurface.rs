//! Test IOSurface creation and cross-process transfer via Mach ports.
//!
//! This tests the IOSurface utilities needed for GPU texture sharing.
//! Run with: cargo run --example test_iosurface

use termsurf_xpc::iosurface;

fn main() {
    println!("=== Testing IOSurface ===\n");

    // Step 1: Create an IOSurface
    println!("1. Creating IOSurface (100x100, BGRA)...");
    let surface = match iosurface::create_iosurface(100, 100) {
        Ok(s) => {
            println!("   SUCCESS: Created IOSurface");
            s
        }
        Err(e) => {
            eprintln!("   FAILED: {}", e);
            std::process::exit(1);
        }
    };

    // Step 2: Verify dimensions
    println!("2. Verifying dimensions...");
    let width = iosurface::get_width(surface);
    let height = iosurface::get_height(surface);
    println!("   Width: {}, Height: {}", width, height);
    if width != 100 || height != 100 {
        eprintln!("   FAILED: Unexpected dimensions");
        std::process::exit(1);
    }
    println!("   SUCCESS: Dimensions match");

    // Step 3: Fill with a color
    println!("3. Filling with hot pink (255, 105, 180, 255)...");
    iosurface::fill_with_color(surface, 255, 105, 180, 255);
    println!("   SUCCESS: Filled");

    // Step 4: Read back a pixel
    println!("4. Reading pixel at (50, 50)...");
    let pixel = iosurface::read_pixel(surface, 50, 50);
    // Pixel is in RGBA format: R in high byte
    let r = ((pixel >> 24) & 0xFF) as u8;
    let g = ((pixel >> 16) & 0xFF) as u8;
    let b = ((pixel >> 8) & 0xFF) as u8;
    let a = (pixel & 0xFF) as u8;
    println!("   Pixel RGBA: ({}, {}, {}, {})", r, g, b, a);
    if r != 255 || g != 105 || b != 180 || a != 255 {
        eprintln!("   FAILED: Unexpected pixel color");
        std::process::exit(1);
    }
    println!("   SUCCESS: Color matches");

    // Step 5: Create Mach port for cross-process transfer
    println!("5. Creating Mach port for transfer...");
    let port = iosurface::create_mach_port(surface);
    println!("   Mach port: {}", port);
    if port == 0 {
        eprintln!("   FAILED: Got null port");
        std::process::exit(1);
    }
    println!("   SUCCESS: Got valid Mach port");

    // Step 6: Reconstruct IOSurface from Mach port (simulating cross-process)
    println!("6. Reconstructing IOSurface from Mach port...");
    let surface2 = match iosurface::lookup_from_mach_port(port) {
        Some(s) => {
            println!("   SUCCESS: Reconstructed IOSurface");
            s
        }
        None => {
            eprintln!("   FAILED: Could not reconstruct IOSurface");
            std::process::exit(1);
        }
    };

    // Step 7: Verify the reconstructed surface has the same content
    println!("7. Verifying reconstructed surface...");
    let width2 = iosurface::get_width(surface2);
    let height2 = iosurface::get_height(surface2);
    println!("   Dimensions: {}x{}", width2, height2);
    if width2 != 100 || height2 != 100 {
        eprintln!("   FAILED: Reconstructed dimensions don't match");
        std::process::exit(1);
    }

    let pixel2 = iosurface::read_pixel(surface2, 50, 50);
    let r2 = ((pixel2 >> 24) & 0xFF) as u8;
    let g2 = ((pixel2 >> 16) & 0xFF) as u8;
    let b2 = ((pixel2 >> 8) & 0xFF) as u8;
    let a2 = (pixel2 & 0xFF) as u8;
    println!("   Pixel RGBA: ({}, {}, {}, {})", r2, g2, b2, a2);
    if r2 != 255 || g2 != 105 || b2 != 180 || a2 != 255 {
        eprintln!("   FAILED: Reconstructed pixel color doesn't match");
        std::process::exit(1);
    }
    println!("   SUCCESS: Content matches");

    println!("\n=== ALL TESTS PASSED ===");
}
