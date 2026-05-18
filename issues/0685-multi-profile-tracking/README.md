+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 685: Multi-Profile Tracking

Fix `web last` and `web devtools` auto-targeting to work correctly when multiple
browser profiles are open simultaneously.

## Background

Issue 684 introduced `last_browser_pane` — a single global variable that tracks
the most recently active browser pane. It's updated in two places:

1. `handleTabReady` — when a browser tab is created (`tab_id > 0`)
2. `handlePaneFocusChanged` — when a non-DevTools pane with `tab_id > 0` gains
   focus

Both `web last` and `web devtools` auto-targeting depend on this global.

## Problem

The single global breaks with multiple profiles:

1. **`web last` fails entirely with multiple profiles open.** Open a browser
   with the default profile, then open another with the "work" profile.
   `web last` (no filter) returns "No active browser tab found." instead of the
   work profile's pane info. The root cause needs investigation — the global
   should point to the most recent pane regardless of profile.
2. **`web last --profile default` fails when "work" was opened last.** The
   profile filter only checks `last_browser_pane`. If that pane belongs to
   "work", the filter rejects it and returns nothing. It does not search other
   panes.
3. **`web last --profile work` works** only because the global happens to point
   to the work pane (most recently created).
4. **`web devtools` auto-targeting has the same limitation.** It uses the same
   `last_browser_pane` global, so it can only target the single most recent
   browser pane.

## Root Cause

The `--profile` flag is defined with `default_value = "default"` and
`global = true` (line 165 of `main.rs`). It always has a value. So bare
`web last` sends `profile = "default"` to `handleQueryLast`. The GUI sees a
non-empty profile filter and takes the filtered path. If `last_browser_pane`
points to the "work" pane, the filter rejects it. The unfiltered path in the GUI
is unreachable from the TUI.

## Relevant Code

- `gui/src/apprt/xpc.zig` — `last_browser_pane` global (line 119),
  `handleTabReady` (line 614), `handlePaneFocusChanged` (line 900),
  `handleQueryLast` (line 790), DevTools auto-targeting (line 490)
- `tui/src/main.rs` — `Commands::Last` subcommand, `Cli` struct (line 155),
  profile usage (lines 217, 315, 336, 347)
- `tui/src/xpc.rs` — `send_query_last`

## Experiment 1: Make `--profile` optional

### Hypothesis

If `--profile` is changed from `default_value = "default"` to `Option<String>`,
then bare `web last` sends no profile filter and the GUI returns whatever
`last_browser_pane` points to — regardless of profile. `web last --profile work`
still filters. The overlay and DevTools paths default to `"default"` in code
instead of in clap.

### Changes

#### 1. TUI (`main.rs`): Change `profile` to `Option<String>`

```rust
/// Browser profile name
#[arg(long, global = true)]
profile: Option<String>,
```

#### 2. TUI (`main.rs`): Derive the working profile after parsing

Replace the current profile validation block (lines 183–193) with:

```rust
let profile_arg = cli.profile; // Option<String>
let profile = profile_arg.clone().unwrap_or_else(|| "default".to_string());

// Validate profile name: lowercase alphanumeric, starts with a letter.
if profile.is_empty()
    || !profile.bytes().next().unwrap().is_ascii_lowercase()
    || !profile
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
{
    eprintln!("Error: profile name must be lowercase alphanumeric, starting with a letter");
    std::process::exit(1);
}
```

`profile` (with default) is used by overlay paths (lines 315, 336, 347) — no
changes needed there. `profile_arg` (the raw option) is used by `web last`.

#### 3. TUI (`main.rs`): Pass raw option to `send_query_last`

Change the `web last` handler (line 217):

```rust
match conn.send_query_last(pid, profile_arg.as_deref().unwrap_or("")) {
```

This sends an empty string when no `--profile` was given (bare `web last`), and
the explicit profile name when `--profile` was given. The GUI already treats
empty profile as "no filter" (line 804 of `xpc.zig`).

### Test

1. Open TermSurf, run `web google.com` (default profile)
2. Open a split, run `web --profile work example.com`
3. Open a split, run `web last` — should return work profile info (most recent)
4. Run `web last --profile default` — **expected to fail** (this experiment only
   fixes bare `web last`; per-profile search is a separate experiment)
5. Run `web last --profile work` — should work
6. Run `web devtools` — should still auto-target correctly

### Result: SUCCESS

Bare `web last` now returns the most recently active browser pane regardless of
profile. The fix: `--profile` changed from `default_value = "default"` to
`Option<String>`. Bare `web last` sends an empty profile string, which the GUI
treats as "no filter" — returning whatever `last_browser_pane` points to.

`web last --profile default` with multiple profiles still fails (expected —
per-profile search is a separate experiment).

## Conclusion

Experiment 1 fixed the primary multi-profile bug: bare `web last` now works
regardless of which profile was most recently active. The root cause was
`--profile` having `default_value = "default"` in clap, so every `web last` call
sent a profile filter even when the user didn't ask for one. Changing it to
`Option<String>` lets bare `web last` send no filter, hitting the GUI's
unfiltered path.

### What works now

- `web last` — returns the most recently active browser pane across all profiles
- `web last --profile work` — returns the last pane for a specific profile (if
  it happens to be the global last)
- `web devtools` — auto-targets the most recent browser tab

### Known limitations (deferred)

- **Per-profile search.** `web last --profile default` fails when "work" was
  opened last. The GUI only checks the single `last_browser_pane` global against
  the filter — it doesn't search all panes. Fix: iterate panes or maintain a
  per-profile map. Low priority — explicit `web devtools://N` works for this
  case.
- **DevTools error feedback.** `web devtools` with no browser tab open hangs
  forever. The GUI silently cleans up the pane without sending an error back to
  the TUI. Fix: send an `error` XPC message so the TUI can exit immediately.
- **Single global tracker.** `last_browser_pane` is a single variable. With many
  profiles and panes, it only remembers the most recent one. This is sufficient
  for the common case (inspect the tab you just opened) but not for complex
  multi-profile workflows.
