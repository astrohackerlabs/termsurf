use crate::shell::ShellState;

/// Trait for shell engines. Each non-nu shell (zsh, etc.)
/// implements this to provide a uniform interface for the REPL.
pub trait ShellEngine {
    /// Inject ahsh state (env vars, cwd) into the shell before execution.
    fn inject_state(&mut self, state: &ShellState);

    /// Execute a command and return the updated state.
    fn execute(&mut self, command: &str) -> ShellState;
}

/// A named shell with its engine and display config.
pub struct ShellSlot {
    pub name: String,
    pub highlighter: Option<String>,
    pub engine: Box<dyn ShellEngine>,
}
