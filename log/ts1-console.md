# Console Bridging and JavaScript API (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x (Ghostty + WKWebView).
> TermSurf 2.0 will use CEF's native console message API instead of JavaScript injection.
> See [cef-rs.md](cef-rs.md) for 2.0 progress.

This document describes how TermSurf bridges browser console output to the
terminal, and the optional JavaScript API for automation and testing.

## Overview

When you open a webview in TermSurf, the browser's console output is captured
and routed to the terminal. This enables powerful Unix-style composition:

- Pipe console output to other tools
- Filter logs with grep, jq, or other utilities
- Feed output to AI tools for debugging assistance
- Capture errors separately from regular output
- Integrate browser-based tests into CI/CD pipelines

## Console Output Mapping

Browser console methods map to terminal streams:

| Browser Method      | Terminal Stream | Notes                      |
| ------------------- | --------------- | -------------------------- |
| `console.log()`     | stdout          | General output             |
| `console.info()`    | stdout          | Informational messages     |
| `console.warn()`    | stderr          | Warnings                   |
| `console.error()`   | stderr          | Errors                     |
| Uncaught exception  | stderr          | Prefixed with "Uncaught:"  |
| Unhandled rejection | stderr          | Prefixed with "Unhandled:" |

### Object Serialization

Objects are serialized using `JSON.stringify()` for readable output:

```javascript
console.log({ user: "alice", count: 42 });
// Output: {"user":"alice","count":42}

console.log("User:", { name: "bob" }, "logged in");
// Output: User: {"name":"bob"} logged in
```

## Usage Examples

### Basic Usage

```bash
termsurf open http://localhost:3000
# Console output appears in terminal as it happens
```

### Separate Error Capture

```bash
termsurf open http://localhost:3000 2>errors.log
# stdout shows in terminal, errors saved to file
```

### Filter Output

```bash
termsurf open http://localhost:3000 | grep "API"
# Only show lines containing "API"
```

### Feed to AI for Debugging

```bash
termsurf open http://localhost:3000 2>&1 | ai-debug
# Send all output to an AI debugging tool
```

### JSON Log Processing

```bash
termsurf open http://localhost:3000 | jq 'select(.level == "error")'
# Parse and filter JSON-structured logs
```

## Optional JavaScript API (`--js-api` flag)

By default, webpages have no special access to TermSurf functionality. With the
`--js-api` flag, a `window.termsurf` object is injected that provides additional
capabilities.

### Security Rationale

The API is opt-in because:

1. **Principle of least privilege** - Normal browsing doesn't need special APIs
2. **Prevent surprises** - Random websites can't close your browser session
3. **Clear intent** - Using `--js-api` signals you're running trusted code

The actual security risk is minimal (a page could only close itself), but opt-in
ensures predictable behavior and clear user intent.

### Enabling the API

```bash
termsurf open http://localhost:3000 --js-api
```

### Available API

#### `window.termsurf.webviewId`

The unique identifier for this webview instance. Useful for debugging.

```javascript
console.log(window.termsurf.webviewId);
// Output: wv-a1b2c3d4
```

#### `window.termsurf.exit(code)`

Exit the webview with the specified exit code. The CLI process exits with this
code. Exit code is clamped to 0-255.

```javascript
// Exit successfully
window.termsurf.exit(0);

// Exit with error
window.termsurf.exit(1);

// Exit code defaults to 0 if not provided or not a number
window.termsurf.exit();
```

## Testing and Automation Use Cases

The combination of console bridging and `--js-api` enables powerful testing
workflows.

### Running Tests with Exit Codes

```javascript
// In your test page
async function runTests() {
  const results = await myTestRunner.run();

  results.failures.forEach(f => console.error("FAIL:", f));
  results.passes.forEach(p => console.log("PASS:", p));

  console.log(`${results.passes.length} passed, ${results.failures.length} failed`);

  window.termsurf.exit(results.failures.length > 0 ? 1 : 0);
}

runTests();
```

```bash
termsurf open http://localhost:3000/test --js-api
echo "Exit code: $?"
```

### CI/CD Integration

```bash
#!/bin/bash
# run-browser-tests.sh

termsurf open http://localhost:3000/test --js-api > test-output.log 2>&1
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "Tests failed!"
  cat test-output.log
  exit 1
fi

echo "All tests passed"
```

### Simple Health Checks

```javascript
// health-check.html
fetch('/api/health')
  .then(r => r.ok ? window.termsurf.exit(0) : window.termsurf.exit(1))
  .catch(() => window.termsurf.exit(1));
```

```bash
termsurf open http://localhost:3000/health-check.html --js-api
```

## Future API Extensions

These are ideas for expanding the `window.termsurf` API. None are currently
implemented.

### Environment Access

```javascript
// Access environment variables (read-only)
const apiKey = window.termsurf.env.API_KEY;
const nodeEnv = window.termsurf.env.NODE_ENV;
```

### Direct Stream Writing

```javascript
// Write directly to stdout/stderr with more control
window.termsurf.stdout.write("Progress: 50%\r");  // Overwrite line
window.termsurf.stderr.write("Warning: slow query\n");
```

### Pane Information

```javascript
// Get information about the hosting pane
const paneId = window.termsurf.paneId;
const sessionId = window.termsurf.sessionId;
```

### Event Subscription

```javascript
// React to terminal events
window.termsurf.on('resize', ({ cols, rows }) => {
  console.log(`Terminal resized to ${cols}x${rows}`);
});

window.termsurf.on('focus', () => {
  console.log('Pane focused');
});
```

## Comparison with Tauri

[Tauri](https://tauri.app/) is a framework for building desktop applications
using web technologies. It gives webapps access to the host operating system
through a secure IPC bridge. TermSurf could offer similar capabilities in a
terminal-native context.

### What Tauri Provides

Tauri apps can:

- Read and write files on the filesystem
- Execute shell commands
- Access system information (OS, CPU, memory)
- Show native dialogs and notifications
- Manage windows and menus
- Access clipboard, global shortcuts, and more

All of this is mediated through a permission system where the app declares what
capabilities it needs.

### How TermSurf Could Offer Similar Capabilities

TermSurf is uniquely positioned because it already lives in the terminal:

```javascript
// Potential future API (not implemented)

// Execute shell command and get output
const result = await window.termsurf.exec('ls -la');
console.log(result.stdout);

// Read a file
const content = await window.termsurf.fs.readFile('/path/to/file');

// Write a file
await window.termsurf.fs.writeFile('/tmp/output.json', JSON.stringify(data));

// Get system info
const info = window.termsurf.system;
console.log(info.platform, info.arch, info.hostname);
```

### Key Differences from Tauri

| Aspect           | Tauri                  | TermSurf (potential)          |
| ---------------- | ---------------------- | ----------------------------- |
| **Context**      | Standalone desktop app | Inside terminal emulator      |
| **Packaging**    | Bundled application    | Opens any URL                 |
| **Use case**     | Ship desktop apps      | Dev tools, local automation   |
| **Shell access** | Via command API        | Native (already in terminal)  |
| **Distribution** | App stores, installers | Just a URL + `web` command    |

### Security Model for OS Access

If TermSurf implements OS access, it would need a tiered permission system:

1. **No flag** - No API, just console bridging
2. **`--js-api`** - Basic API (exit, webviewId)
3. **`--js-api=full`** or **`--trusted`** - Full OS access (filesystem, exec)

The full access tier would only be used for:

- Local development tools
- Trusted internal tools
- CLI applications with web UIs

Example workflow:

```bash
# Local dev tool with full access
termsurf open http://localhost:8080/admin --js-api=full

# Public website, no special access
termsurf open https://example.com
```

### Use Cases for OS Access

1. **Local dev dashboards** - Web UI that can read logs, restart services
2. **CLI tools with GUIs** - Terminal command with a web-based interface
3. **File browsers/editors** - Web-based tools that operate on local files
4. **Build tool UIs** - Webpack/Vite dashboards that can trigger rebuilds
5. **Database GUIs** - Web interface that connects to local databases

This positions TermSurf as a lightweight alternative to Electron/Tauri for local
tooling, with the advantage of terminal integration and no packaging required.

## Implementation Notes

### How Console Capture Works

Console output is captured via JavaScript injection at document start:

```javascript
// Injected into every page
(function() {
  const originalLog = console.log;
  console.log = function(...args) {
    window.webkit.messageHandlers.consoleLog.postMessage({
      level: 'log',
      message: args.map(formatArg).join(' ')
    });
    originalLog.apply(console, args);
  };
  // Similar for error, warn, info...
})();
```

### Output Routing

Console output is streamed via the Unix socket to the blocking CLI process. The
CLI writes to its stdout/stderr, which appears in the terminal. This approach
avoids direct PTY access while still routing output to the correct terminal.

Flow:
1. WebView console.log() → Swift WKScriptMessageHandler
2. Swift sends `{"event":"console","data":{"level":"log","message":"..."}}` via socket
3. CLI receives event, writes to stdout (or stderr for warn/error)
4. Output appears in terminal

### Current Implementation Status

| Feature                     | Status      |
| --------------------------- | ----------- |
| Console capture (JS)        | Implemented |
| stdout/stderr routing       | Implemented |
| CLI event streaming         | Implemented |
| `--js-api` flag             | Implemented |
| `window.termsurf.exit()`    | Implemented |
| `window.termsurf.webviewId` | Implemented |
| Environment access          | Future      |
| Full OS access (Tauri-like) | Future      |
