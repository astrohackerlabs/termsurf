+++
status = "closed"
opened = "2026-01-22"
closed = "2026-01-22"
+++

# Web Command

The `web` command provides CLI access to TermSurf's browser functionality.

## Product Requirements

### Commands

The `web` command supports the following subcommands:

| Command                                        | Description                      |
| ---------------------------------------------- | -------------------------------- |
| `web open <url> [--profile N \| --incognito]`  | Open a URL in a browser pane     |
| `web file <path> [--profile N \| --incognito]` | Open a local file in the browser |
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

Browsers can use numbered profiles to isolate cookies, localStorage, and other
session data. Profiles follow Chrome's internal naming convention and are stored
in `~/.config/termsurf/cef/`.

> **Note:** Multi-profile support requires separate CEF processes due to Chrome
> runtime limitations. See [profile.md](profile.md) for the full research and
> architecture details.

```bash
# Default profile (profile 0)
termsurf cli web open https://example.com
termsurf cli web open https://example.com --profile 0

# Profile 1
termsurf cli web open https://example.com --profile 1

# Profile 5 (numbers don't need to be sequential)
termsurf cli web open https://example.com --profile 5
```

**Profile storage:**

| CLI Flag      | Directory Name | Storage Path                        |
| ------------- | -------------- | ----------------------------------- |
| (none)        | `Default`      | `~/.config/termsurf/cef/Default/`   |
| `--profile 0` | `Default`      | `~/.config/termsurf/cef/Default/`   |
| `--profile 1` | `Profile 1`    | `~/.config/termsurf/cef/Profile 1/` |
| `--profile N` | `Profile N`    | `~/.config/termsurf/cef/Profile N/` |

Use `--incognito` for in-memory only mode where no data persists:

```bash
termsurf cli web open https://example.com --incognito
```

The `--profile` and `--incognito` flags are mutually exclusive.

### Invocation

Phase 1: Subcommand of `termsurf cli`:

```bash
termsurf cli web open https://example.com
termsurf cli web open https://example.com --profile 1
termsurf cli web file ./index.html
termsurf cli web close
```

Phase 2: Standalone `web` command:

```bash
web open https://example.com
web open https://example.com --profile 1
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

**Status:** Failed

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

**Result:**

The implementation was completed and the profile directories are created (e.g.,
`~/.config/termsurf/profiles/default/`), but no data is being persisted to them.
After logging into Google and closing the browser, reopening shows the user is
not logged in. The profile directory remains empty.

This suggests that setting `cache_path` in `RequestContextSettings` alone is not
sufficient for CEF to persist session data. Further investigation is needed to
determine why CEF is not writing to the profile directory.

---

### Experiment 5: Fix Profile Path Hierarchy

**Status:** Pending

**Goal:** Fix browser profile persistence by correcting the path hierarchy and
enabling session cookie persistence.

**Root Cause Analysis:**

CEF requires per-browser `RequestContextSettings.cache_path` to be under the
global `Settings.root_cache_path`. Our current paths violate this:

| Setting                            | Current Value                            |
| ---------------------------------- | ---------------------------------------- |
| Global `root_cache_path` (main.rs) | `~/Library/Caches/termsurf/cef/` (macOS) |
| Profile `cache_path` (cef_browser) | `~/.config/termsurf/profiles/<name>/`    |

Since `~/.config/termsurf/profiles/` is not under
`~/Library/Caches/termsurf/cef/`, CEF silently ignores our cache_path setting.

Additionally, `persist_session_cookies` was not set to `1`, so even if the path
were correct, session cookies would not be persisted.

**Plan:**

1. Change global `root_cache_path` to `~/.config/termsurf/cef/` (`main.rs`):

   ```rust
   // Before:
   let cef_cache = config::CACHE_DIR.join("cef");
   // = ~/Library/Caches/termsurf/cef/

   // After:
   let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
   let cef_cache = PathBuf::from(format!("{}/.config/termsurf/cef", home));
   ```

   This makes `root_cache_path` = `~/.config/termsurf/cef/`, which encompasses
   our profile paths at `~/.config/termsurf/cef/profiles/<name>/`.

2. Update profile path to be under the new root (`cef_browser/mod.rs`):

   ```rust
   // Before (Experiment 4):
   let profile_dir = format!("{}/.config/termsurf/profiles/{}", home, profile_name);

   // After:
   let profile_dir = format!("{}/.config/termsurf/cef/profiles/{}", home, profile_name);
   ```

3. Enable session cookie persistence (`cef_browser/mod.rs`):

   ```rust
   let request_context_settings = RequestContextSettings {
       cache_path: cache_path.as_str().into(),
       persist_session_cookies: 1,  // Add this line
       ..Default::default()
   };
   ```

4. Build and test:

   ```bash
   ./scripts/build-debug.sh --open
   termsurf cli web open https://google.com
   # Log in to Google
   # Close browser (Ctrl+W or close pane)
   termsurf cli web open https://google.com
   # Verify still logged in
   ```

5. Verify profile directory contains data:

   ```bash
   ls -la ~/.config/termsurf/cef/profiles/default/
   # Should see: Cookies, Visited Links, Local Storage/, etc.
   ```

**Final directory structure:**

```
~/.config/termsurf/
└── cef/
    └── profiles/
        ├── default/
        │   ├── Cookies
        │   ├── Visited Links
        │   ├── Local Storage/
        │   ├── IndexedDB/
        │   ├── Cache/
        │   └── Preferences
        └── myproject/
            └── ...
```

**What CEF stores automatically (once path is correct):**

| Data Type     | Location in Profile Directory |
| ------------- | ----------------------------- |
| Cookies       | `Cookies` (SQLite database)   |
| Visited links | `Visited Links` (file)        |
| localStorage  | `Local Storage/` (directory)  |
| IndexedDB     | `IndexedDB/` (directory)      |
| HTTP cache    | `Cache/` (directory)          |
| Preferences   | `Preferences` (JSON file)     |

No additional flags are needed for these—they persist automatically when
`cache_path` is valid and under `root_cache_path`.

**Files to modify:**

| File                                 | Change                                   |
| ------------------------------------ | ---------------------------------------- |
| `wezterm-gui/src/main.rs`            | Change root_cache_path to ~/.config      |
| `wezterm-gui/src/cef_browser/mod.rs` | Update profile path, add persist_session |

**Dependencies:** Experiment 4 must be complete (profile infrastructure exists).

---

### Experiment 6: Chrome-Native Profile Naming

**Status:** Failed

**Goal:** Enable multi-profile support by using Chrome's native profile naming
convention (`Default`, `Profile 1`, `Profile 2`, etc.) instead of custom names.

**Background:**

Experiment 5 showed that using the global context successfully persists data to
`~/.config/termsurf/cef/Default/`. However, attempts to create custom
RequestContexts with arbitrary profile paths failed with:

```
ERROR:cef/libcef/browser/chrome/chrome_browser_context.cc:115]
Cannot create profile at path /Users/ryan/.config/termsurf/cef/profiles/default
```

This error occurs because CEF's Chrome-based backend requires profiles to follow
Chrome's internal naming conventions. Chrome profiles use:

- `Default` for the primary profile
- `Profile 1`, `Profile 2`, etc. for additional profiles

These are **directory names**, not user-facing names. Chrome stores the
user-facing "display name" in a `Preferences` file inside each profile
directory.

**Key findings:**

1. Profile directory names must be `Default`, `Profile 1`, `Profile 2`, etc.
2. Profile numbers do NOT need to be sequential (can have `Profile 5` without
   `Profile 1-4`)
3. CEF rejects arbitrary names like `profiles/default` or `myproject`

**Plan:**

1. Change CLI from `--profile <name>` to `--profile <number>` (`web.rs`):

   ```rust
   #[derive(Debug, Parser, Clone)]
   pub struct WebOpen {
       /// The URL to open
       url: String,

       /// Browser profile number (0 = Default, 1+ = Profile N)
       #[arg(long)]
       profile: Option<u32>,

       /// Use incognito mode (in-memory only, no persistence)
       #[arg(long, conflicts_with = "profile")]
       incognito: bool,
   }
   ```

2. Map profile number to Chrome directory name:

   ```rust
   fn profile_dir_name(profile_num: Option<u32>) -> String {
       match profile_num {
           None | Some(0) => "Default".to_string(),
           Some(n) => format!("Profile {}", n),
       }
   }
   ```

3. Update socket request to send profile number (`web.rs`):

   ```rust
   let request = TermsurfRequest {
       id: request_id,
       action: "open".to_string(),
       pane_id: Some(pane_id),
       data: Some(serde_json::json!({
           "url": self.url,
           "profile": self.profile,  // Option<u32> or null
           "incognito": self.incognito,
       })),
   };
   ```

4. Update socket server to parse profile number (`termsurf_socket/mod.rs`):

   ```rust
   let profile_num: Option<u32> = request.data.as_ref()
       .and_then(|d| d.get("profile"))
       .and_then(|v| v.as_u64())
       .map(|n| n as u32);
   ```

5. Update `MuxNotification::WebOpen` to use profile number (`mux/src/lib.rs`):

   ```rust
   WebOpen {
       pane_id: PaneId,
       url: String,
       browser_id: String,
       profile: Option<u32>,  // Changed from Option<String>
       incognito: bool,
   },
   ```

6. Update `BrowserState::new` to use Chrome paths (`cef_browser/mod.rs`):

   ```rust
   pub fn new(
       // ... existing params ...
       profile: Option<u32>,
       incognito: bool,
   ) -> anyhow::Result<Self> {
       // Compute cache_path using Chrome naming convention
       let cache_path = if incognito {
           String::new()  // Empty = in-memory only
       } else {
           let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
           let profile_dir = match profile {
               None | Some(0) => "Default".to_string(),
               Some(n) => format!("Profile {}", n),
           };
           format!("{}/.config/termsurf/cef/{}", home, profile_dir)
       };

       // Create request context settings
       let request_context_settings = if cache_path.is_empty() {
           RequestContextSettings::default()  // Incognito
       } else {
           RequestContextSettings {
               cache_path: CefString::from(cache_path.as_str()),
               persist_session_cookies: 1,
               ..Default::default()
           }
       };

       // Create custom RequestContext (should work with Chrome naming)
       let request_context = cef::request_context_create_context(
           Some(&request_context_settings),
           None,
       );

       // Create browser with custom context
       let browser = cef::browser_host_create_browser_sync(
           Some(&window_info),
           Some(&mut client),
           Some(&url.into()),
           Some(&browser_settings),
           None,
           request_context.as_ref(),  // Use custom context
       );
   }
   ```

7. Remove profile name validation (no longer needed - numbers are
   self-validating).

**CLI Usage:**

```bash
# Default profile (stored in ~/.config/termsurf/cef/Default/)
termsurf cli web open https://example.com
termsurf cli web open https://example.com --profile 0

# Profile 1 (stored in ~/.config/termsurf/cef/Profile 1/)
termsurf cli web open https://example.com --profile 1

# Profile 5 (works even without Profile 1-4)
termsurf cli web open https://example.com --profile 5

# Incognito (in-memory only)
termsurf cli web open https://example.com --incognito
```

**Directory structure:**

```
~/.config/termsurf/cef/
├── Default/
│   ├── Cookies
│   ├── Visited Links
│   ├── Local Storage/
│   └── ...
├── Profile 1/
│   └── ...
├── Profile 5/
│   └── ...
└── (CEF shared files)
```

**Files to modify:**

| File                                     | Change                                    |
| ---------------------------------------- | ----------------------------------------- |
| `wezterm/src/cli/web.rs`                 | Change --profile to u32, remove validator |
| `wezterm-gui/src/termsurf_socket/mod.rs` | Parse profile as u32, remove validator    |
| `mux/src/lib.rs`                         | Change profile to Option<u32>             |
| `wezterm-gui/src/cef_browser/mod.rs`     | Use Chrome naming, re-enable RequestCtx   |
| `docs/209-web.md`                        | Update profile documentation              |

**Test plan:**

1. Build and run: `./scripts/build-debug.sh --open`
2. Test default profile:
   ```bash
   termsurf cli web open https://google.com
   # Log in to Google, close browser
   termsurf cli web open https://google.com
   # Verify still logged in
   ```
3. Test Profile 1:
   ```bash
   termsurf cli web open https://google.com --profile 1
   # Should NOT be logged in (separate profile)
   # Log in, close browser
   termsurf cli web open https://google.com --profile 1
   # Verify still logged in
   ```
4. Verify directory structure:
   ```bash
   ls -la ~/.config/termsurf/cef/
   # Should see: Default/, Profile 1/
   ```

**Dependencies:** Experiment 5 progress (global context working with
persistence).

**Result:**

The implementation was completed, but custom profiles still fail. Observed
behavior:

| Profile Flag  | cache_path                         | Result                 |
| ------------- | ---------------------------------- | ---------------------- |
| (none)        | `~/.config/termsurf/cef/Default`   | Works                  |
| `--profile 0` | `~/.config/termsurf/cef/Default`   | Works                  |
| `--profile 1` | `~/.config/termsurf/cef/Profile 1` | Browser creation fails |
| `--profile 5` | `~/.config/termsurf/cef/Profile 5` | Browser creation fails |
| `--incognito` | (empty)                            | Works                  |

Log output for `--profile 1`:

```
[CEF] Using profile directory: /Users/ryan/.config/termsurf/cef/Profile 1 (profile: Some(1))
[CEF] RequestContext created for profile Some(1), cache_path: /Users/ryan/.config/termsurf/cef/Profile 1
ERROR [CEF] Failed to create browser for pane 0: Failed to create CEF browser
```

The `request_context_create_context()` call succeeds (returns `Some`), but
`browser_host_create_browser_sync()` returns `None` for any cache_path that
isn't `Default`.

**Conclusion:**

Chrome's profile naming convention (`Default`, `Profile 1`, `Profile 2`) is for
Chrome's internal use only. Simply creating a RequestContext with a cache_path
matching these names is not sufficient - Chrome's profile management system must
internally recognize and register the profile. CEF does not expose an API to
create new profiles programmatically.

**What works:**

- Global context → automatically creates/uses `Default` profile
- Custom context with empty cache_path → incognito mode

**What doesn't work:**

- Custom context with ANY non-empty cache_path (including Chrome-style names)

**Research: Why Custom Profiles Fail**

Further investigation revealed this is **documented behavior in CEF's Chrome
runtime**. From the
[official CEF documentation](https://cef-builds.spotifycdn.com/docs/120.2/structcef__settings__t.html):

> "When using the Chrome runtime any child directory value will be ignored and
> the 'default' profile (also a child directory) will be used instead."

This is not a bug or misconfiguration - CEF's Chrome runtime intentionally
ignores custom `cache_path` values.

**Why this happens:**

1. **CEF 126+ uses Chrome Bootstrap by default** - This brought the full Chrome
   profile management system
2. **Chrome's profile system** expects profiles to be managed internally, not
   via arbitrary directory paths
3. **The Alloy runtime** (which supported custom `cache_path`) is deprecated and
   will be removed

**Confirmed behavior:**

| Approach                   | Result                       |
| -------------------------- | ---------------------------- |
| `cache_path = "Default"`   | Works (Chrome recognizes it) |
| `cache_path = "Profile 1"` | Ignored → uses "Default"     |
| `cache_path = ""` (empty)  | Works (incognito mode)       |
| Any custom path under root | Ignored → uses "Default"     |

**Sources:**

- [CEF Settings Documentation](https://cef-builds.spotifycdn.com/docs/120.2/structcef__settings__t.html)
- [CefSharp Issue #4961](https://github.com/cefsharp/CefSharp/issues/4961) -
  Same problem, unresolved
- [CEF Forum - Chrome vs Alloy Runtime](https://www.magpcss.org/ceforum/viewtopic.php?f=17&t=18750)
- [CefSharp Discussion #4899](https://github.com/cefsharp/CefSharp/discussions/4899) -
  CachePath not working with ChromeRuntime

**Remaining options for multi-profile support:**

1. **Multiple `root_cache_path` values** - Each profile needs its own CEF
   instance with a different root directory (e.g.,
   `~/.config/termsurf/cef/profile-1/`, `~/.config/termsurf/cef/profile-2/`).
   Each would have its own `Default` subdirectory. This is the only way to have
   truly isolated profiles with Chrome runtime.

2. **Use Alloy runtime** (`chrome_runtime = false`) - This would support custom
   `cache_path`, but Alloy is deprecated and being removed. Not a viable
   long-term solution.

3. **Accept single profile** - Use the `Default` profile for all browsers, offer
   `--incognito` for isolation when needed.

See [profile.md](profile.md) for the complete research findings and recommended
architecture for multi-profile support.

---

### Experiment 7: CEF in Separate Process

**Status:** Failed

**Goal:** Move CEF into a separate process to enable future multi-profile
support (one CEF process per profile). This experiment focuses on the
architectural change with a single CEF process; multi-profile will be a
follow-up.

**Background:**

Currently, CEF is initialized in the main TermSurf process:

```
termsurf-gui (Main Process)
├── WezTerm terminal
├── CEF browser process (initialized here)
├── Creates browsers in OSR mode
└── Composites textures
     │
     └── CEF spawns internally:
          └── TermSurf Helper (renderer/GPU subprocesses)
```

This works, but CEF can only be initialized once per process with one
`root_cache_path`. To support multiple profiles (each needing its own
`root_cache_path`), we need CEF in a separate process.

**Target architecture:**

```
termsurf-gui (Main Process)
├── WezTerm terminal
├── NO CEF initialization
├── Receives texture handles via IPC
└── Composites textures
     │
     └── Spawns via IPC:
          └── termsurf-browser (CEF Browser Process)
               ├── root_cache_path = ~/.config/termsurf/cef/
               ├── Creates browsers in OSR mode
               ├── Sends texture handles + events via IPC
               └── CEF spawns internally:
                    └── TermSurf Helper (renderer/GPU)
```

**IPC Protocol:**

Communication via Unix domain socket (or named pipe on Windows). Messages are
newline-delimited JSON.

**Main → CEF Process (Commands & Input):**

```jsonc
// Browser commands
{"type": "create_browser", "pane_id": 1, "url": "https://example.com"}
{"type": "close_browser", "pane_id": 1}
{"type": "navigate", "pane_id": 1, "url": "https://other.com"}
{"type": "go_back", "pane_id": 1}
{"type": "go_forward", "pane_id": 1}
{"type": "reload", "pane_id": 1}

// Input events
{"type": "key_event", "pane_id": 1, "event_type": "keydown", "modifiers": 2, "windows_key_code": 65, "native_key_code": 0, "character": 97, "unmodified_character": 97}
{"type": "key_event", "pane_id": 1, "event_type": "keyup", "modifiers": 0, "windows_key_code": 65, "native_key_code": 0, "character": 97, "unmodified_character": 97}
{"type": "key_event", "pane_id": 1, "event_type": "char", "modifiers": 0, "windows_key_code": 65, "native_key_code": 0, "character": 97, "unmodified_character": 97}

{"type": "mouse_move", "pane_id": 1, "x": 100, "y": 200, "modifiers": 0, "mouse_leave": false}
{"type": "mouse_click", "pane_id": 1, "x": 100, "y": 200, "modifiers": 0, "button": "left", "mouse_up": false, "click_count": 1}
{"type": "mouse_wheel", "pane_id": 1, "x": 100, "y": 200, "modifiers": 0, "delta_x": 0, "delta_y": -120}

{"type": "resize", "pane_id": 1, "width": 800, "height": 600}
{"type": "focus", "pane_id": 1, "focused": true}
```

**CEF → Main Process (Events):**

```jsonc
// Texture updates (platform-specific handle)
{"type": "texture_ready", "pane_id": 1, "iosurface_id": 12345, "width": 800, "height": 600, "format": "bgra8"}  // macOS
{"type": "texture_ready", "pane_id": 1, "dmabuf_fd": 5, "width": 800, "height": 600, "format": "bgra8"}  // Linux (fd passed via SCM_RIGHTS)
{"type": "texture_ready", "pane_id": 1, "shared_handle": 12345, "width": 800, "height": 600, "format": "bgra8"}  // Windows

// Browser events
{"type": "browser_created", "pane_id": 1}
{"type": "browser_closed", "pane_id": 1}
{"type": "console", "pane_id": 1, "level": "log", "message": "Hello", "source": "https://example.com/app.js", "line": 42}
{"type": "load_start", "pane_id": 1, "url": "https://example.com"}
{"type": "load_end", "pane_id": 1, "url": "https://example.com", "status_code": 200}
{"type": "title_changed", "pane_id": 1, "title": "Example Site"}
{"type": "url_changed", "pane_id": 1, "url": "https://example.com/page"}
{"type": "cursor_changed", "pane_id": 1, "cursor": "pointer"}
```

**Plan:**

1. Create `termsurf-browser` binary (`wezterm-gui/src/bin/termsurf-browser.rs`):

   ```rust
   // New CEF host process
   fn main() {
       // 1. Parse args (socket path passed by parent)
       let socket_path = std::env::args().nth(1).expect("socket path required");

       // 2. Initialize CEF (moved from main.rs)
       init_cef()?;

       // 3. Connect to parent via Unix socket
       let socket = UnixStream::connect(&socket_path)?;

       // 4. Message loop: read commands, send events
       loop {
           // Pump CEF messages
           cef::do_message_loop_work();

           // Process incoming commands (non-blocking)
           if let Some(cmd) = read_command(&socket) {
               handle_command(cmd);
           }

           // Send pending events
           send_pending_events(&socket);
       }
   }
   ```

2. Move CEF initialization from `main.rs` to `termsurf-browser`:

   - Move `init_cef()` function
   - Move `BrowserState` and handlers
   - Keep texture import code in main process (receives handles via IPC)

3. Update main process to spawn and communicate with CEF process:

   ```rust
   // In termsurf-gui main.rs or new module
   pub struct CefProcess {
       child: std::process::Child,
       socket: UnixStream,
       texture_cache: HashMap<PaneId, wgpu::Texture>,
   }

   impl CefProcess {
       pub fn spawn() -> Result<Self> {
           // Create socket pair
           let socket_path = format!("/tmp/termsurf-cef-{}.sock", std::process::id());

           // Spawn termsurf-browser with socket path
           let child = Command::new("termsurf-browser")
               .arg(&socket_path)
               .spawn()?;

           // Wait for connection
           let listener = UnixListener::bind(&socket_path)?;
           let (socket, _) = listener.accept()?;

           Ok(Self { child, socket, texture_cache: HashMap::new() })
       }

       pub fn create_browser(&mut self, pane_id: PaneId, url: &str) {
           self.send(json!({"type": "create_browser", "pane_id": pane_id, "url": url}));
       }

       pub fn send_key_event(&mut self, pane_id: PaneId, event: &CefKeyEvent) {
           self.send(json!({
               "type": "key_event",
               "pane_id": pane_id,
               "event_type": event.event_type,
               // ... other fields
           }));
       }

       pub fn poll_events(&mut self) -> Vec<CefEvent> {
           // Non-blocking read of events from CEF process
       }
   }
   ```

4. Handle texture imports in main process:

   ```rust
   fn handle_texture_ready(&mut self, event: &TextureReadyEvent, device: &wgpu::Device) {
       // Create AcceleratedPaintInfo-like struct from IPC data
       let texture = match std::env::consts::OS {
           "macos" => import_iosurface(event.iosurface_id, event.width, event.height, device),
           "linux" => import_dmabuf(event.dmabuf_fd, event.width, event.height, device),
           "windows" => import_d3d11(event.shared_handle, event.width, event.height, device),
           _ => panic!("unsupported platform"),
       };
       self.texture_cache.insert(event.pane_id, texture);
   }
   ```

5. Update `BrowserState` to be IPC-based:

   Currently `BrowserState` holds a CEF `Browser` directly. Change it to hold a
   `pane_id` and reference to the `CefProcess`:

   ```rust
   // Before (in-process)
   pub struct BrowserState {
       browser: Option<Browser>,
       // ...
   }

   // After (cross-process)
   pub struct BrowserState {
       pane_id: PaneId,
       cef_process: Arc<Mutex<CefProcess>>,
       // ...
   }

   impl BrowserState {
       pub fn send_key_event(&self, event: &CefKeyEvent) {
           self.cef_process.lock().unwrap()
               .send_key_event(self.pane_id, event);
       }
   }
   ```

6. Update app bundling to include `termsurf-browser`:

   ```
   TermSurf.app/
   └── Contents/
       ├── MacOS/
       │   ├── termsurf-gui          # Main process
       │   └── termsurf-browser      # CEF browser process (NEW)
       └── Frameworks/
           ├── Chromium Embedded Framework.framework/
           └── TermSurf Helper.app/  # CEF's internal subprocesses
   ```

**Texture handle passing by platform:**

| Platform | Handle Type       | How to Pass                        |
| -------- | ----------------- | ---------------------------------- |
| macOS    | IOSurfaceID (u32) | Serialize as integer in JSON       |
| Linux    | DMA-BUF fd        | Send via SCM_RIGHTS ancillary data |
| Windows  | HANDLE            | DuplicateHandle + serialize as u64 |

**Files to create:**

| File                                      | Description                |
| ----------------------------------------- | -------------------------- |
| `wezterm-gui/src/bin/termsurf-browser.rs` | New CEF host process       |
| `wezterm-gui/src/cef_process/mod.rs`      | IPC client in main process |
| `wezterm-gui/src/cef_process/protocol.rs` | Shared message types       |

**Files to modify:**

| File                                 | Change                                |
| ------------------------------------ | ------------------------------------- |
| `wezterm-gui/src/main.rs`            | Remove CEF init, spawn CefProcess     |
| `wezterm-gui/src/cef_browser/mod.rs` | Change to IPC-based BrowserState      |
| `wezterm-gui/src/termwindow/mod.rs`  | Use CefProcess for browser operations |
| `scripts/build-debug.sh`             | Build and bundle termsurf-browser     |

**Test plan:**

1. Build: `./scripts/build-debug.sh --open`
2. Open browser: `termsurf cli web open https://example.com`
3. Verify:
   - Browser renders correctly
   - Keyboard input works
   - Mouse input works (clicks, scroll)
   - Console output streams to CLI
   - Browser closes cleanly
4. Check process tree:
   ```bash
   pstree -p $(pgrep termsurf-gui)
   # Should show: termsurf-gui -> termsurf-browser -> TermSurf Helper (multiple)
   ```

**Future work (not in this experiment):**

- Multiple CEF processes for multi-profile support
- Process crash recovery and restart
- Lazy spawning (only when first browser is requested)

**Result: Failed**

The implementation was completed but revealed fundamental architectural flaws
that make this approach unsuitable.

**What was built:**

- `termsurf-browser` binary with full CEF initialization
- `cef_process/` module for spawning and IPC communication
- `cef_browser_ipc/` module for IPC-based BrowserState
- JSON protocol for commands (create, resize, input, clipboard, navigation)
- IOSurface texture sharing via surface ID lookup
- Event polling in render loop

**Critical bugs discovered:**

1. **Multi-browser event loss:** With multiple browser panes, the first
   `BrowserState` to call `poll_events()` drains ALL events from the shared
   socket, then filters to only its `pane_id`. Events for other browsers are
   silently dropped. Only the first browser in the HashMap iteration receives
   texture updates; all others show blank content.

2. **Architecture doesn't solve the original problem:** CEF can only have one
   profile per process. A shared `termsurf-browser` process cannot support
   multiple profiles anyway—we would need one subprocess per profile, not one
   shared subprocess. The "shared CEF process" design was fundamentally wrong
   for the multi-profile goal.

3. **Excessive complexity:** The IPC layer required:
   - Duplicated protocol types in two places (prone to desync)
   - Event routing by `pane_id` (bug-prone, as proven above)
   - Texture handle serialization and lookup
   - Process lifecycle management
   - Polling integration with render loop

4. **Wrong abstraction:** We created a hidden daemon when the natural entry
   point (`termsurf web` command) should have been the browser process itself.

**Lessons learned:**

1. **One process per profile is required** - CEF initializes with a single
   `root_cache_path`. Multiple profiles require multiple CEF processes, each
   with its own root directory.

2. **The CLI command should BE the browser process** - When the user types
   `termsurf web <url>`, that process should own the browser lifecycle, not
   send a message to a hidden daemon. This matches the user's mental model.

3. **Event routing across all browsers is fragile** - Each browser should have
   its own dedicated communication channel, not share a multiplexed socket
   where events must be filtered by `pane_id`.

4. **Start with the right architecture** - The complexity of retrofitting a
   multi-process model onto the existing single-process design was greater than
   the value gained. The code changes were extensive and introduced subtle bugs.

**Decision:**

Abandon Experiment 7. Do not commit the code changes. Restore ts2 to its
pre-experiment working state (single-browser support with in-process CEF).

**Future direction: TermSurf 3.0**

Start fresh with a new architecture where:

```
┌─────────────────┐
│  termsurf-gui   │  (window, renders textures, forwards input)
└────────┬────────┘
         │ IPC per profile-process
         ▼
┌─────────────────────────────────────────────────────────┐
│                    termsurf web                          │
│                    (coordinator)                         │
│                                                          │
│  Spawns/manages one subprocess per profile:             │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Profile A    │  │ Profile B    │  │ Incognito    │  │
│  │ (CEF proc)   │  │ (CEF proc)   │  │ (CEF proc)   │  │
│  │ tab1, tab2   │  │ tab3         │  │ tab4         │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
```

Key differences from Experiment 7:

- `termsurf web` command is the browser coordinator, not a hidden daemon
- One CEF process per profile (enables true multi-profile support)
- Each profile process handles only its own tabs (no cross-browser event
  routing)
- Point-to-point IPC per profile, not multiplexed by `pane_id`
- Natural lifecycle: profile process lives while any tab uses it

This architecture will be implemented as TermSurf 3.0, a fresh fork of WezTerm
+ cef-rs with the correct architecture from day one. ts2 will be preserved as
reference code for CEF integration patterns (texture import, input handling,
etc.).
