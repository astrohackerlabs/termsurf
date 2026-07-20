# Astrohacker Shell

Astrohacker Shell is the Astrohacker port of Shannon. It is exposed as the
`ahsh` binary and builds on a patched Nushell fork plus Reedline.

**Modes:** default interactive mode is **Nushell** (`nu`). Traditional alt mode
is **zsh** (not bash): a persistent login zsh worker loads user config (including
`.zshrc` under `ZDOTDIR` when set) and injects that environment into Nushell at
startup. Mode toggle is `nu` ↔ `zsh`.

This crate is **excluded** from the monorepo root Cargo workspace (own lockfile
and fork path deps). Build from the monorepo root:

```sh
cargo build --manifest-path rust/ahsh/Cargo.toml
cargo run --manifest-path rust/ahsh/Cargo.toml --
```

Prepare `forks/nushell` and `forks/reedline` first (see release-manifest /
patch archives).
