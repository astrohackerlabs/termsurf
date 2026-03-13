use super::proto;
use super::proto::TermSurfMessage;
use super::proto::term_surf_message::Msg;
use ::window::{
    KeyCode, Modifiers, MouseButtons, MouseCursor, MouseEvent, MouseEventKind as WMEK, MousePress,
};
use prost::Message;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct MouseState {
    left_click_count: i64,
    last_click_time: Option<Instant>,
    last_click_x: f64,
    last_click_y: f64,
    drag_pane: Option<String>,
}

static MOUSE_STATE: OnceLock<Mutex<MouseState>> = OnceLock::new();

fn mouse_state() -> &'static Mutex<MouseState> {
    MOUSE_STATE.get_or_init(|| {
        Mutex::new(MouseState {
            left_click_count: 0,
            last_click_time: None,
            last_click_x: 0.0,
            last_click_y: 0.0,
            drag_pane: None,
        })
    })
}

fn compute_click_count(x: f64, y: f64) -> i64 {
    let mut ms = mouse_state().lock().unwrap();
    let now = Instant::now();
    let dist = ((x - ms.last_click_x).powi(2) + (y - ms.last_click_y).powi(2)).sqrt();
    let expired = ms
        .last_click_time
        .map(|t| now.duration_since(t).as_millis() > 500)
        .unwrap_or(true);
    if dist > 5.0 || expired {
        ms.left_click_count = 0;
    }
    ms.left_click_count = (ms.left_click_count % 3) + 1;
    ms.last_click_time = Some(now);
    ms.last_click_x = x;
    ms.last_click_y = y;
    ms.left_click_count
}

fn clamp_to_overlay(pane_id_str: &str, event: &MouseEvent) -> Option<(f64, f64)> {
    let state = super::shared_state()?;
    let st = state.lock().unwrap();
    let pane = st.panes.get(pane_id_str)?;
    let ox = pane.overlay_origin_x;
    let oy = pane.overlay_origin_y;
    let ow = pane.pixel_width as f64;
    let oh = pane.pixel_height as f64;
    let scale = pane.overlay_scale;
    let mx = (event.coords.x as f64).clamp(ox, ox + ow - 1.0);
    let my = (event.coords.y as f64).clamp(oy, oy + oh - 1.0);
    Some(((mx - ox) / scale, (my - oy) / scale))
}

/// Check if pane is browsing and forward key. Returns Some(true) if consumed.
pub fn try_forward_key(
    pane_id: usize,
    keycode: &KeyCode,
    modifiers: Modifiers,
    is_down: bool,
    key_event: Option<&::window::KeyEvent>,
    only_key_bindings: bool,
) -> Option<bool> {
    if only_key_bindings {
        return None;
    }
    let pane_id_str = pane_id.to_string();
    let state = super::shared_state()?;
    let browsing = {
        let st = state.lock().unwrap();
        let pane = st.panes.get(&pane_id_str)?;
        pane.browsing
    };

    if !browsing {
        return None;
    }

    // Esc key press (no Ctrl) exits browse mode
    if is_down {
        let is_esc = matches!(keycode, KeyCode::Char('\u{1b}'));
        if is_esc && !modifiers.contains(Modifiers::CTRL) {
            {
                let mut st = state.lock().unwrap();
                if let Some(pane) = st.panes.get_mut(&pane_id_str) {
                    pane.browsing = false;
                }
            }
            send_mode_and_focus(&pane_id_str, false);
            return Some(true);
        }
    }

    // Build and send KeyEvent proto
    let event_type = if is_down { "down" } else { "up" };
    let windows_key_code = keycode_to_windows_vk(keycode);
    let mods = modifiers_to_termsurf(modifiers);

    // Extract UTF-8 text from the key event if available
    let utf8 = match key_event {
        Some(ke) => match &ke.key {
            KeyCode::Char(c) => {
                if modifiers.intersects(Modifiers::CTRL | Modifiers::ALT | Modifiers::SUPER) {
                    String::new()
                } else {
                    c.to_string()
                }
            }
            KeyCode::Composed(s) => s.clone(),
            _ => String::new(),
        },
        None => match keycode {
            KeyCode::Char(c) => {
                if modifiers.intersects(Modifiers::CTRL | Modifiers::ALT | Modifiers::SUPER) {
                    String::new()
                } else {
                    c.to_string()
                }
            }
            _ => String::new(),
        },
    };

    send_to_chromium(
        &pane_id_str,
        Msg::KeyEvent(proto::KeyEvent {
            tab_id: 0, // filled by send_to_chromium
            r#type: event_type.to_string(),
            windows_key_code,
            utf8,
            modifiers: mods,
        }),
    );

    Some(true)
}

/// Try to forward mouse/scroll event. Returns true if consumed.
pub fn try_forward_mouse(pane_id: usize, event: &MouseEvent) -> bool {
    let pane_id_str = pane_id.to_string();
    let Some(state) = super::shared_state() else {
        return false;
    };

    let (has_pane, has_tab, was_browsing) = {
        let st = state.lock().unwrap();
        match st.panes.get(&pane_id_str) {
            Some(pane) => (true, pane.tab_id != 0, pane.browsing),
            None => (false, false, false),
        }
    };

    if !has_pane || !has_tab {
        return false;
    }

    if let Some((rel_x, rel_y)) = hit_test_overlay(&pane_id_str, event) {
        // Hit inside overlay
        match &event.kind {
            WMEK::Press(MousePress::Left) if !was_browsing => {
                // Click on overlay while not browsing → enter browse mode
                {
                    let mut st = state.lock().unwrap();
                    if let Some(pane) = st.panes.get_mut(&pane_id_str) {
                        pane.browsing = true;
                    }
                }
                send_mode_and_focus(&pane_id_str, true);
                // Also forward the click
                let mods = modifiers_to_termsurf(event.modifiers);
                let cc = compute_click_count(rel_x, rel_y);
                send_to_chromium(
                    &pane_id_str,
                    Msg::MouseEvent(proto::MouseEvent {
                        tab_id: 0,
                        r#type: "down".to_string(),
                        button: "left".to_string(),
                        x: rel_x,
                        y: rel_y,
                        click_count: cc,
                        modifiers: mods,
                    }),
                );
                mouse_state().lock().unwrap().drag_pane = Some(pane_id_str.clone());
                return true;
            }
            WMEK::Press(press) => {
                let (button_str, event_type) = match press {
                    MousePress::Left => ("left", "down"),
                    MousePress::Right => ("right", "down"),
                    MousePress::Middle => ("middle", "down"),
                };
                let mods = modifiers_to_termsurf(event.modifiers);
                let cc = if matches!(press, MousePress::Left) {
                    compute_click_count(rel_x, rel_y)
                } else {
                    1
                };
                send_to_chromium(
                    &pane_id_str,
                    Msg::MouseEvent(proto::MouseEvent {
                        tab_id: 0,
                        r#type: event_type.to_string(),
                        button: button_str.to_string(),
                        x: rel_x,
                        y: rel_y,
                        click_count: cc,
                        modifiers: mods,
                    }),
                );
                mouse_state().lock().unwrap().drag_pane = Some(pane_id_str.clone());
                return true;
            }
            WMEK::Release(press) => {
                let button_str = match press {
                    MousePress::Left => "left",
                    MousePress::Right => "right",
                    MousePress::Middle => "middle",
                };
                let mods = modifiers_to_termsurf(event.modifiers);
                send_to_chromium(
                    &pane_id_str,
                    Msg::MouseEvent(proto::MouseEvent {
                        tab_id: 0,
                        r#type: "up".to_string(),
                        button: button_str.to_string(),
                        x: rel_x,
                        y: rel_y,
                        click_count: 1,
                        modifiers: mods,
                    }),
                );
                mouse_state().lock().unwrap().drag_pane = None;
                return true;
            }
            WMEK::Move => {
                let mut mods = modifiers_to_termsurf(event.modifiers);
                if event.mouse_buttons.contains(MouseButtons::LEFT) {
                    mods |= 64;
                }
                if event.mouse_buttons.contains(MouseButtons::RIGHT) {
                    mods |= 256;
                }
                send_to_chromium(
                    &pane_id_str,
                    Msg::MouseMove(proto::MouseMove {
                        tab_id: 0,
                        x: rel_x,
                        y: rel_y,
                        modifiers: mods,
                    }),
                );
                return true;
            }
            WMEK::VertWheel(_) | WMEK::HorzWheel(_) => {
                // Consumed — raw scroll already forwarded via RawScrollEvent.
                return true;
            }
        }
    }

    // Drag outside overlay — clamp to overlay bounds
    if matches!(&event.kind, WMEK::Move | WMEK::Release(_)) {
        let drag = mouse_state().lock().unwrap().drag_pane.clone();
        if let Some(ref dp) = drag {
            if *dp == pane_id_str {
                if let Some((cx, cy)) = clamp_to_overlay(&pane_id_str, event) {
                    match &event.kind {
                        WMEK::Move => {
                            let mut mods = modifiers_to_termsurf(event.modifiers);
                            if event.mouse_buttons.contains(MouseButtons::LEFT) {
                                mods |= 64;
                            }
                            if event.mouse_buttons.contains(MouseButtons::RIGHT) {
                                mods |= 256;
                            }
                            send_to_chromium(
                                &pane_id_str,
                                Msg::MouseMove(proto::MouseMove {
                                    tab_id: 0,
                                    x: cx,
                                    y: cy,
                                    modifiers: mods,
                                }),
                            );
                            return true;
                        }
                        WMEK::Release(press) => {
                            let button_str = match press {
                                MousePress::Left => "left",
                                MousePress::Right => "right",
                                MousePress::Middle => "middle",
                            };
                            let mods = modifiers_to_termsurf(event.modifiers);
                            send_to_chromium(
                                &pane_id_str,
                                Msg::MouseEvent(proto::MouseEvent {
                                    tab_id: 0,
                                    r#type: "up".to_string(),
                                    button: button_str.to_string(),
                                    x: cx,
                                    y: cy,
                                    click_count: 1,
                                    modifiers: mods,
                                }),
                            );
                            mouse_state().lock().unwrap().drag_pane = None;
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Miss — click outside overlay
    if matches!(&event.kind, WMEK::Press(MousePress::Left)) && was_browsing {
        {
            let mut st = state.lock().unwrap();
            if let Some(pane) = st.panes.get_mut(&pane_id_str) {
                pane.browsing = false;
            }
        }
        send_mode_and_focus(&pane_id_str, false);
    }

    false
}

fn keycode_to_windows_vk(key: &KeyCode) -> i64 {
    match key {
        KeyCode::Char(c) => match c {
            'a'..='z' => (*c as u8 - b'a' + 0x41) as i64,
            'A'..='Z' => (*c as u8 - b'A' + 0x41) as i64,
            '0'..='9' => (*c as u8) as i64, // 0x30-0x39
            ' ' => 0x20,
            ';' | ':' => 0xBA,
            '=' | '+' => 0xBB,
            ',' | '<' => 0xBC,
            '-' | '_' => 0xBD,
            '.' | '>' => 0xBE,
            '/' | '?' => 0xBF,
            '`' | '~' => 0xC0,
            '[' | '{' => 0xDB,
            '\\' | '|' => 0xDC,
            ']' | '}' => 0xDD,
            '\'' | '"' => 0xDE,
            '\r' => 0x0D,
            '\t' => 0x09,
            '\u{08}' => 0x08,
            '\u{1b}' => 0x1B,
            '\u{7f}' => 0x2E, // Delete
            _ => 0,
        },
        KeyCode::Function(n) => 0x70 + (*n as i64 - 1),
        KeyCode::UpArrow => 0x26,
        KeyCode::DownArrow => 0x28,
        KeyCode::LeftArrow => 0x25,
        KeyCode::RightArrow => 0x27,
        KeyCode::Home => 0x24,
        KeyCode::End => 0x23,
        KeyCode::PageUp => 0x21,
        KeyCode::PageDown => 0x22,
        KeyCode::Insert => 0x2D,
        _ => 0,
    }
}

fn modifiers_to_termsurf(mods: Modifiers) -> u64 {
    let mut result: u64 = 0;
    // WezTerm SHIFT (1<<1) → TermSurf (1<<0)
    if mods.contains(Modifiers::SHIFT) {
        result |= 1 << 0;
    }
    // WezTerm CTRL (1<<3) → TermSurf (1<<1)
    if mods.contains(Modifiers::CTRL) {
        result |= 1 << 1;
    }
    // WezTerm ALT (1<<2) → TermSurf (1<<2)
    if mods.contains(Modifiers::ALT) {
        result |= 1 << 2;
    }
    // WezTerm SUPER (1<<4) → TermSurf (1<<3)
    if mods.contains(Modifiers::SUPER) {
        result |= 1 << 3;
    }
    result
}

fn send_to_chromium(pane_id_str: &str, mut msg: Msg) {
    let Some(state) = super::shared_state() else {
        return;
    };
    let st = state.lock().unwrap();
    let Some(pane) = st.panes.get(pane_id_str) else {
        return;
    };
    if pane.tab_id == 0 {
        return;
    }
    let tab_id = pane.tab_id;

    // Fill in the tab_id on the message
    match &mut msg {
        Msg::KeyEvent(ref mut e) => e.tab_id = tab_id,
        Msg::MouseEvent(ref mut e) => e.tab_id = tab_id,
        Msg::MouseMove(ref mut e) => e.tab_id = tab_id,
        Msg::ScrollEvent(ref mut e) => e.tab_id = tab_id,
        Msg::FocusChanged(ref mut e) => e.tab_id = tab_id,
        _ => {}
    }

    let key = super::state::TermSurfState::server_key(&pane.profile, &pane.browser);
    let Some(server) = st.servers.get(&key) else {
        return;
    };
    let Some(ref server_tx) = server.tx else {
        return;
    };
    let wrapped = TermSurfMessage { msg: Some(msg) };
    let _ = server_tx.try_send(wrapped.encode_to_vec());
}

fn send_mode_and_focus(pane_id_str: &str, browsing: bool) {
    let Some(state) = super::shared_state() else {
        return;
    };

    // Send ModeChanged to TUI
    {
        let st = state.lock().unwrap();
        if let Some(pane) = st.panes.get(pane_id_str) {
            let msg = TermSurfMessage {
                msg: Some(Msg::ModeChanged(proto::ModeChanged {
                    browsing,
                    pane_id: pane_id_str.to_string(),
                })),
            };
            let _ = pane.tui_tx.try_send(msg.encode_to_vec());
        }
    }

    // Send FocusChanged to Chromium
    send_to_chromium(
        pane_id_str,
        Msg::FocusChanged(proto::FocusChanged {
            tab_id: 0, // filled by send_to_chromium
            focused: browsing,
        }),
    );
}

/// Handle pane focus change. Sends FocusChanged(false) to old pane's Chromium,
/// FocusChanged(true) to new pane's Chromium if it's in browse mode.
pub fn handle_pane_focus(pane_id: usize) {
    let pane_id_str = pane_id.to_string();
    let Some(state) = super::shared_state() else {
        return;
    };

    let (old_pane, new_is_browsing) = {
        let mut st = state.lock().unwrap();
        let old = st.focused_pane.take();
        let new_is_browsing = st
            .panes
            .get(&pane_id_str)
            .map(|p| p.browsing)
            .unwrap_or(false);
        st.focused_pane = Some(pane_id_str.clone());
        (old, new_is_browsing)
    };

    // Unfocus old pane's Chromium
    if let Some(ref old_id) = old_pane {
        if *old_id != pane_id_str {
            send_to_chromium(
                old_id,
                Msg::FocusChanged(proto::FocusChanged {
                    tab_id: 0,
                    focused: false,
                }),
            );
        }
    }

    // Focus new pane's Chromium if browsing
    if new_is_browsing {
        send_to_chromium(
            &pane_id_str,
            Msg::FocusChanged(proto::FocusChanged {
                tab_id: 0,
                focused: true,
            }),
        );
    }
}

/// Map a pane's Chromium cursor type to a WezTerm MouseCursor.
pub fn cursor_for_pane(pane_id: usize) -> MouseCursor {
    let pane_id_str = pane_id.to_string();
    let Some(state) = super::shared_state() else {
        return MouseCursor::Arrow;
    };
    let st = state.lock().unwrap();
    let Some(pane) = st.panes.get(&pane_id_str) else {
        return MouseCursor::Arrow;
    };
    match pane.cursor_type {
        2 => MouseCursor::Hand,
        3 => MouseCursor::Text,
        _ => MouseCursor::Arrow,
    }
}

fn hit_test_overlay_at(pane_id_str: &str, mx: f64, my: f64) -> Option<(f64, f64)> {
    let state = super::shared_state()?;
    let st = state.lock().unwrap();
    let pane = st.panes.get(pane_id_str)?;

    let ox = pane.overlay_origin_x;
    let oy = pane.overlay_origin_y;
    let ow = pane.pixel_width as f64;
    let oh = pane.pixel_height as f64;

    if mx >= ox && my >= oy && mx < ox + ow && my < oy + oh {
        let scale = pane.overlay_scale;
        Some(((mx - ox) / scale, (my - oy) / scale))
    } else {
        None
    }
}

fn hit_test_overlay(pane_id_str: &str, event: &MouseEvent) -> Option<(f64, f64)> {
    hit_test_overlay_at(pane_id_str, event.coords.x as f64, event.coords.y as f64)
}

/// Forward raw scroll event to Chromium. Returns true if consumed.
pub fn try_forward_raw_scroll(
    pane_id: usize,
    coords: ::window::Point,
    delta_x: f64,
    delta_y: f64,
    phase: u64,
    momentum_phase: u64,
    precise: bool,
    modifiers: Modifiers,
) -> bool {
    let pane_id_str = pane_id.to_string();
    let Some(state) = super::shared_state() else {
        return false;
    };
    let (has_pane, has_tab) = {
        let st = state.lock().unwrap();
        match st.panes.get(&pane_id_str) {
            Some(pane) => (true, pane.tab_id != 0),
            None => (false, false),
        }
    };
    if !has_pane || !has_tab {
        return false;
    }

    if let Some((rel_x, rel_y)) =
        hit_test_overlay_at(&pane_id_str, coords.x as f64, coords.y as f64)
    {
        let mods = modifiers_to_termsurf(modifiers);
        send_to_chromium(
            &pane_id_str,
            Msg::ScrollEvent(proto::ScrollEvent {
                tab_id: 0,
                x: rel_x,
                y: rel_y,
                delta_x,
                delta_y,
                phase,
                momentum_phase,
                precise,
                modifiers: mods,
            }),
        );
        return true;
    }
    false
}
