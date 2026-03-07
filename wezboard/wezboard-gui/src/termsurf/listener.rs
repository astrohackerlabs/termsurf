use super::state::SharedState;
use anyhow::Context;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

pub fn spawn_termsurf_server(sock_path: PathBuf, state: SharedState) -> anyhow::Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create dir {}", parent.display()))?;
    }

    // Remove stale socket
    std::fs::remove_file(&sock_path).ok();

    let listener =
        UnixListener::bind(&sock_path).with_context(|| format!("bind {}", sock_path.display()))?;

    log::info!("TermSurf socket listening on {}", sock_path.display());

    // SAFETY: called once during single-threaded startup before spawning any threads.
    unsafe { std::env::set_var("TERMSURF_SOCKET", &sock_path) };

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let peer = format!("{:?}", stream.peer_addr());
                    log::info!("TermSurf client connected: {}", peer);
                    let conn_state = state.clone();
                    promise::spawn::spawn_into_main_thread(async move {
                        if let Err(err) = super::conn::handle_connection(stream, conn_state).await {
                            log::error!("TermSurf connection error: {:#}", err);
                        }
                    })
                    .detach();
                }
                Err(err) => {
                    log::error!("TermSurf accept error: {:#}", err);
                }
            }
        }
        std::fs::remove_file(&sock_path).ok();
    });

    Ok(())
}
