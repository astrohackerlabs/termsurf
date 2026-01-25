//! Test that anonymous XPC connections work correctly.
//!
//! This tests the peer-to-peer XPC pattern with bidirectional communication.
//! Run with: cargo run --example test_anonymous_listener

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use termsurf_xpc::*;

fn main() {
    println!("=== Testing Anonymous XPC Connection ===\n");

    // Track which steps completed
    let server_received = Arc::new(AtomicBool::new(false));
    let client_received = Arc::new(AtomicBool::new(false));
    let server_received_clone = server_received.clone();
    let client_received_clone = client_received.clone();

    // We need to store the peer connection to keep it alive
    let peer_storage: Arc<Mutex<Option<XpcConnection>>> = Arc::new(Mutex::new(None));
    let peer_storage_clone = peer_storage.clone();

    // Step 1: Create anonymous connection (server side)
    println!("1. Creating anonymous connection (server)...");
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

    // Step 2: Get endpoint
    println!("2. Getting endpoint from server...");
    let endpoint = match server.get_endpoint() {
        Ok(ep) => {
            println!("   SUCCESS: Got endpoint (ptr: {:?})", ep);
            ep
        }
        Err(e) => {
            eprintln!("   FAILED: {}", e);
            std::process::exit(1);
        }
    };

    // Step 3: Set up server handler for peer connections
    println!("3. Setting up server connection handler...");
    set_new_connection_handler(&server, move |peer| {
        println!("   Server: Received peer connection!");

        let server_received_inner = server_received_clone.clone();

        // Set up handler for messages from this peer
        set_event_handler(&peer, move |event| {
            match event {
                Ok(msg) => {
                    println!("   Server: Received message from peer!");
                    if let Some(text) = msg.get_string("greeting") {
                        println!("   Server: Client said: '{}'", text);
                        if text == "hello from client" {
                            server_received_inner.store(true, Ordering::SeqCst);
                        }
                    }
                }
                Err(e) => {
                    println!("   Server peer event: {}", e);
                }
            }
        });
        peer.resume();
        println!("   Server: Peer handler set and resumed");

        // Send reply to client
        println!("   Server: Sending reply to client...");
        let reply = XpcDictionary::new();
        reply.set_string("reply", "hello from server");
        peer.send(&reply);
        println!("   Server: Reply sent!");

        // Store the peer to keep it alive!
        *peer_storage_clone.lock().unwrap() = Some(peer);
    });
    server.resume();
    println!("   Server handler set and resumed");

    // Step 4: Create client connection from endpoint
    println!("4. Creating client connection from endpoint...");
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

    // Step 5: Set up client handler
    println!("5. Setting up client event handler...");
    set_event_handler(&client, move |event| {
        match event {
            Ok(msg) => {
                println!("   Client: Received message!");
                if let Some(text) = msg.get_string("reply") {
                    println!("   Client: Server replied: '{}'", text);
                    if text == "hello from server" {
                        client_received_clone.store(true, Ordering::SeqCst);
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

    // Step 6: Give the connection time to establish before sending
    println!("6. Waiting for connection to establish...");
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Step 7: Client sends a message to server
    println!("7. Client sending message to server...");
    let msg = XpcDictionary::new();
    msg.set_string("greeting", "hello from client");
    client.send(&msg);
    println!("   Message sent!");

    // Run event loop with timeout and check for completion
    println!("\n   Running event loop (timeout in 5 seconds)...\n");

    let server_received_check = server_received.clone();
    let client_received_check = client_received.clone();

    std::thread::spawn(move || {
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let sr = server_received_check.load(Ordering::SeqCst);
            let cr = client_received_check.load(Ordering::SeqCst);
            if sr && cr {
                println!("=== TEST PASSED ===");
                println!("   - Server received client's message");
                println!("   - Client received server's reply");
                std::process::exit(0);
            }
        }
        let sr = server_received_check.load(Ordering::SeqCst);
        let cr = client_received_check.load(Ordering::SeqCst);
        eprintln!("\n=== TEST FAILED: Timeout ===");
        eprintln!("   - Server received: {}", sr);
        eprintln!("   - Client received: {}", cr);
        std::process::exit(1);
    });

    // Keep peer_storage in scope
    let _keep_alive = peer_storage;

    run_loop();
}
