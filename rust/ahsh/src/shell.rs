use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct ShellState {
    pub env: HashMap<String, String>,
    pub cwd: PathBuf,
    pub last_exit_code: i32,
}

impl ShellState {
    pub fn from_current_env() -> Self {
        ShellState {
            env: std::env::vars().collect(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            last_exit_code: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_state_from_current_env() {
        let state = ShellState::from_current_env();
        assert!(state.env.contains_key("PATH"));
        assert!(state.cwd.is_absolute());
        assert_eq!(state.last_exit_code, 0);
    }
}
