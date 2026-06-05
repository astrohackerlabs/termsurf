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

# Experiment 531: the XDG config-directory resolution (xdg_config_dir)

## Description

Toward the default config-path resolution (`loadDefaultFiles`), this experiment
ports its app-name-agnostic primitive: the **XDG config-directory** resolution —
upstream `internal_os.xdg.config` (via `xdg.dir`). It resolves a config path
from `$XDG_CONFIG_HOME` (or `$HOME/.config`) plus a subdir. `loadDefaultFiles`'s
orchestration (the macOS app-support paths, the legacy paths, the dedup
warnings, the template creation) and roastty's concrete subdir name come in
following experiments.

## Upstream behavior

`xdg.config` → `xdg.dir` (`os/xdg.zig:23`, `:56`), on **macOS** (non-Windows):

```zig
pub fn config(alloc, opts) ![]u8 {
    return try dir(alloc, opts, .{ .env = "XDG_CONFIG_HOME", .default_subdir = ".config" });
}

fn dir(alloc, opts, internal_opts) ![]u8 {
    if (opts.home) |home| return join(home, ".config", opts.subdir orelse "");   // explicit home
    const env_ = getenvNotEmpty(internal_opts.env);   // XDG_CONFIG_HOME (non-empty)
    if (env_) |env| {
        if (opts.subdir) |subdir| return join(env.value, subdir);
        return dupe(env.value);                       // env as-is when no subdir
    }
    if (homedir.home(&buf)) |home| return join(home, ".config", opts.subdir orelse "");
    return error.NoHomeDir;
}
```

So the config dir is:

- `$XDG_CONFIG_HOME` when set and **non-empty** — joined with the subdir (or
  used as-is when there is no subdir); **else**
- `$HOME/.config` joined with the subdir; **else**
- `error.NoHomeDir`.

(`getenvNotEmpty` treats an empty env value as unset. `homedir.home` resolves
`$HOME` with a `getpwuid` fallback — that fallback is essentially never needed
on a macOS GUI session and is deferred.)

## Rust mapping (`roastty/src/config/loader.rs`)

A pure core (fully testable without mutating the process environment) plus an
env-reading wrapper:

```rust
/// Resolve the XDG config directory from explicit env values (upstream `xdg.dir`'s
/// core for macOS): `$XDG_CONFIG_HOME` (joined with `subdir`, or used as-is with no
/// subdir) when present; else `$HOME/.config` joined with `subdir`; else `None`
/// (upstream `error.NoHomeDir`). `xdg_config_home` / `home` are the **non-empty**
/// env values (`None` when unset or empty).
fn resolve_xdg_config(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
    subdir: Option<&str>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home {
        let mut p = PathBuf::from(xdg);
        if let Some(s) = subdir {
            p.push(s);
        }
        return Some(p);
    }
    if let Some(home) = home {
        let mut p = PathBuf::from(home);
        p.push(".config");
        if let Some(s) = subdir {
            p.push(s);
        }
        return Some(p);
    }
    None
}

/// The XDG config directory (upstream `internal_os.xdg.config` for macOS): reads
/// `$XDG_CONFIG_HOME` / `$HOME` from the environment and resolves the config path.
pub(crate) fn xdg_config_dir(subdir: Option<&str>) -> Option<PathBuf> {
    fn env_nonempty(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
    resolve_xdg_config(
        env_nonempty("XDG_CONFIG_HOME").as_deref(),
        env_nonempty("HOME").as_deref(),
        subdir,
    )
}
```

`resolve_xdg_config` is the pure logic; `xdg_config_dir` reads the (non-empty)
env values. `$XDG_CONFIG_HOME` takes precedence over `$HOME/.config`; an empty
env value is treated as unset; neither set ⇒ `None`.

## Scope / faithfulness notes

- **Ported (bridged)**: `internal_os.xdg.config` / `xdg.dir`'s macOS core, as
  `config::loader::xdg_config_dir` (+ the pure `resolve_xdg_config`).
- **Faithful**: `$XDG_CONFIG_HOME` precedence (non-empty), joined with the
  subdir (or used as-is with no subdir); the `$HOME/.config` fallback joined
  with the subdir; the `None` (`NoHomeDir`) case.
- **Faithful adaptation**: Zig allocator-returned `[]u8` → `PathBuf`;
  `getenvNotEmpty` → an env-var read filtered to non-empty; `std.fs.path.join` →
  `PathBuf::push`.
- **Documented narrowings / deferred**:
  - the `opts.home` explicit-home override (a caching/test hook) is not ported —
    the pure core's explicit params serve testing.
  - the `homedir.home` `getpwuid` fallback (when `$HOME` is unset) is not
    modeled — unreachable on a macOS GUI session; `None` is returned instead.
  - the Windows `LOCALAPPDATA` fallback is N-A (macOS-only).
- **Deferred**: roastty's concrete config subdir/filename; the macOS app-support
  path (`macos.appSupportDir`); the legacy paths; the `loadDefaultFiles`
  orchestration (load-all, dedup warnings, template creation); the `--key=value`
  CLI form. `background-image-opacity` stays float-blocked.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/loader.rs`: add `resolve_xdg_config` and
   `xdg_config_dir`.
2. Tests (in `loader.rs`): `resolve_xdg_config` — `$XDG_CONFIG_HOME` set ⇒
   `xdg/subdir` (precedence over `$HOME`); set with no subdir ⇒ `xdg` as-is;
   only `$HOME` set ⇒ `home/.config/subdir`; `$HOME` with no subdir ⇒
   `home/.config`; neither ⇒ `None`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty xdg_config
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `resolve_xdg_config` reproduces upstream's macOS config-dir logic
  (`$XDG_CONFIG_HOME` precedence with the subdir / as-is, the `$HOME/.config`
  fallback, the `None` case), and `xdg_config_dir` reads the non-empty env
  values;
- the tests pass (the precedence / no-subdir / home-fallback / neither cases),
  and the existing tests still pass;
- the app-support paths and `loadDefaultFiles` orchestration stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the resolution diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. For the macOS/non-Windows path with no `opts.home`, the precedence
is correct — non-empty `XDG_CONFIG_HOME` wins, else the user home dir plus
`.config`, else `NoHomeDir` (`xdg.zig:70`/`:91`); `getenvNotEmpty` treating
empty env vars as unset is faithful (`env.zig:102`). The subdir behavior is
modeled correctly: with the XDG env present, upstream returns it as-is with no
subdir, or joins `env/subdir` with one (`xdg.zig:79`); the home fallback always
includes `.config`, with the optional subdir after it (`xdg.zig:93`). The
narrowings are acceptable for this macOS slice (no explicit `opts.home`, no
`getpwuid` fallback, no Windows `LOCALAPPDATA`), and splitting a pure resolver
from the env-reading wrapper avoids racy env mutation in tests — with the `home`
parameter documented as the `$HOME` fallback input (not Zig's higher-precedence
`opts.home`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-191100-d531-prompt.md` (design)
- Result: `logs/codex-review/20260604-191100-d531-last-message.md` (design)

## Result

**Result:** Pass

`resolve_xdg_config` (pure) and `xdg_config_dir` (env-reading wrapper) were
added to `config/loader.rs`. The resolution matches upstream's macOS XDG core:
non-empty `$XDG_CONFIG_HOME` wins (used as-is with no subdir, or joined with
one), else `$HOME/.config[/subdir]`, else `None`. The new test
`resolve_xdg_config_precedence_and_fallback` covers the precedence, no-subdir,
home-fallback, and no-home cases.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3021 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches the approved macOS XDG core — non-empty
`XDG_CONFIG_HOME` wins (returned as-is or joined with the subdir), else
`$HOME/.config[/subdir]`, else `None`; the doc clarification that `home` is the
`$HOME` fallback (not Zig's higher-priority `opts.home`) resolves the only
ambiguity; the tests cover precedence, no-subdir, home-fallback, and no-home;
gates are clean and the remaining default-path orchestration is deferred.
"Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-191304-r531-prompt.md` (result)
- Result: `logs/codex-review/20260604-191304-r531-last-message.md` (result)

## Conclusion

The XDG config-directory resolution is ported. The next building block is the
**macOS app-support path** (`macos.appSupportDir` —
`~/Library/Application Support/<bundle>`), then the `loadDefaultFiles`
orchestration (resolve the XDG + app-support candidate paths, `load_file` each
that exists, warn on duplicates, write a template when none exist) with
roastty's concrete config subdir/filename. The `--key=value` CLI-arg form also
remains; `background-image-opacity` stays float-blocked.
