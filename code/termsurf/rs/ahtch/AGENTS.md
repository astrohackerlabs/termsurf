# AGENTS.md — ahtch

Astrohacker Torch is a nested Cargo workspace, excluded from the monorepo root
workspace. Its `.cargo/config.toml` pins LibTorch and rpaths so they cannot
leak into other crates. PATH CLI and daemon are the same binary: `ahtch` and
`ahtch --daemon`.

Run Cargo only after entering this directory; `--manifest-path` from the
monorepo root does not load the local LibTorch configuration:

```nu
cd code/termsurf/rs/ahtch
cargo test
cargo run --bin ahtch -- --help
cargo fmt
```

The bootstrap script creates gitignored `.venv-torch`, `.libtorch`, and
`target/`. It may reuse `~/dev/nutorch/.venv-torch` by symlink, never copy.

## Runtime contract

The daemon owns all tensors in a Rust registry and returns string handles;
tensor data never crosses the process boundary. It is Apple-silicon/MPS-only
and refuses to start without MPS. Device selection is daemon-wide, never a
per-tensor flag.

Each CLI invocation performs one operation. Both pipeline and argument forms
must work; stdin fills only missing leftmost tensor operands and is never read
when all operands are present. Match PyTorch names, argument order, defaults,
broadcasting, and semantics. Do not silently cast, and validate shapes, dims,
and dtypes in Rust before tch-rs.

Nu cannot pass list values directly to external commands; encode shapes as
JSON:

```nu
ahtch randn ([6 6] | to json)
```

Regenerate `ahtch.nu` from this directory after generator changes:

```nu
cargo run --bin ahtch -- nu-module | save -f ahtch.nu
```

The daemon auto-starts, has a renewable one-hour idle TTL, and owns handle
lifetime. Public command documentation lives at
`https://termsurf.com/docs/ahtch`.
