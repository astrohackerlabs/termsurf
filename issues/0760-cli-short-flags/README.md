+++
status = "closed"
opened = "2026-03-17"
closed = "2026-03-20"
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

## Experiments

### Experiment 1: Add short to clap arg attributes

#### Description

Add `short` to both `#[arg()]` attributes. Clap derives the short flag from the
first character of the field name: `profile` → `-p`, `browser` → `-b`.

#### Changes

**1. TUI: `webtui/src/main.rs`**

Change (~line 178):

```rust
#[arg(long, global = true)]
profile: Option<String>,
```

to:

```rust
#[arg(short, long, global = true)]
profile: Option<String>,
```

Change (~line 182):

```rust
#[arg(long, global = true)]
browser: Option<String>,
```

to:

```rust
#[arg(short, long, global = true)]
browser: Option<String>,
```

#### Verification

```bash
cd webtui && cargo build
```

| #   | Test              | Command                            | Expected                           |
| --- | ----------------- | ---------------------------------- | ---------------------------------- |
| 1   | Short profile     | `web -p work localhost:3000`       | Uses "work" profile                |
| 2   | Short browser     | `web -b roamium localhost`         | Uses "roamium" browser             |
| 3   | Both short        | `web -p work -b roamium localhost` | Both flags work together           |
| 4   | Long still works  | `web --profile work localhost`     | Long form still accepted           |
| 5   | Help shows shorts | `web --help`                       | Shows `-p` and `-b` in help output |

**Result:** Pass

All flags work: `-p` for profile, `-b` for browser, both together, long forms
still accepted, and help output shows the short forms.

#### Conclusion

Adding `short` to clap's `#[arg()]` attribute is all it takes. Clap infers `-p`
from `profile` and `-b` from `browser` automatically.

## Conclusion

Added `-p` and `-b` short flags for `--profile` and `--browser`. One-word change
per flag in the clap derive attribute.
