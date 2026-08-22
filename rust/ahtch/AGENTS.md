# Astrohacker Torch (`ahtch`)

Nested Cargo workspace. **Not** a member of the monorepo root workspace.
LibTorch is pinned in **this** tree’s `.cargo/config.toml` so those rpaths
and `LIBTORCH` env values cannot leak onto `ahweb`.

PATH CLI: **`ahtch`**. Daemon: **`ahtch --daemon`** (same binary).
Product name: **Astrohacker Torch**. Shipped on the Homebrew cask
`astrohacker`. Contributors still bootstrap LibTorch in this tree.

New product work uses root **`docs/issues/`** and the
`issues-and-experiments` skill. Public command docs: `/docs/ahtch`.

## Bootstrap

From the monorepo root (Nu):

```nu
cd ~/dev/astrohacker
# Optional: reuse the old NuTorch checkout venv (symlink; do not copy):
let venv = ($env.HOME | path join "dev/nutorch/.venv-torch")
if ($venv | path exists) and not ("rust/ahtch/.venv-torch" | path exists) {
  ^ln -s $venv rust/ahtch/.venv-torch
}
rust/ahtch/scripts/bootstrap.sh
```

`.venv-torch`, `.libtorch`, and `target/` stay gitignored.

## Build / test

`cd` into this directory before `cargo test` / `cargo build`. Cargo loads
`.cargo/config.toml` from the current directory (and parents); invoking
`cargo test --manifest-path rust/ahtch/Cargo.toml` from the **monorepo
root** does **not** apply the LibTorch pin, and `torch-sys` fails.

```nu
cd rust/ahtch
cargo test
cargo run --bin ahtch -- --help
cargo fmt
```

Nushell dual-input (PATH must include `ahtch`):

```nu
$env.PATH = ($env.PWD | path join "rust/ahtch/target/debug" | prepend $env.PATH)
nu rust/ahtch/scripts/test-dual-input.nu
```

Nushell **cannot pass a list** into the external (`cannot_pass_list_to_external`).
Shapes go through **`to json`**, not spread:

```nu
ahtch randn ([6 6] | to json)
```

Regenerate the module after generator changes:

```nu
cargo run --bin ahtch -- nu-module | save -f ahtch.nu
```

(from `rust/ahtch/`, after `cd`). When editing Rust, run `cargo fmt` and
accept its output.

## Architecture

Tensors live in a Rust-owned registry. The shell passes **string
handles**; tensor data never crosses the process boundary.

```
bash / zsh / fish / nushell / python / anything
    ↓ `ahtch` CLI (one op per invocation)
    ↓ Unix socket (request/response)
`ahtch --daemon`    ← same binary; registry, LibTorch, GPU, autograd
    ↓ tch-rs
LibTorch (C++)
    ↓ Metal (MPS)
Apple-silicon GPU
```

- The **daemon process** (`ahtch --daemon`) owns the tensor database.
  Handles are string identifiers.
- **GPU-only, Apple silicon for now:** every tensor is on MPS. There is
  no device option. The daemon refuses to start without MPS. A future
  CUDA/Linux port is a daemon-level “the GPU” decision, never a
  per-tensor flag.
- **`ahtch`** sends one operation per invocation and prints handles to
  stdout so POSIX pipelines compose.
- Any shell works. Nushell is the structured client (`ahtch.nu`).
- Daemon lifecycle is plumbing: `ahtch` auto-starts itself as
  `ahtch --daemon`; idle TTL defaults to 1 hour (ops renew the lease);
  `ahtch daemon status|ttl|stop|restart|start` inspects it. Tensors
  live as long as the daemon.

## Principles

1. **String handles are the interface.** Tensor data never leaves Rust.
2. **Dual Input Pattern.** Pipeline form (`$t1 | ahtch add $t2`) and
   argument form (`ahtch add $t1 $t2`) both work. Stdin fills the
   leftmost missing tensor slots, one handle per line, and is never
   read when nothing is missing.
3. **PyTorch API fidelity.** Names, argument order, defaults, and
   semantics match PyTorch wherever possible.
4. **Explicit over implicit.** No silent auto-casting. Broadcasting
   matches PyTorch; non-broadcastable shapes error with both shapes
   named.
5. **Validate in Rust, not C++.** Pre-validate shapes, dims, and dtypes
   before tch-rs calls.

## Layout

```
rust/ahtch/
├── AGENTS.md              # this file
├── Cargo.toml             # nested workspace
├── .cargo/config.toml     # LIBTORCH pin + rpaths
├── ahtch.nu               # generated Nushell module
├── nutorchd/              # daemon library (serve loop)
├── torch-cli/             # PATH binary ahtch (CLI + --daemon)
├── ops/                   # shared op table (nutorch-ops)
└── scripts/               # bootstrap, goldens, dual-input, train
```
