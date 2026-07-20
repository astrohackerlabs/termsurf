use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Mutex;

use prost::Message;

use crate::ffi;
use crate::proto::TermSurfMessage;

/// Global write streams. The GUI connection and any listener clients are all
/// stored here. `send()` broadcasts to every connection.
static WRITERS: Mutex<Vec<UnixStream>> = Mutex::new(Vec::new());

/// Connect to the GUI's Unix socket. Returns the read half for the reader
/// thread and stores the write half in WRITERS.
pub fn connect(path: &str) -> Option<UnixStream> {
    let stream = match UnixStream::connect(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[WebKit] connect failed: {e}");
            return None;
        }
    };
    let reader = stream.try_clone().ok()?;
    WRITERS.lock().unwrap().push(stream);
    Some(reader)
}

/// Send a protobuf message to all connected writers (4-byte LE length prefix).
/// Disconnected writers are silently removed. Returns the number of writers
/// that received the message.
pub fn send(msg: &TermSurfMessage) -> usize {
    let mut sent = 0;
    WRITERS.lock().unwrap().retain_mut(|stream| {
        if write_message(stream, msg).is_err() {
            return false;
        }
        sent += 1;
        true
    });
    sent
}

pub fn write_message(stream: &mut UnixStream, msg: &TermSurfMessage) -> io::Result<()> {
    let payload = msg.encode_to_vec();
    let len = (payload.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(&payload)?;
    Ok(())
}

/// Start a listener on the given Unix socket path. Accepts connections in a
/// background thread; each client gets a reader thread and a write half pushed
/// into WRITERS for broadcast.
pub fn listen(path: &str) {
    // Remove stale socket file from a previous crash.
    let _ = std::fs::remove_file(path);

    // Ensure parent directory exists.
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let listener = match UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[WebKit] listener bind failed: {e}");
            return;
        }
    };
    eprintln!("[WebKit] listener bound: {path}");

    std::thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    eprintln!("[WebKit] client connected");
                    let reader = match stream.try_clone() {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("[WebKit] clone failed: {e}");
                            continue;
                        }
                    };
                    let mut writer = stream;
                    crate::dispatch::replay_state_to_client(&mut writer);
                    WRITERS.lock().unwrap().push(writer);
                    std::thread::spawn(move || {
                        read_messages(reader, false);
                    });
                }
                Err(e) => {
                    eprintln!("[WebKit] accept error: {e}");
                }
            }
        }
    });
}

/// Reader loop for the GUI connection. EOF/error causes a quit.
pub fn reader_loop(stream: UnixStream) {
    read_messages(stream, true);
}

/// Shared message-reading loop. Reads length-prefixed protobuf messages and
/// posts each to the UI thread. If `quit_on_eof` is true, requests a quit
/// when the connection drops (used for the GUI connection).
fn read_messages(mut stream: UnixStream, quit_on_eof: bool) {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    loop {
        let n = match stream.read(&mut tmp) {
            Ok(0) => {
                if quit_on_eof {
                    eprintln!("[WebKit] socket EOF — requesting quit");
                    unsafe { ffi::ts_post_task(Some(quit_trampoline), std::ptr::null_mut()) };
                } else {
                    eprintln!("[WebKit] client disconnected");
                }
                return;
            }
            Ok(n) => n,
            Err(e) => {
                if quit_on_eof {
                    eprintln!("[WebKit] socket read error: {e} — requesting quit");
                    unsafe { ffi::ts_post_task(Some(quit_trampoline), std::ptr::null_mut()) };
                } else {
                    eprintln!("[WebKit] client read error: {e}");
                }
                return;
            }
        };
        buf.extend_from_slice(&tmp[..n]);

        while buf.len() >= 4 {
            let msg_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            if buf.len() < 4 + msg_len {
                break;
            }

            let payload = &buf[4..4 + msg_len];
            if let Ok(msg) = TermSurfMessage::decode(payload) {
                // Box the message and post to the UI thread.
                let ptr = Box::into_raw(Box::new(msg)) as *mut c_void;
                unsafe {
                    ffi::ts_post_task(Some(dispatch_trampoline), ptr);
                }
            }
            buf.drain(..4 + msg_len);
        }
    }
}

/// Trampoline called on the UI thread to quit the browser process.
unsafe extern "C" fn quit_trampoline(_data: *mut c_void) {
    unsafe { ffi::ts_quit() };
}

/// Trampoline called on the UI thread by ts_post_task.
unsafe extern "C" fn dispatch_trampoline(data: *mut c_void) {
    let msg = unsafe { Box::from_raw(data as *mut TermSurfMessage) };
    crate::dispatch::handle_message(&msg);
}
