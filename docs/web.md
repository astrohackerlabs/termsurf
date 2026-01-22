# Web Command

The `web` command provides CLI access to TermSurf's browser functionality.

## Product Requirements

### Commands

The `web` command supports the following subcommands:

| Command           | Description                      |
| ----------------- | -------------------------------- |
| `web open <url>`  | Open a URL in a browser pane     |
| `web file <path>` | Open a local file in the browser |
| `web close`       | Close the browser overlay        |

### Invocation

Phase 1: Subcommand of `termsurf cli`:

```bash
termsurf cli web open https://example.com
termsurf cli web file ./index.html
termsurf cli web close
```

Phase 2: Standalone `web` command:

```bash
web open https://example.com
web file ./index.html
web close
```

### Current State

The command is currently `web-open` (flat, not nested):

```bash
termsurf cli web-open <url>
```

---

## Experiments

### Experiment 1: Convert `web-open` to `web open`

**Status:** Pending

**Goal:** Restructure the CLI to use nested subcommands (`web open`) instead of
flat commands (`web-open`).

**Plan:**

1. Create `wezterm/src/cli/web.rs` with nested subcommand structure:

   ```rust
   use clap::{Parser, Subcommand};
   use wezterm_client::client::Client;

   #[derive(Debug, Parser, Clone)]
   pub struct WebCommand {
       #[command(subcommand)]
       pub sub: WebSubCommand,
   }

   #[derive(Debug, Subcommand, Clone)]
   pub enum WebSubCommand {
       /// Open a URL in a browser pane
       #[command(name = "open")]
       Open(WebOpen),
   }

   #[derive(Debug, Parser, Clone)]
   pub struct WebOpen {
       /// The URL to open
       url: String,
   }

   impl WebCommand {
       pub async fn run(&self, client: Client) -> anyhow::Result<()> {
           match &self.sub {
               WebSubCommand::Open(cmd) => cmd.run(client).await,
           }
       }
   }

   impl WebOpen {
       pub async fn run(&self, client: Client) -> anyhow::Result<()> {
           let pane_id = client.resolve_pane_id(None).await?;
           let response = client
               .web_open(codec::WebOpen {
                   pane_id,
                   url: self.url.clone(),
               })
               .await?;
           println!("{}", response.message);
           Ok(())
       }
   }
   ```

2. Update `wezterm/src/cli/mod.rs`:

   - Add `mod web;`
   - Replace `WebOpen` variant with `Web(web::WebCommand)`
   - Update dispatch match arm
   - Remove `mod web_open;`

3. Delete `wezterm/src/cli/web_open.rs`

4. Build and test:
   ```bash
   ./scripts/build-debug.sh --open
   termsurf cli web open https://example.com
   ```

**Files changed:**

| File                          | Change                                     |
| ----------------------------- | ------------------------------------------ |
| `wezterm/src/cli/web.rs`      | New file with `WebCommand` and subcommands |
| `wezterm/src/cli/mod.rs`      | Register `Web` variant, remove `WebOpen`   |
| `wezterm/src/cli/web_open.rs` | Delete                                     |
