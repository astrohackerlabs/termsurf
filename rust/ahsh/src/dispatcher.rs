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
/// immediately so the first Nu prompt is not blocked. Bootstrap env is a
/// one-shot merge into Nu:
/// - non-blocking poll at REPL loop head
/// - **blocking** poll immediately before Nu command execution if not yet applied
///
/// First zsh-mode [`ModeDispatcher::execute`] waits for readiness if needed but
/// never discards an unapplied Nu env merge.
pub struct ShannonDispatcher {
    rx: Option<Receiver<BootstrapResult>>,
    zsh: Option<ZshProcess>,
    /// Captured bootstrap env not yet yielded to Nu (one-shot).
    pending_merge: Option<HashMap<String, String>>,
    /// True after Nu has been offered bootstrap env once (success or hard fail).
    nu_env_applied: bool,
    error: Option<String>,
    error_reported: bool,
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
            nu_env_applied: false,
            error: None,
            error_reported: false,
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

    /// Yield bootstrap env to Nu at most once.
    fn yield_pending_for_nu(&mut self) -> Option<HashMap<String, String>> {
        if self.nu_env_applied {
            return None;
        }
        if let Some(err) = &self.error {
            if !self.error_reported {
                eprintln!("ahsh: zsh env unavailable: {err}");
                self.error_reported = true;
            }
            // Do not block forever on later takes; continue with parent env.
            self.nu_env_applied = true;
            return None;
        }
        if let Some(env) = self.pending_merge.take() {
            self.nu_env_applied = true;
            return Some(env);
        }
        None
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

    #[cfg(test)]
    pub fn nu_env_applied_for_test(&self) -> bool {
        self.nu_env_applied
    }

    #[cfg(test)]
    pub fn has_pending_merge_for_test(&self) -> bool {
        self.pending_merge.is_some()
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
                // Do not discard unapplied Nu env merge — leave pending_merge for
                // Nu take / pre-command barrier (Issue 26072213251282).
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
        self.yield_pending_for_nu()
    }

    fn take_pending_env_merge_blocking(&mut self) -> Option<HashMap<String, String>> {
        if self.nu_env_applied {
            return None;
        }
        self.ensure_ready();
        self.yield_pending_for_nu()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nu_cli::ModeDispatcher;

    #[test]
    fn lazy_new_returns_before_blocking_on_zsh() {
        let t0 = std::time::Instant::now();
        let mut d = ShannonDispatcher::new();
        let construct_ms = t0.elapsed().as_millis();
        assert!(
            construct_ms < 200,
            "ShannonDispatcher::new must not wait on zsh bootstrap, took {construct_ms}ms"
        );
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
        assert!(d.nu_env_applied_for_test());
        let second = d.take_pending_env_merge();
        assert!(second.is_none(), "second take must be None (one-shot)");
        assert!(d.take_pending_env_merge_blocking().is_none());
    }

    #[test]
    fn blocking_take_waits_and_yields_path() {
        let mut d = ShannonDispatcher::new();
        // Do not wait_ready first — blocking take must join.
        let env = d.take_pending_env_merge_blocking();
        assert!(env.is_some(), "blocking take must yield after bootstrap");
        assert!(env.unwrap().contains_key("PATH"));
        assert!(d.nu_env_applied_for_test());
        assert!(d.take_pending_env_merge_blocking().is_none());
    }

    #[test]
    fn first_zsh_execute_waits_for_ready() {
        let mut d = ShannonDispatcher::new();
        let result = d.execute(
            "zsh",
            "echo AHSH_LAZY_EXECUTE_OK",
            HashMap::new(),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        );
        assert_eq!(result.exit_code, 0);
        assert!(d.zsh.is_some());
    }

    #[test]
    fn zsh_execute_before_nu_take_does_not_discard_pending() {
        let mut d = ShannonDispatcher::new();
        d.wait_ready_for_test(Duration::from_secs(30)).unwrap();
        assert!(
            d.has_pending_merge_for_test(),
            "ready bootstrap should leave pending merge for Nu"
        );
        let result = d.execute(
            "zsh",
            "true",
            HashMap::new(),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        );
        assert_eq!(result.exit_code, 0);
        assert!(
            !d.nu_env_applied_for_test(),
            "zsh execute must not mark Nu env applied"
        );
        let pending = d.take_pending_env_merge();
        assert!(
            pending.is_some(),
            "pending Nu merge must survive zsh execute before Nu take"
        );
        assert!(pending.unwrap().contains_key("PATH"));
    }
}
