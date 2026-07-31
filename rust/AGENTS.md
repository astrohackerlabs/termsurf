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

### Operator product smoke (`ahweb`)

When telling the operator how to open a URL in TermSurf for engine smoke:

1. **Outer** shell: build/codesign/`^open` the host app (see root `AGENTS.md`).
2. **Inside** Astrohacker TermSurf.app only: run `ahweb` via Cargo from the
   monorepo root — **not** in the outer shell after `^open`, and not via
   `target/release/ahweb` or bare `ahweb` on PATH unless the user asks for an
   installed binary:

```nu
# ONLY inside TermSurf
cd ~/dev/astrohacker
let engine = ($env.PWD | path join "forks/chromium/src/out/Default/ah-chromiumd")
cargo run --release -p ahweb -- --browser $engine <url>
```

Pass `--browser` and the URL after `--`. Full two-shell sequence, `^open`,
paths with spaces, codesign, and fixtures: **root `AGENTS.md`**.

## Hygiene

- Keep `target/`, native `build/` dirs under crates, and app bundles out of git.
- Add crate-local `AGENTS.md` only when a subdirectory needs extra guidance.
