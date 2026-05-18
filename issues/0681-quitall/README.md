+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 681: Quit All and Shortest-Match Dispatch

Typing `:qa` in command mode is a no-op. Vim muscle memory expects `:qa` to quit
all. TermSurf has only one TUI instance per pane, so "quit all" and "quit" are
the same thing — but `:qa` should still work.

## Problem

The current command dispatch uses unique prefix matching: if exactly one command
starts with the typed prefix, it executes. If zero or multiple match, it's a
no-op. Adding `quitall` to the COMMANDS table would make `:q` ambiguous (matches
both `quit` and `quitall`) and break `:q`.

## Solution: Subsequence Matching + Shortest-Match Priority

Replace prefix matching (`starts_with`) with subsequence matching: every
character in the input must appear in the command name in order, but not
necessarily contiguously. This is how vim handles abbreviations like `:qa` for
`:quitall` — the `q` and `a` appear in order in **q**uit**a**ll.

When multiple commands match:

1. **Exact match** wins (`:quit` → `quit`, even if `quitall` also matches)
2. **Shortest name** wins (`:q` → `quit` over `quitall`)
3. **Unique match** works as before (`:col` → `colorscheme`)

## Experiment 1: Shortest-match dispatch + quitall

### Hypothesis

Changing the dispatch function to prefer the shortest matching command when
multiple match, and adding a `quitall` command, will make `:q`, `:qa`, `:quit`,
and `:quitall` all work.

### Changes

#### 1. Add subsequence matcher (`tui/src/main.rs`)

```rust
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    for c in needle.chars() {
        loop {
            match hay.next() {
                Some(h) if h == c => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}
```

#### 2. Update dispatch logic (`tui/src/main.rs`)

Replace `starts_with` with `is_subsequence`, add shortest-match tiebreaker:

```rust
let matches: Vec<&Command> = COMMANDS
    .iter()
    .filter(|c| is_subsequence(prefix, c.name))
    .collect();
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

#### 3. Add `quitall` command (`tui/src/main.rs`)

```rust
Command {
    name: "quitall",
    exec: |_| CommandResult::Quit,
},
```

### Test

1. `:q` → quits (subsequence of `quit` and `quitall`, shortest wins)
2. `:qa` → quits (subsequence of `quitall` only)
3. `:qall` → quits (subsequence of `quitall` only)
4. `:quit` → quits (exact match)
5. `:quita` → quits (subsequence of `quitall` only)
6. `:quitall` → quits (exact match)
7. `:col d` → still works (subsequence of `colorscheme`)

### Result: SUCCESS

All vim-style abbreviations work. `:q`, `:qa`, `:qall`, `:quit`, `:quita`, and
`:quitall` all quit. `:col d` still switches to dark mode.

## Conclusion

Command dispatch now uses subsequence matching instead of prefix matching. Any
ordered character sequence from a command name is a valid abbreviation — `:qa`
matches `quitall` because **q**uit**a**ll. When multiple commands match, exact
matches win first, then the shortest command name wins. This matches vim
conventions and scales naturally as new commands are added.

### Files changed

- `tui/src/main.rs` — `is_subsequence` function, updated `dispatch` with
  subsequence filter and shortest-match tiebreaker, added `quitall` command
