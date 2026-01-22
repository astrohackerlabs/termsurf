# Web Command

The `web` command provides CLI access to TermSurf's browser functionality.

## Product Requirements

### Commands

The `web` command supports the following subcommands:

| Command                                        | Description                      |
| ---------------------------------------------- | -------------------------------- |
| `web open <url> [--profile X \| --incognito]`  | Open a URL in a browser pane     |
| `web file <path> [--profile X \| --incognito]` | Open a local file in the browser |
| `web close`                                    | Close the browser overlay        |

### Console Output

When running `web open` or `web file`, all browser console output is redirected
to the terminal:

- `console.log()` → stdout
- `console.error()` → stderr
- `console.warn()` → stderr
- `console.info()` → stdout
- `console.debug()` → stdout

This enables using the browser as a scripting environment where output flows
back to the terminal, similar to running `node script.js`. The CLI command
remains running and streams console output until the browser is closed or the
user presses Ctrl+C.

### One Browser Per Pane

Each pane supports only one browser at a time. If `web open` or `web file` is
called on a pane that already has an open browser, the command will fail with an
error. To open a different URL, first close the existing browser with
`web close`, then open the new one.

This constraint simplifies the implementation by avoiding browser stacking
complexity.

### Browser Profiles

Browsers can use named profiles to isolate cookies, localStorage, and other
session data. Profiles are stored in `~/.config/termsurf/profiles/<name>/`.

```bash
termsurf cli web open https://example.com --profile myproject
termsurf cli web open https://example.com --profile testing
```

**Profile name constraints:**

- Lowercase alphanumeric characters only (`a-z`, `0-9`)
- Must start with a letter
- Examples: `myproject`, `test1`, `devenv`
- Invalid: `MyProject`, `123test`, `my-project`, `my_project`

If `--profile` is omitted, it defaults to `default`.

Use `--incognito` for in-memory only mode where no data persists:

```bash
termsurf cli web open https://example.com --incognito
```

The `--profile` and `--incognito` flags are mutually exclusive.

The profile name validator is implemented once and reused by both the CLI tool
and the server-side handler to ensure consistent validation.

### Invocation

Phase 1: Subcommand of `termsurf cli`:

```bash
termsurf cli web open https://example.com
termsurf cli web open https://example.com --profile myproject
termsurf cli web file ./index.html
termsurf cli web close
```

Phase 2: Standalone `web` command:

```bash
web open https://example.com
web open https://example.com --profile myproject
web file ./index.html
web close
```

### Current State

The `web open` subcommand is implemented:

```bash
termsurf cli web open https://example.com
```

---

## Experiments

### Experiment 1: Convert `web-open` to `web open`

**Status:** Success

**Goal:** Restructure the CLI to use nested subcommands (`web open`) instead of
flat commands (`web-open`).

**Result:** The `termsurf cli web open <url>` command now works as expected.

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

---

### Experiment 2: Replace RPC with Unix Socket

**Status:** Success

**Goal:** Replace WezTerm's RPC mechanism for the `web` command with a Unix
domain socket approach, matching TS1's architecture. This enables bidirectional
communication needed for streaming console output.

**Background:**

The current implementation uses WezTerm's RPC/PDU system:

```
CLI ──WebOpen PDU──► Server ──► GUI creates browser
CLI ◄──WebOpenResponse──       (CLI exits, no event streaming)
```

TS1 uses a Unix domain socket for bidirectional communication:

```
CLI ◄──────────────────────► Socket Server (in GUI)
    request: open
    response: opened
    event: console
    event: console
    event: closed
    (CLI stays connected until browser closes)
```

**Plan:**

1. Add Unix socket server to GUI (`wezterm-gui/src/termsurf_socket/`):

   ```rust
   // mod.rs - Socket server that listens for CLI connections
   pub struct TermsurfSocketServer {
       socket_path: PathBuf,
       // ...
   }

   impl TermsurfSocketServer {
       pub fn start() -> anyhow::Result<Self>;
       pub fn emit_to_pane(&self, pane_id: PaneId, event: TermsurfEvent);
   }
   ```

   ```rust
   // protocol.rs - JSON message types (matching TS1)
   #[derive(Serialize, Deserialize)]
   pub struct TermsurfRequest {
       pub id: String,
       pub command: String,  // "open", "close"
       pub pane_id: Option<PaneId>,
       pub params: serde_json::Value,
   }

   #[derive(Serialize, Deserialize)]
   pub struct TermsurfResponse {
       pub id: String,
       pub success: bool,
       pub message: Option<String>,
       pub error: Option<String>,
   }

   #[derive(Serialize, Deserialize)]
   pub struct TermsurfEvent {
       pub id: String,
       pub event: String,  // "console", "closed"
       pub data: serde_json::Value,
   }
   ```

   ```rust
   // connection.rs - Per-client connection handler
   pub struct TermsurfConnection {
       stream: UnixStream,
       subscribed_panes: HashSet<PaneId>,
   }
   ```

2. Start socket server on GUI launch and set environment variable:

   ```rust
   // In GUI startup code
   let socket_server = TermsurfSocketServer::start()?;
   std::env::set_var("TERMSURF_SOCKET", socket_server.socket_path());
   ```

   Socket path: `/tmp/termsurf-{pid}.sock`

3. Update CLI `web.rs` to use socket instead of RPC:

   ```rust
   impl WebOpen {
       pub fn run(&self) -> anyhow::Result<()> {
           // 1. Connect to socket
           let socket_path = std::env::var("TERMSURF_SOCKET")
               .map_err(|_| anyhow!("Not running inside TermSurf"))?;
           let mut stream = UnixStream::connect(&socket_path)?;

           // 2. Get pane ID from environment
           let pane_id: PaneId = std::env::var("WEZTERM_PANE")?.parse()?;

           // 3. Send open request
           let request = TermsurfRequest {
               id: uuid::Uuid::new_v4().to_string(),
               command: "open".to_string(),
               pane_id: Some(pane_id),
               params: json!({"url": self.url}),
           };
           writeln!(stream, "{}", serde_json::to_string(&request)?)?;

           // 4. Read response
           let mut reader = BufReader::new(stream);
           let mut line = String::new();
           reader.read_line(&mut line)?;
           let response: TermsurfResponse = serde_json::from_str(&line)?;

           if !response.success {
               anyhow::bail!(response.error.unwrap_or_default());
           }

           // 5. Event loop (for future console streaming)
           // For now, just exit after successful open
           println!("{}", response.message.unwrap_or_default());
           Ok(())
       }
   }
   ```

4. Handle "open" command in socket server:

   ```rust
   // In TermsurfSocketServer
   fn handle_request(&self, conn: &mut TermsurfConnection, req: TermsurfRequest) {
       match req.command.as_str() {
           "open" => {
               let url = req.params["url"].as_str().unwrap();
               let pane_id = req.pane_id.unwrap();

               // Create browser (same logic as current handle_web_open)
               // ...

               // Subscribe connection to events for this pane
               conn.subscribed_panes.insert(pane_id);

               // Send response
               conn.send(TermsurfResponse {
                   id: req.id,
                   success: true,
                   message: Some(format!("Opening {}", url)),
                   error: None,
               });
           }
           // ...
       }
   }
   ```

5. Remove RPC-based web open:

   - Remove `Web` variant from `CliSubCommand` enum in `mod.rs`
   - Remove `WebOpen`/`WebOpenResponse` handling from `sessionhandler.rs`
   - Keep `MuxNotification::WebOpen` for internal use (or remove if unused)

6. Update `mod.rs` to run `web` command directly (not through RPC client):

   ```rust
   // In CliSubCommand enum, web is now handled separately
   // Before entering run_cli_async(), check if it's a web command
   // and handle it directly without creating an RPC client

   pub fn run_cli(opts: &crate::Opt, cli: CliCommand) -> anyhow::Result<()> {
       // Handle web commands directly (no RPC)
       if let CliSubCommand::Web(cmd) = &cli.sub {
           return cmd.run();  // Uses socket, not RPC
       }

       // All other commands use RPC as before
       let executor = promise::spawn::ScopedExecutor::new();
       // ...
   }
   ```

7. Build and test:
   ```bash
   ./scripts/build-debug.sh --open
   termsurf cli web open https://example.com
   ```

**Protocol (newline-delimited JSON):**

Request (CLI → Server):

```json
{"id":"abc123","command":"open","pane_id":1,"params":{"url":"https://example.com"}}
```

Response (Server → CLI):

```json
{"id":"abc123","success":true,"message":"Opening https://example.com"}
```

Event (Server → CLI, future):

```json
{"id":"abc123","event":"console","data":{"level":"log","message":"Hello"}}
```

**Files changed:**

| File                                            | Change                          |
| ----------------------------------------------- | ------------------------------- |
| `wezterm-gui/src/termsurf_socket/mod.rs`        | New: Socket server              |
| `wezterm-gui/src/termsurf_socket/protocol.rs`   | New: JSON message types         |
| `wezterm-gui/src/termsurf_socket/connection.rs` | New: Connection handler         |
| `wezterm-gui/src/lib.rs`                        | Start socket server on launch   |
| `wezterm/src/cli/web.rs`                        | Replace RPC with socket client  |
| `wezterm/src/cli/mod.rs`                        | Handle `web` command separately |
| `wezterm-mux-server-impl/src/sessionhandler.rs` | Remove WebOpen handler          |

**Note:** This experiment maintains the same user-facing behavior. The
`web open` command will work exactly as before, but uses the socket internally.
Console streaming will be added in a future experiment.

---

### Experiment 3: Console Output Streaming

**Status:** Pending

**Goal:** Stream console output (console.log, console.error, etc.) from the
browser to the CLI's stdout/stderr.

**Research Findings:**

CEF provides native console capture via `DisplayHandler::on_console_message()`:

- No JavaScript injection required (unlike TS1's WKWebView approach)
- Receives: log level, message, source URL, line number
- Already implemented in cef-rs (`cef/src/handlers/display_handler.rs`)

**Plan:**

1. Create DisplayHandler for console capture:

   - File: `wezterm-gui/src/cef_render/display_handler.rs` (new)
   - Implement `DisplayHandler` trait with `on_console_message`
   - Convert CEF log levels to event types:
     - `LOGSEVERITY_DEBUG` → `console_debug`
     - `LOGSEVERITY_INFO` → `console_info`
     - `LOGSEVERITY_WARNING` → `console_warn`
     - `LOGSEVERITY_ERROR` → `console_error`
     - Default → `console_log`
   - Send events to socket server via `broadcast_event`

2. Track request_id in BrowserState:

   - File: `wezterm-gui/src/cef_render/browser_state.rs`
   - Add `request_id: Option<String>` field
   - Set when browser is created via socket request
   - Include in console events for correlation

3. Broadcast console events via socket:

   - Events sent to connections subscribed to the pane
   - Event format:
     ```json
     {
       "id": "<request_id>",
       "event": "console_log",
       "data": {
         "level": "info",
         "message": "Hello world",
         "source": "https://example.com/app.js",
         "line": 42
       }
     }
     ```

4. CLI event loop:

   - File: `wezterm/src/cli/web.rs`
   - After sending `open` request, enter read loop for events
   - Route events to appropriate output:
     - `console_log`, `console_info`, `console_debug` → stdout
     - `console_warn`, `console_error` → stderr
   - Exit on `closed` event or connection close

5. Send `closed` event when browser closes:

   - When browser is destroyed, send `{event: "closed"}` to socket
   - CLI exits cleanly on receiving this event

**Files to modify:**

| File                                            | Change                   |
| ----------------------------------------------- | ------------------------ |
| `wezterm-gui/src/cef_render/display_handler.rs` | New: DisplayHandler impl |
| `wezterm-gui/src/cef_render/browser_state.rs`   | Add request_id field     |
| `wezterm-gui/src/cef_render/mod.rs`             | Register DisplayHandler  |
| `wezterm-gui/src/termsurf_socket/protocol.rs`   | Add console event types  |
| `wezterm/src/cli/web.rs`                        | Add event loop           |

**Dependencies:** Experiment 2 must be complete (Unix socket communication).
