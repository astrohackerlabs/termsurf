# ah-chromiumd

Chromium-backed browser engine process for Astrohacker Terminal.

Package folder, Cargo package name, and binary are all **`ah-chromiumd`**. It speaks the shared TermSurf
protobuf/socket protocol and links against the patched Chromium work tracked
through `forks/chromium/` and `patches/chromium/`.

Useful commands from `rust/`:

```sh
cargo check -p ah-chromiumd
cargo build -p ah-chromiumd
```

Chromium must be prepared separately before full linking or runtime tests.
