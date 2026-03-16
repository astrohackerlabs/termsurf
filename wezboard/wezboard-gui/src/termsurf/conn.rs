use super::proto;
use super::proto::term_surf_message::Msg;
use super::proto::TermSurfMessage;
use super::state::{Pane, Server, SharedState, TermSurfState};
use anyhow::Context;
use prost::Message;
use sha2::{Digest, Sha256};
use smol::channel::Sender;
use smol::io::{AsyncReadExt, AsyncWriteExt};
use smol::Async;
use std::collections::HashSet;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnType {
    Unknown,
    Tui,
    Chromium,
}

pub async fn handle_connection(stream: UnixStream, state: SharedState) -> anyhow::Result<()> {
    log::info!("handle_connection: starting");
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
    let mut msg_count: u64 = 0;

    loop {
        let n = match (&*stream).read(&mut tmp).await {
            Ok(n) => n,
            Err(e) => {
                log::error!("handle_connection: read error: {:#}", e);
                handle_disconnect(conn_type, &tx, &state);
                tx.close();
                return Err(e.into());
            }
        };
        if n == 0 {
            log::info!(
                "handle_connection: EOF conn_type={:?} msg_count={}",
                conn_type,
                msg_count
            );
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

            msg_count += 1;
            log::info!(
                "handle_connection: msg #{} type={} conn_type={:?}",
                msg_count,
                msg_type_name(&msg),
                conn_type
            );

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

fn msg_type_name(msg: &TermSurfMessage) -> &'static str {
    match &msg.msg {
        Some(Msg::ServerRegister(_)) => "ServerRegister",
        Some(Msg::SetOverlay(_)) => "SetOverlay",
        Some(Msg::TabReady(_)) => "TabReady",
        Some(Msg::HelloRequest(_)) => "HelloRequest",
        Some(Msg::HelloReply(_)) => "HelloReply",
        Some(Msg::UrlChanged(_)) => "UrlChanged",
        Some(Msg::LoadingState(_)) => "LoadingState",
        Some(Msg::TitleChanged(_)) => "TitleChanged",
        Some(Msg::Navigate(_)) => "Navigate",
        Some(Msg::SetColorScheme(_)) => "SetColorScheme",
        Some(Msg::ModeChanged(_)) => "ModeChanged",
        Some(Msg::CaContext(_)) => "CaContext",
        Some(Msg::QueryLastRequest(_)) => "QueryLastRequest",
        Some(Msg::QueryLastReply(_)) => "QueryLastReply",
        Some(Msg::QueryDevtoolsRequest(_)) => "QueryDevtoolsRequest",
        Some(Msg::QueryDevtoolsReply(_)) => "QueryDevtoolsReply",
        Some(Msg::QueryTabsRequest(_)) => "QueryTabsRequest",
        Some(Msg::QueryTabsReply(_)) => "QueryTabsReply",
        Some(Msg::Resize(_)) => "Resize",
        Some(Msg::CreateTab(_)) => "CreateTab",
        Some(Msg::CloseTab(_)) => "CloseTab",
        Some(Msg::CursorChanged(_)) => "CursorChanged",
        Some(Msg::OpenSplit(_)) => "OpenSplit",
        Some(Msg::SetDevtoolsOverlay(_)) => "SetDevtoolsOverlay",
        Some(_) => "Other",
        None => "None",
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
        Some(Msg::UrlChanged(u)) => {
            log::debug!(
                "UrlChanged: tab_id={} url={} (direct to TUI)",
                u.tab_id,
                u.url
            );
        }
        Some(Msg::LoadingState(l)) => {
            log::debug!(
                "LoadingState: tab_id={} state={} (direct to TUI)",
                l.tab_id,
                l.state
            );
        }
        Some(Msg::TitleChanged(t)) => {
            log::debug!(
                "TitleChanged: tab_id={} title={} (direct to TUI)",
                t.tab_id,
                t.title
            );
        }
        Some(Msg::Navigate(n)) => {
            log::debug!(
                "Navigate: pane_id={} url={} (direct to browser)",
                n.pane_id,
                n.url
            );
        }
        Some(Msg::SetColorScheme(s)) => {
            log::info!("SetColorScheme: pane_id={} dark={}", s.pane_id, s.dark);
            let mut st = state.lock().unwrap();
            if let Some(pane) = st.panes.get_mut(&s.pane_id) {
                pane.dark = s.dark;
            }
        }
        Some(Msg::ModeChanged(m)) => {
            log::info!("ModeChanged: pane_id={} browsing={}", m.pane_id, m.browsing);
            {
                let mut st = state.lock().unwrap();
                if let Some(pane) = st.panes.get_mut(&m.pane_id) {
                    pane.browsing = m.browsing;
                }
            }
            let browsing = m.browsing;
            forward_to_chromium(
                &m.pane_id,
                |tab_id| {
                    Msg::FocusChanged(proto::FocusChanged {
                        tab_id,
                        focused: browsing,
                    })
                },
                state,
            );
        }
        Some(Msg::CaContext(c)) => {
            log::info!(
                "CaContext: tab_id={} context_id={}",
                c.tab_id,
                c.ca_context_id
            );
            #[cfg(target_os = "macos")]
            if c.ca_context_id != 0 {
                handle_ca_context(c, state);
            }
        }
        Some(Msg::CursorChanged(c)) => {
            log::debug!(
                "CursorChanged: tab_id={} cursor_type={}",
                c.tab_id,
                c.cursor_type
            );
            let mut st = state.lock().unwrap();
            if let Some(pane_id) = st.tab_to_pane.get(&c.tab_id).cloned() {
                if let Some(pane) = st.panes.get_mut(&pane_id) {
                    pane.cursor_type = c.cursor_type;
                }
            }
        }
        Some(Msg::QueryLastRequest(q)) => {
            log::info!(
                "QueryLastRequest: pane_id={} profile={}",
                q.pane_id,
                q.profile
            );
            let reply = {
                let st = state.lock().unwrap();
                if let Some(ref last_id) = st.last_browser_pane {
                    if let Some(pane) = st.panes.get(last_id) {
                        if q.profile.is_empty() || pane.profile == q.profile {
                            proto::QueryLastReply {
                                pane_id: last_id.clone(),
                                tab_id: pane.tab_id,
                                profile: pane.profile.clone(),
                                error: String::new(),
                            }
                        } else {
                            proto::QueryLastReply {
                                error: "No matching pane for profile".into(),
                                ..Default::default()
                            }
                        }
                    } else {
                        proto::QueryLastReply {
                            error: "Last pane no longer exists".into(),
                            ..Default::default()
                        }
                    }
                } else {
                    proto::QueryLastReply {
                        error: "No browser pane yet".into(),
                        ..Default::default()
                    }
                }
            };
            let msg = TermSurfMessage {
                msg: Some(Msg::QueryLastReply(reply)),
            };
            let payload = msg.encode_to_vec();
            let len = (payload.len() as u32).to_le_bytes();
            (&**stream).write_all(&len).await?;
            (&**stream).write_all(&payload).await?;
        }
        Some(Msg::QueryDevtoolsRequest(q)) => {
            log::info!(
                "QueryDevtoolsRequest: pane_id={} inspected_tab_id={} profile={}",
                q.pane_id,
                q.inspected_tab_id,
                q.profile
            );
            let reply = {
                let st = state.lock().unwrap();

                // Resolve inspected_tab_id (0 means auto-target to last browser pane)
                let resolved_tab_id = if q.inspected_tab_id != 0 {
                    q.inspected_tab_id
                } else if let Some(ref last_id) = st.last_browser_pane {
                    st.panes.get(last_id).map(|p| p.tab_id).unwrap_or(0)
                } else {
                    0
                };

                if resolved_tab_id == 0 {
                    proto::QueryDevtoolsReply {
                        error: "No browser tab found".into(),
                        ..Default::default()
                    }
                } else {
                    // Check for duplicate DevTools
                    let already_open = st
                        .panes
                        .values()
                        .any(|p| p.inspected_tab_id == resolved_tab_id);
                    if already_open {
                        proto::QueryDevtoolsReply {
                            error: format!("Tab {} already has DevTools open", resolved_tab_id),
                            ..Default::default()
                        }
                    } else if let Some(inspected_pane_id) = st.tab_to_pane.get(&resolved_tab_id) {
                        let inspected_pane = st.panes.get(inspected_pane_id).unwrap();
                        proto::QueryDevtoolsReply {
                            tab_id: resolved_tab_id,
                            browser: inspected_pane.browser.clone(),
                            profile: inspected_pane.profile.clone(),
                            error: String::new(),
                        }
                    } else {
                        proto::QueryDevtoolsReply {
                            error: "Inspected tab not found".into(),
                            ..Default::default()
                        }
                    }
                }
            };
            let msg = TermSurfMessage {
                msg: Some(Msg::QueryDevtoolsReply(reply)),
            };
            let payload = msg.encode_to_vec();
            let len = (payload.len() as u32).to_le_bytes();
            (&**stream).write_all(&len).await?;
            (&**stream).write_all(&payload).await?;
        }
        Some(Msg::QueryTabsRequest(q)) => {
            log::info!(
                "QueryTabsRequest: pane_id={} profile={}",
                q.pane_id,
                q.profile
            );
            let reply = {
                let st = state.lock().unwrap();
                let gui_panes = st
                    .panes
                    .values()
                    .filter(|p| q.profile.is_empty() || p.profile == q.profile)
                    .count() as i64;
                proto::QueryTabsReply {
                    gui_panes,
                    chromium_tabs: 0,
                    chromium_browser: 0,
                    chromium_devtools: 0,
                    tabs: vec![],
                    error: String::new(),
                }
            };
            let msg = TermSurfMessage {
                msg: Some(Msg::QueryTabsReply(reply)),
            };
            let payload = msg.encode_to_vec();
            let len = (payload.len() as u32).to_le_bytes();
            (&**stream).write_all(&len).await?;
            (&**stream).write_all(&payload).await?;
        }
        Some(Msg::OpenSplit(o)) => {
            log::info!(
                "OpenSplit: pane_id={} direction={} command={}",
                o.pane_id,
                o.direction,
                o.command
            );
            handle_open_split(o);
        }
        Some(Msg::SetDevtoolsOverlay(d)) => {
            handle_set_devtools_overlay(d, tx.clone(), state)?;
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

fn forward_to_chromium(pane_id: &str, build_msg: impl FnOnce(i64) -> Msg, state: &SharedState) {
    let st = state.lock().unwrap();
    let Some(pane) = st.panes.get(pane_id) else {
        log::warn!("forward_to_chromium: unknown pane_id={}", pane_id);
        return;
    };
    if pane.tab_id == 0 {
        log::warn!("forward_to_chromium: pane {} has no tab yet", pane_id);
        return;
    }
    let tab_id = pane.tab_id;
    let key = TermSurfState::server_key(&pane.profile, &pane.browser);
    let Some(server) = st.servers.get(&key) else {
        return;
    };
    let Some(ref server_tx) = server.tx else {
        return;
    };
    let msg = TermSurfMessage {
        msg: Some(build_msg(tab_id)),
    };
    let _ = server_tx.try_send(msg.encode_to_vec());
}

fn handle_set_overlay(
    overlay: proto::SetOverlay,
    tui_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    log::info!(
        "SetOverlay state: panes={:?} servers={:?}",
        st.panes.keys().collect::<Vec<_>>(),
        st.servers.keys().collect::<Vec<_>>()
    );
    let browser = if overlay.browser.is_empty() {
        "roamium".to_string()
    } else {
        overlay.browser.clone()
    };

    let (cell_w, cell_h, _, _, _, _) = super::metrics::get();
    let pixel_w = if cell_w > 0 {
        overlay.width * cell_w as u64
    } else {
        overlay.width * 10
    };
    let pixel_h = if cell_h > 0 {
        overlay.height * cell_h as u64
    } else {
        overlay.height * 20
    };

    let is_new = !st.panes.contains_key(&overlay.pane_id);
    log::info!(
        "SetOverlay: pane_id={} is_new={} pixel={}x{}",
        overlay.pane_id,
        is_new,
        pixel_w,
        pixel_h
    );

    if !is_new {
        // Resize: update dimensions, extract values before releasing mutable borrow
        let (tab_id, profile, browser_name) = {
            let pane = st.panes.get_mut(&overlay.pane_id).unwrap();
            pane.pixel_width = pixel_w;
            pane.pixel_height = pixel_h;
            pane.col = overlay.col;
            pane.row = overlay.row;
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
        col: overlay.col,
        row: overlay.row,
        pixel_width: pixel_w,
        pixel_height: pixel_h,
        tab_id: 0,
        tui_tx,
        browsing: overlay.browsing,
        dark: false,
        inspected_tab_id: 0,
        ca_context_id: 0,
        pending_context_id: None,
        ca_layer_host: 0,
        ca_layer_flipped: 0,
        ca_layer_positioning: 0,
        overlay_origin_x: 0.0,
        overlay_origin_y: 0.0,
        overlay_scale: 1.0,
        cursor_type: 0,
        visible: false,
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
        let server = st.servers.get_mut(&key).unwrap();
        server.pane_count += 1;
        let has_tx = server.tx.is_some();
        log::info!(
            "SetOverlay: reusing server key={} pane_count={} has_tx={}",
            key,
            server.pane_count,
            has_tx
        );
        let server_tx = server.tx.clone();
        if let Some(ref stx) = server_tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_tab(stx, pane)?;
        } else {
            log::warn!("SetOverlay: server exists but tx is None — CreateTab not sent!");
        }
    }

    Ok(())
}

fn handle_set_devtools_overlay(
    overlay: proto::SetDevtoolsOverlay,
    tui_tx: Sender<Vec<u8>>,
    state: &SharedState,
) -> anyhow::Result<()> {
    let mut st = state.lock().unwrap();
    let browser = if overlay.browser.is_empty() {
        "roamium".to_string()
    } else {
        overlay.browser.clone()
    };

    let (cell_w, cell_h, _, _, _, _) = super::metrics::get();
    let pixel_w = if cell_w > 0 {
        overlay.width * cell_w as u64
    } else {
        overlay.width * 10
    };
    let pixel_h = if cell_h > 0 {
        overlay.height * cell_h as u64
    } else {
        overlay.height * 20
    };

    let is_new = !st.panes.contains_key(&overlay.pane_id);

    if !is_new {
        // Resize path — same as SetOverlay
        let (tab_id, profile, browser_name) = {
            let pane = st.panes.get_mut(&overlay.pane_id).unwrap();
            pane.pixel_width = pixel_w;
            pane.pixel_height = pixel_h;
            pane.col = overlay.col;
            pane.row = overlay.row;
            (pane.tab_id, pane.profile.clone(), pane.browser.clone())
        };
        if tab_id != 0 {
            let key = TermSurfState::server_key(&profile, &browser_name);
            if let Some(server) = st.servers.get(&key) {
                if let Some(ref server_tx) = server.tx {
                    let msg = TermSurfMessage {
                        msg: Some(Msg::Resize(proto::Resize {
                            tab_id,
                            pixel_width: pixel_w,
                            pixel_height: pixel_h,
                        })),
                    };
                    let _ = server_tx.try_send(msg.encode_to_vec());
                }
            }
        }
        return Ok(());
    }

    log::info!(
        "SetDevtoolsOverlay: pane_id={} profile={} browser={} inspected_tab_id={}",
        overlay.pane_id,
        overlay.profile,
        browser,
        overlay.inspected_tab_id
    );

    // Create new pane — like SetOverlay but with inspected_tab_id, no url
    let pane = Pane {
        pane_id: overlay.pane_id.clone(),
        profile: overlay.profile.clone(),
        browser: browser.clone(),
        url: String::new(),
        col: overlay.col,
        row: overlay.row,
        pixel_width: pixel_w,
        pixel_height: pixel_h,
        tab_id: 0,
        tui_tx,
        browsing: overlay.browsing,
        dark: false,
        inspected_tab_id: overlay.inspected_tab_id,
        ca_context_id: 0,
        pending_context_id: None,
        ca_layer_host: 0,
        ca_layer_flipped: 0,
        ca_layer_positioning: 0,
        overlay_origin_x: 0.0,
        overlay_origin_y: 0.0,
        overlay_scale: 1.0,
        cursor_type: 0,
        visible: false,
    };
    st.panes.insert(overlay.pane_id.clone(), pane);

    // Reuse existing server (DevTools uses the same Chromium process)
    let key = TermSurfState::server_key(&overlay.profile, &browser);
    if !st.servers.contains_key(&key) {
        drop(st);
        let server = spawn_server(&overlay.profile, &browser)?;
        let mut st = state.lock().unwrap();
        st.servers.insert(key.clone(), server);
        let server = st.servers.get(&key).unwrap();
        if let Some(ref server_tx) = server.tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_devtools_tab(server_tx, pane)?;
        }
    } else {
        let server = st.servers.get_mut(&key).unwrap();
        server.pane_count += 1;
        let server_tx = server.tx.clone();
        if let Some(ref stx) = server_tx {
            let pane = st.panes.get(&overlay.pane_id).unwrap();
            send_create_devtools_tab(stx, pane)?;
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
            if pane.inspected_tab_id != 0 {
                send_create_devtools_tab(&server_tx, pane)?;
            } else {
                send_create_tab(&server_tx, pane)?;
            }
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

        // Send BrowserReady to the TUI so it can connect directly to the browser.
        let pane = st.panes.get(&ready.pane_id).unwrap();
        let server_key = TermSurfState::server_key(&pane.profile, &pane.browser);
        let listen_socket = st
            .servers
            .get(&server_key)
            .map(|s| s.listen_socket.clone())
            .unwrap_or_default();
        if !listen_socket.is_empty() {
            let browser_ready = TermSurfMessage {
                msg: Some(Msg::BrowserReady(proto::BrowserReady {
                    pane_id: ready.pane_id.clone(),
                    tab_id: ready.tab_id,
                    browser_socket: listen_socket.clone(),
                })),
            };
            let payload = browser_ready.encode_to_vec();
            let tui_tx = pane.tui_tx.clone();
            let _ = tui_tx.try_send(payload);
            log::info!(
                "BrowserReady: pane_id={} tab_id={} socket={}",
                ready.pane_id,
                ready.tab_id,
                listen_socket
            );
        }

        log::info!(
            "TabReady: pane_id={} tab_id={} tab_to_pane_count={}",
            ready.pane_id,
            ready.tab_id,
            st.tab_to_pane.len()
        );
    } else {
        log::warn!("TabReady: unknown pane_id={}", ready.pane_id);
    }
    Ok(())
}

fn handle_open_split(split: proto::OpenSplit) {
    use mux::tab::{SplitDirection, SplitRequest, SplitSize};

    // Parse direction
    let (direction, target_is_second) = match split.direction.as_str() {
        "right" => (SplitDirection::Horizontal, true),
        "left" => (SplitDirection::Horizontal, false),
        "down" => (SplitDirection::Vertical, true),
        "up" => (SplitDirection::Vertical, false),
        other => {
            log::warn!("OpenSplit: unknown direction '{}'", other);
            return;
        }
    };

    let numeric_pane_id: usize = match split.pane_id.parse() {
        Ok(id) => id,
        Err(_) => {
            log::warn!("OpenSplit: invalid pane_id '{}'", split.pane_id);
            return;
        }
    };

    // Build the command from the space-separated string
    let args: Vec<String> = split
        .command
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if args.is_empty() {
        log::warn!("OpenSplit: empty command");
        return;
    }

    let request = SplitRequest {
        direction,
        target_is_second,
        top_level: false,
        size: SplitSize::Percent(50),
    };

    let command = split.command.clone();
    promise::spawn::spawn(async move {
        let mux = mux::Mux::get();
        let cmd_builder = portable_pty::CommandBuilder::from_argv(
            args.iter().map(|s| s.as_str().into()).collect(),
        );
        match mux
            .split_pane(
                numeric_pane_id,
                request,
                mux::domain::SplitSource::Spawn {
                    command: Some(cmd_builder),
                    command_dir: None,
                },
                config::keyassignment::SpawnTabDomain::CurrentPaneDomain,
            )
            .await
        {
            Ok((pane, _size)) => {
                log::info!(
                    "OpenSplit: created pane {} with command '{}'",
                    pane.pane_id(),
                    command
                );
            }
            Err(e) => {
                log::error!("OpenSplit: split_pane failed: {:#}", e);
            }
        }
    })
    .detach();
}

fn handle_disconnect(conn_type: ConnType, tx: &Sender<Vec<u8>>, state: &SharedState) {
    let mut st = state.lock().unwrap();
    log::info!(
        "handle_disconnect: conn_type={:?} panes={} servers={} tab_to_pane={}",
        conn_type,
        st.panes.len(),
        st.servers.len(),
        st.tab_to_pane.len()
    );
    for (key, server) in &st.servers {
        log::info!(
            "  server key={} profile={} has_tx={} pane_count={}",
            key,
            server.profile,
            server.tx.is_some(),
            server.pane_count
        );
    }
    match conn_type {
        ConnType::Tui => {
            // Remove panes whose tui_tx matches this connection's tx
            let to_remove: Vec<String> = st
                .panes
                .iter()
                .filter(|(_, p)| p.tui_tx.same_channel(tx))
                .map(|(id, _)| id.clone())
                .collect();
            let mut servers_to_remove: Vec<String> = vec![];
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
                                if server.pane_count == 0 {
                                    servers_to_remove.push(key.clone());
                                }
                            }
                        }
                    }
                    #[cfg(target_os = "macos")]
                    if pane.ca_layer_host != 0 {
                        remove_ca_layers(
                            pane.ca_layer_host,
                            pane.ca_layer_positioning,
                            pane.ca_layer_flipped,
                        );
                    }
                    log::info!("removed pane {} on TUI disconnect", pane_id);
                }
            }
            for key in &servers_to_remove {
                st.servers.remove(key);
                log::info!("removed server {} (no remaining panes)", key);
            }
            log::info!(
                "handle_disconnect: after TUI cleanup panes={} servers={} tab_to_pane={}",
                st.panes.len(),
                st.servers.len(),
                st.tab_to_pane.len()
            );
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

    let candidates: &[(&str, &str)] = &[("roamium", "/usr/local/roamium/roamium")];

    for (n, path) in candidates {
        if *n == name && std::path::Path::new(path).exists() {
            return Ok(path.to_string());
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

    // Construct listen socket path before spawn. Use browser name + 4-byte hash
    // of the full path (for testing different builds) + GUI PID + profile.
    let browser_name = std::path::Path::new(&browser)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("browser");
    let hash = Sha256::digest(browser.as_bytes());
    let hash_hex = format!(
        "{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3]
    );
    let listen_sock = format!(
        "{}/termsurf/{}-{}-{}-{}.sock",
        std::env::temp_dir().display(),
        browser_name,
        hash_hex,
        std::process::id(),
        profile,
    );

    let child = std::process::Command::new(&binary)
        .arg(format!("--ipc-socket={}", sock))
        .arg(format!("--user-data-dir={}", user_data_dir))
        .arg(format!("--listen-socket={}", listen_sock))
        .arg("--hidden")
        .arg("--no-sandbox")
        .arg("--enable-logging")
        .arg(format!("--log-file={}", log_file))
        .spawn()
        .with_context(|| format!("spawn {}", binary))?;

    log::info!(
        "spawned {} (pid={}) for profile={} listen_socket={}",
        browser,
        child.id(),
        profile,
        listen_sock
    );

    Ok(Server {
        profile: profile.to_string(),
        browser: browser.to_string(),
        process: Some(child),
        tx: None,
        listen_socket: listen_sock,
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

fn send_create_devtools_tab(server_tx: &Sender<Vec<u8>>, pane: &Pane) -> anyhow::Result<()> {
    let msg = TermSurfMessage {
        msg: Some(Msg::CreateDevtoolsTab(proto::CreateDevtoolsTab {
            pane_id: pane.pane_id.clone(),
            inspected_tab_id: pane.inspected_tab_id,
            pixel_width: pane.pixel_width,
            pixel_height: pane.pixel_height,
            dark: pane.dark,
        })),
    };
    let payload = msg.encode_to_vec();
    server_tx.try_send(payload)?;
    log::info!(
        "sent CreateDevtoolsTab: pane_id={} inspected_tab_id={}",
        pane.pane_id,
        pane.inspected_tab_id
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn cls(name: &[u8]) -> &'static objc2::runtime::AnyClass {
    let cname = std::ffi::CStr::from_bytes_with_nul(name).unwrap();
    objc2::runtime::AnyClass::get(cname).unwrap()
}

#[cfg(target_os = "macos")]
fn register_overlay_class() -> &'static objc2::runtime::AnyClass {
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use std::ffi::CStr;

    static ONCE: std::sync::Once = std::sync::Once::new();
    let name = CStr::from_bytes_with_nul(b"TermSurfOverlayView\0").unwrap();

    ONCE.call_once(|| {
        let superclass = AnyClass::get(CStr::from_bytes_with_nul(b"NSView\0").unwrap()).unwrap();
        let mut cls = ClassBuilder::new(name, superclass)
            .expect("Unable to register TermSurfOverlayView class");

        // hitTest: returns nil — all mouse events pass through to the terminal view
        extern "C" fn hit_test(
            _this: *mut AnyObject,
            _sel: Sel,
            _point: objc2_core_foundation::CGPoint,
        ) -> *mut AnyObject {
            std::ptr::null_mut()
        }

        extern "C" fn accepts_first_responder(_this: *mut AnyObject, _sel: Sel) -> Bool {
            Bool::NO
        }

        unsafe {
            cls.add_method(
                objc2::sel!(hitTest:),
                hit_test
                    as extern "C" fn(
                        *mut AnyObject,
                        Sel,
                        objc2_core_foundation::CGPoint,
                    ) -> *mut AnyObject,
            );
            cls.add_method(
                objc2::sel!(acceptsFirstResponder),
                accepts_first_responder as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
        }

        cls.register();
    });

    AnyClass::get(name).unwrap()
}

/// Get or create the transparent overlay NSView and return its root layer.
/// The overlay is layer-hosting (we own the layer tree), so CALayerHost
/// sublayers composite correctly — unlike WezTerm's layer-backed terminal view.
#[cfg(target_os = "macos")]
fn get_or_create_overlay(
    state: &mut super::state::TermSurfState,
    mux_window_id: usize,
) -> Option<*mut objc2::runtime::AnyObject> {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_core_foundation::CGRect;

    if let Some(&ptr) = state.overlay_views.get(&mux_window_id) {
        // Already created — return its layer
        let view = ptr as *mut AnyObject;
        unsafe {
            let layer: *mut AnyObject = msg_send![view, layer];
            return if layer.is_null() { None } else { Some(layer) };
        }
    }

    let fe = crate::frontend::try_front_end()?;
    let ns_view = fe.ns_view_for_mux_window(mux_window_id)?;
    let ns_view = ns_view as *mut AnyObject;

    unsafe {
        // Get superview (contentView of the window)
        let superview: *mut AnyObject = msg_send![ns_view, superview];
        if superview.is_null() {
            log::warn!("get_or_create_overlay: terminal view has no superview");
            return None;
        }

        // Get terminal view's frame for overlay sizing
        let frame: CGRect = msg_send![ns_view, frame];

        // Create overlay view
        let overlay_class = register_overlay_class();
        let overlay: *mut AnyObject = msg_send![overlay_class, alloc];
        let overlay: *mut AnyObject = msg_send![overlay, initWithFrame: frame];

        // Set autoresizing mask: width + height sizable (follows parent resizes)
        // NSView uses NSUInteger (u64) for autoresizingMask, unlike CALayer (u32)
        let _: () = msg_send![overlay, setAutoresizingMask: 18u64];

        // Create root layer and make the overlay layer-hosting.
        // CRITICAL: assign layer BEFORE setting wantsLayer (layer-hosting order).
        let ca_layer_class = cls(b"CALayer\0");
        let root_layer: *mut AnyObject = msg_send![ca_layer_class, layer];
        let _: () = msg_send![root_layer, setOpaque: Bool::NO];
        // Set contentsScale to match the screen's backing scale factor (2.0 on Retina).
        // Without this, contentsScale defaults to 1.0 and all pixel→point conversions
        // in set_overlay_frame are wrong.
        let window: *mut AnyObject = msg_send![superview, window];
        if !window.is_null() {
            let backing_scale: f64 = msg_send![window, backingScaleFactor];
            let _: () = msg_send![root_layer, setContentsScale: backing_scale];
            log::info!("overlay root layer contentsScale={}", backing_scale);
        }
        let _: () = msg_send![overlay, setLayer: root_layer];
        let _: () = msg_send![overlay, setWantsLayer: Bool::YES];

        // Add overlay as subview on top of terminal view
        let _: () = msg_send![superview, addSubview: overlay];

        // Retain overlay so it stays alive
        let _: *mut AnyObject = msg_send![overlay, retain];

        state.overlay_views.insert(mux_window_id, overlay as usize);
        log::info!("created overlay NSView");

        if root_layer.is_null() {
            None
        } else {
            Some(root_layer)
        }
    }
}

#[cfg(target_os = "macos")]
fn handle_ca_context(ca_context: proto::CaContext, state: &SharedState) {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_core_foundation::CGPoint;

    let mut st = state.lock().unwrap();
    let Some(pane_id) = st.tab_to_pane.get(&ca_context.tab_id).cloned() else {
        log::warn!("handle_ca_context: unknown tab_id={}", ca_context.tab_id);
        return;
    };
    if !st.panes.contains_key(&pane_id) {
        return;
    }

    // Look up which mux window this pane belongs to
    let Some(mux_window_id) = get_pane_mux_window(&pane_id) else {
        log::warn!("handle_ca_context: pane {} not in any mux window", pane_id);
        return;
    };

    // Ensure overlay NSView exists before borrowing pane mutably
    if get_or_create_overlay(&mut st, mux_window_id).is_none() {
        log::warn!("handle_ca_context: no overlay root layer");
        return;
    };

    let pane = st.panes.get_mut(&pane_id).unwrap();
    log::info!(
        "handle_ca_context: tab_id={} pane_id={} has_layers={}",
        ca_context.tab_id,
        pane_id,
        pane.ca_layer_host != 0
    );

    let context_id = ca_context.ca_context_id as u32;
    pane.ca_context_id = context_id;

    // Update pixel dimensions from CaContext if provided
    if ca_context.pixel_width > 0 {
        pane.pixel_width = ca_context.pixel_width;
    }
    if ca_context.pixel_height > 0 {
        pane.pixel_height = ca_context.pixel_height;
    }

    if pane.ca_layer_host == 0 {
        // Defer CALayerHost creation to the render pass, which knows the
        // correct split-aware position. This avoids a one-frame flash.
        pane.pending_context_id = Some(context_id);
        log::info!(
            "deferred CALayerHost creation: pane_id={} contextId={}",
            pane_id,
            context_id
        );
    } else {
        unsafe {
            let ca_transaction = cls(b"CATransaction\0");
            let _: () = msg_send![ca_transaction, begin];
            let _: () = msg_send![ca_transaction, setDisableActions: Bool::YES];
            // Atomic swap: create new host, add, remove old, release old
            let ca_layer_host_class = cls(b"CALayerHost\0");
            let new_host: *mut AnyObject = msg_send![ca_layer_host_class, layer];
            let zero_point = CGPoint::new(0.0, 0.0);
            let _: () = msg_send![new_host, setContextId: context_id];
            let _: () = msg_send![new_host, setAnchorPoint: zero_point];
            let _: () = msg_send![new_host, setAutoresizingMask: 36u32];

            let positioning = pane.ca_layer_positioning as *mut AnyObject;
            let _: () = msg_send![positioning, addSublayer: new_host];
            let _: *mut AnyObject = msg_send![new_host, retain];

            let old_host = pane.ca_layer_host as *mut AnyObject;
            let _: () = msg_send![old_host, removeFromSuperlayer];
            let _: () = msg_send![old_host, release];

            pane.ca_layer_host = new_host as usize;

            log::info!("swapped CALayerHost contextId={}", context_id);

            let _: () = msg_send![ca_transaction, commit];
        }
    }
}

fn get_pane_mux_window(pane_id: &str) -> Option<mux::window::WindowId> {
    let numeric_id: usize = pane_id.parse().ok()?;
    let mux = mux::Mux::get();
    for window_id in mux.iter_windows() {
        if let Some(w) = mux.get_window(window_id) {
            for tab in w.iter() {
                for pos in tab.iter_panes() {
                    if pos.pane.pane_id() == numeric_id {
                        return Some(window_id);
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn set_overlay_frame(
    pane_id: usize,
    x_backing: f64,
    y_backing: f64,
    w_backing: f64,
    h_backing: f64,
    dpi: usize,
) {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let Some(state) = super::state::global() else {
        return;
    };
    let mut st = state.lock().unwrap();
    let id = pane_id.to_string();
    let Some(pane) = st.panes.get_mut(&id) else {
        return;
    };
    if pane.ca_layer_positioning == 0 {
        return;
    }

    let scale = dpi as f64 / 72.0;
    let scale = if scale > 0.0 { scale } else { 1.0 };

    pane.overlay_origin_x = x_backing;
    pane.overlay_origin_y = y_backing;
    pane.overlay_scale = scale;

    let x = x_backing / scale;
    let y = y_backing / scale;
    let w = w_backing / scale;
    let h = h_backing / scale;
    unsafe {
        let layer = pane.ca_layer_positioning as *mut AnyObject;
        let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));
        let ca = cls(b"CATransaction\0");
        let _: () = msg_send![ca, begin];
        let _: () = msg_send![ca, setDisableActions: Bool::YES];
        let _: () = msg_send![layer, setFrame: frame];
        let _: () = msg_send![ca, commit];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_overlay_frame(_pane_id: usize, _x: f64, _y: f64, _w: f64, _h: f64, _dpi: usize) {}

#[cfg(target_os = "macos")]
pub fn create_pending_ca_layer_host(
    pane_id: usize,
    x_backing: f64,
    y_backing: f64,
    w_backing: f64,
    h_backing: f64,
    dpi: usize,
) {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let Some(state) = super::state::global() else {
        return;
    };
    let mut st = state.lock().unwrap();
    let id = pane_id.to_string();

    let context_id = match st.panes.get_mut(&id) {
        Some(pane) => match pane.pending_context_id.take() {
            Some(id) => id,
            None => return,
        },
        None => return,
    };

    let Some(mux_window_id) = get_pane_mux_window(&id) else {
        return;
    };
    let Some(root_layer) = get_or_create_overlay(&mut st, mux_window_id) else {
        return;
    };

    let pane = st.panes.get_mut(&id).unwrap();

    let scale = dpi as f64 / 72.0;
    let scale = if scale > 0.0 { scale } else { 1.0 };

    pane.overlay_origin_x = x_backing;
    pane.overlay_origin_y = y_backing;
    pane.overlay_scale = scale;

    let x = x_backing / scale;
    let y = y_backing / scale;
    let w = w_backing / scale;
    let h = h_backing / scale;

    unsafe {
        let ca_transaction = cls(b"CATransaction\0");
        let _: () = msg_send![ca_transaction, begin];
        let _: () = msg_send![ca_transaction, setDisableActions: Bool::YES];

        let ca_layer_class = cls(b"CALayer\0");
        let ca_layer_host_class = cls(b"CALayerHost\0");

        // flipped_layer
        let flipped: *mut AnyObject = msg_send![ca_layer_class, layer];
        let _: () = msg_send![flipped, setGeometryFlipped: Bool::YES];
        let zero_point = CGPoint::new(0.0, 0.0);
        let _: () = msg_send![flipped, setAnchorPoint: zero_point];
        let _: () = msg_send![flipped, setAutoresizingMask: 18u32]; // widthSizable | heightSizable
        let parent_bounds: CGRect = msg_send![root_layer, bounds];
        let _: () = msg_send![flipped, setFrame: parent_bounds];
        let _: () = msg_send![root_layer, addSublayer: flipped];
        let _: *mut AnyObject = msg_send![flipped, retain];

        // positioning_layer — created at the correct position
        let positioning: *mut AnyObject = msg_send![ca_layer_class, layer];
        let _: () = msg_send![positioning, setAnchorPoint: zero_point];
        let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));
        let _: () = msg_send![positioning, setFrame: frame];
        let _: () = msg_send![flipped, addSublayer: positioning];
        let _: *mut AnyObject = msg_send![positioning, retain];

        // CALayerHost
        let host: *mut AnyObject = msg_send![ca_layer_host_class, layer];
        let _: () = msg_send![host, setContextId: context_id];
        let _: () = msg_send![host, setAnchorPoint: zero_point];
        let _: () = msg_send![host, setAutoresizingMask: 36u32]; // maxXMargin | maxYMargin
        let _: () = msg_send![positioning, addSublayer: host];
        let _: *mut AnyObject = msg_send![host, retain];

        pane.ca_layer_flipped = flipped as usize;
        pane.ca_layer_positioning = positioning as usize;
        pane.ca_layer_host = host as usize;

        log::info!(
            "CALayerHost created at ({:.1},{:.1},{:.1},{:.1}): pane_id={} contextId={} flipped={:#x} host={:#x}",
            x,
            y,
            w,
            h,
            pane_id,
            context_id,
            pane.ca_layer_flipped,
            pane.ca_layer_host
        );

        let _: () = msg_send![ca_transaction, commit];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn create_pending_ca_layer_host(
    _pane_id: usize,
    _x: f64,
    _y: f64,
    _w: f64,
    _h: f64,
    _dpi: usize,
) {
}

#[cfg(target_os = "macos")]
fn remove_ca_layers(host: usize, positioning: usize, flipped: usize) {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};

    unsafe {
        let ca_transaction = cls(b"CATransaction\0");
        let _: () = msg_send![ca_transaction, begin];
        let _: () = msg_send![ca_transaction, setDisableActions: Bool::YES];

        if host != 0 {
            let layer = host as *mut AnyObject;
            let _: () = msg_send![layer, removeFromSuperlayer];
            let _: () = msg_send![layer, release];
        }
        if positioning != 0 {
            let layer = positioning as *mut AnyObject;
            let _: () = msg_send![layer, removeFromSuperlayer];
            let _: () = msg_send![layer, release];
        }
        if flipped != 0 {
            let layer = flipped as *mut AnyObject;
            let _: () = msg_send![layer, removeFromSuperlayer];
            let _: () = msg_send![layer, release];
        }

        let _: () = msg_send![ca_transaction, commit];
    }
}

#[cfg(target_os = "macos")]
pub fn sync_overlay_visibility(active_pane_ids: &HashSet<String>) {
    let Some(state) = super::shared_state() else {
        return;
    };
    let mut st = state.lock().unwrap();
    log::info!(
        "sync_overlay_visibility: active_ids={:?} pane_count={}",
        active_pane_ids,
        st.panes.len()
    );
    for (pane_id, pane) in &mut st.panes {
        if pane.ca_layer_flipped == 0 {
            log::info!("  pane_id={} skipped (no layer)", pane_id);
            continue;
        }
        let is_active = active_pane_ids.contains(pane_id.as_str());
        pane.visible = is_active;
        log::info!(
            "  pane_id={} is_active={} ca_layer_flipped={:#x}",
            pane_id,
            is_active,
            pane.ca_layer_flipped
        );
        unsafe {
            use objc2::msg_send;
            use objc2::runtime::Bool;
            let ca = cls(b"CATransaction\0");
            let _: () = msg_send![ca, begin];
            let _: () = msg_send![ca, setDisableActions: Bool::YES];
            let layer = pane.ca_layer_flipped as *mut objc2::runtime::AnyObject;
            let hidden = if is_active { Bool::NO } else { Bool::YES };
            let _: () = msg_send![layer, setHidden: hidden];
            let _: () = msg_send![ca, commit];
        }
    }
}
