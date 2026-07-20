use std::collections::HashMap;
use std::path::PathBuf;

use nu_cli::{ModeDispatcher, ModeResult};

use crate::shell::ShellState;
use crate::shell_engine::ShellEngine;
use crate::zsh_process::ZshProcess;

pub struct ShannonDispatcher {
    zsh: ZshProcess,
}

impl ShannonDispatcher {
    pub fn new() -> Self {
        let zsh = ZshProcess::new();
        ShannonDispatcher { zsh }
    }

    /// Get the current env vars from zsh (after login + .zshrc initialization).
    /// Used to inject zsh env vars into nushell's Stack at startup.
    pub fn env_vars(&mut self) -> HashMap<String, String> {
        self.zsh.capture_env()
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
            env,
            cwd,
            last_exit_code: 0,
        };
        match mode {
            "zsh" => {
                self.zsh.inject_state(&state);
                let result = self.zsh.execute(command);
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
}
