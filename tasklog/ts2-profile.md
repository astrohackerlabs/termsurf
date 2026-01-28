# CEF Browser Profiles

This document summarizes our research into enabling multiple isolated browser
profiles in TermSurf using CEF (Chromium Embedded Framework).

## Goal

Support multiple browser profiles, each with isolated:

- Cookies
- localStorage / IndexedDB
- Session data
- Cache
- History

This enables use cases like:

- Multiple logged-in accounts side by side
- Per-project browser contexts
- Isolated dev/test environments

## The Problem

CEF's Chrome runtime (the default and only supported bootstrap since M128) does
not support multiple profiles within a single process.

### What We Tried

| Experiment               | Approach                                                       | Result                    |
| ------------------------ | -------------------------------------------------------------- | ------------------------- |
| Custom `cache_path`      | Set `RequestContextSettings.cache_path` to a profile directory | Ignored by Chrome runtime |
| Chrome naming convention | Use `Default`, `Profile 1`, `Profile 2` directory names        | Browser creation fails    |
| Incognito contexts       | Empty `cache_path` for in-memory isolation                     | Works, but no persistence |

### Why It Fails

From the
[official CEF documentation](https://cef-builds.spotifycdn.com/docs/120.2/structcef__settings__t.html):

> "When using the Chrome runtime any child directory value will be ignored and
> the 'default' profile (also a child directory) will be used instead."

This is intentional behavior, not a bug. Chrome's profile system expects
profiles to be managed internally, not via arbitrary directory paths.

## Background: Alloy vs Chrome Runtime

CEF historically had two runtimes:

| Runtime    | Description                      | Multi-Profile              |
| ---------- | -------------------------------- | -------------------------- |
| **Alloy**  | Lightweight, embedder-controlled | Yes, via `cache_path`      |
| **Chrome** | Full Chrome behavior             | No, single Default profile |

### Timeline

| Version  | Change                                               |
| -------- | ---------------------------------------------------- |
| M125     | Alloy split into "style" and "bootstrap" components  |
| M125     | Alloy style introduced (runs under Chrome bootstrap) |
| M127     | Alloy bootstrap deprecated                           |
| **M128** | **Alloy bootstrap removed**                          |

Since M128, only Chrome bootstrap is available. You can still create
"Alloy-style" browsers for features like off-screen rendering, but:

> "Chrome runtime bootstrap means that all global objects and CefRequestContext
> (Profile) will be Chrome objects."

This means even Alloy-style browsers use Chrome's `ProfileImpl`, which ignores
custom `cache_path` values.

**Sources:**

- [CEF Issue #3685 - Delete Alloy bootstrap](https://github.com/chromiumembedded/cef/issues/3685)
- [CEF Issue #3681 - Alloy-style windows](https://github.com/chromiumembedded/cef/issues/3681)
- [CefSharp Issue #4961 - cache_path issues](https://github.com/cefsharp/CefSharp/issues/4961)

## The Solution: Separate Processes

The only way to achieve true multi-profile isolation in modern CEF is to run
**separate CEF processes**, each with its own `root_cache_path`.

This is how Chrome itself handles multiple profiles - each profile runs in a
separate process group with its own user data directory.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    TermSurf Main Process                     │
│                   (UI, Terminal, Routing)                    │
└─────────────────────────────────────────────────────────────┘
          │                    │                    │
         IPC                  IPC                  IPC
          │                    │                    │
          ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│  CEF Process 0  │  │  CEF Process 1  │  │  CEF Process 2  │
│                 │  │                 │  │                 │
│ root_cache_path │  │ root_cache_path │  │ root_cache_path │
│ ~/.../profile-0 │  │ ~/.../profile-1 │  │ ~/.../profile-2 │
│                 │  │                 │  │                 │
│   ┌─────────┐   │  │   ┌─────────┐   │  │   ┌─────────┐   │
│   │ Default │   │  │   │ Default │   │  │   │ Default │   │
│   └─────────┘   │  │   └─────────┘   │  │   └─────────┘   │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

Each CEF process:

- Has its own `root_cache_path` (e.g., `~/.config/termsurf/cef/profile-0/`)
- Gets its own `Default` subdirectory automatically
- Is fully isolated from other profiles
- Communicates with the main process via IPC

### Directory Structure

```
~/.config/termsurf/cef/
├── profile-0/
│   └── Default/
│       ├── Cookies
│       ├── Local Storage/
│       └── ...
├── profile-1/
│   └── Default/
│       ├── Cookies
│       ├── Local Storage/
│       └── ...
└── profile-2/
    └── Default/
        └── ...
```

### Key Constraints

1. **One CEF initialize per process** - `cef_initialize()` can only be called
   once. Chromium uses global objects that cannot be reinitialized.

2. **No shared `root_cache_path`** - Multiple processes cannot share the same
   root directory. CEF uses a process singleton lock to prevent this.

3. **Process-per-profile** - Each profile requires its own CEF worker process.

## Implementation Options

### Option A: CEF Worker Processes

Spawn separate helper processes for each profile:

```rust
// Main process spawns workers
SpawnCefWorker {
    profile_id: 0,
    root_cache_path: "~/.config/termsurf/cef/profile-0",
}
```

Workers communicate with the main process via:

- Unix domain sockets
- Shared memory for texture data
- Custom IPC protocol for browser commands

### Option B: Incognito-Only Multi-Session

If persistence isn't required, use incognito contexts:

```rust
let settings = RequestContextSettings {
    cache_path: "".into(),  // Empty = incognito
    ..Default::default()
};
```

This provides runtime isolation without persistence. Works well for:

- Temporary sessions
- Testing different accounts
- Sandboxed browsing

### Option C: Single Profile + Incognito

Accept the Chrome runtime limitation:

- Default profile for persistent sessions
- `--incognito` flag for isolated sessions
- No true multi-profile support

## Recommendation

For TermSurf's use case (multiple persistent browser profiles), **Option A
(separate CEF processes per profile)** is the correct approach.

This matches how Chrome handles profiles and is the only way to get true
isolation with modern CEF.

## Alternatives Considered

| Alternative                       | Why Not Viable                                 |
| --------------------------------- | ---------------------------------------------- |
| Alloy bootstrap                   | Removed in M128                                |
| Alloy style with Chrome bootstrap | Still uses Chrome's ProfileImpl                |
| Hacking Chrome's profile system   | Undocumented, fragile, unsupported             |
| WebView2 (Windows)                | Platform-specific, doesn't help cross-platform |
| Electron                          | Different architecture entirely                |

## References

- [CEF Settings Documentation](https://cef-builds.spotifycdn.com/docs/120.2/structcef__settings__t.html)
- [CEF Issue #3685 - Delete Alloy bootstrap](https://github.com/chromiumembedded/cef/issues/3685)
- [CEF Issue #3681 - Alloy-style windows](https://github.com/chromiumembedded/cef/issues/3681)
- [CefSharp Issue #4961 - cache_path issues](https://github.com/cefsharp/CefSharp/issues/4961)
- [Chromium Profile Documentation](https://www.chromium.org/developers/creating-and-using-profiles/)
- [CEF Forum - Chrome vs Alloy](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=19867)
