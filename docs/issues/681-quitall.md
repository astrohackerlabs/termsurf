# Issue 681: Quit All and Shortest-Match Dispatch

Typing `:qa` in command mode is a no-op. Vim muscle memory expects `:qa` to quit
all. TermSurf has only one TUI instance per pane, so "quit all" and "quit" are
the same thing — but `:qa` should still work.

## Problem

The current command dispatch uses unique prefix matching: if exactly one command
starts with the typed prefix, it executes. If zero or multiple match, it's a
no-op. Adding `quitall` to the COMMANDS table would make `:q` ambiguous (matches
both `quit` and `quitall`) and break `:q`.

## Solution: Shortest-Match Priority

When multiple commands match a prefix:

1. **Exact match** wins (`:quit` → `quit`, even if `quitall` also exists)
2. **Shortest name** wins (`:q` → `quit` over `quitall`)
3. **Unique prefix** works as before (`:col` → `colorscheme`)

This matches vim's behavior where shorter commands take priority over longer
variants, and scales as more commands are added.

## Experiment 1: Shortest-match dispatch + quitall

### Hypothesis

Changing the dispatch function to prefer the shortest matching command when
multiple match, and adding a `quitall` command, will make `:q`, `:qa`, `:quit`,
and `:quitall` all work.

### Changes

#### 1. Update dispatch logic (`tui/src/main.rs`)

Replace the ambiguity check with shortest-match priority:

```rust
match matches.len() {
    0 => CommandResult::None,
    1 => (matches[0].exec)(&args),
    _ => {
        if let Some(cmd) = matches.iter().find(|c| c.name == prefix) {
            (cmd.exec)(&args)
        } else {
            let shortest = matches.iter().min_by_key(|c| c.name.len()).unwrap();
            (shortest.exec)(&args)
        }
    }
}
```

#### 2. Add `quitall` command (`tui/src/main.rs`)

```rust
Command {
    name: "quitall",
    exec: |_| CommandResult::Quit,
},
```

### Test

1. `:q` → quits (shortest match: `quit` over `quitall`)
2. `:qa` → quits (only matches `quitall`)
3. `:quit` → quits (exact match)
4. `:quitall` → quits (exact match)
5. `:col d` → still works (unique prefix, unaffected)
6. `:colorscheme light` → still works (exact match)
