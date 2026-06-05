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

# Experiment 534: the CLI-arg config form (parse_cli_arg)

## Description

Alongside the config-file loader, config is also settable from **CLI arguments**
of the form `--key=value`. This experiment ports the per-arg extraction of
upstream `cli.args.parse` as `config::loader::parse_cli_arg` — the CLI-side
counterpart to `parse_config_line` (Experiment 528), turning one argument into a
`(key, value)` config pair (or signalling a non-flag argument). It needs no
naming decision (unlike `loadDefaultFiles`). The multi-arg driver over
`Config::set` is layered on top later.

## Upstream behavior

`cli.args.parse` (`cli/args.zig:55`), per argument (ignoring the
`parseManuallyHook` / `--help` hooks, which are N-A for roastty config):

```zig
// If this doesn't start with "--" then it isn't a config flag. We don't support
// positional arguments or configuration values set with spaces so this is an error.
if (!mem.startsWith(u8, arg, "--")) {
    // → an "invalid field" diagnostic; continue.
}

var key: []const u8 = arg[2..];
const value: ?[]const u8 = if (mem.indexOf(u8, key, "=")) |idx| value: {
    defer key = key[0..idx];
    break :value key[idx + 1 ..];
} else null;

parseIntoField(T, …, key, value);   // → Config::set(key, value)
```

So, per argument:

- an argument not starting with `--` is **not a config flag** (no positional
  args, no space-separated values) — upstream records an "invalid field"
  diagnostic and continues.
- otherwise `key = arg[2..]`; the **first** `=` splits `key` into
  `(key, value)`; with no `=`, the value is absent.

The `--key=value` / `--key` pair then feeds `parseIntoField` — the roastty
`Config::set(key, value)`.

## Rust mapping (`roastty/src/config/loader.rs`)

```rust
/// Parse one CLI argument into a `(key, value)` config pair (upstream
/// `cli.args.parse`'s per-arg logic). A `--key=value` argument yields
/// `(key, Some(value))` and a `--key` argument yields `(key, None)`; the first `=`
/// splits the key from the value. A non-`--` argument is not a config flag and yields
/// `None` (upstream records an "invalid field" diagnostic). roastty does not support
/// positional arguments or space-separated values.
pub(crate) fn parse_cli_arg(arg: &str) -> Option<(&str, Option<&str>)> {
    let key = arg.strip_prefix("--")?;
    match key.find('=') {
        Some(idx) => Some((&key[..idx], Some(&key[idx + 1..]))),
        None => Some((key, None)),
    }
}
```

`strip_prefix("--")` rejects a non-flag argument (`None`); otherwise the
**first** `=` splits the key from the value (so `--key=a=b` ⇒
`("key", Some("a=b"))`), and a `--key` with no `=` ⇒ `("key", None)`. The
returned slices borrow `arg`.

## Scope / faithfulness notes

- **Ported (bridged)**: the per-arg key/value extraction of `cli.args.parse`, as
  `config::loader::parse_cli_arg`.
- **Faithful**: the `--` flag requirement (a non-`--` arg is not a config flag);
  the `key = arg[2..]` strip; the first-`=` key/value split; the no-`=` no-value
  form.
- **Faithful adaptation**: the iterator's `parseIntoField` call → returning
  `(key, Option<value>)` directly (the roastty driver calls `Config::set`);
  upstream's "invalid field" diagnostic for a non-`--` arg → a `None` result the
  driver records; `parseManuallyHook` / `--help` / the `compatibility` fallback
  are N-A for roastty config and not ported.
- **Deferred**: the multi-arg `Config::set_cli_args` driver (iterating args,
  calling `Config::set`, recording diagnostics for non-flag args and field
  errors); the `loadDefaultFiles` orchestration (pending roastty's config
  naming). `background-image-opacity` stays float-blocked.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/loader.rs`: add `parse_cli_arg`.
2. Tests (in `loader.rs`): `--key=value` ⇒ `("key", Some("value"))`; `--key` ⇒
   `("key", None)`; the first `=` splits (`--key=a=b` ⇒ `("key", Some("a=b"))`);
   an empty value (`--key=`) ⇒ `("key", Some(""))`; a non-`--` arg (`key=value`,
   `-h`) ⇒ `None`; `--` alone ⇒ `("", None)`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty parse_cli_arg
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `parse_cli_arg` reproduces upstream's per-arg extraction (the `--`
  requirement, the `arg[2..]` strip, the first-`=` split, the no-`=` no-value
  form, `None` for a non-flag arg);
- the tests pass (the value / no-value / first-`=` / empty / non-flag / `--`
  cases), and the existing tests still pass;
- the multi-arg driver and `loadDefaultFiles` stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the arg parse diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. The extraction is faithful — upstream requires a `--` prefix, uses
`arg[2..]` as the key payload, and splits on the first `=` if present
(`args.zig:109`/`:125`), so `--key=a=b`, `--key=`, and bare `--` behave exactly
as described. Returning `None` for non-`--` args is the right helper boundary as
long as the future multi-arg driver records the invalid-field diagnostic and
continues; ignoring `parseManuallyHook`, `compatibility`, and `help` is
acceptable for this roastty config slice — without a help concept, `-h` as a
non-flag and `--help` as a normal unknown key is a reasonable narrowing.

Review artifacts:

- Prompt: `logs/codex-review/20260604-192701-d534-prompt.md` (design)
- Result: `logs/codex-review/20260604-192701-d534-last-message.md` (design)

## Result

**Result:** Pass

`config::loader::parse_cli_arg` was added — the per-arg extraction of
`cli.args.parse`: require `--`, strip it, split on the first `=` (preserving an
empty value), and treat a no-`=` arg as a bare flag; a non-`--` arg yields
`None` (the invalid-field diagnostic is the deferred driver's job). The new test
`parse_cli_arg_extracts_flag_key_value` covers the `--key=value` / `--key`
forms, the first-`=` split (`--key=a=b`), an empty value, bare `--`, and the
non-flag args.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3024 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream's per-arg extraction — require
`--`, strip it, split on the first `=`, preserve empty values, and treat no `=`
as a bare flag; the edge-case tests cover the important behavior (`--key=a=b`,
`--key=`, bare `--`, non-flag args); the deferred multi-arg diagnostic driver
boundary is clean; gates are passing. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-192823-r534-prompt.md` (result)
- Result: `logs/codex-review/20260604-192823-r534-last-message.md` (result)

## Conclusion

Both config-source per-item parsers now exist — `parse_config_line` (a
config-file line) and `parse_cli_arg` (a CLI argument) — each producing a
`(key, Option<value>)` pair for `Config::set`. The remaining config work is the
multi-arg **CLI driver** (`Config::set_cli_args` — iterate args, `parse_cli_arg`
→ `Config::set`, recording an "invalid field" diagnostic for a non-flag arg) and
the **`loadDefaultFiles` orchestration** (pending roastty's config naming
decision). `background-image-opacity` stays float-blocked. After the config
subsystem, the entire non-config rewrite remains.
