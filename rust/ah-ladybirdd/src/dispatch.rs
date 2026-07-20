use std::sync::Mutex;

use crate::engine;
use crate::proto::termsurf;
use crate::proto::{Msg, TermSurfMessage};

static SHUTDOWN_REQUESTED: Mutex<bool> = Mutex::new(false);

pub fn should_shutdown() -> bool {
    *SHUTDOWN_REQUESTED.lock().unwrap()
}

pub fn handle_message(msg: &TermSurfMessage) {
    let Some(ref msg) = msg.msg else {
        eprintln!("[Ladybird] ignored empty protobuf message");
        return;
    };

    match msg {
        Msg::CreateTab(m) => create_tab(m),
        Msg::CreateDevtoolsTab(m) => create_devtools_tab(m),
        Msg::Resize(m) => resize(m),
        Msg::CloseTab(m) => close_tab(m.tab_id),
        Msg::Navigate(m) => navigate(m),
        Msg::NavigationAction(m) => navigation_action(m),
        Msg::MouseEvent(m) => mouse_event(m),
        Msg::MouseMove(m) => mouse_move(m),
        Msg::ScrollEvent(m) => scroll_event(m),
        Msg::KeyEvent(m) => key_event(m),
        Msg::FocusChanged(m) => focus_changed(m),
        Msg::SetColorScheme(m) => set_color_scheme(m),
        Msg::SetGuiActive(m) => set_gui_active(m),
        Msg::JavascriptDialogReply(m) => javascript_dialog_reply(m),
        Msg::HttpAuthReply(m) => http_auth_reply(m),
        Msg::QueryTabsRequest(_) => send_query_tabs_reply(),
        Msg::SetOverlay(_) => ignored_not_engine("SetOverlay", "GUI-only overlay geometry"),
        Msg::SetDevtoolsOverlay(_) => {
            ignored_not_engine("SetDevtoolsOverlay", "GUI-only devtools overlay geometry")
        }
        Msg::OpenSplit(_) => ignored_not_engine("OpenSplit", "GUI-only split request"),
        Msg::ModeChanged(_) => ignored_not_engine("ModeChanged", "GUI-to-TUI mode notification"),
        Msg::HelloRequest(_) => ignored_not_engine("HelloRequest", "TUI/GUI handshake"),
        Msg::HelloReply(_) => ignored_not_engine("HelloReply", "TUI/GUI handshake response"),
        Msg::QueryLastRequest(_) => ignored_not_engine("QueryLastRequest", "GUI-owned query"),
        Msg::QueryLastReply(_) => ignored_not_engine("QueryLastReply", "GUI-owned query reply"),
        Msg::QueryDevtoolsRequest(_) => {
            ignored_not_engine("QueryDevtoolsRequest", "GUI-owned devtools query")
        }
        Msg::QueryDevtoolsReply(_) => {
            ignored_not_engine("QueryDevtoolsReply", "GUI-owned devtools query reply")
        }
        Msg::BrowserReady(_) => ignored_not_engine("BrowserReady", "GUI-to-TUI notification"),
        Msg::ServerRegister(_)
        | Msg::TabReady(_)
        | Msg::CaContext(_)
        | Msg::RenderSurface(_)
        | Msg::UrlChanged(_)
        | Msg::LoadingState(_)
        | Msg::NavigationState(_)
        | Msg::TitleChanged(_)
        | Msg::CursorChanged(_)
        | Msg::TargetUrlChanged(_)
        | Msg::QueryTabsReply(_)
        | Msg::JavascriptDialogRequest(_)
        | Msg::ConsoleMessage(_)
        | Msg::HttpAuthRequest(_)
        | Msg::RendererCrashed(_) => {
            eprintln!("[Ladybird] ignored inbound send-side message");
        }
    }
}

fn create_tab(m: &termsurf::CreateTab) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] CreateTab failed pane_id={} url={}: engine service is not initialized",
            m.pane_id, m.url
        );
        return;
    };
    let width = i32::try_from(m.pixel_width).unwrap_or(i32::MAX);
    let height = i32::try_from(m.pixel_height).unwrap_or(i32::MAX);
    match engine.create_tab(m.url.clone(), m.pane_id.clone(), width, height, m.dark) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed CreateTab tab_id={} pane_id={} url={} size={}x{} dark={}",
            tab.id, m.pane_id, tab.url, m.pixel_width, m.pixel_height, tab.dark
        ),
        Err(error) => eprintln!(
            "[Ladybird] CreateTab failed pane_id={} url={}: {error}",
            m.pane_id, m.url
        ),
    }
}

fn create_devtools_tab(m: &termsurf::CreateDevtoolsTab) {
    eprintln!(
        "[Ladybird] unsupported CreateDevtoolsTab pane_id={} inspected_tab_id={} mode=unsupported: Ladybird devtools are not wired yet",
        m.pane_id, m.inspected_tab_id
    );
}

fn javascript_dialog_reply(m: &termsurf::JavaScriptDialogReply) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] JavaScriptDialogReply failed tab_id={} request_id={}: engine service is not initialized",
            m.tab_id, m.request_id
        );
        return;
    };
    match engine.javascript_dialog_reply(
        m.tab_id,
        m.request_id,
        m.accepted,
        m.prompt_text.clone(),
    ) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed JavaScriptDialogReply tab_id={} request_id={} accepted={} ok=true reply_count={}",
            tab.id, m.request_id, m.accepted, tab.javascript_dialog_reply_count
        ),
        Err(error) => eprintln!(
            "[Ladybird] JavaScriptDialogReply failed tab_id={} request_id={} accepted={}: {error}",
            m.tab_id, m.request_id, m.accepted
        ),
    }
}

fn http_auth_reply(m: &termsurf::HttpAuthReply) {
    eprintln!(
        "[Ladybird] unsupported HttpAuthReply tab_id={} request_id={} accepted={} username_len={} mode=unsupported: no Ladybird HTTP auth embedder hook is wired",
        m.tab_id,
        m.request_id,
        m.accepted,
        m.username.len()
    );
}

fn resize(m: &termsurf::Resize) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] Resize failed tab_id={} size={}x{}: engine service is not initialized",
            m.tab_id, m.pixel_width, m.pixel_height
        );
        return;
    };
    let width = i32::try_from(m.pixel_width).unwrap_or(i32::MAX);
    let height = i32::try_from(m.pixel_height).unwrap_or(i32::MAX);
    match engine.resize_tab(m.tab_id, width, height) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed Resize tab_id={} size={}x{}",
            tab.id, tab.width, tab.height
        ),
        Err(error) => eprintln!(
            "[Ladybird] Resize failed tab_id={} size={}x{}: {error}",
            m.tab_id, m.pixel_width, m.pixel_height
        ),
    }
}

fn mouse_event(m: &termsurf::MouseEvent) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] MouseEvent failed tab_id={} type={} button={}: engine service is not initialized",
            m.tab_id, m.r#type, m.button
        );
        return;
    };
    let click_count = i32::try_from(m.click_count).unwrap_or(i32::MAX);
    match engine.mouse_event(
        m.tab_id,
        m.r#type.clone(),
        m.button.clone(),
        m.x,
        m.y,
        click_count,
        m.modifiers,
    ) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed MouseEvent tab_id={} type={} button={} coords=({:.2}, {:.2}) click_count={} modifiers={} mouse_button_events={} mode=input-queued",
            tab.id, m.r#type, m.button, m.x, m.y, click_count, m.modifiers, tab.mouse_button_events
        ),
        Err(error) => eprintln!(
            "[Ladybird] MouseEvent failed tab_id={} type={} button={} coords=({:.2}, {:.2}) click_count={} modifiers={}: {error}",
            m.tab_id, m.r#type, m.button, m.x, m.y, click_count, m.modifiers
        ),
    }
}

fn mouse_move(m: &termsurf::MouseMove) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] MouseMove failed tab_id={} coords=({:.2}, {:.2}): engine service is not initialized",
            m.tab_id, m.x, m.y
        );
        return;
    };
    match engine.mouse_move(m.tab_id, m.x, m.y, m.modifiers) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed MouseMove tab_id={} coords=({:.2}, {:.2}) modifiers={} mouse_move_events={} mode=input-queued",
            tab.id, m.x, m.y, m.modifiers, tab.mouse_move_events
        ),
        Err(error) => eprintln!(
            "[Ladybird] MouseMove failed tab_id={} coords=({:.2}, {:.2}) modifiers={}: {error}",
            m.tab_id, m.x, m.y, m.modifiers
        ),
    }
}

fn scroll_event(m: &termsurf::ScrollEvent) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] ScrollEvent failed tab_id={} coords=({:.2}, {:.2}) delta=({:.2}, {:.2}): engine service is not initialized",
            m.tab_id, m.x, m.y, m.delta_x, m.delta_y
        );
        return;
    };
    match engine.scroll_event(
        m.tab_id,
        m.x,
        m.y,
        m.delta_x,
        m.delta_y,
        m.phase,
        m.momentum_phase,
        m.precise,
        m.modifiers,
    ) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed ScrollEvent tab_id={} coords=({:.2}, {:.2}) delta=({:.2}, {:.2}) phase={} momentum_phase={} precise={} modifiers={} scroll_events={} mode=input-queued",
            tab.id,
            m.x,
            m.y,
            m.delta_x,
            m.delta_y,
            m.phase,
            m.momentum_phase,
            m.precise,
            m.modifiers,
            tab.scroll_events
        ),
        Err(error) => eprintln!(
            "[Ladybird] ScrollEvent failed tab_id={} coords=({:.2}, {:.2}) delta=({:.2}, {:.2}) phase={} momentum_phase={} precise={} modifiers={}: {error}",
            m.tab_id,
            m.x,
            m.y,
            m.delta_x,
            m.delta_y,
            m.phase,
            m.momentum_phase,
            m.precise,
            m.modifiers
        ),
    }
}

fn key_event(m: &termsurf::KeyEvent) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] KeyEvent failed tab_id={} type={} windows_key_code={}: engine service is not initialized",
            m.tab_id, m.r#type, m.windows_key_code
        );
        return;
    };
    let windows_key_code = i32::try_from(m.windows_key_code).unwrap_or(i32::MAX);
    match engine.key_event(
        m.tab_id,
        m.r#type.clone(),
        windows_key_code,
        m.utf8.clone(),
        m.modifiers,
    ) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed KeyEvent tab_id={} type={} windows_key_code={} utf8_len={} modifiers={} key_events={} mode=input-queued",
            tab.id,
            m.r#type,
            windows_key_code,
            m.utf8.len(),
            m.modifiers,
            tab.key_events
        ),
        Err(error) => eprintln!(
            "[Ladybird] KeyEvent failed tab_id={} type={} windows_key_code={} utf8_len={} modifiers={}: {error}",
            m.tab_id,
            m.r#type,
            windows_key_code,
            m.utf8.len(),
            m.modifiers
        ),
    }
}

fn focus_changed(m: &termsurf::FocusChanged) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] FocusChanged failed tab_id={} focused={}: engine service is not initialized",
            m.tab_id, m.focused
        );
        return;
    };
    match engine.set_focus(m.tab_id, m.focused) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed FocusChanged tab_id={} focused={} mode=tracked",
            tab.id, tab.focused
        ),
        Err(error) => eprintln!(
            "[Ladybird] FocusChanged failed tab_id={} focused={}: {error}",
            m.tab_id, m.focused
        ),
    }
}

fn set_color_scheme(m: &termsurf::SetColorScheme) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] SetColorScheme failed tab_id={} dark={}: engine service is not initialized",
            m.tab_id, m.dark
        );
        return;
    };
    match engine.set_color_scheme(m.tab_id, m.dark) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed SetColorScheme tab_id={} dark={} mode=color-scheme-abi",
            tab.id, tab.dark
        ),
        Err(error) => eprintln!(
            "[Ladybird] SetColorScheme failed tab_id={} dark={}: {error}",
            m.tab_id, m.dark
        ),
    }
}

fn set_gui_active(m: &termsurf::SetGuiActive) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] SetGuiActive failed tab_id={} active={} reason={}: engine service is not initialized",
            m.tab_id, m.active, m.reason
        );
        return;
    };
    match engine.set_gui_active(m.tab_id, m.active) {
        Ok(outcome) => eprintln!(
            "[Ladybird] engine-backed SetGuiActive tab_id={} active={} reason={} target_count={} mode=visibility-state-abi",
            m.tab_id, m.active, m.reason, outcome.affected_count
        ),
        Err(error) => eprintln!(
            "[Ladybird] SetGuiActive failed tab_id={} active={} reason={}: {error}",
            m.tab_id, m.active, m.reason
        ),
    }
}

fn close_tab(tab_id: i64) {
    let Some(engine) = engine::global() else {
        eprintln!("[Ladybird] CloseTab ignored tab_id={tab_id}: engine service is not initialized");
        return;
    };
    match engine.close_tab(tab_id) {
        Ok(outcome) => {
            eprintln!(
                "[Ladybird] engine-backed CloseTab tab_id={tab_id} removed={} remaining_browser_tabs={}",
                outcome.removed, outcome.remaining_browser_tabs
            );
            if outcome.remaining_browser_tabs == 0 {
                *SHUTDOWN_REQUESTED.lock().unwrap() = true;
            }
        }
        Err(error) => eprintln!("[Ladybird] CloseTab failed tab_id={tab_id}: {error}"),
    }
}

fn navigate(m: &termsurf::Navigate) {
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] Navigate failed tab_id={} url={}: engine service is not initialized",
            m.tab_id, m.url
        );
        return;
    };
    match engine.navigate(m.tab_id, m.url.clone()) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed Navigate tab_id={} url={}",
            tab.id, tab.url
        ),
        Err(error) => eprintln!(
            "[Ladybird] Navigate failed tab_id={} url={}: {error}",
            m.tab_id, m.url
        ),
    }
}

fn navigation_action(m: &termsurf::NavigationAction) {
    if let Err(error) = validate_navigation_action_message(m) {
        eprintln!(
            "[Ladybird] NavigationAction rejected tab_id={} pane_id={} action={} reason={error}",
            m.tab_id, m.pane_id, m.action
        );
        return;
    }
    let Some(engine) = engine::global() else {
        eprintln!(
            "[Ladybird] NavigationAction failed tab_id={} action={}: engine service is not initialized",
            m.tab_id, m.action
        );
        return;
    };
    match engine.navigation_action(m.tab_id, m.action.clone(), m.request_id) {
        Ok(tab) => eprintln!(
            "[Ladybird] engine-backed NavigationAction tab_id={} action={} can_go_back={} can_go_forward={} can_refresh={}",
            tab.id, m.action, tab.can_go_back, tab.can_go_forward, tab.can_refresh
        ),
        Err(error) => eprintln!(
            "[Ladybird] NavigationAction failed tab_id={} action={}: {error}",
            m.tab_id, m.action
        ),
    }
}

fn validate_navigation_action_message(m: &termsurf::NavigationAction) -> Result<(), &'static str> {
    if m.tab_id <= 0 {
        return Err("tab_id must be positive");
    }
    if !m.pane_id.is_empty() {
        return Err("pane_id must be empty at engine boundary");
    }
    match m.action.as_str() {
        "back" | "forward" if m.request_id == 0 => {}
        "refresh" if m.request_id != 0 => {}
        _ => return Err("invalid action or request_id"),
    }
    Ok(())
}

fn send_query_tabs_reply() {
    let mut browser_count = 0;
    let tabs = engine::global()
        .map(|engine| engine.snapshot())
        .unwrap_or_default()
        .iter()
        .map(|tab| {
            if tab.inspected_tab_id == 0 {
                browser_count += 1;
            }
            engine::tab_info_from_snapshot(tab)
        })
        .collect::<Vec<_>>();

    let reply = TermSurfMessage {
        msg: Some(Msg::QueryTabsReply(termsurf::QueryTabsReply {
            gui_panes: 0,
            chromium_tabs: tabs.len() as i64,
            chromium_browser: browser_count,
            chromium_devtools: 0,
            tabs,
            error: String::new(),
        })),
    };
    let sent = crate::ipc::send(&reply);
    eprintln!("[Ladybird] QueryTabsReply sent_to={sent}");
}

fn ignored_not_engine(message: &str, reason: &str) {
    eprintln!("[Ladybird] ignored not-engine-relevant {message}: {reason}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn validates_tab_addressed_navigation_protobuf() {
        let valid = termsurf::NavigationAction {
            tab_id: 7,
            pane_id: String::new(),
            action: "back".to_string(),
            request_id: 0,
        };
        let forward = termsurf::NavigationAction {
            action: "forward".to_string(),
            ..valid.clone()
        };
        let envelope = TermSurfMessage {
            msg: Some(Msg::NavigationAction(valid.clone())),
        };
        let decoded = TermSurfMessage::decode(envelope.encode_to_vec().as_slice()).unwrap();
        let Some(Msg::NavigationAction(decoded_action)) = decoded.msg else {
            panic!("expected decoded NavigationAction");
        };
        assert_eq!(validate_navigation_action_message(&decoded_action), Ok(()));
        assert_eq!(validate_navigation_action_message(&forward), Ok(()));
        let refresh = termsurf::NavigationAction {
            action: "refresh".to_string(),
            request_id: 9,
            ..valid.clone()
        };
        assert_eq!(validate_navigation_action_message(&refresh), Ok(()));

        let loading = TermSurfMessage {
            msg: Some(Msg::LoadingState(termsurf::LoadingState {
                tab_id: 7,
                state: "error".to_string(),
                progress: 0,
                navigation_request_id: 9,
            })),
        };
        let decoded = TermSurfMessage::decode(loading.encode_to_vec().as_slice()).unwrap();
        let Some(Msg::LoadingState(decoded_loading)) = decoded.msg else {
            panic!("expected decoded LoadingState");
        };
        assert_eq!(decoded_loading.navigation_request_id, 9);

        for invalid in [
            termsurf::NavigationAction {
                tab_id: 0,
                ..valid.clone()
            },
            termsurf::NavigationAction {
                tab_id: -1,
                ..valid.clone()
            },
            termsurf::NavigationAction {
                pane_id: "pane-1".to_string(),
                ..valid.clone()
            },
            termsurf::NavigationAction {
                action: "refresh".to_string(),
                ..valid.clone()
            },
            termsurf::NavigationAction {
                action: "future".to_string(),
                ..valid.clone()
            },
        ] {
            assert!(validate_navigation_action_message(&invalid).is_err());
        }
    }
}
