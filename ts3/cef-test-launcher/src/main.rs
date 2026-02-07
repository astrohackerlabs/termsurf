//! cef-test launcher — XPC bootstrap service for endpoint exchange.
//!
//! Spawns profile server processes and relays XPC endpoints between
//! the GUI and profile servers. Simplified version of termsurf-launcher.
//!
//! Service name: com.cef-test.launcher

use std::collections::HashMap;
use std::fs::File;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use termsurf_xpc::*;

static GUI_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" {
    fn dup2(oldfd: i32, newfd: i32) -> i32;
}

fn redirect_output() {
    use std::os::unix::io::AsRawFd;
    let file = match File::create("/tmp/cef-test-launcher.log") {
        Ok(f) => f,
        Err(_) => return,
    };
    let fd = file.as_raw_fd();
    unsafe {
        dup2(fd, 1); // stdout
        dup2(fd, 2); // stderr
    }
    std::mem::forget(file);
}

fn main() {
    redirect_output();
    println!("Launcher: Starting...");

    // Session storage: session_id -> GUI endpoint
    let sessions: Arc<Mutex<HashMap<String, XpcEndpoint>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Keep client connections alive
    let clients: Arc<Mutex<Vec<Arc<XpcConnection>>>> = Arc::new(Mutex::new(Vec::new()));

    // Path to profile server binary
    // Launcher is at: .app/Contents/XPCServices/com.cef-test.launcher.xpc/Contents/MacOS/cef-test-launcher
    // Profile is at:  .app/Contents/Frameworks/cef-test-profile
    let exe_path = std::env::current_exe().expect("Failed to get exe path");
    let profile_bin_path = exe_path
        .parent() // MacOS
        .and_then(|p| p.parent()) // Contents
        .and_then(|p| p.parent()) // com.cef-test.launcher.xpc
        .and_then(|p| p.parent()) // XPCServices
        .and_then(|p| p.parent()) // Contents
        .map(|p| p.join("Frameworks").join("cef-test-profile"))
        .unwrap_or_else(|| {
            exe_path
                .parent()
                .map(|p| p.join("cef-test-profile"))
                .unwrap_or_default()
        });
    println!("Launcher: Profile binary: {:?}", profile_bin_path);

    // Create listener for this XPC service
    let listener = match XpcListener::new_mach_service("com.cef-test.launcher") {
        Ok(l) => {
            println!("Launcher: Created Mach service listener");
            l
        }
        Err(e) => {
            eprintln!("Launcher: Failed to create listener: {}", e);
            std::process::exit(1);
        }
    };

    let sessions_clone = sessions.clone();
    let clients_clone = clients.clone();

    set_new_connection_handler(&listener, move |conn| {
        let count = GUI_CONNECTION_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        println!("Launcher: New connection (total: {})", count);

        let conn = Arc::new(conn);
        let conn_for_handler = conn.clone();
        let sessions = sessions_clone.clone();
        let profile_bin_path = profile_bin_path.clone();
        let clients_inner = clients_clone.clone();

        set_event_handler(&*conn, move |event| match event {
            Ok(msg) => {
                let action = msg.get_string("action").unwrap_or_default();
                println!("Launcher: Received action: {}", action);

                match action.as_str() {
                    "spawn_profile" => {
                        let session_id = match msg.get_string("session_id") {
                            Some(id) => id,
                            None => {
                                eprintln!("Launcher: Missing session_id");
                                return;
                            }
                        };
                        let gui_endpoint = match msg.get_endpoint("gui_endpoint") {
                            Some(ep) => ep,
                            None => {
                                eprintln!("Launcher: Missing gui_endpoint");
                                return;
                            }
                        };

                        let url = msg
                            .get_string("url")
                            .unwrap_or_else(|| "about:blank".to_string());
                        let profile = msg
                            .get_string("profile")
                            .unwrap_or_else(|| "default".to_string());
                        let width = msg.get_i64("width");
                        let height = msg.get_i64("height");
                        let scale = msg
                            .get_string("scale")
                            .unwrap_or_else(|| "2.0".to_string());

                        println!(
                            "Launcher: Storing endpoint for session {} (url={}, profile={}, size={}x{}, scale={})",
                            session_id, url, profile, width, height, scale
                        );
                        sessions
                            .lock()
                            .unwrap()
                            .insert(session_id.clone(), gui_endpoint);

                        // Spawn profile server process
                        println!("Launcher: Spawning profile (session={})...", session_id);
                        let log_path =
                            format!("/tmp/cef-test-profile-{}.log", session_id);
                        let mut cmd = Command::new(&profile_bin_path);
                        cmd.args(["--session-id", &session_id])
                            .args(["--url", &url])
                            .args(["--profile", &profile])
                            .args(["--width", &width.to_string()])
                            .args(["--height", &height.to_string()])
                            .args(["--scale", &scale])
                            .args(["--service", "com.cef-test.launcher"]);
                        if let Ok(log_file) = File::create(&log_path) {
                            if let Ok(log_file2) = log_file.try_clone() {
                                cmd.stdout(log_file).stderr(log_file2);
                            }
                        }
                        match cmd.spawn() {
                            Ok(child) => println!(
                                "Launcher: Spawned profile (pid: {}, log: {})",
                                child.id(),
                                log_path
                            ),
                            Err(e) => eprintln!("Launcher: Failed to spawn: {}", e),
                        }
                    }

                    "claim_session" => {
                        let session_id = match msg.get_string("session_id") {
                            Some(id) => id,
                            None => {
                                eprintln!("Launcher: Missing session_id in claim_session");
                                return;
                            }
                        };

                        println!("Launcher: Claim request for session {}", session_id);

                        let endpoint = {
                            let mut sessions = sessions.lock().unwrap();
                            sessions.remove(&session_id)
                        };

                        let reply = match XpcDictionary::create_reply(&msg) {
                            Ok(r) => r,
                            Err(e) => {
                                eprintln!("Launcher: Failed to create reply: {}", e);
                                return;
                            }
                        };

                        if let Some(ep) = endpoint {
                            reply.set_endpoint("endpoint", ep);
                            println!("Launcher: Session {} claimed successfully", session_id);
                        } else {
                            reply.set_string("error", "session not found");
                            println!("Launcher: Session {} not found", session_id);
                        }

                        conn_for_handler.send(&reply);
                    }

                    _ => {
                        eprintln!("Launcher: Unknown action: {}", action);
                    }
                }
            }
            Err(e) => match e {
                XpcError::ConnectionInterrupted | XpcError::ConnectionInvalid => {
                    let count = GUI_CONNECTION_COUNT.fetch_sub(1, Ordering::Relaxed) - 1;
                    println!("Launcher: Connection closed (remaining: {})", count);
                    if count == 0 {
                        println!("Launcher: No more connections, exiting...");
                        stop_run_loop();
                    }
                }
                _ => eprintln!("Launcher: Connection error: {}", e),
            },
        });

        conn.resume();
        clients_inner.lock().unwrap().push(conn);
    });

    listener.resume();

    println!("Launcher: Running...");
    run_loop();
    println!("Launcher: Exiting...");
}
