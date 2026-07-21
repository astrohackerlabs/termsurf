# ah-ladybirdd

Ladybird-backed browser engine prototype for Astrohacker Terminal.

Package folder, Cargo package name, and binary are all **`ah-ladybirdd`**. It speaks the shared
TermSurf protobuf/socket protocol used by the terminal frontend and wraps the
Ladybird embedding work under `libtermsurf_ladybird/`.

Useful commands from `rust/`:

```sh
cargo check -p ah-ladybirdd
cargo build -p ah-ladybirdd
```

The `libtermsurf_ladybird` name is an internal ABI compatibility name. Do not
rename it without a dedicated compatibility issue.

## Host render service

Astrohacker Terminal supplies every controlled engine process with the optional
`--render-surface-service=<NAME>` host argument, including launches selected by
absolute executable path. `ah-ladybirdd` connects to that service and transfers
its IOSurface attachment to the host. The browser selector remains an opaque
identity; Ladybird behavior does not depend on the executable's basename.
