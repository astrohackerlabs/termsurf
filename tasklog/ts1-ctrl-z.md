# Background/Foreground (ctrl+z / fg) - Deferred

This document explains the ctrl+z/fg feature for webviews, why it was deferred,
and the edge cases that would need to be addressed if revisited.

## Feature Overview

The idea: when viewing a webview, press ctrl+z to hide it and return to the
shell. The CLI process suspends (like any Unix program). Later, type `fg` to
restore the webview and resume where you left off.

```bash
termsurf open example.com
# Webview appears
# Press Esc → ctrl+z
# Webview hides, shell shows "[1]+ Stopped termsurf open example.com"
ls -la  # do some work
fg
# Webview reappears
```

## Proposed Implementation

### Basic Flow

1. User presses ctrl+z in control mode
2. SurfaceView catches it, calls `container.onSuspend?(webviewId)`
3. WebViewManager hides the webview (`isHidden = true`)
4. WebViewManager sends `{"event":"suspended"}` to CLI via socket
5. CLI receives event, calls `raise(SIGTSTP)` to suspend itself
6. Shell detects child stopped, shows "[1]+ Stopped", returns prompt
7. User types `fg`
8. Shell sends SIGCONT to CLI
9. CLI resumes, sends `{"action":"show","data":{"webviewId":"..."}}` via socket
10. WebViewManager unhides the webview, focuses control bar

### Code Locations

- **SurfaceView_AppKit.swift**: Intercept ctrl+z in `keyDown()` (control mode
  only)
- **WebViewContainer.swift**: Add `onSuspend` callback
- **WebViewManager.swift**: Add `suspendWebView()` method
- **main.zig**: Handle "suspended" event, call `raise(SIGTSTP)`, send "show" on
  resume

## Edge Cases and Problems

### Problem 1: Stacked Webviews (Parallel Commands)

When multiple webviews are opened concurrently:

```nushell
["google.com", "github.com"] | par-each { |url| termsurf open $url }
```

This creates:

- ONE shell job (`par-each`)
- TWO CLI processes
- TWO webviews stacked on the same pane

**Issue**: If ctrl+z only suspends the topmost webview:

- CLI #2 stops, but CLI #1 keeps running
- Shell may not show "[1]+ Stopped" because the job isn't fully stopped
- One webview is hidden, but the other is still visible with no way to interact

**Attempted solution**: Suspend ALL webviews on the pane:

- Both webviews hide
- Both CLIs receive "suspended" and stop
- But `par-each` is still waiting for its children
- Shell prompt never returns - user is stuck

The fundamental issue is that `par-each` waits for all children to complete.
When children stop (not exit), `par-each` doesn't propagate this to the shell.
The user cannot type `fg` because the prompt hasn't returned.

### Problem 2: Freeze One, Open Another

Even if we limit ctrl+z to single webviews:

1. User opens webview #1
2. User presses ctrl+z - webview #1 is frozen, hidden
3. User is back at shell prompt
4. User opens webview #2
5. Now there are TWO webviews: one frozen, one active
6. User presses ctrl+z on webview #2
7. Now both are frozen
8. User types `fg` - which one resumes?

**Possible solutions**:

- Prevent opening new webview if one is frozen (restrictive, confusing errors)
- Track frozen webviews separately from active ones (complex state management)
- Let shell job control handle it (each `fg` resumes one job)

None of these are clean solutions.

### Problem 3: Process Group Complexity

We considered tracking process group ID (PGID) to properly group webviews that
belong to the same shell job:

1. CLI includes its PGID (`getpgrp()`) in the open request
2. WebViewManager groups webviews by PGID
3. ctrl+z suspends all webviews with same PGID as topmost

This would correctly handle:

- Parallel commands (same PGID, all suspend together)
- Background jobs (different PGID, not affected)

But this adds significant complexity and still doesn't solve Problem 1 (the
`par-each` blocking issue).

## Why We Chose Stacking Over Freezing

### Stacking Benefits

1. **Already implemented and working** - No additional development needed
2. **Graceful handling of parallel commands** - `par-each` just works, no errors
3. **No artificial restrictions** - Users can open multiple webviews if they
   want
4. **Simple mental model for what it does** - Newer webviews stack on top, close
   from top down
5. **Stack indicator provides visibility** - "(2/3)" shows position in stack

### Freezing Drawbacks

1. **Complex edge cases** - Stacked webviews, frozen + new, parallel commands
2. **Doesn't work with parallel commands** - Fundamental Unix job control
   limitation
3. **Marginal value** - Use cases are covered by pane switching
4. **Artificial restrictions required** - Would need "one webview per pane"
   limit
5. **Error-prone UX** - Parallel opens would fail with confusing errors

### The Deciding Factor

Every ctrl+z use case is already solved by pane switching:

| Use Case             | ctrl+z Solution                   | Pane Switching Solution                                  |
| -------------------- | --------------------------------- | -------------------------------------------------------- |
| Run terminal command | Freeze webview, run command, `fg` | Switch pane (ctrl+l), run command, switch back (ctrl+h)  |
| Check logs           | Freeze to see stdout behind       | Logs stream to stdout in real-time; or use Web Inspector |
| Come back later      | Freeze, resume with `fg`          | Switch panes, webview stays open                         |
| Multiple webviews    | Not supported (conflicts)         | One webview per pane, navigate between                   |

TermSurf is a terminal multiplexer. The "TermSurf way" to do something else
while keeping a webview open is to use another pane, not to freeze the current
process.

## Alternatives That Exist Today

### Pane Switching

```bash
termsurf open example.com   # webview opens
# Press Esc to enter control mode
ctrl+l                      # switch to right pane (or cmd+d to create one)
ls -la                      # do your work
ctrl+h                      # switch back to webview pane
Enter                       # back in browse mode
```

The webview never closes. It stays exactly where you left it.

### Console Output

Console output streams to stdout via the socket connection. Options:

- Redirect to file: `termsurf open example.com > logs.txt 2>&1`
- Use Safari Web Inspector (cmd+alt+i in browse mode)
- Close webview to see accumulated output in terminal

### Multiple Panes

For viewing multiple webviews:

```bash
# Pane 1
termsurf open google.com
# cmd+d to split
# Pane 2
termsurf open github.com
# ctrl+h/l to switch between them
```

Each webview is independent, no stacking complexity.

## Future Considerations

If ctrl+z support is revisited, consider:

1. **Single webview only**: Only support ctrl+z when exactly one webview exists
   on the pane. Error or no-op otherwise.

2. **Remove stacking**: Enforce one webview per pane. Parallel opens would
   error. This simplifies the model but reduces flexibility.

3. **Process group tracking**: Pass PGID from CLI to Swift to properly group
   related webviews. Still doesn't solve the `par-each` blocking issue.

4. **Let terminal handle it**: Don't intercept ctrl+z in Swift. Let it pass
   through to terminal driver. Would need webviews to detect CLI stopped and
   auto-hide. Complex and fragile.

5. **Different keybinding**: Use a different key (not ctrl+z) that doesn't
   conflict with Unix job control semantics. For example, a custom "minimize
   webview" command.

## Decision

**Status**: Deferred

**Reason**: The complexity of handling edge cases (stacked webviews, parallel
commands, frozen + new) is not justified given that pane switching already
covers the use cases. Stacking is implemented and working, providing a graceful
experience for parallel commands.

**Date**: January 2026
