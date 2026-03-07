use super::proto;
use super::proto::TermSurfMessage;
use super::proto::term_surf_message::Msg;
use super::state::{Pane, Server, SharedState, TermSurfState};
use anyhow::Context;
use prost::Message;
use smol::Async;
use smol::channel::Sender;
use smol::io::{AsyncReadExt, AsyncWriteExt};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnType {
    Unknown,
    Tui,
    Chromium,
}

pub async fn handle_connection(stream: UnixStream, state: SharedState) -> anyhow::Result<()> {
    let stream = Arc::new(Async::new(stream).context("make stream async")?);
    let (tx, rx) = smol::channel::bounded::<Vec<u8>>(64);

    // Spawn writer task: drains channel and writes length-prefixed messages
    let write_stream = stream.clone();
    promise::spawn::spawn_into_main_thread(async move {
        while let Ok(payload) = rx.recv().await {
            let len = (payload.len() as u32).to_le_bytes();
            if let Err(e) = (&*write_stream).write_all(&len).await {
                log::error!("TermSurf write len error: {:#}", e);
                break;
            }
            if let Err(e) = (&*write_stream).write_all(&payload).await {
                log::error!("TermSurf write payload error: {:#}", e);
                break;
            }
        }
    })
    .detach();

    let mut buf = Vec::with_capacity(4096);
    let mut conn_type = ConnType::Unknown;
    let mut tmp = [0u8; 4096];

    loop {
        let n = (&*stream).read(&mut tmp).await?;
        if n == 0 {
            log::info!("TermSurf client disconnected ({:?})", conn_type);
            handle_disconnect(conn_type, &tx, &state);
            tx.close();
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);

        while buf.len() >= 4 {
            let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            if buf.len() < 4 + len {
                break;
            }

            let msg_bytes = &buf[4..4 + len];
            let msg = TermSurfMessage::decode(msg_bytes).context("decode TermSurfMessage")?;

            if conn_type == ConnType::Unknown {
                conn_type = match &msg.msg {
                    Some(Msg::ServerRegister(_)) => ConnType::Chromium,
                    _ => ConnType::Tui,
                };
                log::info!("TermSurf connection type: {:?}", conn_type);
            }

            if let Err(err) = handle_message(msg, &stream, &tx, &state).await {
                log::error!("TermSurf handle error: {:#}", err);
            }

            buf.drain(..4 + len);
        }
    }
}

async fn handle_message(
    msg: TermSurfMessage,
    stream: &Arc<Async<UnixStream>>,
    tx: &Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    match msg.msg {
        Some(Msg::ServerRegister(r)) => {
            handle_server_register(r, tx.clone(), state)?;
        }
        Some(Msg::SetOverlay(o)) => {
            handle_set_overlay(o, tx.clone(), state)?;
        }
        Some(Msg::TabReady(r)) => {
            handle_tab_ready(r, state)?;
        }
        Some(Msg::HelloRequest(h)) => {
            log::info!("HelloRequest: pane_id={}", h.pane_id);
            let reply = TermSurfMessage {
                msg: Some(Msg::HelloReply(proto::HelloReply {
                    homepage: String::new(),
                    browsers: vec![],
                })),
            };
            let payload = reply.encode_to_vec();
            let len = (payload.len() as u32).to_le_bytes();
            (&**stream).write_all(&len).await?;
            (&**stream).write_all(&payload).await?;
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

fn handle_set_overlay(
    overlay: proto::SetOverlay,
    tui_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    let browser = if overlay.browser.is_empty() {
        "roamium".to_string()
    } else {
        overlay.browser.clone()
    };

    // Placeholder pixel dimensions: grid cells * approximate cell size
    let pixel_w = overlay.width * 10;
    let pixel_h = overlay.height * 20;

    let is_new = !st.panes.contains_key(&overlay.pane_id);

    if !is_new {
        // Resize: update dimensions, extract values before releasing mutable borrow
        let (tab_id, profile, browser_name) = {
            let pane = st.panes.get_mut(&overlay.pane_id).unwrap();
            pane.pixel_width = pixel_w;
            pane.pixel_height = pixel_h;
            (pane.tab_id, pane.profile.clone(), pane.browser.clone())
        };
        log::info!(
            "SetOverlay resize: pane_id={} {}x{}",
            overlay.pane_id,
            pixel_w,
            pixel_h
        );
        if tab_id != 0 {
            let key = TermSurfState::server_key(&profile, &browser_name);
            if let Some(server) = st.servers.get(&key) {
                if let Some(ref server_tx) = server.tx {
                    let resize_msg = TermSurfMessage {
                        msg: Some(Msg::Resize(proto::Resize {
                            tab_id,
                            pixel_width: pixel_w,
                            pixel_height: pixel_h,
                        })),
                    };
                    let _ = server_tx.try_send(resize_msg.encode_to_vec());
                }
            }
        }
        return Ok(());
    }

    log::info!(
        "SetOverlay: pane_id={} profile={} browser={} url={}",
        overlay.pane_id,
        overlay.profile,
        browser,
        overlay.url
    );

    // Create new pane
    let pane = Pane {
        pane_id: overlay.pane_id.clone(),
        profile: overlay.profile.clone(),
        browser: browser.clone(),
        url: overlay.url.clone(),
        pixel_width: pixel_w,
        pixel_height: pixel_h,
        tab_id: 0,
        tui_tx,
        browsing: overlay.browsing,
        dark: false,
        inspected_tab_id: 0,
    };
    st.panes.insert(overlay.pane_id.clone(), pane);

    // Get or create server
    let key = TermSurfState::server_key(&overlay.profile, &browser);
    if !st.servers.contains_key(&key) {
        // Must drop lock before spawning (spawn_server doesn't need state)
        drop(st);
        let server = spawn_server(&overlay.profile, &browser)?;
        let mut st = state.lock().unwrap();
        st.servers.insert(key.clone(), server);
        // If server already connected (unlikely for fresh spawn), send CreateTab
        let server = st.servers.get(&key).unwrap();
        if let Some(ref server_tx) = server.tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_tab(server_tx, pane)?;
        }
    } else {
        st.servers.get_mut(&key).unwrap().pane_count += 1;
        let server_tx = st.servers.get(&key).unwrap().tx.clone();
        if let Some(ref stx) = server_tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_tab(stx, pane)?;
        }
    }

    Ok(())
}

fn handle_server_register(
    reg: proto::ServerRegister,
    server_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();

    log::info!("ServerRegister: profile={}", reg.profile);

    // Find the server with matching profile that has no tx yet
    let matched = st.servers.iter_mut().find_map(|(key, server)| {
        if server.profile == reg.profile && server.tx.is_none() {
            server.tx = Some(server_tx.clone());
            Some((key.clone(), server.browser.clone(), server.profile.clone()))
        } else {
            None
        }
    });

    if let Some((_key, browser, profile)) = matched {
        // Flush pending tabs
        let pending: Vec<String> = st
            .panes
            .iter()
            .filter(|(_, p)| p.profile == profile && p.browser == browser && p.tab_id == 0)
            .map(|(id, _)| id.clone())
            .collect();

        for pane_id in pending {
            let pane = st.panes.get(&pane_id).unwrap();
            send_create_tab(&server_tx, pane)?;
        }
    } else {
        log::warn!(
            "ServerRegister: no matching server for profile={}",
            reg.profile
        );
    }

    Ok(())
}

fn handle_tab_ready(ready: proto::TabReady, state: &SharedState) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    if st.panes.contains_key(&ready.pane_id) {
        st.panes.get_mut(&ready.pane_id).unwrap().tab_id = ready.tab_id;
        st.tab_to_pane.insert(ready.tab_id, ready.pane_id.clone());
        let inspected = st.panes.get(&ready.pane_id).unwrap().inspected_tab_id;
        if inspected == 0 {
            st.last_browser_pane = Some(ready.pane_id.clone());
        }
        log::info!(
            "TabReady: pane_id={} tab_id={}",
            ready.pane_id,
            ready.tab_id
        );
    } else {
        log::warn!("TabReady: unknown pane_id={}", ready.pane_id);
    }
    Ok(())
}

fn handle_disconnect(conn_type: ConnType, tx: &Sender<Vec<u8>>, state: &SharedState) {
    let mut st = state.lock().unwrap();
    match conn_type {
        ConnType::Tui => {
            // Remove panes whose tui_tx matches this connection's tx
            let to_remove: Vec<String> = st
                .panes
                .iter()
                .filter(|(_, p)| p.tui_tx.same_channel(tx))
                .map(|(id, _)| id.clone())
                .collect();
            for pane_id in &to_remove {
                if let Some(pane) = st.panes.remove(pane_id) {
                    if pane.tab_id != 0 {
                        st.tab_to_pane.remove(&pane.tab_id);
                        // Send CloseTab to server
                        let key = TermSurfState::server_key(&pane.profile, &pane.browser);
                        if let Some(server) = st.servers.get_mut(&key) {
                            server.pane_count = server.pane_count.saturating_sub(1);
                            if let Some(ref server_tx) = server.tx {
                                let msg = TermSurfMessage {
                                    msg: Some(Msg::CloseTab(proto::CloseTab {
                                        tab_id: pane.tab_id,
                                    })),
                                };
                                let _ = server_tx.try_send(msg.encode_to_vec());
                            }
                        }
                    }
                    log::info!("removed pane {} on TUI disconnect", pane_id);
                }
            }
        }
        ConnType::Chromium => {
            // Clear server tx for any server whose tx matches
            for (_, server) in st.servers.iter_mut() {
                if let Some(ref stx) = server.tx {
                    if stx.same_channel(tx) {
                        server.tx = None;
                        log::info!(
                            "cleared server tx on Chromium disconnect: profile={}",
                            server.profile
                        );
                        break;
                    }
                }
            }
        }
        ConnType::Unknown => {}
    }
}

fn resolve_browser_path(browser: &str) -> anyhow::Result<String> {
    let name = if browser.is_empty() {
        "roamium"
    } else {
        browser
    };

    if name.starts_with('/') {
        return Ok(name.to_string());
    }

    let home = std::env::var("HOME")?;
    let candidates = &[(
        "roamium",
        format!("{}/dev/termsurf/chromium/src/out/Default/roamium", home),
    )];

    for (n, path) in candidates {
        if *n == name && std::path::Path::new(path).exists() {
            return Ok(path.clone());
        }
    }

    anyhow::bail!("browser '{}' not found", name)
}

fn spawn_server(profile: &str, browser: &str) -> anyhow::Result<Server> {
    let binary = resolve_browser_path(browser)?;
    let sock = std::env::var("TERMSURF_SOCKET")?;

    let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{}/.local/share", home)
    });
    let user_data_dir = format!("{}/termsurf/chromium-profiles/{}", data_home, profile);

    let state_home = std::env::var("XDG_STATE_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{}/.local/state", home)
    });
    let log_file = format!("{}/termsurf/chromium-server.log", state_home);

    let child = std::process::Command::new(&binary)
        .arg(format!("--ipc-socket={}", sock))
        .arg(format!("--user-data-dir={}", user_data_dir))
        .arg("--hidden")
        .arg("--no-sandbox")
        .arg("--enable-logging")
        .arg(format!("--log-file={}", log_file))
        .spawn()
        .with_context(|| format!("spawn {}", binary))?;

    log::info!(
        "spawned {} (pid={}) for profile={}",
        browser,
        child.id(),
        profile
    );

    Ok(Server {
        profile: profile.to_string(),
        browser: browser.to_string(),
        process: Some(child),
        tx: None,
        pane_count: 1,
    })
}

fn send_create_tab(server_tx: &Sender<Vec<u8>>, pane: &Pane) -> anyhow::Result<()> {
    let msg = TermSurfMessage {
        msg: Some(Msg::CreateTab(proto::CreateTab {
            url: pane.url.clone(),
            pane_id: pane.pane_id.clone(),
            pixel_width: pane.pixel_width,
            pixel_height: pane.pixel_height,
            dark: pane.dark,
        })),
    };
    let payload = msg.encode_to_vec();
    server_tx.try_send(payload)?;
    log::info!("sent CreateTab: pane_id={} url={}", pane.pane_id, pane.url);
    Ok(())
}
