use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
#[cfg(test)]
use std::time::Duration;

use nu_cli::{ModeDispatcher, ModeResult};

use crate::shell::ShellState;
use crate::shell_engine::ShellEngine;
use crate::zsh_process::ZshProcess;

type BootstrapResult = Result<(ZshProcess, HashMap<String, String>), String>;

/// Interactive mode dispatcher with **lazy** background zsh bootstrap.
///
/// Construction starts zsh login + `.zshrc` on a background thread and returns
/// immediately so the first Nu prompt is not blocked. Env is available for a
/// one-shot Nu stack merge via [`ModeDispatcher::take_pending_env_merge`]. First
/// zsh-mode [`ModeDispatcher::execute`] waits for readiness if needed.
pub struct ShannonDispatcher {
    rx: Option<Receiver<BootstrapResult>>,
    zsh: Option<ZshProcess>,
    /// Captured bootstrap env not yet yielded to Nu (one-shot).
    pending_merge: Option<HashMap<String, String>>,
    error: Option<String>,
}

impl ShannonDispatcher {
    /// Start background zsh bootstrap; do not join.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = ZshProcess::try_new().map(|zp| {
                let env = zp.bootstrap_env().clone();
                (zp, env)
            });
            let _ = tx.send(result);
        });
        ShannonDispatcher {
            rx: Some(rx),
            zsh: None,
            pending_merge: None,
            error: None,
        }
    }

    /// Non-blocking: if bootstrap finished, take ownership of the worker.
    fn try_finish_bootstrap(&mut self) {
        if self.zsh.is_some() || self.error.is_some() {
            return;
        }
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok((zp, env))) => {
                self.pending_merge = Some(env);
                self.zsh = Some(zp);
                self.rx = None;
            }
            Ok(Err(e)) => {
                self.error = Some(e);
                self.rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.error = Some("zsh bootstrap channel disconnected".to_string());
                self.rx = None;
            }
        }
    }

    /// Blocking wait until bootstrap completes (or fails).
    fn ensure_ready(&mut self) {
        self.try_finish_bootstrap();
        if self.zsh.is_some() || self.error.is_some() {
            return;
        }
        if let Some(rx) = self.rx.take() {
            match rx.recv() {
                Ok(Ok((zp, env))) => {
                    self.pending_merge = Some(env);
                    self.zsh = Some(zp);
                }
                Ok(Err(e)) => {
                    self.error = Some(e);
                }
                Err(_) => {
                    self.error = Some("zsh bootstrap channel disconnected".to_string());
                }
            }
        }
    }

    /// For tests: spin until ready or timeout.
    #[cfg(test)]
    pub fn wait_ready_for_test(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            self.try_finish_bootstrap();
            if self.zsh.is_some() {
                return Ok(());
            }
            if let Some(e) = &self.error {
                return Err(e.clone());
            }
            if std::time::Instant::now() >= deadline {
                // Fall back to blocking ensure (bootstrap should be near done).
                self.ensure_ready();
                if self.zsh.is_some() {
                    return Ok(());
                }
                return Err(self
                    .error
                    .clone()
                    .unwrap_or_else(|| "timeout waiting for zsh bootstrap".into()));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Default for ShannonDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ModeDispatcher for ShannonDispatcher {
    fn execute(
        &mut self,
        mode: &str,
        command: &str,
        env: HashMap<String, String>,
        cwd: PathBuf,
    ) -> ModeResult {
        let state = ShellState {
            env: env.clone(),
            cwd: cwd.clone(),
            last_exit_code: 0,
        };
        match mode {
            "zsh" => {
                self.ensure_ready();
                if let Some(err) = &self.error {
                    eprintln!("ahsh: zsh worker unavailable: {err}");
                    return ModeResult {
                        env,
                        cwd,
                        exit_code: 1,
                    };
                }
                let Some(zsh) = self.zsh.as_mut() else {
                    eprintln!("ahsh: zsh worker missing after ensure_ready");
                    return ModeResult {
                        env,
                        cwd,
                        exit_code: 1,
                    };
                };
                // If Nu never polled pending merge, drop it so execute path owns env.
                // execute still injects the live Nu env for this command.
                let _ = self.pending_merge.take();
                zsh.inject_state(&state);
                let result = zsh.execute(command);
                ModeResult {
                    env: result.env,
                    cwd: result.cwd,
                    exit_code: result.last_exit_code,
                }
            }
            _ => ModeResult {
                env: state.env,
                cwd: state.cwd,
                exit_code: 127,
            },
        }
    }

    fn take_pending_env_merge(&mut self) -> Option<HashMap<String, String>> {
        self.try_finish_bootstrap();
        self.pending_merge.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nu_cli::ModeDispatcher;

    #[test]
    fn lazy_new_returns_before_blocking_on_zsh() {
        // Construction must not wait for zsh - just spawn the thread.
        let t0 = std::time::Instant::now();
        let mut d = ShannonDispatcher::new();
        let construct_ms = t0.elapsed().as_millis();
        assert!(
            construct_ms < 200,
            "ShannonDispatcher::new must not wait on zsh bootstrap, took {construct_ms}ms"
        );
        // Still becomes ready.
        d.wait_ready_for_test(Duration::from_secs(30)).unwrap();
        assert!(d.zsh.is_some());
    }

    #[test]
    fn pending_env_merge_is_oneshot() {
        let mut d = ShannonDispatcher::new();
        d.wait_ready_for_test(Duration::from_secs(30)).unwrap();
        let first = d.take_pending_env_merge();
        assert!(first.is_some(), "first take after ready must yield env");
        assert!(
            first.as_ref().unwrap().contains_key("PATH"),
            "bootstrap env should include PATH"
        );
        let second = d.take_pending_env_merge();
        assert!(second.is_none(), "second take must be None (one-shot)");
    }

    #[test]
    fn first_zsh_execute_waits_for_ready() {
        let mut d = ShannonDispatcher::new();
        // Do not call wait_ready first — execute must join.
        let result = d.execute(
            "zsh",
            "echo AHSH_LAZY_EXECUTE_OK",
            HashMap::new(),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        );
        assert_eq!(result.exit_code, 0);
        assert!(d.zsh.is_some());
    }
}
