+++
status = "closed"
opened = "2026-03-11"
closed = "2026-03-11"
+++

# Issue 743: Cmd+R intercepted by Wezboard

## Goal

Pressing Cmd+R in a browser pane should reload the web page, not reload the
Wezboard configuration.

## Background

WezTerm binds Cmd+R to `ReloadConfiguration` — reloading the terminal's config
file. Wezboard inherits this binding. When a user presses Cmd+R while viewing a
web page, Wezboard intercepts the key and reloads its config instead of
forwarding it to the browser.

Cmd+[ and Cmd+] (back/forward) work correctly because they are _not_ in
Wezboard's default key bindings. Unbound keys propagate through
`try_forward_key()` in `termsurf/input.rs` and reach Chromium. Cmd+R never
reaches `try_forward_key()` because `process_key()` matches it to
`ReloadConfiguration` first.

The relevant code path:

1. Key event enters `process_key()` in `keyevent.rs`
2. `try_forward_key()` is called — but for raw key events,
   `only_key_bindings=true` causes it to return `None` immediately
3. Key binding lookup finds `ReloadConfiguration` for Cmd+R
4. Config is reloaded; browser never sees the key

### Why removing the binding is safe

Wezboard already monitors its config file for changes and reloads automatically.
The `ReloadConfiguration` menu item and Cmd+R shortcut are redundant. Removing
Cmd+R from the binding lets it propagate to the browser like Cmd+[/] do. Users
who want manual config reload can still trigger it from the menu (without a
keyboard shortcut) or by saving their config file.

### Where Cmd+R is defined

`wezboard/wezboard-gui/src/commands.rs` (lines 1264–1271):

```rust
ReloadConfiguration => CommandDef {
    brief: "Reload configuration".into(),
    doc: "Reloads the configuration file".into(),
    keys: vec![(Modifiers::SUPER, "r".into())],
    args: &[],
    menubar: &["TermSurf Wezboard"],
    icon: Some("md_reload"),
},
```

The `keys` field binds Cmd+R. The `menubar` field places it in the application
menu. These are independent — removing the key binding doesn't remove the menu
item.

## Experiments

### Experiment 1: Remove Cmd+R from ReloadConfiguration

#### Description

Remove the Cmd+R keyboard shortcut from the `ReloadConfiguration` command
definition. The menu item stays in the "TermSurf Wezboard" menu (without a
shortcut indicator), and the config file auto-reload continues to work. With the
key binding gone, Cmd+R will propagate through `try_forward_key()` to Chromium
like Cmd+[/] already do.

#### Changes

**`wezboard/wezboard-gui/src/commands.rs`** (line 1267)

Change `keys` from `vec![(Modifiers::SUPER, "r".into())]` to `vec![]`:

```rust
ReloadConfiguration => CommandDef {
    brief: "Reload configuration".into(),
    doc: "Reloads the configuration file".into(),
    keys: vec![],
    args: &[],
    menubar: &["TermSurf Wezboard"],
    icon: Some("md_reload"),
},
```

#### Verification

1. `scripts/build.sh wezboard` — builds without errors.
2. Launch Wezboard, open a web page with `web`, press Cmd+R — page reloads.
3. Edit the Wezboard config file and save — config reloads automatically
   (confirming auto-reload still works).
4. Open the "TermSurf Wezboard" menu — "Reload configuration" is present but has
   no keyboard shortcut displayed.

**Result:** Pass

Cmd+R now reloads the web page in the browser pane. Config auto-reload still
works.

#### Conclusion

Removing the key binding was sufficient. Cmd+R propagates through
`try_forward_key()` to Chromium like Cmd+[/] do.

## Conclusion

Removed the Cmd+R keyboard shortcut from the `ReloadConfiguration` command in
`commands.rs`. The menu item remains accessible without a shortcut. Wezboard's
config file auto-reload is unaffected. Cmd+R now reaches the browser and reloads
the page as expected.
