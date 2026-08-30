# AGENTS.md — ahsh

Excluded Cargo package with its own lockfile and path dependencies on the
Nushell and Reedline forks. Default interactive mode is Nu; alternate mode is a
persistent login zsh worker that loads the user's zsh environment and injects
it into Nu. Bash is not a mode.

```nu
cargo build --manifest-path code/termsurf/rs/ahsh/Cargo.toml
```
