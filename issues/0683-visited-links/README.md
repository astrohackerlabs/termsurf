+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 683: Visited Links

CSS `:visited` styling doesn't work. After clicking a link, it still appears
unvisited. Pages that style visited links (e.g. Google search results turning
purple) never change color.

## Background

### How `:visited` Works in Chromium

The `:visited` pseudo-class requires a full integration chain across browser and
renderer processes:

```
HistoryService → VisitedLinkWriter → Shared Memory → VisitedLinkReader → Blink VisitedLinkState
   (browser)        (browser)           (IPC)          (renderer)            (CSS engine)
```

**Browser process:**

- **VisitedLinkWriter** — maintains an in-memory hash table of visited URL
  fingerprints (64-bit MD5 hashes) in shared memory. Persists to disk. Created
  per BrowserContext.
- **VisitedLinkDelegate** — abstract interface for loading visited URLs from a
  history database. The embedder must implement `RebuildTable()` to bootstrap
  the hash table from persistent storage.
- **VisitedLinkEventListener** — bridges VisitedLinkWriter to renderer processes
  via Mojo IPC. Creates per-renderer `VisitedLinkUpdater` instances. Coalesces
  updates (100ms batching) before sending.
- **ContentVisitDelegate** — bridges HistoryService to VisitedLinkWriter.
  Implements both VisitDelegate and VisitedLinkDelegate.

**Renderer process:**

- **VisitedLinkReader** — receives the shared memory hash table from the browser
  process. Implements `VisitedLinkNotificationSink` Mojo interface. Maintains
  per-origin salts for partitioned visited links.
- **VisitedLinkState** — per-Document Blink state. Calls VisitedLinkReader to
  determine if an element should match `:visited`. Receives per-origin salt from
  navigation commit parameters.

**Navigation integration:**

- **VisitedLinkNavigationThrottle** — on navigation commit, queries
  HistoryService for the per-origin salt and passes it to the renderer via
  `NavigationCommitParams`. Required for partitioned visited links (modern
  security model).

**Mojo interface** (`components/visitedlink/common/visitedlink.mojom`):

```
interface VisitedLinkNotificationSink {
  UpdateVisitedLinks(ReadOnlySharedMemoryRegion table_region);
  AddVisitedLinks(array<uint64> link_hashes);
  ResetVisitedLinks(bool invalidate_cached_hashes);
  UpdateOriginSalts(map<Origin, uint64> origin_salts);
};
```

### What TermSurf Has

Nothing. The TermSurf Chromium profile server (`chromium_profile_server/`)
implements none of this:

- No VisitedLinkWriter instantiation
- No HistoryService integration
- No VisitedLinkNavigationThrottle registration
- No Mojo binding for VisitedLinkNotificationSink
- No VisitedLinkDelegate implementation
- No ContentVisitDelegate

Without the writer, the renderer's hash table is always empty — every link reads
as "not visited."

### What Would Be Required

To make `:visited` work, the profile server would need:

1. **VisitedLinkWriter** per BrowserContext with a VisitedLinkDelegate
2. **Navigation commit hooks** to call `VisitedLinkWriter::AddURL()` when pages
   load
3. **VisitedLinkNavigationThrottle** registered in
   `ContentBrowserClient::CreateThrottlesForNavigation()`
4. **HistoryService** (or a lightweight substitute) to persist visits across
   sessions and supply per-origin salts
5. **VisitedLinkEventListener** to bridge writer updates to renderer processes

This is not a missing flag or config — it's an entire subsystem that content
shell deliberately omits.

### Key Source Files

| Component                     | Path                                                                    |
| ----------------------------- | ----------------------------------------------------------------------- |
| VisitedLinkWriter             | `components/visitedlink/browser/visitedlink_writer.h`                   |
| VisitedLinkDelegate           | `components/visitedlink/browser/visitedlink_delegate.h`                 |
| VisitedLinkEventListener      | `components/visitedlink/browser/visitedlink_event_listener.h`           |
| VisitedLinkReader             | `components/visitedlink/renderer/visitedlink_reader.h`                  |
| VisitedLinkState              | `third_party/blink/renderer/core/dom/visited_link_state.h`              |
| ContentVisitDelegate          | `components/history/content/browser/content_visit_delegate.h`           |
| VisitedLinkNavigationThrottle | `components/history/content/browser/visited_link_navigation_throttle.h` |
| Mojo interface                | `components/visitedlink/common/visitedlink.mojom`                       |

All paths relative to `chromium/src/`.

## Conclusion

Visited links don't work because the TermSurf Chromium profile server has no
visited link infrastructure — no VisitedLinkWriter, no HistoryService, no Mojo
bindings, no navigation throttle. This is an entire subsystem that content shell
omits by design.

Fixing it would require adding significant C++ code to the profile server:
history storage, a VisitedLinkWriter per BrowserContext, Mojo wiring, and a
navigation throttle. This is deferred because a rewrite of the profile server
from C++ to Zig or Rust is under consideration. Adding a new C++ subsystem now
would be wasted work if the server is rewritten. Visited links remain
non-functional until the server language decision is made.
