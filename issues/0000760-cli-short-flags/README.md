+++
status = "open"
opened = "2026-03-17"
+++

# Issue 760: Add short flags for TUI CLI arguments

## Goal

Add single-character shorthand flags to the `web` TUI's CLI arguments so users
can type `web -p work` instead of `web --profile work`.

## Background

The `web` TUI uses clap's derive API for argument parsing (`webtui/src/main.rs`,
line 168). Currently there are two long-only flags:

| Flag        | Purpose                           | Proposed short |
| ----------- | --------------------------------- | -------------- |
| `--profile` | Browser profile name              | `-p`           |
| `--browser` | Browser binary (e.g., "chromium") | `-b`           |

Neither has a short form. Adding `short` to the clap `#[arg()]` attribute is a
one-line change per flag.

### Current definition

```rust
#[arg(long, global = true)]
profile: Option<String>,

#[arg(long, global = true)]
browser: Option<String>,
```

### Target

```rust
#[arg(short, long, global = true)]
profile: Option<String>,

#[arg(short, long, global = true)]
browser: Option<String>,
```

Clap infers `-p` from `profile` and `-b` from `browser` automatically.
