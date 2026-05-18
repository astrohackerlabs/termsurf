+++
status = "closed"
opened = "2026-01-10"
closed = "2026-01-10"
+++

# Merging Upstream Repositories

This document describes how to merge changes from upstream repositories into
TermSurf while preserving our modifications.

## Overview

TermSurf integrates upstream projects:

| Project | Directory | Upstream | Remote | Branch | Purpose |
|---------|-----------|----------|--------|--------|---------|
| Ghostty | `ts1/` | [ghostty-org/ghostty](https://github.com/ghostty-org/ghostty) | `upstream` | main | Terminal emulator (TermSurf 1.x) |
| WezTerm | `ts2/` | [wezterm/wezterm](https://github.com/wezterm/wezterm) | `wezterm` | main | Terminal emulator (TermSurf 2.0) |
| WezTerm | `ts3/` | [wezterm/wezterm](https://github.com/wezterm/wezterm) | `wezterm` | main | Terminal emulator (TermSurf 3.0) |
| cef-rs | `cef-rs/` | [tauri-apps/cef-rs](https://github.com/tauri-apps/cef-rs) | `cef-rs-upstream` | dev | CEF Rust bindings |

Each upstream is tracked via a git remote and merged periodically to get bug
fixes, performance improvements, and new features.

## Git Remotes

Set up remotes if not already configured:

```bash
# Ghostty
git remote add upstream https://github.com/ghostty-org/ghostty.git

# WezTerm (used for both ts2 and ts3)
git remote add wezterm https://github.com/wezterm/wezterm.git

# cef-rs
git remote add cef-rs-upstream https://github.com/tauri-apps/cef-rs.git
```

## Common Merge Process

The process is similar for all three upstreams:

### 1. Pre-Merge Checklist

- [ ] Working tree is clean (`git status` shows no changes)
- [ ] All local changes are committed
- [ ] Note current HEAD: `git rev-parse HEAD`

### 2. Fetch Upstream

```bash
git fetch <remote-name>
```

### 3. Review Changes

```bash
# Count new commits
git rev-list --count HEAD..<remote>/<branch>

# See commit summaries
git log --oneline HEAD..<remote>/<branch> | head -50

# Check for conflicts in files we've modified
git diff HEAD..<remote>/<branch> -- <path-to-check>
```

### 4. Merge

```bash
git merge -X subtree=<directory> <remote>/<branch> -m "Merge upstream <name>"
```

### 5. Resolve Conflicts

See repo-specific sections below for conflict resolution strategies.

### 6. Verify Build

Test that everything compiles and works correctly.

### 7. Rollback (If Needed)

```bash
# Before committing:
git merge --abort

# After committing:
git reset --hard ORIG_HEAD
```

---

## Ghostty (ts1/)

### Our Modifications

Our modifications fall into two categories:

1. **Upstream-friendly changes** - Additive APIs that could be submitted as PRs
   to Ghostty (e.g., custom config directory support)

2. **TermSurf-specific changes** - Branding and features unique to TermSurf
   (e.g., web CLI command, surfer emoji)

See [libghostty.md](libghostty.md) for detailed documentation of our changes.

### Modified Files Inventory

#### Upstream-Friendly (Low Conflict Risk)

These are additive changes that don't modify existing Ghostty code paths:

| File | Change | Notes |
|------|--------|-------|
| `ts1/include/ghostty.h` | Added `ghostty_config_load_files` declaration | End of file |
| `ts1/src/config/Config.zig` | Added `loadFiles` method | New public method |
| `ts1/src/config/CApi.zig` | Added C API wrapper | New function |
| `ts1/src/os/macos.zig` | Added `appSupportDirWithBundleId` helper | New function |

#### TermSurf-Specific (Branding)

Simple string replacements, easy to re-apply if conflicts occur:

| File | Change |
|------|--------|
| `ts1/src/cli/help.zig` | "ghostty" -> "termsurf", app name references |
| `ts1/src/cli/version.zig` | "Ghostty" -> "TermSurf" in version banner |
| `ts1/src/cli/list_themes.zig` | Ghost emoji -> surfer emoji in preview title |

#### TermSurf-Specific (Functional)

These modify existing Ghostty code and have higher conflict risk:

| File | Change | Conflict Risk |
|------|--------|---------------|
| `ts1/src/cli/ghostty.zig` | Added `web` action, `detectMultiCall` | **High** |
| `ts1/src/cli/action.zig` | Multi-call binary detection via `argv[0]` | **High** |
| `ts1/src/cli/web.zig` | **New file** (no conflict) | None |

#### Build System

| File | Change |
|------|--------|
| `ts1/build.zig` | XCFramework output to both `macos/` and `termsurf-macos/` |
| `ts1/src/build/GhosttyXCFramework.zig` | Dual output paths |

### Merge Commands

```bash
# Fetch
git fetch upstream

# Review
git log --oneline HEAD..upstream/main -- ts1/ | head -20
git diff HEAD..upstream/main -- ts1/src/cli/

# Merge
git merge -X subtree=ts1 upstream/main -m "Merge upstream Ghostty"
```

### Conflict Resolution Guide

#### ts1/src/cli/ghostty.zig

This file defines the CLI entry point. We added:

- Import for `web.zig`
- `detectMultiCall` function
- `web` case in the action switch

**Resolution strategy:**

1. Keep all upstream changes to existing code
2. Re-add our `web` import at the top
3. Re-add `detectMultiCall` function (search for it in our version)
4. Re-add `.web` case in the action switch statement

```zig
// Our additions to look for:
const web = @import("web.zig");

fn detectMultiCall(argv0: []const u8) ?Action.Tag { ... }

// In the switch statement:
.web => web.run(alloc),
```

#### ts1/src/cli/action.zig

We added multi-call binary detection. Look for our changes to the `init`
function that checks `argv[0]` for "web".

**Resolution strategy:**

1. Accept upstream changes
2. Re-add our multi-call detection logic in `init`

#### ts1/src/cli/help.zig, version.zig, list_themes.zig

Simple branding changes.

**Resolution strategy:**

1. Accept upstream changes (they may have added new text)
2. Re-apply our branding substitutions:
   - "ghostty" -> "termsurf"
   - "Ghostty" -> "TermSurf"
   - Ghost emoji -> surfer emoji

#### ts1/src/config/Config.zig, CApi.zig

We added new methods/functions. These are additive and unlikely to conflict.

**Resolution strategy:**

1. Accept upstream changes
2. Verify our added functions are still present
3. If removed by conflict, re-add them

#### ts1/include/ghostty.h

We added a single function declaration at the end.

**Resolution strategy:**

1. Accept upstream changes
2. Ensure our declaration is still at the end:
   ```c
   void ghostty_config_load_files(ghostty_config_t, const char*, const char*);
   ```

### Post-Merge: Review macOS App Changes

After merging upstream, review changes to `ts1/macos/` (Ghostty's macOS app)
and port relevant updates to `ts1/termsurf-macos/`.

```bash
# List files changed in macos/ during the merge
git diff --name-only <pre-merge-commit>..HEAD -- ts1/macos/Sources/

# For each changed file, check if termsurf-macos has a corresponding file
ls ts1/termsurf-macos/Sources/Ghostty/
```

### Ghostty Test Commands

```bash
cd ts1
zig build
zig build test
./scripts/build-debug.sh --open
```

---

## WezTerm (ts2/)

### Our Modifications

WezTerm is used for TermSurf 2.0. ts2 served as a testbed for CEF integration
experiments. It validated that WezTerm + CEF works but revealed architectural
limitations (see `docs/301-architecture.md`).

### Modified Files Inventory

*To be documented as modifications are made.*

### Merge Commands

```bash
# Fetch
git fetch wezterm

# Review
git log --oneline HEAD..wezterm/main -- ts2/ | head -20

# Merge
git merge -X subtree=ts2 wezterm/main -m "Merge upstream WezTerm into ts2"
```

### Conflict Resolution Guide

*To be documented as modifications are made.*

### WezTerm (ts2) Test Commands

```bash
cd ts2
cargo build
cargo test
```

---

## WezTerm (ts3/)

### Our Modifications

WezTerm is used for TermSurf 3.0. ts3 is a fresh start with the correct
architecture: the `web` command as a coordinator that spawns browser
subprocesses (one per profile).

**Base commit:** `05343b387` (January 2026)

### Modified Files Inventory

*To be documented as modifications are made.*

### Submodules

ts3 uses the following submodules (defined in root `.gitmodules`):

| Submodule | Path |
|-----------|------|
| harfbuzz | `ts3/deps/harfbuzz/harfbuzz` |
| freetype2 | `ts3/deps/freetype/freetype2` |
| libpng | `ts3/deps/freetype/libpng` |
| zlib | `ts3/deps/freetype/zlib` |

After cloning, initialize with:
```bash
git submodule update --init ts3/deps/harfbuzz/harfbuzz ts3/deps/freetype/libpng ts3/deps/freetype/zlib ts3/deps/freetype/freetype2
```

### Merge Commands

```bash
# Fetch
git fetch wezterm

# Review
git log --oneline HEAD..wezterm/main -- ts3/ | head -20

# Merge
git merge -X subtree=ts3 wezterm/main -m "Merge upstream WezTerm into ts3"
```

### Conflict Resolution Guide

*To be documented as modifications are made.*

### WezTerm (ts3) Test Commands

```bash
cd ts3
cargo build
cargo test
```

---

## cef-rs

### Our Modifications

cef-rs provides CEF (Chromium Embedded Framework) Rust bindings. Our
modifications are minimal and mostly additive.

#### Modified Files

| File | Change | Notes |
|------|--------|-------|
| `cef-rs/cef/src/osr_texture_import/iosurface.rs` | Fixed macOS IOSurface texture import | Metal API type fix |

### Merge Commands

```bash
# Fetch
git fetch cef-rs-upstream

# Review
git log --oneline HEAD..cef-rs-upstream/dev -- cef-rs/ | head -20

# Merge
git merge -X subtree=cef-rs cef-rs-upstream/dev -m "Merge upstream cef-rs"
```

### Conflict Resolution Guide

#### cef-rs/cef/src/osr_texture_import/iosurface.rs

We fixed a Metal API type issue for macOS IOSurface texture import.

**Our fix:**
```rust
// Use proper Ref types that implement Message trait
let device_ref: &metal::DeviceRef = raw_device;
let desc_ref: &metal::TextureDescriptorRef = metal_desc.as_ref();
let texture: metal::Texture = objc::msg_send![
    device_ref,
    newTextureWithDescriptor:desc_ref
    iosurface:self.handle
    plane:0usize
];
```

**Resolution strategy:**
1. Check if upstream has fixed this differently
2. If not, re-apply our fix after accepting their changes

### cef-rs Test Commands

```bash
cd cef-rs
cargo build --example osr
./target/debug/examples/osr
```

---

## Merge Frequency

**Recommended:** Merge upstream monthly, or more frequently if:

- A security fix is released
- A bug affecting TermSurf is fixed
- A feature we want is added

**Before major releases:** Always merge upstream to get latest fixes.

## Submitting Upstream PRs

After TermSurf MVP, consider submitting our upstream-friendly changes:

### Ghostty

1. **Custom config directory API** - Useful for any app embedding libghostty
2. **Any bug fixes** we make to libghostty

See the "Submitting Upstream" section in [libghostty.md](libghostty.md).

### cef-rs

1. **IOSurface Metal fix** - The type annotation fix benefits all macOS users
