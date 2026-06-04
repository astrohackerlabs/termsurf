+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 464: grow the Config struct with the notification group

## Description

Continuing the incremental growth of the aggregating `Config` struct
(Experiments 461–463), this experiment adds the **notify-on-command-finish**
group: `notify_on_command_finish` and `notify_on_command_finish_action` — both
already-ported leaf types (`NotifyOnCommandFinish`,
`NotifyOnCommandFinishAction`). It adds the two fields and their upstream
`Config`-field defaults to `Config` and its `Default`. The parser and the rest
of upstream `Config` stay deferred.

## Upstream behavior

In `config/Config.zig`, the notification group's field defaults:

```zig
@"notify-on-command-finish": NotifyOnCommandFinish = .never,
@"notify-on-command-finish-action": NotifyOnCommandFinishAction = .{
    .bell = true,
    .notify = false,
},
```

`notify-on-command-finish` defaults to `.never`;
`notify-on-command-finish-action` defaults to
`{ .bell = true, .notify = false }` (which is also the struct's own field
defaults).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
pub(crate) struct Config {
    // ... clipboard (461), mouse/click (462), shell-integration (463) ...
    /// `notify-on-command-finish`.
    pub notify_on_command_finish: NotifyOnCommandFinish,
    /// `notify-on-command-finish-action`.
    pub notify_on_command_finish_action: NotifyOnCommandFinishAction,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // ... earlier groups ...
            notify_on_command_finish: NotifyOnCommandFinish::Never,
            notify_on_command_finish_action: NotifyOnCommandFinishAction::default(),
        }
    }
}
```

The defaults are upstream's Config-field defaults: `notify-on-command-finish`
`Never`, and `notify-on-command-finish-action` the
`{ bell = true, notify = false }` literal — which
`NotifyOnCommandFinishAction::default()` (Experiment 450) implements exactly.

## Scope / faithfulness notes

- **Ported (bridged)**: the notification field group of the aggregating `Config`
  struct (upstream `config.Config`) — the two fields and their `Default`.
- **Faithful**: the two fields use the already-ported types
  (`NotifyOnCommandFinish`, `NotifyOnCommandFinishAction`); their `Default`
  values match upstream's Config-field defaults (`.never`;
  `{ bell = true, notify = false }` = `NotifyOnCommandFinishAction::default()`).
- **Faithful adaptation**: the `notify-on-command-finish-action` field default
  literal (`bell = true`, `notify = false`) maps to
  `NotifyOnCommandFinishAction::default()` (which Experiment 450 implemented to
  those values). The struct continues to grow one coherent field group per
  experiment. The derive set (`Clone`/`PartialEq`) is unchanged.
- **Deferred**: the rest of upstream `Config`'s fields (added group by group in
  later slices), the parser, the `changeConfig` machinery, and the
  conditional-config system. (Consumed by later slices; this experiment grows
  the struct with the notification group.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add the two fields `notify_on_command_finish: NotifyOnCommandFinish` and
     `notify_on_command_finish_action: NotifyOnCommandFinishAction` to `Config`,
     and their defaults (`Never`; `NotifyOnCommandFinishAction::default()`) to
     the `Default` impl.
2. Tests (in `config/mod.rs`):
   - extend the `Config::default()` assertion for the new fields:
     `notify_on_command_finish == NotifyOnCommandFinish::Never`,
     `notify_on_command_finish_action == NotifyOnCommandFinishAction::default()`;
     the existing group defaults still hold.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config_default
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Config` gains the two notification fields, and `Config::default()` sets their
  upstream defaults (`notify-on-command-finish` `Never`;
  `notify-on-command-finish-action` `NotifyOnCommandFinishAction::default()`)
  while the earlier group defaults still hold — a faithful partial of upstream's
  `Config`;
- the tests pass (the new defaults; the existing defaults), and the existing
  tests still pass;
- the rest of upstream `Config` and the parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a default is wrong, a field uses the wrong type, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream:
`notify_on_command_finish = NotifyOnCommandFinish::Never` matches the `.never`
default (`Config.zig:1218`);
`notify_on_command_finish_action = NotifyOnCommandFinishAction::default()` is
the right mapping for upstream's literal `{ bell = true, notify = false }`
(`Config.zig:1232`), which also matches the action struct's own field defaults
(`Config.zig:10221`); the notification group is coherent and self-contained; and
the test plan is adequate (assert the two new defaults and keep the existing
group defaults covered as `Config::default()` grows).

Review artifacts:

- Prompt: `logs/codex-review/20260604-121514-d464-prompt.md` (design)
- Result: `logs/codex-review/20260604-121514-d464-last-message.md` (design)

## Result

**Result:** Pass

The `Config` struct now carries the notification field group.

- `roastty/src/config/mod.rs`: `Config` gains
  `notify_on_command_finish: NotifyOnCommandFinish` and
  `notify_on_command_finish_action: NotifyOnCommandFinishAction`;
  `Config::default()` sets their upstream Config-field defaults —
  `NotifyOnCommandFinish::Never` and `NotifyOnCommandFinishAction::default()`
  (the `{ bell = true, notify = false }` literal).

Test (in `config/mod.rs`): `config_default_clipboard_group` extended to assert
the new notification defaults (`Never`;
`NotifyOnCommandFinishAction::default()`) alongside the existing clipboard,
mouse/click, and shell-integration defaults; the modified-config inequality and
the `Clone`/`PartialEq` round-trip remain.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2952 passed, 0 failed (no regressions; the existing
  `config_default` test was extended).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The aggregating `Config` struct now holds four field groups — clipboard (461),
mouse/click (462), shell-integration (463), and notification — ten fields total.
Like the shell-integration group, the notification group reuses a leaf type's
own `Default` (`NotifyOnCommandFinishAction::default()`, Experiment 450) as the
aggregate's field default, confirming the composition pattern. The parser, the
`changeConfig` machinery, the conditional-config system, and the remaining
upstream `Config` fields stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed `notify_on_command_finish` defaults to `Never`;
`notify_on_command_finish_action` defaults through
`NotifyOnCommandFinishAction::default()`, preserving `bell = true` and
`notify = false`; extending the existing `Config::default()` test is adequate
and keeps the prior groups covered; and the deferred parser / `changeConfig` /
conditional-config work remains properly scoped. No public C ABI/header impact;
nothing needed to change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-121700-r464-prompt.md` (result)
- Result: `logs/codex-review/20260604-121700-r464-last-message.md` (result)
