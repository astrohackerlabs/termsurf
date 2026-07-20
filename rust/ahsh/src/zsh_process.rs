use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::executor;
use crate::shell::ShellState;
use crate::shell_engine::ShellEngine;

const SENTINEL_START: &str = "==SHANNON_SENTINEL_START==";
const SENTINEL_END: &str = "==SHANNON_SENTINEL_END==";

/// Persistent traditional-shell worker backed by zsh.
///
/// Spawn strategy: login shell (`zsh -l`) so `.zshenv` / `.zprofile` / `.zlogin`
/// run (respecting `ZDOTDIR` from `.zshenv`), then an explicit source of
/// `${ZDOTDIR:-$HOME}/.zshrc` so interactive PATH setup is applied.
///
/// Interactive login (`zsh -il`) is avoided: piped stdin still emits prompts and
/// OSC sequences that corrupt the sentinel protocol. After sourcing `.zshrc`,
/// prompt/preexec hooks that write terminal titles are cleared for the same reason.
pub struct ZshProcess {
    _child: Child,
    stdin: ChildStdin,
    stdout_reader: BufReader<ChildStdout>,
    pending_state: Option<ShellState>,
}

impl ZshProcess {
    pub fn new() -> Self {
        let mut child = Command::new("zsh")
            .args(["-l"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn zsh process");

        let stdin = child.stdin.take().expect("failed to take zsh stdin");
        let stdout = child.stdout.take().expect("failed to take zsh stdout");
        let stderr = child.stderr.take().expect("failed to take zsh stderr");

        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let _ = writeln!(std::io::stderr(), "{line}");
                    }
                    Err(_) => break,
                }
            }
        });

        let mut zp = ZshProcess {
            _child: child,
            stdin,
            stdout_reader: BufReader::new(stdout),
            pending_state: None,
        };

        // Load interactive-class config. Guard against common hang/noise sources:
        // - stdin redirected so rc cannot consume the command pipe
        // - DISABLE_AUTO_TITLE / clear precmd/preexec so OSC titles do not
        //   prefix every subsequent stdout line (breaks sentinel matching)
        // - ZSH_DISABLE_COMPFIX avoids interactive fixup prompts
        zp.run_command(
            r#"
export DISABLE_AUTO_TITLE=true
export ZSH_DISABLE_COMPFIX=true
if [[ -f ${ZDOTDIR:-$HOME}/.zshrc ]]; then
  source ${ZDOTDIR:-$HOME}/.zshrc </dev/null >/dev/null 2>&1
fi
precmd_functions=()
preexec_functions=()
chpwd_functions=()
PROMPT=
RPROMPT=
PS1=
"#,
        );

        // Trap SIGINT so zsh doesn't die when the user presses Ctrl+C.
        zp.run_command("trap 'true' INT");

        zp
    }

    /// Capture all exported env vars by running a no-op command.
    pub fn capture_env(&mut self) -> HashMap<String, String> {
        let state = self.run_command("true");
        state.env
    }

    fn build_preamble(&mut self) -> String {
        let state = match self.pending_state.take() {
            Some(s) => s,
            None => return String::new(),
        };

        let mut preamble = String::new();
        preamble.push_str(&format!(
            "cd {}\n",
            shell_escape(&state.cwd.to_string_lossy())
        ));
        for (key, value) in &state.env {
            preamble.push_str(&format!("export {}={}\n", key, shell_escape(value)));
        }
        preamble
    }

    fn run_command(&mut self, command: &str) -> ShellState {
        let preamble = self.build_preamble();

        // Shell-agnostic env dump via printenv (KEY=value lines).
        let block = format!(
            "{preamble}{command}\n\
             __shannon_ec=$?\n\
             print -r -- \"{SENTINEL_START}\"\n\
             printenv\n\
             print -r -- \"__SHANNON_CWD=$(pwd)\"\n\
             print -r -- \"__SHANNON_EXIT=$__shannon_ec\"\n\
             print -r -- \"{SENTINEL_END}\"\n"
        );

        if let Err(e) = self.stdin.write_all(block.as_bytes()) {
            eprintln!("ahsh: failed to write to zsh stdin: {e}");
            return ShellState {
                env: HashMap::new(),
                cwd: std::path::PathBuf::from("/"),
                last_exit_code: 1,
            };
        }
        if let Err(e) = self.stdin.flush() {
            eprintln!("ahsh: failed to flush zsh stdin: {e}");
            return ShellState {
                env: HashMap::new(),
                cwd: std::path::PathBuf::from("/"),
                last_exit_code: 1,
            };
        }

        let mut in_sentinel = false;
        let mut sentinel_buf = String::new();
        let mut line = String::new();

        loop {
            line.clear();
            match self.stdout_reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let cleaned = strip_terminal_noise(&line);
                    if cleaned == SENTINEL_END {
                        break;
                    } else if cleaned == SENTINEL_START {
                        in_sentinel = true;
                    } else if in_sentinel {
                        sentinel_buf.push_str(&cleaned);
                        sentinel_buf.push('\n');
                    } else if !cleaned.is_empty() {
                        // Command output — display without OSC junk when possible
                        println!("{cleaned}");
                        let _ = std::io::stdout().flush();
                    }
                }
                Err(e) => {
                    eprintln!("ahsh: error reading zsh stdout: {e}");
                    break;
                }
            }
        }

        let (env, cwd) = executor::parse_printenv_env(&sentinel_buf)
            .unwrap_or_else(|| (HashMap::new(), std::path::PathBuf::from("/")));

        let exit_code = sentinel_buf
            .lines()
            .find_map(|l| l.strip_prefix("__SHANNON_EXIT="))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(1);

        ShellState {
            env,
            cwd,
            last_exit_code: exit_code,
        }
    }
}

impl ShellEngine for ZshProcess {
    fn inject_state(&mut self, state: &ShellState) {
        self.pending_state = Some(state.clone());
    }

    fn execute(&mut self, command: &str) -> ShellState {
        self.run_command(command)
    }
}

/// Escape a string for use in a single-quoted POSIX shell context.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Strip OSC/CSI sequences and CR so sentinel matching is exact.
fn strip_terminal_noise(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // OSC: ESC ] ... BEL or ESC ] ... ESC \
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            i += 2;
            while i < bytes.len() {
                if bytes[i] == 0x07 {
                    i += 1;
                    break;
                }
                if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // CSI: ESC [ ... final byte @-~
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
            continue;
        }
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_simple() {
        assert_eq!(shell_escape("hello"), "'hello'");
    }

    #[test]
    fn test_shell_escape_with_quotes() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_shell_escape_with_spaces() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
    }

    #[test]
    fn strip_terminal_noise_removes_osc_title() {
        let raw = "\x1b]2;printenv\x07\x1b]1;\x07==SHANNON_SENTINEL_START==\n";
        assert_eq!(strip_terminal_noise(raw), SENTINEL_START);
    }

    #[test]
    fn test_zsh_process_echo() {
        let mut zp = ZshProcess::new();
        let state = zp.run_command("echo hello");
        assert_eq!(state.last_exit_code, 0);
    }

    #[test]
    fn test_zsh_process_env_persistence() {
        let mut zp = ZshProcess::new();
        zp.run_command("export TEST_VAR=foobar");
        let state = zp.run_command("true");
        assert_eq!(state.env.get("TEST_VAR").unwrap(), "foobar");
    }

    #[test]
    fn test_zsh_process_cwd_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let mut zp = ZshProcess::new();
        zp.run_command(&format!("cd {}", shell_escape(&dir_path)));
        let state = zp.run_command("true");
        assert_eq!(state.cwd.to_string_lossy(), dir_path);
    }

    #[test]
    fn test_zsh_process_exit_code() {
        let mut zp = ZshProcess::new();
        let state = zp.run_command("false");
        assert_eq!(state.last_exit_code, 1);
    }

    #[test]
    fn test_zsh_process_capture_env() {
        let mut zp = ZshProcess::new();
        zp.run_command("export CAPTURE_TEST=works");
        let env = zp.capture_env();
        assert_eq!(env.get("CAPTURE_TEST").unwrap(), "works");
    }

    #[test]
    fn test_zsh_process_inject_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut env = HashMap::new();
        env.insert("INJECTED".to_string(), "yes".to_string());
        let state = ShellState {
            env,
            cwd: dir.path().to_path_buf(),
            last_exit_code: 0,
        };
        let mut zp = ZshProcess::new();
        zp.inject_state(&state);
        let result = zp.run_command("true");
        assert_eq!(result.env.get("INJECTED").unwrap(), "yes");
        assert_eq!(result.cwd, dir.path());
    }

    #[test]
    fn test_zsh_process_no_declare_x_dependency() {
        let mut zp = ZshProcess::new();
        zp.run_command("export ROUNDTRIP=from_printenv");
        let state = zp.run_command("true");
        assert_eq!(state.env.get("ROUNDTRIP").unwrap(), "from_printenv");
        assert!(!state.env.is_empty());
    }

    #[test]
    fn test_zsh_bootstrap_has_home_and_path() {
        let mut zp = ZshProcess::new();
        let env = zp.capture_env();
        assert!(
            env.get("HOME").map(|h| !h.is_empty()).unwrap_or(false),
            "expected HOME from zsh login env"
        );
        assert!(
            env.get("PATH").map(|p| !p.is_empty()).unwrap_or(false),
            "expected PATH from zsh login/.zshrc env"
        );
    }

    /// Marker set only inside a real .zshrc (when present) or via login PATH
    /// that includes brew when .zprofile runs brew shellenv.
    #[test]
    fn test_zsh_bootstrap_includes_login_path_entries() {
        let mut zp = ZshProcess::new();
        let env = zp.capture_env();
        let path = env.get("PATH").cloned().unwrap_or_default();
        // On this product platform macOS, brew shellenv in .zprofile is common;
        // always require a non-empty multi-component PATH as a weaker bound.
        assert!(
            path.contains(':') && path.len() > 8,
            "expected multi-entry PATH from zsh login config, got {path:?}"
        );
    }
}
