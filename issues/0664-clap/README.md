+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 664: Clap CLI Parser

Add clap as the CLI argument parser for the `web` TUI, introducing subcommands.

## Problem

The `web` TUI parses arguments manually with `std::env::args()` and a
hand-rolled `while` loop (lines 110–127 of `tui/src/main.rs`). This works for
the current `web <url> [--profile <name>]` syntax but doesn't scale to
subcommands. We want to introduce `web url <url>` as the canonical way to open a
URL, while keeping `web <url>` as a backwards-compatible fallback when the
argument isn't a known command.

## Solution

Replace the manual argument parsing with [clap](https://crates.io/crates/clap)
using derive macros. Define a `Cli` struct with an optional subcommand enum.
When no subcommand matches, treat the first positional argument as a URL
(backwards compatibility).

### Subcommand structure

```
web url <url> [--profile <name>]    # canonical
web <url> [--profile <name>]        # fallback (if <url> isn't a command name)
```

Initial subcommands:

- `url <url>` — open a URL in the browser pane

Future subcommands (not in scope): `config`, `profile`, `version`, etc.

### Changes

In `tui/`:

1. **Add clap dependency** — `cargo add clap --features derive` in `tui/`.

2. **Define CLI structs** — in `tui/src/main.rs` (or a new `cli.rs` module):
   - `Cli` struct with `#[derive(Parser)]`, a `--profile` global option, and an
     optional `#[command(subcommand)]` field
   - `Commands` enum with `#[derive(Subcommand)]` containing a
     `Url { url:
String }` variant

3. **Replace manual parsing** — replace lines 110–142 with `Cli::parse()`.
   Extract the URL from either `Commands::Url { url }` or the fallback
   positional argument.

4. **Profile validation** — keep the existing validation logic, applied to
   clap's parsed `--profile` value.

### Fallback behavior

When the user types `web https://example.com`:

- clap sees `https://example.com` as the first positional argument
- It doesn't match any subcommand
- The `Cli` struct captures it as a fallback `url` field (`Option<String>`)
- If neither a subcommand nor a fallback URL is provided, clap shows help

### Concerns

- **Breaking change** — `web url https://example.com` is new syntax. The old
  `web https://example.com` must continue to work.
- **Argument collision** — if a future subcommand name happens to also be a
  valid URL (unlikely), the subcommand takes priority. This is the intended
  behavior.

## Experiment 1: Replace manual parsing with clap

### Hypothesis

Adding clap with derive macros and an optional subcommand enum will replace the
hand-rolled argument loop while preserving both `web url <url>` (new) and
`web <url>` (fallback) syntax.

### Changes

1. **Add clap dependency** — run `cargo add clap --features derive` in `tui/`.

2. **Define CLI structs** in `tui/src/main.rs` above `main()`:

   ```rust
   use clap::{Parser, Subcommand};

   #[derive(Parser)]
   #[command(name = "web", about = "Terminal browser")]
   struct Cli {
       #[command(subcommand)]
       command: Option<Commands>,

       /// URL to open (fallback when no subcommand given)
       url: Option<String>,

       /// Browser profile name
       #[arg(long, default_value = "default", global = true)]
       profile: String,
   }

   #[derive(Subcommand)]
   enum Commands {
       /// Open a URL in the browser pane
       Url {
           /// The URL to open
           url: String,
       },
   }
   ```

3. **Replace lines 110–142** (manual parsing) with:

   ```rust
   let cli = Cli::parse();

   let profile = cli.profile;
   // (existing profile validation)

   let mut url = match cli.command {
       Some(Commands::Url { url }) => url,
       None => cli.url.unwrap_or_else(|| {
           eprintln!("Usage: web [url] <url> [--profile <name>]");
           std::process::exit(1);
       }),
   };
   ```

4. **Keep profile validation** unchanged (lines 128–137).

### Test

1. `cargo build` in `tui/` — compiles without errors
2. `web url https://example.com` — opens the URL (new syntax)
3. `web https://example.com` — opens the URL (fallback, backwards compatible)
4. `web url https://example.com --profile test` — uses profile "test"
5. `web https://example.com --profile test` — uses profile "test"
6. `web` — shows help/error (no URL provided)
7. `web --help` — shows clap-generated help with subcommands listed

### Result

Pass. All seven test cases verified. clap replaces the manual `while` loop with
derive macros. Both `web url <url>` and `web <url>` work. `--profile` works as a
global flag with both syntaxes. `web` shows a usage error and `web --help` shows
clap-generated help with the `url` subcommand listed.

## Conclusion

One experiment replaced the hand-rolled argument parser with clap. The `web` CLI
now supports subcommands (`web url <url>`) while preserving backwards
compatibility (`web <url>`). `--profile` is a global flag available to all
subcommands. Future subcommands can be added by extending the `Commands` enum.
