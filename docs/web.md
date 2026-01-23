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

**Status:** Success

**Goal:** Stream console output (console.log, console.error, etc.) from the
browser to the CLI's stdout/stderr.

**Research Findings:**

CEF provides native console capture via `DisplayHandler::on_console_message()`:

- No JavaScript injection required (unlike TS1's WKWebView approach)
- Receives: log level, message, source URL, line number
- Already implemented in cef-rs (`cef/src/handlers/display_handler.rs`)

TS1's routing approach (to support multiple simultaneous browser panes):

- Routes by **browser → connection**, not by pane broadcast
- Stores `browserId → connection` and `browserId → request_id` mappings
- Connection is passed through when creating browser
- Events sent directly to the stored connection

**Plan:**

1. Store browser → connection mappings in socket server:

   ```rust
   // In termsurf_socket/mod.rs
   pub struct TermsurfSocketServer {
       // ...existing fields...
       /// Maps browser_id to the connection that created it (weak ref)
       browser_connections: RwLock<HashMap<String, Weak<TermsurfConnection>>>,
       /// Maps browser_id to request_id for event correlation
       browser_request_ids: RwLock<HashMap<String, String>>,
   }

   impl TermsurfSocketServer {
       /// Register a browser with its creating connection
       pub fn register_browser(
           &self,
           browser_id: String,
           connection: Arc<TermsurfConnection>,
           request_id: String,
       ) {
           self.browser_connections.write().unwrap()
               .insert(browser_id.clone(), Arc::downgrade(&connection));
           self.browser_request_ids.write().unwrap()
               .insert(browser_id, request_id);
       }

       /// Send event to the connection that created a browser
       pub fn send_browser_event(&self, browser_id: &str, event_type: &str, data: Value) {
           let conn = self.browser_connections.read().unwrap()
               .get(browser_id).and_then(|w| w.upgrade());
           let request_id = self.browser_request_ids.read().unwrap()
               .get(browser_id).cloned();

           if let (Some(conn), Some(request_id)) = (conn, request_id) {
               let event = TermsurfEvent {
                   id: request_id,
                   event: event_type.to_string(),
                   data: Some(data),
               };
               let _ = conn.send_event_direct(&event);
           }
       }

       /// Unregister a browser when it closes
       pub fn unregister_browser(&self, browser_id: &str) {
           self.browser_connections.write().unwrap().remove(browser_id);
           self.browser_request_ids.write().unwrap().remove(browser_id);
       }
   }
   ```

2. Update "open" handler to register browser:

   ```rust
   // In handle_open()
   fn handle_open(&self, conn: &Arc<TermsurfConnection>, request: &TermsurfRequest) -> TermsurfResponse {
       // ...existing validation...

       // Generate browser_id (or get from CEF)
       let browser_id = format!("browser-{}", pane_id);

       // Register this browser with its connection
       self.register_browser(browser_id.clone(), conn.clone(), request.id.clone());

       // Notify GUI to open browser
       mux.notify(mux::MuxNotification::WebOpen {
           pane_id,
           url: url.clone(),
           browser_id: browser_id.clone(),  // Pass browser_id for event routing
       });

       TermsurfResponse::ok(request.id.clone(), Some(json!({"browser_id": browser_id})))
   }
   ```

3. Create DisplayHandler for console capture:

   - File: `wezterm-gui/src/cef_render/display_handler.rs` (new)
   - Implement `DisplayHandler` trait with `on_console_message`
   - Convert CEF log levels:
     - `LOGSEVERITY_DEBUG` → `"debug"`
     - `LOGSEVERITY_INFO` → `"info"`
     - `LOGSEVERITY_WARNING` → `"warn"`
     - `LOGSEVERITY_ERROR` → `"error"`
     - Default → `"log"`
   - Call `socket_server.send_browser_event(browser_id, "console", data)`

   ```rust
   impl DisplayHandler for TermsurfDisplayHandler {
       fn on_console_message(
           &self,
           _browser: &Browser,
           level: LogSeverity,
           message: &str,
           source: &str,
           line: i32,
       ) -> bool {
           let level_str = match level {
               LogSeverity::Debug => "debug",
               LogSeverity::Info => "info",
               LogSeverity::Warning => "warn",
               LogSeverity::Error | LogSeverity::Fatal => "error",
               _ => "log",
           };

           if let Some(server) = get_server() {
               server.send_browser_event(
                   &self.browser_id,
                   "console",
                   json!({
                       "level": level_str,
                       "message": message,
                       "source": source,
                       "line": line,
                   }),
               );
           }

           false // Don't suppress default handling
       }
   }
   ```

4. Add direct event sending to connection:

   ```rust
   // In termsurf_socket/connection.rs
   impl TermsurfConnection {
       /// Send event directly (bypasses subscription check)
       pub fn send_event_direct(&self, event: &TermsurfEvent) -> std::io::Result<()> {
           self.send_message(event)
       }
   }
   ```

5. CLI event loop:

   ```rust
   // In wezterm/src/cli/web.rs
   impl WebOpen {
       pub fn run(&self) -> anyhow::Result<()> {
           // ...existing connect and send request code...

           // Read response
           let response: TermsurfResponse = read_response(&mut reader)?;
           if response.status != "ok" {
               return Err(anyhow!(response.error.unwrap_or_default()));
           }

           // Event loop - read until closed or Ctrl+C
           loop {
               let mut line = String::new();
               match reader.read_line(&mut line) {
                   Ok(0) => break, // Connection closed
                   Ok(_) => {
                       if let Ok(event) = serde_json::from_str::<TermsurfEvent>(&line) {
                           match event.event.as_str() {
                               "console" => {
                                   if let Some(data) = &event.data {
                                       let level = data.get("level")
                                           .and_then(|v| v.as_str())
                                           .unwrap_or("log");
                                       let message = data.get("message")
                                           .and_then(|v| v.as_str())
                                           .unwrap_or("");

                                       match level {
                                           "warn" | "error" => eprintln!("{}", message),
                                           _ => println!("{}", message),
                                       }
                                   }
                               }
                               "closed" => break,
                               _ => {}
                           }
                       }
                   }
                   Err(_) => break,
               }
           }

           Ok(())
       }
   }
   ```

6. Send "closed" event when browser closes:

   ```rust
   // When browser is destroyed (in cef_render or wherever close is handled)
   if let Some(server) = get_server() {
       server.send_browser_event(&browser_id, "closed", json!({}));
       server.unregister_browser(&browser_id);
   }
   ```

**Event format:**

```json
{"id":"req-123","event":"console","data":{"level":"log","message":"Hello world","source":"https://example.com/app.js","line":42}}
{"id":"req-123","event":"closed","data":{}}
```

**Files to modify:**

| File                                            | Change                              |
| ----------------------------------------------- | ----------------------------------- |
| `wezterm-gui/src/termsurf_socket/mod.rs`        | Add browser→connection registry     |
| `wezterm-gui/src/termsurf_socket/connection.rs` | Add `send_event_direct` method      |
| `wezterm-gui/src/cef_render/display_handler.rs` | New: DisplayHandler impl            |
| `wezterm-gui/src/cef_render/mod.rs`             | Register DisplayHandler, send close |
| `wezterm/src/cli/web.rs`                        | Add event loop                      |
| `mux/src/lib.rs`                                | Add browser_id to WebOpen notif     |

**Dependencies:** Experiment 2 must be complete (Unix socket communication).

---

### Experiment 4: Browser Profiles

**Status:** Pending

**Goal:** Implement named browser profiles to isolate cookies, localStorage, and
other session data between different use cases.

**Background:**

CEF uses `cache_path` in `RequestContextSettings` to determine where to store
browser data. When `cache_path` is empty, CEF uses "incognito mode" with
in-memory storage only. When set to a directory path, CEF persists data there.

Currently, all browsers use `RequestContextSettings::default()` which has an
empty `cache_path`, meaning every session is incognito.

**Behavior after implementation:**

- Default: Use profile `default` → `~/.config/termsurf/profiles/default/`
- `--profile <name>`: Use named profile → `~/.config/termsurf/profiles/<name>/`
- `--incognito`: Use in-memory storage only (no persistence)

**Plan:**

1. Add CLI flags to `WebOpen` (`wezterm/src/cli/web.rs`):

   ```rust
   #[derive(Debug, Parser, Clone)]
   pub struct WebOpen {
       /// The URL to open
       url: String,

       /// Browser profile name (default: "default")
       #[arg(long, default_value = "default")]
       profile: String,

       /// Use incognito mode (in-memory only, no persistence)
       #[arg(long, conflicts_with = "profile")]
       incognito: bool,
   }
   ```

2. Add profile name validation function:

   ```rust
   /// Validate profile name: lowercase alphanumeric, must start with letter
   fn validate_profile_name(name: &str) -> anyhow::Result<()> {
       if name.is_empty() {
           anyhow::bail!("Profile name cannot be empty");
       }
       if !name.chars().next().unwrap().is_ascii_lowercase() {
           anyhow::bail!("Profile name must start with a lowercase letter");
       }
       if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
           anyhow::bail!("Profile name must contain only lowercase letters and digits");
       }
       Ok(())
   }
   ```

3. Include profile/incognito in socket request (`wezterm/src/cli/web.rs`):

   ```rust
   // Validate profile name (unless incognito)
   if !self.incognito {
       validate_profile_name(&self.profile)?;
   }

   let request = TermsurfRequest {
       id: request_id,
       action: "open".to_string(),
       pane_id: Some(pane_id),
       data: Some(serde_json::json!({
           "url": self.url,
           "profile": if self.incognito { None::<String> } else { Some(&self.profile) },
           "incognito": self.incognito,
       })),
   };
   ```

4. Extract and validate profile in socket server (`termsurf_socket/mod.rs`):

   ```rust
   fn handle_open(...) -> TermsurfResponse {
       // ... existing url/pane_id extraction ...

       let profile = request.data.as_ref()
           .and_then(|d| d.get("profile"))
           .and_then(|v| v.as_str())
           .map(|s| s.to_string());

       let incognito = request.data.as_ref()
           .and_then(|d| d.get("incognito"))
           .and_then(|v| v.as_bool())
           .unwrap_or(false);

       // Validate profile name server-side
       if let Some(ref name) = profile {
           if let Err(e) = validate_profile_name(name) {
               return TermsurfResponse::error(request.id.clone(), e.to_string());
           }
       }

       // Pass to MuxNotification
       mux.notify(mux::MuxNotification::WebOpen {
           pane_id,
           url: url.clone(),
           browser_id: browser_id.clone(),
           profile,
           incognito,
       });
   }
   ```

5. Update `MuxNotification::WebOpen` (`mux/src/lib.rs`):

   ```rust
   WebOpen {
       pane_id: PaneId,
       url: String,
       browser_id: String,
       profile: Option<String>,  // None for incognito
       incognito: bool,
   },
   ```

6. Update `handle_web_open` (`termwindow/mod.rs`):

   ```rust
   pub fn handle_web_open(
       &self,
       pane_id: PaneId,
       url: String,
       browser_id: String,
       profile: Option<String>,
       incognito: bool,
   ) {
       // Pass to BrowserState::new
   }
   ```

7. Update `BrowserState::new` to create profile directory and set cache_path
   (`cef_browser/mod.rs`):

   ```rust
   pub fn new(
       // ... existing params ...
       browser_id: String,
       profile: Option<String>,
       incognito: bool,
   ) -> anyhow::Result<Self> {
       // Compute cache_path
       let cache_path = if incognito {
           None
       } else {
           let profile_name = profile.as_deref().unwrap_or("default");
           let path = dirs::config_dir()
               .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
               .join("termsurf")
               .join("profiles")
               .join(profile_name);

           // Create directory if it doesn't exist
           std::fs::create_dir_all(&path)?;

           Some(path)
       };

       // Create request context with cache_path
       let mut context_settings = RequestContextSettings::default();
       if let Some(ref path) = cache_path {
           context_settings.cache_path = path.to_string_lossy().to_string().into();
       }

       let mut context = cef::request_context_create_context(
           Some(&context_settings),
           Some(&mut CefRequestContextHandlerBuilder::build()),
       );

       // ... rest of browser creation ...
   }
   ```

**Profile validation rules:**

- Lowercase alphanumeric only (`a-z`, `0-9`)
- Must start with a letter
- Valid: `default`, `myproject`, `test1`
- Invalid: `MyProject`, `123test`, `my-project`

**Files to modify:**

| File                                     | Change                              |
| ---------------------------------------- | ----------------------------------- |
| `wezterm/src/cli/web.rs`                 | Add --profile and --incognito flags |
| `wezterm-gui/src/termsurf_socket/mod.rs` | Extract and validate profile        |
| `mux/src/lib.rs`                         | Add profile/incognito to WebOpen    |
| `wezterm-gui/src/termwindow/mod.rs`      | Pass profile to BrowserState        |
| `wezterm-gui/src/cef_browser/mod.rs`     | Create profile dir, set cache_path  |
| `wezterm-mux-server-impl/sessionhandler` | Update RPC path (for completeness)  |

**Dependencies:** Experiment 3 must be complete.
