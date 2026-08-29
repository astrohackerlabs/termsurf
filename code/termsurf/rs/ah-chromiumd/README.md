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

## Host render service

Astrohacker Terminal supplies every controlled engine process with the optional
`--render-surface-service=<NAME>` host argument, including launches selected by
absolute executable path. `ah-chromiumd` accepts and consumes this host-only
argument before entering Chromium; Chromium presentation continues to use its
CAContext transport.
