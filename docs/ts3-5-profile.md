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

- [ ] Fix profile path to use `~/.config/termsurf/cef/<profile>/` instead of
      `~/Library/Application Support/termsurf/cef/<profile>/`
- [ ] Verify different `--profile` values create different directories
- [ ] Verify profiles are isolated (separate cookies, storage, cache)

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
