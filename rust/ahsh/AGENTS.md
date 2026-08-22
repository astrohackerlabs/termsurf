# AGENTS.md — rust/ahsh

Astrohacker Shell (`ahsh`). **Excluded** from the root Cargo
workspace (own lockfile; path deps on `forks/nushell` and
`forks/reedline`). See `patches/nushell/` and `patches/reedline/`.

**Modes:** default interactive is **Nushell** (`nu`). Alt is **zsh**
(not bash): a persistent login zsh worker loads user config
(`.zshrc` under `ZDOTDIR` when set) and injects that environment
into Nushell. Toggle is `nu` ↔ `zsh`.

```nu
cargo build --manifest-path rust/ahsh/Cargo.toml
cargo run --manifest-path rust/ahsh/Cargo.toml --
```
