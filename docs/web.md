# Web Command

The `web` command provides CLI access to TermSurf's browser functionality.

## Current State

The command is currently `web-open`:

```bash
termsurf cli web-open <url>
```

## Target Architecture

### Phase 1: Subcommand of `termsurf cli`

```bash
termsurf cli web <subcommand> [options]
```

Subcommands:

| Command           | Description                                  |
| ----------------- | -------------------------------------------- |
| `web open <url>`  | Open a URL in a browser pane                 |
| `web file <path>` | Open a local file in a browser pane (future) |

### Phase 2: Standalone `web` Command

```bash
web <subcommand> [options]
```

The standalone `web` binary will be a thin wrapper that connects to the running
TermSurf instance, similar to how `termsurf cli` works.

## Implementation Plan

### Step 1: Create `web.rs` Module

Create `wezterm/src/cli/web.rs` with nested subcommand structure:

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

### Step 2: Update `mod.rs`

In `wezterm/src/cli/mod.rs`:

1. Add module declaration:
   ```rust
   mod web;
   ```

2. Replace the `WebOpen` variant in `CliSubCommand` enum:
   ```rust
   // Remove:
   #[command(name = "web-open", rename_all = "kebab")]
   WebOpen(web_open::WebOpen),

   // Add:
   #[command(name = "web", about = "Browser commands")]
   Web(web::WebCommand),
   ```

3. Update the dispatch match:
   ```rust
   // Remove:
   CliSubCommand::WebOpen(cmd) => cmd.run(client).await,

   // Add:
   CliSubCommand::Web(cmd) => cmd.run(client).await,
   ```

4. Remove the `mod web_open;` declaration.

### Step 3: Delete `web_open.rs`

Remove `wezterm/src/cli/web_open.rs` (code has moved to `web.rs`).

### Step 4: Build and Test

```bash
./scripts/build-debug.sh --open
termsurf cli web open https://example.com
```

## Files Changed

| File                          | Change                                     |
| ----------------------------- | ------------------------------------------ |
| `wezterm/src/cli/web.rs`      | New file with `WebCommand` and subcommands |
| `wezterm/src/cli/mod.rs`      | Register `Web` variant, remove `WebOpen`   |
| `wezterm/src/cli/web_open.rs` | Delete                                     |

## Future Subcommands

### `web file`

Open a local file in the browser:

```bash
termsurf cli web file ./index.html
termsurf cli web file /path/to/app/dist/index.html
```

Implementation will:

1. Resolve the path to an absolute path
2. Convert to `file://` URL
3. Call the same `web_open` RPC

### `web close`

Close the browser overlay in a pane:

```bash
termsurf cli web close
termsurf cli web close --pane-id 123
```

## RPC Protocol

The web commands use the existing RPC protocol:

```rust
// codec/src/lib.rs
pub struct WebOpen {
    pub pane_id: PaneId,
    pub url: String,
}

pub struct WebOpenResponse {
    pub message: String,
}
```

Future commands may need new PDU types added to the codec.
