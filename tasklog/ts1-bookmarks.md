# Bookmarks Implementation Plan (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x (Ghostty + WKWebView).
> Bookmarks for TermSurf 2.0 will need to be reimplemented for the WezTerm + cef-rs architecture.

This document describes the implementation plan for bookmarks in TermSurf.

## Overview

Bookmarks in TermSurf follow a CLI-first philosophy:

- **Storage:** JSON files in `~/.config/termsurf/{profile}.json`
- **Management:** Via `web bookmark` CLI commands
- **UI shortcut:** cmd+b to bookmark current page
- **Architecture:** All JSON I/O happens in Swift via socket communication

### Key Design Decisions

1. **Per-profile storage** - Each profile gets its own JSON file
2. **Object mapping for bookmarks** - Names are keys, enforcing uniqueness
3. **Socket-based I/O** - All reads/writes go through Swift app (no race
   conditions)
4. **Profile UUIDs** - Stored in JSON, used for WebKit data store identification

---

## Storage Format

**Location:** `~/.config/termsurf/{profile}.json`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "bookmarks": {
    "google": {
      "title": "Google",
      "url": "https://www.google.com"
    },
    "github": {
      "title": "GitHub",
      "url": "https://github.com"
    }
  }
}
```

- `id` - UUID for WebKit's `WKWebsiteDataStore(forIdentifier:)`
- `bookmarks` - Object mapping where keys are unique bookmark names

**Default profile:** `default.json`

---

## Phase 1: Profile Manager (Swift)

Create the foundation for profile/bookmark storage in Swift.

### Technical Details

**New file:** `termsurf-macos/Sources/Features/Bookmarks/ProfileManager.swift`

**Profile struct:**

```swift
struct Profile: Codable {
    let id: UUID
    var bookmarks: [String: Bookmark]
}

struct Bookmark: Codable {
    var title: String
    var url: String
}
```

**ProfileManager responsibilities:**

- `loadProfile(name: String) -> Profile` - Load or create profile
- `saveProfile(name: String, profile: Profile)` - Save profile to disk
- `getProfilePath(name: String) -> URL` - Returns
  `~/.config/termsurf/{name}.json`
- `uuidForProfile(name: String) -> UUID` - Deterministic hash-based UUID from
  name
- Thread-safe access (serial queue for all operations)

**UUID Generation:**

Use the same deterministic hash-based approach already in
`WebViewOverlay.swift`:

- Hash the profile name to generate a consistent UUID
- This ensures the same profile name always maps to the same UUID
- Also save the UUID to the JSON file for reverse lookup (UUID -> profile name)

The JSON storage enables future features that need to go from UUID back to
profile name (e.g., listing all profiles, UI that shows profile names).

**Integration with WebViewOverlay:**

- Keep existing hash-based UUID generation logic
- When a profile is first used, ProfileManager creates the JSON file with the
  hashed UUID and empty bookmarks
- WebViewOverlay can continue using its existing UUID generation (or call
  ProfileManager for consistency)

### Checklist

- [ ] Create `ProfileManager.swift` with Profile/Bookmark structs
- [ ] Implement `uuidForProfile(name:)` - deterministic hash-based UUID
      generation
- [ ] Implement `loadProfile(name:)` - loads existing or creates new with hashed
      UUID
- [ ] Implement `saveProfile(name:profile:)` - writes JSON to disk
- [ ] Implement `getProfilePath(name:)` - returns correct path in config dir
- [ ] Add serial DispatchQueue for thread-safe access
- [ ] Update `WebViewOverlay.swift` to call ProfileManager when profile is used
      (ensures JSON file is created with UUID for reverse lookup)
- [ ] Test: Create profile, verify JSON file created with correct UUID
- [ ] Test: Reopen profile, verify same UUID is used (hash is deterministic)

---

## Phase 2: Bookmark CRUD Operations (Swift)

Add bookmark management methods to ProfileManager.

### Technical Details

**New methods on ProfileManager:**

```swift
func addBookmark(profile: String, name: String, title: String, url: String) throws
func getBookmark(profile: String, name: String) -> Bookmark?
func listBookmarks(profile: String) -> [String: Bookmark]
func updateBookmark(profile: String, name: String, title: String?, url: String?) throws
func deleteBookmark(profile: String, name: String) throws
```

**Error handling:**

- `BookmarkError.alreadyExists` - When adding duplicate name
- `BookmarkError.notFound` - When getting/updating/deleting non-existent
  bookmark
- `BookmarkError.profileNotFound` - When profile doesn't exist (for get/list)

**Name derivation helper:**

```swift
static func deriveNameFromURL(_ url: URL) -> String
// google.com -> google
// www.google.com -> google
// blog.myname.com -> blog
// google.co.uk -> google
```

Algorithm:

1. Get host from URL
2. Split by "."
3. Skip "www" if first
4. Return first part

### Checklist

- [ ] Add `BookmarkError` enum
- [ ] Implement `addBookmark(profile:name:title:url:)`
- [ ] Implement `getBookmark(profile:name:)`
- [ ] Implement `listBookmarks(profile:)`
- [ ] Implement `updateBookmark(profile:name:title:url:)`
- [ ] Implement `deleteBookmark(profile:name:)`
- [ ] Implement `deriveNameFromURL(_:)` helper
- [ ] Test: Add bookmark, verify in JSON file
- [ ] Test: Add duplicate name, verify error
- [ ] Test: Get bookmark, verify correct data returned
- [ ] Test: List bookmarks, verify all returned
- [ ] Test: Update bookmark, verify changes persisted
- [ ] Test: Delete bookmark, verify removed from JSON

---

## Phase 3: Socket Protocol for Bookmarks (Swift)

Extend the socket protocol to handle bookmark commands.

### Technical Details

**New action:** `bookmark`

**Subactions:** `add`, `get`, `list`, `update`, `delete`

**Request format:**

```json
{"id": "1", "action": "bookmark", "subaction": "add", "data": {
  "profile": "default",
  "name": "google",
  "title": "Google",
  "url": "https://google.com"
}}

{"id": "2", "action": "bookmark", "subaction": "get", "data": {
  "profile": "default",
  "name": "google"
}}

{"id": "3", "action": "bookmark", "subaction": "list", "data": {
  "profile": "default"
}}

{"id": "4", "action": "bookmark", "subaction": "update", "data": {
  "profile": "default",
  "name": "google",
  "title": "New Title",
  "url": "https://new-url.com"
}}

{"id": "5", "action": "bookmark", "subaction": "delete", "data": {
  "profile": "default",
  "name": "google"
}}
```

**Response format:**

```json
{"id": "1", "status": "ok"}
{"id": "1", "status": "error", "error": "Bookmark 'google' already exists"}

{"id": "2", "status": "ok", "data": {"title": "Google", "url": "https://google.com"}}
{"id": "2", "status": "error", "error": "Bookmark 'google' not found"}

{"id": "3", "status": "ok", "data": {"bookmarks": {"google": {"title": "...", "url": "..."}}}}

{"id": "4", "status": "ok"}
{"id": "5", "status": "ok"}
```

**Files to modify:**

- `TermsurfProtocol.swift` - Add bookmark request/response types
- `CommandHandler.swift` - Handle bookmark action, dispatch to ProfileManager

### Checklist

- [ ] Add bookmark request types to `TermsurfProtocol.swift`
- [ ] Add bookmark response types to `TermsurfProtocol.swift`
- [ ] Update `CommandHandler.swift` to handle "bookmark" action
- [ ] Implement handler for "add" subaction
- [ ] Implement handler for "get" subaction
- [ ] Implement handler for "list" subaction
- [ ] Implement handler for "update" subaction
- [ ] Implement handler for "delete" subaction
- [ ] Test: Send bookmark add via socket, verify response
- [ ] Test: Send bookmark get via socket, verify URL returned

---

## Phase 4: CLI Commands (Zig)

Add `web bookmark` commands to the CLI tool.

### Technical Details

**Commands:**

```bash
web bookmark add <name> --url <url> [--title <title>] [--profile <profile>]
web bookmark get <name> [--profile <profile>]
web bookmark list [--profile <profile>]
web bookmark update <name> [--url <url>] [--title <title>] [--profile <profile>]
web bookmark delete <name> [--profile <profile>]
```

**Defaults:**

- `--profile` defaults to "default"
- `--title` defaults to name if not provided

**Output:**

- `add`: Silent on success, error message on failure
- `get`: Prints URL to stdout (for piping)
- `list`: Prints bookmarks (format TBD - maybe name\turl per line)
- `update`: Silent on success
- `delete`: Silent on success

**File to modify:** `src/cli/web.zig`

**Implementation:**

- Parse "bookmark" as subcommand
- Parse subaction (add/get/list/update/delete)
- Build JSON request
- Send via socket
- Parse response, output result

### Checklist

- [ ] Add "bookmark" subcommand parsing
- [ ] Implement argument parsing for each subaction
- [ ] Implement `bookmark add` - send request, handle response
- [ ] Implement `bookmark get` - send request, print URL
- [ ] Implement `bookmark list` - send request, print bookmarks
- [ ] Implement `bookmark update` - send request, handle response
- [ ] Implement `bookmark delete` - send request, handle response
- [ ] Test: `web bookmark add google --url https://google.com`
- [ ] Test: `web bookmark get google` prints URL
- [ ] Test: `web bookmark list` shows bookmarks
- [ ] Test: `web bookmark delete google`

---

## Phase 5: Bookmark Lookup in `web open`

Allow `web open <bookmark-name>` to open bookmarks.

### Technical Details

**Modified behavior of `web open`:**

1. If argument looks like a URL (has scheme or contains `.`), open as URL
2. Otherwise, try to resolve as bookmark name:
   - Send `bookmark get` request with profile (default: "default")
   - If found, open the returned URL
   - If not found, error: "Bookmark 'name' not found"

**New flag:** `--profile <name>` for `web open`

**Examples:**

```bash
web open https://google.com          # Opens URL directly
web open google.com                  # Opens URL (contains .)
web open google                      # Looks up bookmark "google"
web open google --profile personal   # Looks up in specific profile
```

### Checklist

- [ ] Add `--profile` flag to `web open` command
- [ ] Implement URL vs bookmark detection logic
- [ ] If bookmark: send `bookmark get` request first
- [ ] If bookmark found: send `open` request with resolved URL
- [ ] If bookmark not found: print error, exit with non-zero status
- [ ] Test: `web open google` opens bookmarked URL
- [ ] Test: `web open nonexistent` shows error
- [ ] Test: `web open https://example.com` still works

---

## Phase 6: cmd+b Keyboard Shortcut (Swift)

Add cmd+b to bookmark the current page.

### Technical Details

**Behavior:**

- Works in both browse mode and control mode
- Gets current URL from WKWebView
- Derives name from domain (first meaningful part)
- Gets title from page title
- Uses webview's profile (or "default")
- Adds bookmark via ProfileManager
- Shows feedback in control bar

**Implementation location:**

- `SurfaceView_AppKit.swift` - Intercept cmd+b in `performKeyEquivalent`
- `WebViewContainer.swift` - Method to get current URL/title and add bookmark

**Feedback in control bar:**

- Success: "Bookmarked as 'google'"
- Error: "Bookmark 'google' already exists"

**Control bar changes:**

- Add method to show temporary status message
- Message auto-clears after ~2 seconds

### Checklist

- [ ] Add `bookmarkCurrentPage()` method to `WebViewContainer`
- [ ] Get current URL from `webViewOverlay.webView.url`
- [ ] Get current title from `webViewOverlay.webView.title`
- [ ] Derive name using `ProfileManager.deriveNameFromURL(_:)`
- [ ] Call `ProfileManager.shared.addBookmark(...)`
- [ ] Add cmd+b handling in `SurfaceView_AppKit.swift` `performKeyEquivalent`
- [ ] Add `showTemporaryMessage(_:)` to `ControlBar`
- [ ] Show success/error message in control bar
- [ ] Test: Press cmd+b on a page, verify bookmark added
- [ ] Test: Press cmd+b on already bookmarked page, verify error shown

---

## Phase 7: Documentation and Cleanup

Update documentation and ensure everything is consistent.

### Checklist

- [ ] Update `CLAUDE.md` with bookmark-related files
- [ ] Update `docs/ts2-architecture.md` if needed
- [ ] Add user-facing documentation for bookmark commands
- [ ] Verify all error messages are clear and helpful
- [ ] Review code for consistency and cleanup

---

## Future Enhancements (Not MVP)

- Open bookmark in existing webview (navigate current page)
- Bookmark folders/categories
- Import/export bookmarks
- Fuzzy search for bookmark names
- Bookmark bar UI (optional, toggleable)
