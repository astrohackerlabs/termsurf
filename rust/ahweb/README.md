# ahweb

Browser chrome for Astrohacker Terminal, rendered inside the terminal pane.
Built with Rust and [ratatui](https://ratatui.rs/).

When the user runs `ahweb` (or types `web google.com` in-shell), this TUI draws
the URL bar, viewport border, and status bar. It connects to the GUI via Unix
socket to send overlay coordinates and receive mode/URL updates. The actual
webpage renders as a GPU texture overlay — the TUI handles only the chrome
around it.

Package folder, Cargo package name, and binary are all **`ahweb`**.

## Build

```bash
cargo build -p ahweb
```
