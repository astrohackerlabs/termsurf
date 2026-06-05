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

# Experiment 532: the macOS Application Support config path (app_support_dir)

## Description

Continuing toward the default config-path resolution (`loadDefaultFiles`), this
experiment ports the **macOS Application Support** config path — upstream
`macos.appSupportDir` (via `commonDir`). It resolves
`<Application Support>/<bundle_id>/<sub_path>`. With `xdg_config_dir`
(Experiment 531) this completes the two candidate-path families; the
`loadDefaultFiles` orchestration and roastty's concrete bundle-id/subdir come
next.

## Upstream behavior

`macos.appSupportDir` → `macos.commonDir` (`os/macos.zig`):

```zig
pub fn appSupportDir(alloc, sub_path) ![]const u8 {
    return try commonDir(alloc, .NSApplicationSupportDirectory, &.{ build_config.bundle_id, sub_path });
}

fn commonDir(alloc, directory, sub_paths) ![]const u8 {
    // NSFileManager.URLForDirectory:inDomain:… for `directory` in NSUserDomainMask
    // → base_dir = the user's Application Support directory (~/Library/Application Support)
    // → join(base_dir, sub_paths…)
}
```

So the app-support config path is
`<~/Library/Application Support>/<bundle_id>/<sub_path>` — the
`NSApplicationSupportDirectory` base joined with the bundle id and the sub-path.

The base directory is obtained from `NSFileManager`'s
`URLForDirectory:inDomain:appropriateForURL:create:error:` (the
`NSApplicationSupportDirectory` in `NSUserDomainMask`), which for a normal
(unsandboxed) macOS process is `$HOME/Library/Application Support`.

## Rust mapping (`roastty/src/config/loader.rs`)

A pure core (testable) plus an env-reading wrapper, mirroring the XDG resolver
(Experiment 531):

```rust
/// Resolve the macOS Application Support config path from the `$HOME` value
/// (upstream `macos.appSupportDir` / `commonDir`): `$HOME/Library/Application
/// Support/<bundle_id>/<sub_path>`, or `None` when `$HOME` is unset.
fn resolve_app_support(home: Option<&str>, bundle_id: &str, sub_path: &str) -> Option<PathBuf> {
    let home = home?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(bundle_id)
            .join(sub_path),
    )
}

/// The macOS Application Support config path (upstream `macos.appSupportDir`): reads
/// `$HOME` and resolves `$HOME/Library/Application Support/<bundle_id>/<sub_path>`.
pub(crate) fn app_support_dir(bundle_id: &str, sub_path: &str) -> Option<PathBuf> {
    resolve_app_support(env_nonempty("HOME").as_deref(), bundle_id, sub_path)
}
```

`env_nonempty` (introduced for `xdg_config_dir`, Experiment 531) is extracted to
a module-level helper so both resolvers share it. `resolve_app_support` is the
pure path construction; `app_support_dir` reads `$HOME`.

## Scope / faithfulness notes

- **Ported (bridged)**: `macos.appSupportDir` / `commonDir`'s path construction,
  as `config::loader::app_support_dir` (+ the pure `resolve_app_support`).
- **Faithful**: the path is the Application Support base joined with the bundle
  id and the sub-path; the base is the user-domain
  `NSApplicationSupportDirectory`.
- **Faithful adaptation**: the `NSFileManager` `URLForDirectory:inDomain:…` call
  (which returns the user's Application Support directory) →
  `$HOME/Library/Application Support` directly — the standard, equivalent
  location for a normal (unsandboxed) macOS process; the objc/Cocoa runtime call
  (which would additionally honor a sandbox container redirect) is the
  documented narrowing. `std.fs.path.join` → `PathBuf::join`.
- **Documented narrowing**: a sandboxed process's container-redirected
  Application Support directory is not modeled (roastty is unsandboxed here);
  the objc base lookup is replaced by the `$HOME`-based path. `$HOME` unset ⇒
  `None`.
- **Deferred**: roastty's concrete `bundle_id` / sub-path; the
  `loadDefaultFiles` orchestration (resolve the XDG + app-support candidates,
  `load_file` each, dedup warnings, template creation); the `--key=value` CLI
  form. `background-image-opacity` stays float-blocked.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/loader.rs`: extract `env_nonempty` to a module-level
   helper; add `resolve_app_support` and `app_support_dir`.
2. Tests (in `loader.rs`): `resolve_app_support` — `$HOME` set ⇒
   `home/Library/Application Support/<bundle_id>/<sub_path>`; `$HOME` unset ⇒
   `None`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty app_support
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `resolve_app_support` builds
  `$HOME/Library/Application Support/<bundle_id>/<sub_path>` (or `None` when
  `$HOME` is unset), and `app_support_dir` reads `$HOME` — faithful to
  upstream's Application Support base join;
- the tests pass (the `$HOME`-set and unset cases), and the existing tests still
  pass;
- the `loadDefaultFiles` orchestration stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the path construction diverges from upstream, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. The construction is faithful for an unsandboxed macOS process —
upstream `appSupportDir(sub_path)` calls
`commonDir(NSApplicationSupportDirectory, [bundle_id, sub_path])`, which asks
`NSFileManager` for `NSApplicationSupportDirectory` in `NSUserDomainMask` and
joins the base with `bundle_id` and `sub_path`
(`macos.zig:24`/`:110`/`:124`/`:143`); for a normal user-domain app that base is
`~/Library/Application Support`, so
`$HOME/Library/Application Support/<bundle_id>/<sub_path>` is the right
practical equivalent. Replacing the Objective-C lookup with the `$HOME`-based
path is an acceptable narrowing for this macOS slice (sandbox container
redirection documented as out of scope); `PathBuf::join("Application Support")`
handles the space normally; and the pure resolver + env wrapper mirrors the XDG
resolver and keeps tests deterministic.

Review artifacts:

- Prompt: `logs/codex-review/20260604-191532-d532-prompt.md` (design)
- Result: `logs/codex-review/20260604-191532-d532-last-message.md` (design)

## Result

**Result:** Pass

`env_nonempty` was extracted to a module-level helper (shared by
`xdg_config_dir` and the new `app_support_dir`), and `resolve_app_support`
(pure) + `app_support_dir` (env-reading) were added. The path is
`$HOME/Library/Application Support/<bundle_id>/<sub_path>` — the faithful
unsandboxed equivalent of upstream's `NSApplicationSupportDirectory` +
`[bundle_id, sub_path]` join. The new test `resolve_app_support_builds_path`
covers the construction and the no-`$HOME` (`None`) case.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3022 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches the approved macOS path slice —
`$HOME/Library/Application Support/<bundle_id>/<sub_path>` is the faithful
unsandboxed equivalent of upstream's `NSApplicationSupportDirectory` +
`[bundle_id, sub_path]` join, with sandbox redirection explicitly out of scope;
extracting `env_nonempty` to module scope does not change XDG behavior (it still
treats missing and empty env vars as `None`, matching `getenvNotEmpty`); the
test covers the path construction and the no-home case; gates are clean.
"Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-191718-r532-prompt.md` (result)
- Result: `logs/codex-review/20260604-191718-r532-last-message.md` (result)

## Conclusion

Both default config-path families are now resolvable — `xdg_config_dir`
(`$XDG_CONFIG_HOME` / `$HOME/.config`) and `app_support_dir`
(`$HOME/Library/Application Support/<bundle_id>`). The next experiment is the
**`loadDefaultFiles` orchestration**: build roastty's concrete candidate paths
(the XDG subdir/filename + the macOS app-support path with the bundle id, and
the legacy variants), `load_file` each that exists (collecting diagnostics),
warn on duplicates, and write a template when none exist. The `--key=value`
CLI-arg form also remains; `background-image-opacity` stays float-blocked.
