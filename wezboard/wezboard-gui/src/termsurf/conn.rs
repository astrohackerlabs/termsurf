use super::proto::term_surf_message::Msg;
use super::proto::TermSurfMessage;
use anyhow::Context;
use prost::Message;
use smol::io::AsyncReadExt;
use smol::Async;
use std::os::unix::net::UnixStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnType {
    Unknown,
    Tui,
    Chromium,
}

pub async fn handle_connection(stream: UnixStream) -> anyhow::Result<()> {
    let mut stream = Async::new(stream).context("make stream async")?;
    let mut buf = Vec::with_capacity(4096);
    let mut conn_type = ConnType::Unknown;
    let mut tmp = [0u8; 4096];

    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            log::info!("TermSurf client disconnected ({:?})", conn_type);
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);

        // Process all complete messages in the buffer
        while buf.len() >= 4 {
            let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            if buf.len() < 4 + len {
                break; // need more data
            }

            let msg_bytes = &buf[4..4 + len];
            let msg = TermSurfMessage::decode(msg_bytes).context("decode TermSurfMessage")?;

            // Detect connection type on first message
            if conn_type == ConnType::Unknown {
                conn_type = match &msg.msg {
                    Some(Msg::ServerRegister(_)) => ConnType::Chromium,
                    _ => ConnType::Tui,
                };
                log::info!("TermSurf connection type: {:?}", conn_type);
            }

            if let Err(err) = handle_message(msg, &mut stream).await {
                log::error!("TermSurf handle error: {:#}", err);
            }

            buf.drain(..4 + len);
        }
    }
}

async fn handle_message(
    msg: TermSurfMessage,
    stream: &mut Async<UnixStream>,
) -> anyhow::Result<()> {
    use smol::io::AsyncWriteExt;

    match msg.msg {
        Some(Msg::ServerRegister(r)) => {
            log::info!("ServerRegister: profile={}", r.profile);
        }
        Some(Msg::SetOverlay(o)) => {
            log::info!("SetOverlay: pane_id={}", o.pane_id);
        }
        Some(Msg::HelloRequest(h)) => {
            log::info!("HelloRequest: pane_id={}", h.pane_id);
            // Send HelloReply
            let reply = TermSurfMessage {
                msg: Some(Msg::HelloReply(super::proto::HelloReply {
                    homepage: String::new(),
                    browsers: vec![],
                })),
            };
            let payload = reply.encode_to_vec();
            let len = (payload.len() as u32).to_le_bytes();
            stream.write_all(&len).await?;
            stream.write_all(&payload).await?;
        }
        Some(other) => {
            log::debug!("unhandled TermSurf message: {:?}", other);
        }
        None => {
            log::debug!("empty TermSurf message");
        }
    }
    Ok(())
}
