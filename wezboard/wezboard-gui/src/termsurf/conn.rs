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
        Some(Msg::UrlChanged(u)) => {
            log::info!("UrlChanged: tab_id={} url={}", u.tab_id, u.url);
            forward_to_tui(u.tab_id, Msg::UrlChanged(u), state);
        }
        Some(Msg::LoadingState(l)) => {
            log::debug!("LoadingState: tab_id={} state={}", l.tab_id, l.state);
            forward_to_tui(l.tab_id, Msg::LoadingState(l), state);
        }
        Some(Msg::TitleChanged(t)) => {
            log::info!("TitleChanged: tab_id={} title={}", t.tab_id, t.title);
            forward_to_tui(t.tab_id, Msg::TitleChanged(t), state);
        }
        Some(Msg::Navigate(n)) => {
            log::info!("Navigate: pane_id={} url={}", n.pane_id, n.url);
            let url = n.url.clone();
            forward_to_chromium(
                &n.pane_id,
                |tab_id| {
                    Msg::Navigate(proto::Navigate {
                        tab_id,
                        pane_id: String::new(),
                        url,
                    })
                },
                state,
            );
        }
        Some(Msg::SetColorScheme(s)) => {
            log::info!("SetColorScheme: pane_id={} dark={}", s.pane_id, s.dark);
            let dark = s.dark;
            {
                let mut st = state.lock().unwrap();
                if let Some(pane) = st.panes.get_mut(&s.pane_id) {
                    pane.dark = dark;
                }
            }
            forward_to_chromium(
                &s.pane_id,
                |tab_id| {
                    Msg::SetColorScheme(proto::SetColorScheme {
                        tab_id,
                        pane_id: String::new(),
                        dark,
                    })
                },
                state,
            );
        }
        Some(Msg::ModeChanged(m)) => {
            log::info!("ModeChanged: pane_id={} browsing={}", m.pane_id, m.browsing);
            let mut st = state.lock().unwrap();
            if let Some(pane) = st.panes.get_mut(&m.pane_id) {
                pane.browsing = m.browsing;
            }
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
        Some(other) => {
            log::debug!("unhandled TermSurf message: {:?}", other);
        }
        None => {
            log::debug!("empty TermSurf message");
        }
    }
    Ok(())
}

fn forward_to_tui(tab_id: i64, msg: Msg, state: &SharedState) {
    let st = state.lock().unwrap();
    let Some(pane_id) = st.tab_to_pane.get(&tab_id) else {
        log::warn!("forward_to_tui: unknown tab_id={}", tab_id);
        return;
    };
    let Some(pane) = st.panes.get(pane_id) else {
        return;
    };
    let wrapped = TermSurfMessage { msg: Some(msg) };
    let _ = pane.tui_tx.try_send(wrapped.encode_to_vec());
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
    let browser = if overlay.browser.is_empty() {
        "roamium".to_string()
    } else {
        overlay.browser.clone()
    };

    let (cell_w, cell_h, _, _) = super::metrics::get();
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
        ca_context_id: 0,
        ca_layer_host: 0,
        ca_layer_flipped: 0,
        ca_layer_positioning: 0,
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
) -> Option<*mut objc2::runtime::AnyObject> {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_core_foundation::CGRect;

    if state.overlay_view != 0 {
        // Already created — return its layer
        let view = state.overlay_view as *mut AnyObject;
        unsafe {
            let layer: *mut AnyObject = msg_send![view, layer];
            return if layer.is_null() { None } else { Some(layer) };
        }
    }

    let fe = crate::frontend::try_front_end()?;
    let ns_view = fe.first_ns_view()?;
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
        let _: () = msg_send![overlay, setLayer: root_layer];
        let _: () = msg_send![overlay, setWantsLayer: Bool::YES];

        // Add overlay as subview on top of terminal view
        let _: () = msg_send![superview, addSubview: overlay];

        // Retain overlay so it stays alive
        let _: *mut AnyObject = msg_send![overlay, retain];

        state.overlay_view = overlay as usize;
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
    use objc2_core_foundation::{CGPoint, CGRect};

    let mut st = state.lock().unwrap();
    let Some(pane_id) = st.tab_to_pane.get(&ca_context.tab_id).cloned() else {
        log::warn!("handle_ca_context: unknown tab_id={}", ca_context.tab_id);
        return;
    };
    if !st.panes.contains_key(&pane_id) {
        return;
    }

    // Get or create overlay before borrowing pane mutably
    let Some(root_layer) = get_or_create_overlay(&mut st) else {
        log::warn!("handle_ca_context: no overlay root layer");
        return;
    };

    let pane = st.panes.get_mut(&pane_id).unwrap();

    let context_id = ca_context.ca_context_id as u32;
    pane.ca_context_id = context_id;

    // Update pixel dimensions from CaContext if provided
    if ca_context.pixel_width > 0 {
        pane.pixel_width = ca_context.pixel_width;
    }
    if ca_context.pixel_height > 0 {
        pane.pixel_height = ca_context.pixel_height;
    }

    unsafe {
        let ca_transaction = cls(b"CATransaction\0");
        let _: () = msg_send![ca_transaction, begin];
        let _: () = msg_send![ca_transaction, setDisableActions: Bool::YES];

        if pane.ca_layer_host == 0 {
            // First time: create the 3-layer hierarchy
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

            // positioning_layer
            let positioning: *mut AnyObject = msg_send![ca_layer_class, layer];
            let _: () = msg_send![positioning, setAnchorPoint: zero_point];
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

            log::info!("created CALayerHost contextId={}", context_id);
        } else {
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
        }

        // Position the overlay
        update_ca_layer_frame(pane, root_layer);

        let _: () = msg_send![ca_transaction, commit];
    }
}

#[cfg(target_os = "macos")]
unsafe fn update_ca_layer_frame(pane: &Pane, root_layer: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let scale: f64 = msg_send![root_layer, contentsScale];
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let w = pane.pixel_width as f64 / scale;
    let h = pane.pixel_height as f64 / scale;
    let (_, _, pad_left, pad_top) = super::metrics::get();
    let x = pad_left as f64 / scale;
    let y = pad_top as f64 / scale;
    let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));

    let positioning = pane.ca_layer_positioning as *mut AnyObject;
    let _: () = msg_send![positioning, setFrame: frame];
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
