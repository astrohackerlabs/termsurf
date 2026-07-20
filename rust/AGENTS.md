# AGENTS.md

Guidance for coding agents working in the Astrohacker **Rust** tree (`rust/`)
under the monorepo-root Cargo workspace.

Root `Cargo.toml` is the workspace; members are paths like `rust/ahweb`.
Package and binary names stay unprefixed (`ahweb`, `ahsh`, …).

`rust/ahsh` is **excluded** from workspace members (own lockfile). Build with:

```sh
cargo build --manifest-path rust/ahsh/Cargo.toml
```

Workspace `target/` is at the **monorepo root**. Fork trees live under top-level
`forks/`; root workspace **excludes** `forks` so nested fork Cargo workspaces
resolve.

## Commands

From monorepo root:

```sh
cargo metadata --no-deps
cargo check --workspace
cargo build -p ahweb
cargo build --manifest-path rust/ahsh/Cargo.toml
```

## Hygiene

- Keep `target/`, native `build/` dirs under crates, and app bundles out of git.
- Add crate-local `AGENTS.md` only when a subdirectory needs extra guidance.
