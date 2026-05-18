+++
status = "closed"
opened = "2026-02-20"
closed = "2026-02-20"
+++

# Issue 605: Two Profiles

## Goal

Two terminal panes side by side, each running `web` with a different
`--profile`, rendering live Chromium frames simultaneously. Two separate
Chromium Profile Server processes, each with its own `--user-data-dir`,
streaming at 60fps to independent grid overlays.

## Background

Issue 604 proved the multi-pane architecture works: serial dispatch queue,
`servers` HashMap keyed by profile, `panes` HashMap keyed by pane UUID,
`display_surface` routing by `pane_id`, per-pane disconnect cleanup. Three panes
on the same profile shared one server process at 60fps each.

But Issue 604 only tested same-profile panes. The `getOrCreateServer` function
is designed to spawn a new server when it sees a new profile name, and `web`
already accepts `--profile <name>` — but this path has never been exercised.

### What we have

- `getOrCreateServer(profile)` looks up `servers` by profile name. If not found,
  spawns a new Chromium Profile Server with
  `--user-data-dir=~/.config/termsurf/chromium-profiles/{profile}`
- `web` accepts `--profile <name>` (defaults to `"default"`) and sends it in
  `set_overlay`
- `display_surface` routes by `pane_id`, not profile — frames from different
  servers already go to the right panes
- Per-pane disconnect decrements the correct server's `pane_count`; server is
  killed when its count reaches zero

### What should work without changes

Everything. The multi-profile path is already implemented in `xpc.zig` and
`web`. This issue is a verification exercise.

## Experiment 1: Two profiles side by side

### Goal

Two panes, two profiles, two servers, both rendering the box demo at 60fps.

### Design

No code changes. Just run:

```bash
cd ts4/box-demo && bun run server.ts &
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
# In pane 1:
cargo run -p web -- http://localhost:9407 --profile default
# Split pane, in pane 2:
cargo run -p web -- http://localhost:9407 --profile work
```

### Verification

Pass criteria:

- Both panes render the box demo simultaneously at 60fps
- Two `chromium_profile_server` processes running (one per profile)
- Each server has a different `--user-data-dir`
- Closing one pane kills only its server
- Closing the other pane kills the remaining server
- No errors in Ghost logs

### Result: Pass

Two profiles side by side, zero code changes. The multi-profile path implemented
in Issue 604 worked on the first try.

## Conclusion

Issue 605 is complete. Two different profiles run as two independent Chromium
Profile Server processes, each with isolated `--user-data-dir`, streaming to
separate panes at 60fps. No code changes were needed — Issue 604's architecture
handled multi-profile out of the box.
