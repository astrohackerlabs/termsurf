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

# Experiment 533: the optional-file load action (Config::load_optional_file)

## Description

The `loadDefaultFiles` orchestration loads each candidate config path _if it
exists_, distinguishing "loaded" from "not found" (to decide whether to warn on
duplicates or write a template). This experiment ports that building block —
upstream `Config.loadOptionalFile` — as `Config::load_optional_file`, an
app-name-agnostic step over `Config::load_file` (Experiment 530). The concrete
candidate paths, the dedup warnings, and the template creation depend on
roastty's not-yet-decided config subdir/bundle-id and template content, and stay
deferred.

## Upstream behavior

`Config.loadOptionalFile` (`Config.zig:3989`):

```zig
pub fn loadOptionalFile(self, alloc, path) OptionalFileAction {
    if (self.loadFile(alloc, path)) {
        return .loaded;
    } else |err| switch (err) {
        error.FileNotFound => return .not_found,
        else => {
            std.log.warn("error reading optional config file, not loading err={} path={s}", .{ err, path });
            return .@"error";
        },
    }
}
```

So `loadOptionalFile` returns a three-way action:

- `.loaded` — the file was read and applied.
- `.not_found` — the file does not exist (`error.FileNotFound`).
- `.@"error"` — any other error (logged, then this action is returned; the load
  is skipped, not aborted).

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The result of `Config::load_optional_file` (upstream `OptionalFileAction`).
#[derive(Debug)]
pub(crate) enum OptionalFileAction {
    /// The file was read and applied; carries its per-line diagnostics.
    Loaded(Vec<ConfigDiagnostic>),
    /// The file does not exist.
    NotFound,
    /// Another IO error occurred reading the file (the load is skipped).
    Error(std::io::Error),
}

impl Config {
    /// Load a config file if it exists (upstream `Config.loadOptionalFile`):
    /// `Loaded` with the diagnostics on success, `NotFound` when the file does not
    /// exist, or `Error` for another IO error (the load is skipped, not aborted).
    pub(crate) fn load_optional_file(&mut self, path: &std::path::Path) -> OptionalFileAction {
        match self.load_file(path) {
            Ok(diagnostics) => OptionalFileAction::Loaded(diagnostics),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => OptionalFileAction::NotFound,
            Err(e) => OptionalFileAction::Error(e),
        }
    }
}
```

A successful `load_file` ⇒ `Loaded(diagnostics)`; an `io::Error` whose kind is
`NotFound` ⇒ `NotFound`; any other `io::Error` ⇒ `Error(e)`. This mirrors
upstream's `.loaded` / `.not_found` / `.@"error"` three-way result.

## Scope / faithfulness notes

- **Ported (bridged)**: `Config.loadOptionalFile`, as
  `Config::load_optional_file` + `OptionalFileAction`.
- **Faithful**: the three-way result — load + apply (with diagnostics),
  file-not-found, and other-error (the load skipped rather than aborting the
  caller).
- **Faithful adaptation**: Zig's `OptionalFileAction` enum → a Rust enum
  carrying the diagnostics (`Loaded`) / the IO error (`Error`);
  `error.FileNotFound` → `io::ErrorKind::NotFound`; upstream's `std.log.warn` on
  the error path → the `Error` variant returned to the caller (roastty has no
  logging layer here yet).
- **Deferred**: the `loadDefaultFiles` orchestration (roastty's concrete XDG /
  app-support candidate paths and bundle-id/subdir, the dedup warnings, the
  template creation) and the `--key=value` CLI form. `background-image-opacity`
  stays float-blocked.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `OptionalFileAction` and
   `Config::load_optional_file`.
2. Tests (in `config/mod.rs`): a config written to a temp file ⇒ `Loaded` (with
   the field applied, verified via `format_config`); a nonexistent path ⇒
   `NotFound`; a directory path (a non-`NotFound` IO error) ⇒ `Error`. (Temp
   paths under `std::env::temp_dir()`, removed after.)
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty load_optional_file
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Config::load_optional_file` returns `Loaded(diagnostics)` for an existing
  file, `NotFound` for a missing path, and `Error` for another IO error —
  faithful to upstream's `OptionalFileAction`;
- the tests pass (the loaded / not-found / error cases), and the existing tests
  still pass;
- the `loadDefaultFiles` orchestration stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the action mapping diverges from upstream, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. The three-way mapping is faithful — upstream returns `.loaded` on a
successful `loadFile`, `.not_found` only for `error.FileNotFound`, and
`.@"error"` for all other errors after logging (`Config.zig:3984`/`:3989`);
mapping `io::ErrorKind::NotFound` to `NotFound` and every other `io::Error` to
`Error(e)` is the right Rust shape. Returning the error in the `Error` variant
is an acceptable adaptation of upstream's warn-and-return-error-action behavior
— the caller can continue just like `loadDefaultFiles` does with
`OptionalFileAction` (`Config.zig:4040`/`:4059`). A directory or permission/read
failure should remain `Error`, not `NotFound`, matching upstream's
`FileNotFound`-vs-all-others distinction.

Review artifacts:

- Prompt: `logs/codex-review/20260604-191947-d533-prompt.md` (design)
- Result: `logs/codex-review/20260604-191947-d533-last-message.md` (design)

## Result

**Result:** Pass

`OptionalFileAction` and `Config::load_optional_file` were added: a successful
`load_file` ⇒ `Loaded(diagnostics)`, a `NotFound` IO error ⇒ `NotFound`, and any
other IO error ⇒ `Error(e)` — faithful to upstream's `.loaded` / `.not_found` /
`.@"error"` three-way action (the error path is non-fatal, returned for the
caller rather than aborting). The new test
`config_load_optional_file_three_way_action` covers an existing file (`Loaded`,
field applied), a missing path (`NotFound`), and a directory (`Error`).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3023 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: the implementation matches upstream `loadOptionalFile` — a
successful load returns loaded, a missing path is distinct, and other IO
failures are non-fatal action results rather than aborts; carrying diagnostics
in `Loaded` is a useful Rust adaptation of the existing `load_file` shape; the
tests cover all three actions and verify the successful file actually applies
configuration; gates are clean and the default-file orchestration remains
deferred. "Approved with no findings."

Review artifacts:

- Prompt: `logs/codex-review/20260604-192436-r533-prompt.md` (result)
- Result: `logs/codex-review/20260604-192436-r533-last-message.md` (result)

## Conclusion

`load_optional_file` is the orchestration step `loadDefaultFiles` builds on. The
remaining config work is the **`loadDefaultFiles` orchestration** itself — which
needs roastty's concrete config naming (the XDG subdir/filename and the macOS
bundle id) and the config-template content; that naming is a product decision
not yet made, so the orchestration stays deferred until it is. The path
resolvers (`xdg_config_dir`, `app_support_dir`), `load_optional_file`,
`load_file`, `load_str`, and the 43-of-44 `Config::set` are all in place to
assemble it. The `--key=value` CLI-arg form also remains;
`background-image-opacity` stays float-blocked.
