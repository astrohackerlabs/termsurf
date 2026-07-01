use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::terminal;
use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

use proto::{term_surf_message::Msg, CloseAppFrontend, OpenApp, SetOverlay, TermSurfMessage};

const APP_ID: &str = "gtui";
const BROWSER_READY_TIMEOUT: Duration = Duration::from_secs(120);

enum AppEvent {
    BrowserReady,
    CtrlC,
    GuiClosed,
}

fn main() -> io::Result<()> {
    let pane_id = std::env::var("TERMSURF_PANE_ID").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "TERMSURF_PANE_ID not set; run termsurf inside TermSurf",
        )
    })?;
    let socket = std::env::var("TERMSURF_SOCKET").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "TERMSURF_SOCKET not set; run termsurf inside TermSurf",
        )
    })?;

    let mut stream = UnixStream::connect(socket)?;
    let reader = stream.try_clone()?;
    send(
        &mut stream,
        Msg::OpenApp(OpenApp {
            pane_id: pane_id.clone(),
            app_id: APP_ID.to_string(),
            browser: browser_name()?,
            profile: "default".to_string(),
            entrypoint: app_entrypoint()?,
        }),
    )?;

    let reply = read_message(&mut stream)?;
    let reply = match reply.msg {
        Some(Msg::OpenAppReply(reply)) => reply,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected OpenAppReply from Ghostboard",
            ))
        }
    };
    if !reply.error.is_empty() {
        return Err(io::Error::new(io::ErrorKind::Other, reply.error));
    }
    if reply.url.is_empty() || reply.frontend_id.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "OpenAppReply missing url or frontend_id",
        ));
    }

    let (exit_tx, exit_rx) = mpsc::channel();
    let ctrlc_tx = exit_tx.clone();
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.send(AppEvent::CtrlC);
    })
    .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    let gui_tx = exit_tx.clone();
    let expected_pane_id = pane_id.clone();
    let expected_frontend_id = reply.frontend_id.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        while let Ok(message) = read_message(&mut reader) {
            match message.msg {
                Some(Msg::BrowserReady(ready)) if ready.pane_id == expected_pane_id => {
                    let _ = gui_tx.send(AppEvent::BrowserReady);
                }
                Some(Msg::CloseAppFrontend(close))
                    if close.app_id == APP_ID && close.frontend_id == expected_frontend_id =>
                {
                    let _ = gui_tx.send(AppEvent::GuiClosed);
                    break;
                }
                _ => {}
            }
        }
    });

    let (cols, rows) = terminal::size()?;
    send(
        &mut stream,
        Msg::SetOverlay(SetOverlay {
            pane_id: pane_id.clone(),
            col: 0,
            row: 0,
            width: cols as u64,
            height: rows as u64,
            url: reply.url.clone(),
            profile: "default".to_string(),
            browsing: true,
            browser: browser_name()?,
        }),
    )?;

    wait_for_browser_ready(&exit_rx, &mut stream, &pane_id, &reply.frontend_id)?;

    println!("TermSurf is open. Press Ctrl+C to close.");
    let reason = loop {
        match exit_rx.recv().unwrap_or(AppEvent::CtrlC) {
            AppEvent::BrowserReady => continue,
            event => break event,
        }
    };

    if matches!(reason, AppEvent::CtrlC) {
        send_close_app_frontend(&mut stream, pane_id, reply.frontend_id)?;
    }
    Ok(())
}

fn wait_for_browser_ready(
    exit_rx: &mpsc::Receiver<AppEvent>,
    stream: &mut UnixStream,
    pane_id: &str,
    frontend_id: &str,
) -> io::Result<()> {
    loop {
        match exit_rx.recv_timeout(BROWSER_READY_TIMEOUT) {
            Ok(AppEvent::BrowserReady) => return Ok(()),
            Ok(AppEvent::GuiClosed) => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "TermSurf app closed before browser became ready",
                ))
            }
            Ok(AppEvent::CtrlC) => {
                send_close_app_frontend(stream, pane_id.to_string(), frontend_id.to_string())?;
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "TermSurf app canceled before browser became ready",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                send_close_app_frontend(stream, pane_id.to_string(), frontend_id.to_string())?;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "TermSurf app browser did not become ready",
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "TermSurf app connection closed before browser became ready",
                ))
            }
        }
    }
}

fn send_close_app_frontend(
    stream: &mut UnixStream,
    pane_id: String,
    frontend_id: String,
) -> io::Result<()> {
    send(
        stream,
        Msg::CloseAppFrontend(CloseAppFrontend {
            pane_id,
            app_id: APP_ID.to_string(),
            frontend_id,
        }),
    )
}

fn app_entrypoint() -> io::Result<String> {
    if let Ok(path) = std::env::var("TERMSURF_GTUI_APP_PATH") {
        return Ok(path);
    }

    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let sibling = exe_dir.join("gtui/app/server.ts");
        if sibling.exists() {
            return path_to_string(sibling);
        }
    }

    for path in [
        "/opt/homebrew/opt/termsurf-gtui/app/server.ts",
        "/usr/local/share/termsurf/gtui/app/server.ts",
    ] {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return path_to_string(candidate);
        }
    }

    let repo_root = repo_root_from_exe()?;
    let entrypoint = repo_root.join("gtui/app/server.ts");
    if !entrypoint.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "TermSurf app entrypoint not found: {}",
                entrypoint.display()
            ),
        ));
    }
    path_to_string(entrypoint)
}

fn browser_name() -> io::Result<String> {
    if let Ok(path) = std::env::var("TERMSURF_ROAMIUM_PATH") {
        return Ok(path);
    }

    let repo_root = repo_root_from_exe()?;
    let roamium = repo_root.join("chromium/src/out/Default/roamium");
    if roamium.exists() {
        return roamium.to_str().map(str::to_string).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Roamium path is not UTF-8")
        });
    }

    Ok("roamium".to_string())
}

fn repo_root_from_exe() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.ancestors()
        .nth(3)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "could not resolve repo root"))
}

fn path_to_string(path: PathBuf) -> io::Result<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "TermSurf app path is not UTF-8"))
}

fn send(stream: &mut UnixStream, msg: Msg) -> io::Result<()> {
    let wrapper = TermSurfMessage { msg: Some(msg) };
    let payload = wrapper.encode_to_vec();
    let len = (payload.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(&payload)?;
    Ok(())
}

fn read_message(stream: &mut UnixStream) -> io::Result<TermSurfMessage> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    TermSurfMessage::decode(payload.as_slice())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}
