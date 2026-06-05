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

# Experiment 535: the CLI-args driver (Config::set_cli_args)

## Description

With the per-arg parser (`parse_cli_arg`, Experiment 534) and `Config::set` (43
of 44 fields), this experiment ports the multi-arg **CLI driver** —
`Config::set_cli_args` — the CLI counterpart to `Config::load_str` (Experiment
529). It iterates the arguments, applies each `--key=value` via `Config::set`,
records an "invalid field" diagnostic for a non-flag argument, and collects
per-arg diagnostics (continuing rather than aborting). This is the last
config-source driver.

## Upstream behavior

Upstream `cli.args.parse` (`cli/args.zig:55`) iterates the args:

- a non-`--` argument is **not a config flag** — it appends a diagnostic
  (`key = arg`, message `"invalid field"`, the iterator's location) and
  continues.
- otherwise `parse_cli_arg` extracts `(key, value)` and `parseIntoField` is
  called; on error it appends a diagnostic and continues.

So loading the CLI args is: for each arg (positionally), `parse_cli_arg`; if it
yields `(key, value)`, `Config::set(key, value)`, recording a diagnostic on
error; if it is a non-flag arg, record an "invalid field" diagnostic. The loader
never aborts on a bad arg.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl Config {
    /// Apply config from CLI arguments (upstream `cli.args.parse` over args): for each
    /// argument, parse the `--key=value` form (`parse_cli_arg`) and apply it via
    /// `Config::set`; a non-flag argument or a `Config::set` error records a
    /// diagnostic, and the loop continues. The diagnostic's `line` is the 1-based
    /// argument position.
    pub(crate) fn set_cli_args<'a, I>(&mut self, args: I) -> Vec<ConfigDiagnostic>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut diagnostics = Vec::new();
        for (i, arg) in args.into_iter().enumerate() {
            match loader::parse_cli_arg(arg) {
                Some((key, value)) => {
                    if let Err(error) = self.set(key, value) {
                        diagnostics.push(ConfigDiagnostic { line: i + 1, key: key.to_string(), error });
                    }
                }
                // A non-flag argument is not a valid config field.
                None => diagnostics.push(ConfigDiagnostic {
                    line: i + 1,
                    key: arg.to_string(),
                    error: ConfigSetError::UnknownField,
                }),
            }
        }
        diagnostics
    }
}
```

Each argument is `parse_cli_arg`-parsed; a `--key=value` arg drives
`Config::set` (recording a diagnostic on a field error); a non-flag arg records
an `UnknownField`-kind diagnostic (the roastty analog of upstream's "invalid
field"). The loop continues past errors; `ConfigDiagnostic.line` carries the
**1-based argument position** (the CLI analog of a file line, reusing the
existing diagnostic type).

## Scope / faithfulness notes

- **Ported (bridged)**: the multi-arg CLI driver of `cli.args.parse`, as
  `Config::set_cli_args`.
- **Faithful**: per-arg iteration; `--key=value` ⇒ `Config::set`; a non-flag arg
  ⇒ an "invalid field" diagnostic; **continue past errors**, collecting a
  diagnostic per failing arg — matching upstream's `parse` (record + continue).
- **Faithful adaptation**: upstream's iterator + `Location` → iterating the args
  with `enumerate()` and reusing `ConfigDiagnostic` with `line` = the 1-based
  argument position; upstream's "invalid field" message for a non-flag arg →
  `ConfigSetError::UnknownField` with the arg as the key (roastty's coarser
  error model, the same "not a valid field" outcome); the `parseManuallyHook` /
  `--help` / `compatibility` hooks are N-A for roastty config.
- **Input contract**: `set_cli_args` receives the **config** arguments.
  Upstream's outer process-args wrapper skips action arguments beginning with
  `+` before the config `parse` sees them; that `+`-arg filtering is a separate
  outer layer (not this driver), so a `+action` passed here would be reported as
  an invalid field.
- **Deferred**: the `loadDefaultFiles` orchestration (pending roastty's config
  naming); a source-aware diagnostic `Location` (file-line vs CLI-arg) — `line`
  doubles as both. `background-image-opacity` stays float-blocked.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `Config::set_cli_args`.
2. Tests (in `config/mod.rs`): a list of `--key=value` args applies each field
   (verified via `format_config`) with no diagnostics; a bare-flag arg
   (`--background-image-repeat` ⇒ `true`); a non-flag arg and an invalid field
   record diagnostics with the correct 1-based positions while the other args
   still apply (continue past errors).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config_set_cli_args
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Config::set_cli_args` applies each `--key=value` arg via `Config::set`,
  records an "invalid field" diagnostic for a non-flag arg, and collects a
  diagnostic per failing arg (1-based position) while continuing — faithful to
  upstream's `parse`;
- the tests pass (a clean args apply + an apply with errors and correct
  positions), and the existing tests still pass;
- the `loadDefaultFiles` orchestration stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the driver diverges from upstream (esp. aborting on
an error or mis-positioning diagnostics), an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (folded into the scope notes): document the `+…` action-arg
behavior — upstream's process-args wrapper increments the CLI index but skips
args beginning with `+` before `parse` sees them (`args.zig:1322`/`:1346`);
`set_cli_args` receives the already-filtered **config** args, so the `+`-arg
filtering is a separate outer layer (a `+action` passed here would be reported
as an invalid field).

Codex found everything else faithful: reusing `ConfigDiagnostic.line` as a
1-based CLI argument position is acceptable for this coarser diagnostic model
(upstream's CLI location is also 1-indexed — `index` starts at 0 and increments
before yielding, `args.zig:1335`/`:1359`); a source-aware location type would be
more precise but is not required for this slice; the non-`--` path recording
`key = arg` with a coarse `UnknownField`/invalid-field diagnostic is an
acceptable narrowing of upstream's distinct `"invalid field"` message
(`args.zig:109`); and continue-past-errors is faithful — upstream appends
diagnostics and continues for both non-flags and parse errors
(`args.zig:115`/`:173`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-193048-d535-prompt.md` (design)
- Result: `logs/codex-review/20260604-193048-d535-last-message.md` (design)

## Result

**Result:** Pass

`Config::set_cli_args` was added — the multi-arg CLI driver. For each argument
it parses the `--key=value` form (`parse_cli_arg`) and applies it via
`Config::set`, recording a `ConfigDiagnostic` (with the 1-based argument
position) for a non-flag argument or a field error and continuing. The new test
`config_set_cli_args_applies_and_collects_diagnostics` covers a clean apply
(including a bare-flag arg ⇒ `true`) with no diagnostics, and an apply with
errors producing the correct positioned diagnostics (a non-flag arg, an unknown
key, an invalid value) while the good args still apply.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3025 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches the approved CLI driver slice — 1-based
argument positions, `--key[=value]` extraction into `Config::set`, diagnostics
collected on errors, and continued processing after each failure; the non-flag
handling is a faithful coarse diagnostic adaptation, and the `+` action-arg
behavior is documented as an outer-layer filtering contract; the tests cover
clean application, bare bool args, non-flag diagnostics, unknown keys, invalid
values, and good-lines-still-apply; gates are clean. "Approved with no
findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-193325-r535-prompt.md` (result)
- Result: `logs/codex-review/20260604-193325-r535-last-message.md` (result)

## Conclusion

All three config sources now have drivers — `Config::load_str` (a config
string), `Config::load_file` (a config file), and `Config::set_cli_args` (CLI
arguments) — each applying over the 43-of-44-field `Config::set` and collecting
diagnostics. The only remaining config piece is the **`loadDefaultFiles`
orchestration**, which needs roastty's concrete config naming (the XDG
subdir/filename and the macOS bundle id) and the config-template content — an
unmade product decision, so it stays deferred until that is settled; every
building block for it (the path resolvers, `load_optional_file`, `load_file`) is
in place. `background-image-opacity` stays float-blocked. After the config
subsystem, the entire non-config rewrite remains.
