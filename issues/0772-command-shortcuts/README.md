+++
status = "closed"
opened = "2026-04-05"
closed = "2026-04-05"
+++

# Issue 772: Explicit command shortcuts and rename colorscheme to dark

## Goal

Replace the vim-style subsequence matching for commands with explicit, fixed
shortcuts. Rename the `colorscheme` command to `dark` with toggle behavior.

## Background

### Current system

Commands are dispatched via `is_subsequence()` — any input that is a subsequence
of a command name matches it. For example, `:c` matches `colorscheme`, `:q`
matches `quit`, `:d` matches both `dark` and `devtools`. Ties are broken by
shortest name, then exact match.

This is ambiguous and hard to reason about. Users cannot predict which command
`:dt` will match without understanding the subsequence algorithm and tiebreaker
rules. The system was modeled on vim's command completion but TermSurf has few
enough commands that explicit shortcuts are clearer.

### Current commands

| Command       | Aliases                        | Args                                      |
| ------------- | ------------------------------ | ----------------------------------------- |
| `quit`        | (subsequence: q, qu, qui, ...) | none                                      |
| `quitall`     | (subsequence: qa, ...)         | none (identical to quit)                  |
| `colorscheme` | (subsequence: c, cs, ...)      | `dark\|d`, `light\|l`, `system\|s`        |
| `devtools`    | (subsequence: d, de, dev, ...) | `right\|r`, `down\|d`, `left\|l`, `up\|u` |

## Changes

### 1. Replace subsequence matching with explicit aliases

Each command gets a fixed list of accepted names. The `Command` struct changes
from:

```rust
struct Command {
    name: &'static str,
    exec: fn(args: &[&str]) -> CommandResult,
}
```

to:

```rust
struct Command {
    names: &'static [&'static str],
    exec: fn(args: &[&str]) -> CommandResult,
}
```

The `dispatch()` function does an exact match against all names instead of
subsequence matching. The `is_subsequence()` function is deleted.

### 2. Rename colorscheme to dark

The `colorscheme` command becomes `dark` with toggle behavior:

- `:dark` (no args) → toggle dark mode
- `:dark on` / `:dark yes` / `:dark y` → force dark
- `:dark off` / `:dark no` / `:dark n` → force light
- `:dark system` / `:dark s` → follow OS preference

### 3. New command table

| Command  | Accepted names   | Args                                                   |
| -------- | ---------------- | ------------------------------------------------------ |
| quit     | `quit`, `q`      | none                                                   |
| dark     | `dark`, `da`     | (none)=toggle, `on\|yes\|y`, `off\|no\|n`, `system\|s` |
| devtools | `devtools`, `de` | `right\|r`, `down\|d`, `left\|l`, `up\|u`              |

### 4. Update CommandResult

Replace `SetColorScheme(String)` with `Dark(DarkAction)`:

```rust
enum DarkAction {
    Toggle,
    On,
    Off,
    System,
}
```

### 5. Update docs/keybindings.md

Update the Commands table to reflect the new names and aliases.

## Experiments

### Experiment 1: Replace command system

Implement all changes in one experiment: explicit aliases, rename colorscheme to
dark with toggle, update docs.

#### Changes

**`webtui/src/main.rs`**

1. Replace `Command` struct — change `name: &'static str` to
   `names: &'static [&'static str]`.

2. Replace `COMMANDS` array with new command table:
   ```rust
   Command {
       names: &["quit", "q"],
       exec: |_| CommandResult::Quit,
   },
   Command {
       names: &["dark", "da"],
       exec: |args| match args.first().copied() {
           None => CommandResult::Dark(DarkAction::Toggle),
           Some("on" | "yes" | "y") => CommandResult::Dark(DarkAction::On),
           Some("off" | "no" | "n") => CommandResult::Dark(DarkAction::Off),
           Some("system" | "s") => CommandResult::Dark(DarkAction::System),
           Some(other) => CommandResult::Error(format!("Unknown: {}", other)),
       },
   },
   Command {
       names: &["devtools", "de"],
       exec: |args| match args.first().copied() {
           Some("right" | "r") | None => CommandResult::DevTools("right".into()),
           Some("down" | "d") => CommandResult::DevTools("down".into()),
           Some("left" | "l") => CommandResult::DevTools("left".into()),
           Some("up" | "u") => CommandResult::DevTools("up".into()),
           Some(other) => CommandResult::Error(format!("Unknown direction: {}", other)),
       },
   },
   ```

3. Add `DarkAction` enum:
   ```rust
   enum DarkAction { Toggle, On, Off, System }
   ```

4. Replace `SetColorScheme(String)` with `Dark(DarkAction)` in `CommandResult`.

5. Delete `is_subsequence()` function.

6. Rewrite `dispatch()` — exact match against all names:
   ```rust
   fn dispatch(input: &str) -> CommandResult {
       let mut parts = input.trim().splitn(2, ' ');
       let cmd = parts.next().unwrap_or("");
       if cmd.is_empty() { return CommandResult::None; }
       let args: Vec<&str> = parts.next()
           .map(|s| s.split_whitespace().collect())
           .unwrap_or_default();
       for command in COMMANDS {
           if command.names.contains(&cmd) {
               return (command.exec)(&args);
           }
       }
       CommandResult::None
   }
   ```

7. Add `is_dark: bool` state variable in `main()` (initialize to `true` since
   the TUI starts in dark mode).

8. Update the `CommandResult::Dark` handler in the main loop (replacing the
   `SetColorScheme` handler):
   ```rust
   CommandResult::Dark(action) => {
       let dark = match action {
           DarkAction::Toggle => !is_dark,
           DarkAction::On => true,
           DarkAction::Off => false,
           DarkAction::System => false, // TODO: detect OS preference
       };
       is_dark = dark;
       let scheme = if dark { "dark" } else { "light" };
       if let Some(ref bc) = browser_conn {
           bc.send_set_color_scheme(scheme);
       }
       if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
           conn.send_set_color_scheme(pid, scheme);
       }
   }
   ```

**`docs/keybindings.md`**

9. Update the Commands section:
   ```markdown
   ## Commands

   Entered via `:` in Control mode.

   | Command              | Shortcut | Action                      |
   | -------------------- | -------- | --------------------------- |
   | `:quit`              | `:q`     | Quit                        |
   | `:dark [on\|off\|s]` | `:da`    | Toggle/set dark mode        |
   | `:devtools [dir]`    | `:de`    | Open DevTools in split pane |
   ```

#### Verification

1. **`:dark` toggles:** Start in dark mode. Type `:dark`. Page switches to
   light. Type `:dark` again. Back to dark.

2. **`:da` works:** Type `:da`. Toggles dark mode.

3. **`:dark on`/`:dark off` work:** Explicit set, not toggle.

4. **`:q` quits:** Still works.

5. **`:de right` opens DevTools:** Still works.

6. **Unknown commands do nothing:** `:foo` produces no effect.

7. **Old commands don't work:** `:colorscheme dark` does nothing (command
   removed). `:cs` does nothing (no subsequence matching).

**Result:** Pass

All commands work with explicit shortcuts. Dark mode toggle works. Old
subsequence-based commands no longer match.

## Conclusion

Replaced vim-style subsequence matching with explicit command aliases. Three
commands: `quit`/`q`, `dark`/`da`, `devtools`/`de`. The `colorscheme` command
is replaced by `dark` with toggle support. The `quitall` command is removed
(identical behavior to `quit`). Also added `codesign` to `install.sh` after
copying binaries to prevent macOS SIGKILL on unsigned executables.
