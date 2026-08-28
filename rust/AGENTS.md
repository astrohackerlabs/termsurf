# AGENTS.md

Guidance for coding agents working in the Astrohacker **Rust** tree (`rust/`)
under the monorepo-root Cargo workspace.

Root `Cargo.toml` is the workspace; members are `rust/ahweb`,
`rust/ah-chromiumd`, `rust/ahnexus`, `rust/ebxlib`,
`rust/ebxfloat`, `rust/ebxpow5`, `rust/ebxmine`. Package and
binary names stay unprefixed (`ahweb`, `ahnexus`, `ebxlib`, …).

`rust/ahnexus` is a workspace member. Nexus protocol lives in **Rust**
(`forks/nexus/nexus-common`), not TypeScript. See
`rust/ahnexus/AGENTS.md`.

`rust/ahsh` is **excluded** from workspace members (own lockfile).
Default interactive mode is **nu**; alt is **zsh** (not bash). See
`rust/ahsh/AGENTS.md`. Build with:

```sh
cargo build --manifest-path rust/ahsh/Cargo.toml
```

`rust/ahtch` is **excluded** (own workspace + LibTorch pin in
`rust/ahtch/.cargo/config.toml`). `cd rust/ahtch` before cargo so that
config applies (a root `--manifest-path` invocation misses it):

```sh
cd rust/ahtch && cargo test
```

EarthBucks 2.0 crates are **root members**: `ebxlib`,
`ebxfloat`, `ebxpow5`, `ebxmine`. Protocol:
[`ebxlib/AGENTS.md`](ebxlib/AGENTS.md). PoW:
[`ebxpow5/AGENTS.md`](ebxpow5/AGENTS.md). DAA:
[`ebxfloat/AGENTS.md`](ebxfloat/AGENTS.md). Mine traps:
[`ebxmine/AGENTS.md`](ebxmine/AGENTS.md). PATH **`ahebx`** is
`bun/bin/ahebx` (pool client of ebxwebapp), not `ebxmine`.
Production pool is `bun/web/ebxwebapp`.

```nu
cargo test -p ebxlib -p ebxfloat -p ebxpow5
cargo check -p ebxmine
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
cd rust/ahtch; cargo test
cargo test -p ebxlib -p ebxfloat -p ebxpow5
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
