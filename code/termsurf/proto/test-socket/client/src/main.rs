use prost::Message;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

pub mod termsurf {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

fn main() {
    // Build socket path from $TMPDIR.
    let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp/".to_string());
    let sep = if tmpdir.ends_with('/') { "" } else { "/" };
    let path = format!("{}{}termsurf-test.sock", tmpdir, sep);

    // Retry connection (server may still be starting).
    let mut stream = {
        let mut attempts = 0;
        loop {
            match UnixStream::connect(&path) {
                Ok(s) => break s,
                Err(e) => {
                    attempts += 1;
                    if attempts >= 20 {
                        panic!("connect failed after 20 attempts: {}", e);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    };

    // Build HelloRequest.
    let msg = termsurf::TermSurfMessage {
        msg: Some(termsurf::term_surf_message::Msg::HelloRequest(
            termsurf::HelloRequest {
                pane_id: "pane-1".to_string(),
            },
        )),
    };

    // Serialize.
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    // Write length prefix (4 bytes LE) + message.
    stream.write_all(&(buf.len() as u32).to_le_bytes()).unwrap();
    stream.write_all(&buf).unwrap();

    // Read reply length prefix.
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).unwrap();
    let reply_len = u32::from_le_bytes(len_buf) as usize;

    // Read reply message.
    let mut reply_buf = vec![0u8; reply_len];
    stream.read_exact(&mut reply_buf).unwrap();

    // Deserialize.
    let reply = termsurf::TermSurfMessage::decode(reply_buf.as_slice()).unwrap();

    // Verify it's a HelloReply with the expected homepage.
    match reply.msg {
        Some(termsurf::term_surf_message::Msg::HelloReply(r)) => {
            assert_eq!(r.homepage, "https://termsurf.com");
        }
        other => panic!("Expected HelloReply, got {:?}", other),
    }

    println!("Rust client: pass");
}
