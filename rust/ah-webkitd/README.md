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
