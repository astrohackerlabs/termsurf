use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::time::Duration;

use prost::Message;

use crate::proto::TermSurfMessage;

static WRITERS: Mutex<Vec<UnixStream>> = Mutex::new(Vec::new());

pub fn connect(path: &str) -> Option<UnixStream> {
    let stream = match UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("[Ladybird] connect failed: {error}");
            return None;
        }
    };
    let reader = stream.try_clone().ok()?;
    WRITERS.lock().unwrap().push(stream);
    Some(reader)
}

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

pub fn listen(path: &str, shutdown: Sender<String>) -> io::Result<()> {
    let _ = std::fs::remove_file(path);

    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(path)?;

    eprintln!("[Ladybird] listener bound: {path}");

    std::thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    eprintln!("[Ladybird] listener client connected");
                    let reader = match stream.try_clone() {
                        Ok(reader) => reader,
                        Err(error) => {
                            eprintln!("[Ladybird] listener clone failed: {error}");
                            continue;
                        }
                    };
                    WRITERS.lock().unwrap().push(stream);
                    spawn_reader(reader, false, shutdown.clone());
                }
                Err(error) => {
                    eprintln!("[Ladybird] listener accept error: {error}");
                    let _ = shutdown.send(format!("listener accept error: {error}"));
                    return;
                }
            }
        }
    });
    Ok(())
}

pub fn spawn_reader(stream: UnixStream, quit_on_eof: bool, shutdown: Sender<String>) {
    std::thread::spawn(move || read_messages(stream, quit_on_eof, shutdown));
}

fn read_messages(mut stream: UnixStream, quit_on_eof: bool, shutdown: Sender<String>) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    loop {
        let n = match stream.read(&mut tmp) {
            Ok(0) => {
                eprintln!("[Ladybird] socket EOF");
                if quit_on_eof {
                    let _ = shutdown.send("gui socket closed".to_string());
                }
                return;
            }
            Ok(n) => n,
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                if crate::dispatch::should_shutdown() {
                    let _ = shutdown.send("dispatch requested shutdown".to_string());
                    return;
                }
                continue;
            }
            Err(error) => {
                eprintln!("[Ladybird] socket read error: {error}");
                if quit_on_eof {
                    let _ = shutdown.send(format!("gui socket read error: {error}"));
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
            match TermSurfMessage::decode(payload) {
                Ok(msg) => crate::dispatch::handle_message(&msg),
                Err(error) => eprintln!("[Ladybird] protobuf decode error: {error}"),
            }
            buf.drain(..4 + msg_len);
        }

        if crate::dispatch::should_shutdown() {
            let _ = shutdown.send("dispatch requested shutdown".to_string());
            return;
        }
    }
}
