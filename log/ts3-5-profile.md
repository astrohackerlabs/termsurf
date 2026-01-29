# TermSurf 3.0 Profile Isolation

## Background

This document continues from [ts3-4-webpage.md](./ts3-4-webpage.md), which got
CEF rendering real webpages in the terminal.

### What We Accomplished (ts3-4)

Five experiments took the pipeline from a pink test square to rendering
google.com:

1. Created `termsurf-profile` -- a CEF profile server that renders webpages and
   sends IOSurface textures to the GUI via XPC
2. Added debug logging to all three processes (GUI, launcher, profile server)
3. Restored launchd Mach service registration for the launcher
4. Fixed CEF API version initialization (`api_hash()` call)

The full rendering pipeline now works:

```
web CLI --> Unix socket --> GUI --> XPC --> launcher --> termsurf-profile
                                                              |
                                                   CEF renders webpage
                                                              |
                                                 IOSurface Mach port via XPC
                                                              |
GUI <-- IOSurfaceLookupFromMachPort <-- XPC ------------------+
  |
  +-- wgpu texture import --> render pipeline --> webpage on screen
```

### New Goal

Complete profile isolation. Each `--profile` value must create a separate CEF
data directory at `~/.config/termsurf/cef/<profile>/`, with isolated cookies,
storage, and cache.

**Current state:** Profiles work but write to the wrong location. Running
`web --profile test1 google.com` creates the directory at
`~/Library/Application Support/termsurf/cef/test1/` instead of
`~/.config/termsurf/cef/test1/`. This is because `termsurf-profile` uses
`dirs_next::config_dir()` which returns `~/Library/Application Support/` on
macOS. ts2 hardcodes `$HOME/.config/termsurf/cef/` instead.

**Success looks like:**

```
$ web --profile myprofile google.com
# Creates: ~/.config/termsurf/cef/myprofile/

$ web --profile work google.com
# Creates: ~/.config/termsurf/cef/work/

$ web google.com
# Creates: ~/.config/termsurf/cef/default/
```

- Different `--profile` values create different directories under
  `~/.config/termsurf/cef/`
- Profiles are isolated (logging into Google in one profile doesn't affect
  others)
- Default profile is `default`

### Tasks

- [x] Fix profile path to use `~/.config/termsurf/cef/<profile>/` instead of
      `~/Library/Application Support/termsurf/cef/<profile>/`
- [x] Verify different `--profile` values create different directories
- [ ] Verify profiles are isolated (separate cookies, storage, cache)

## Experiments

### Experiment 1: Fix Profile Path and Verify Isolation

**Status:** SUCCESS

**Goal:** Fix the profile cache path to use `~/.config/termsurf/cef/<profile>/`
and verify that different profiles create separate, isolated directories.

**Result:** All three profiles (`default`, `test1`, `test2`) created separate
directories under `~/.config/termsurf/cef/`. The first `web` invocation timed
out (race condition with CEF startup), but succeeded on retry. Each subsequent
profile created its own directory immediately.

#### Fix

One line change in `termsurf-profile/src/main.rs`.

**File:** `ts3/termsurf-profile/src/main.rs`

Change (line 94-97):

```rust
let cache_path = dirs_next::config_dir()
    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
    .join("termsurf/cef")
    .join(&args.profile);
```

To:

```rust
let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
let cache_path = std::path::PathBuf::from(home)
    .join(".config/termsurf/cef")
    .join(&args.profile);
```

This matches ts2's `init_cef()` which uses `$HOME/.config/termsurf/cef/`.

The `dirs_next` dependency can also be removed from
`ts3/termsurf-profile/Cargo.toml` since it is no longer used.

#### Files to Modify

| File                               | Changes                                        |
| ---------------------------------- | ---------------------------------------------- |
| `ts3/termsurf-profile/src/main.rs` | Replace `dirs_next::config_dir()` with `$HOME` |
| `ts3/termsurf-profile/Cargo.toml`  | Remove `dirs-next` dependency                  |

#### Verification

```bash
# Clean up stale profiles from previous experiments
rm -rf ~/Library/Application\ Support/termsurf/cef/
rm -rf ~/.config/termsurf/cef/

# Build
cd ts3
./scripts/build-debug.sh --open

# Test 1: default profile
web google.com
# Check: ls ~/.config/termsurf/cef/default/

# Test 2: named profile
web --profile work google.com
# Check: ls ~/.config/termsurf/cef/work/

# Test 3: second named profile
web --profile personal google.com
# Check: ls ~/.config/termsurf/cef/personal/

# Test 4: verify all three directories exist independently
ls ~/.config/termsurf/cef/
# Expected: default/  work/  personal/
```

#### Success Criteria

- [x] `~/.config/termsurf/cef/default/` is created when running `web google.com`
- [x] `~/.config/termsurf/cef/test1/` is created when running
      `web --profile test1 google.com`
- [x] `~/.config/termsurf/cef/test2/` is created when running
      `web --profile test2 google.com`
- [x] All three directories exist simultaneously under `~/.config/termsurf/cef/`
- [x] No profile directories are created under
      `~/Library/Application Support/termsurf/cef/`
- [ ] Each profile directory contains CEF data (cookies, cache, `Default/`
      subdirectory)

---

## What We Accomplished

One experiment, one fix. Replaced `dirs_next::config_dir()` (which returns
`~/Library/Application Support/` on macOS) with `$HOME/.config/termsurf/cef/` to
match ts2's behavior. Removed the `dirs-next` dependency entirely.

Verified that `default`, `test1`, and `test2` profiles each create separate
directories under `~/.config/termsurf/cef/`. The `--profile` flag works as
designed: omitting it uses `default`, and each named profile gets its own CEF
data directory.

**Not yet verified:** actual session isolation (logging into Google in one
profile and confirming the other profile is not logged in). The directory
structure is correct, but cookie/storage isolation is assumed from CEF's
`root_cache_path` behavior rather than tested directly.

**State of the system after ts3-4 + ts3-5:**

- `web <url>` renders a real webpage in the terminal via CEF
- `web --profile <name> <url>` creates an isolated CEF profile at
  `~/.config/termsurf/cef/<name>/`
- The full pipeline (CLI → socket → GUI → XPC → launcher → profile server → CEF
  → IOSurface → wgpu) is working end-to-end

---

### Next Steps (After This Document)

Once profile isolation is verified:

1. **Multiple pages** -- Open multiple webviews with different profiles
   simultaneously
2. **Keyboard input** -- Type in form fields, use keyboard shortcuts
3. **Mouse input** -- Click links, scroll, hover states
4. **Resize handling** -- CEF resizes when pane resizes, sends new IOSurface
5. **Navigation** -- Back, forward, reload, URL changes
6. **Page lifecycle** -- Handle page loads, errors, redirects
7. **DevTools** -- Open Chrome DevTools for debugging
