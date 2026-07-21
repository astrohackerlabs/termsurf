# ah-webkitd

WebKit-backed browser engine process for Astrohacker Terminal.

Package folder, Cargo package name, and binary are all **`ah-webkitd`**. It speaks the shared TermSurf protobuf/socket
protocol and uses the macOS WebKit ABI wrapper under `libtermsurf_webkit/`.

Useful commands from `rust/`:

```sh
cargo check -p ah-webkitd
cargo build -p ah-webkitd
```

The `libtermsurf_webkit` name is an internal ABI compatibility name. Do not
rename it without a dedicated compatibility issue.

## Host render service

Astrohacker Terminal supplies every controlled engine process with the optional
`--render-surface-service=<NAME>` host argument, including launches selected by
absolute executable path. `ah-webkitd` accepts and consumes this host-only
argument before entering WebKit; WebKit presentation continues to use its
CAContext transport.
