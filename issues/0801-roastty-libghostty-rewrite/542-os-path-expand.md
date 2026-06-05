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

# Experiment 542: PATH executable search (os::path::expand)

## Description

Continuing the `os` module (Experiment 541 opened it with `os::hostname`), this
experiment ports upstream `os/path.zig` — the **`expand` PATH search**: given a
command name, find the absolute path of the matching executable in `PATH` (the
`which(1)` operation). This is the shell-resolution helper the eventual termio
layer uses to turn a bare command into a runnable path; it is PTY/IO-adjacent
and genuinely unported.

## Upstream behavior

`os/path.zig`:

```zig
/// Search for "cmd" in PATH and return the absolute path (allocates on a non-null result).
pub fn expand(alloc, cmd: []const u8) !?[]u8 {
    // A command containing '/' is already absolute/relative; return as-is.
    if (std.mem.indexOfScalar(u8, cmd, '/') != null) return try alloc.dupe(u8, cmd);

    const PATH = std.posix.getenvZ("PATH") orelse return null;   // (posix arm)

    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    var it = std.mem.tokenizeScalar(u8, PATH, std.fs.path.delimiter);   // ':' on posix
    var seen_eacces = false;
    while (it.next()) |search_path| {
        const path_len = search_path.len + cmd.len + 1;
        if (path_buf.len < path_len) return error.PathTooLong;
        // full = search_path ++ '/' ++ cmd
        const f = std.fs.cwd().openFile(full_path, .{}) catch |err| switch (err) {
            error.FileNotFound => continue,
            error.AccessDenied => { seen_eacces = true; continue; },
            else => return err,
        };
        defer f.close();
        const stat = try f.stat();
        if (stat.kind != .directory and isExecutable(stat.mode)) return try alloc.dupe(u8, full_path);
    }

    if (seen_eacces) return error.AccessDenied;
    return null;
}

fn isExecutable(mode) bool { return mode & 0o0111 != 0; }   // (posix arm)
```

- A command with a `/` is returned unchanged (no filesystem check).
- Otherwise `PATH` is split on `:`; each directory is joined with the command
  and **opened** (which requires read access — an execute-only file yields
  `AccessDenied`); `FileNotFound` skips, `AccessDenied` is remembered
  (`seen_eacces`) and skipped, other errors propagate.
- A non-directory whose mode has any execute bit (`& 0o111`) is the match.
- If no match but an `AccessDenied` was seen, return `error.AccessDenied`; else
  `null`.
- A combined path longer than `max_path_bytes` (PATH_MAX, 1024 on macOS) ⇒
  `error.PathTooLong`.

## Rust mapping (`roastty/src/os/path.rs`)

`expand` returns `Result<Option<PathBuf>, ExpandError>` (the faithful shape of
`!?[]u8`). The PATH-searching core is a testable `expand_in(cmd, path_var)` so
the search semantics can be checked with a controlled `PATH` (no global-env
mutation, which would race other parallel tests); `expand(cmd)` is the thin
env-reading wrapper. Two faithfulness details the first design got wrong (Codex
design review) are fixed here:

- **Skip empty `PATH` components.** Upstream's
  `std.mem.tokenizeScalar(PATH, ':')` skips empty tokens;
  `std::env::split_paths` instead yields an empty `PathBuf` for a
  leading/trailing/doubled colon (which would search the current directory). So
  empty entries are `continue`d, matching `tokenizeScalar`.
- **Raw `dir + "/" + cmd` construction.** Upstream builds the candidate by raw
  byte concatenation, always emitting one `/` separator (so a `PATH` entry
  ending in `/` yields a `//`). `PathBuf::join` would normalize that. So the
  candidate is built by concatenating the bytes via `OsString::push`, preserving
  upstream's exact result bytes.

```rust
//! Filesystem path helpers (port of upstream `os/path`).

use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// An error from `expand` (upstream's `error.PathTooLong` / `error.AccessDenied` plus
/// propagated I/O errors).
#[derive(Debug)]
pub(crate) enum ExpandError {
    /// The combined directory + command path exceeded `PATH_MAX`.
    PathTooLong,
    /// No match was found, but a candidate was access-denied.
    AccessDenied,
    /// Another I/O error from opening/stat-ing a candidate.
    Io(std::io::Error),
}

/// Search for `cmd` in `PATH` and return the absolute path of the matching executable, or
/// `None` if not found (upstream `os.path.expand`). A `cmd` containing `/` is returned
/// as-is (assumed absolute/relative).
pub(crate) fn expand(cmd: &str) -> Result<Option<PathBuf>, ExpandError> {
    if cmd.contains('/') {
        return Ok(Some(PathBuf::from(cmd)));
    }

    match std::env::var_os("PATH") {
        Some(path_var) => expand_in(cmd, &path_var),
        None => Ok(None),
    }
}

/// The PATH-searching core, parameterized over the `PATH` value for testability. `cmd` is
/// assumed not to contain `/` (the caller handles that case).
fn expand_in(cmd: &str, path_var: &OsStr) -> Result<Option<PathBuf>, ExpandError> {
    // PATH_MAX is 1024 on macOS, the same bound as upstream's `std.fs.max_path_bytes`.
    const MAX_PATH_BYTES: usize = libc::PATH_MAX as usize;

    let mut seen_eacces = false;
    for dir in std::env::split_paths(path_var) {
        // Upstream's tokenizeScalar skips empty PATH components; split_paths does not.
        if dir.as_os_str().is_empty() {
            continue;
        }

        // dir + '/' + cmd must fit, mirroring upstream's fixed-buffer guard.
        if dir.as_os_str().len() + cmd.len() + 1 > MAX_PATH_BYTES {
            return Err(ExpandError::PathTooLong);
        }

        // Build `dir + "/" + cmd` by raw byte concatenation (upstream emits one '/' even
        // when dir already ends with '/', so the result bytes match exactly).
        let mut full_os = OsString::with_capacity(dir.as_os_str().len() + 1 + cmd.len());
        full_os.push(dir.as_os_str());
        full_os.push("/");
        full_os.push(cmd);
        let full = PathBuf::from(full_os);

        let file = match std::fs::File::open(&full) {
            Ok(file) => file,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => continue,
                std::io::ErrorKind::PermissionDenied => {
                    seen_eacces = true;
                    continue;
                }
                _ => return Err(ExpandError::Io(err)),
            },
        };

        let metadata = file.metadata().map_err(ExpandError::Io)?;
        if !metadata.is_dir() && is_executable(metadata.permissions().mode()) {
            return Ok(Some(full));
        }
    }

    if seen_eacces {
        return Err(ExpandError::AccessDenied);
    }

    Ok(None)
}

fn is_executable(mode: u32) -> bool {
    mode & 0o111 != 0
}
```

`File::open` follows upstream's `openFile` (read-access required, so
execute-only files are `AccessDenied`); `metadata().permissions().mode()` gives
the Unix mode whose execute bits `is_executable` checks. The `PathTooLong` check
mirrors upstream's fixed-buffer guard even though `PathBuf` is dynamic, keeping
the error behavior faithful.

## Scope / faithfulness notes

- **Ported (bridged)**: `os.path.expand` → `os::path::expand`;
  `os.path.isExecutable` → `os::path::is_executable`; the `PathTooLong` /
  `AccessDenied` errors → `ExpandError`.
- **Faithful**: the `/`-passthrough; `PATH` split on `:` **skipping empty
  components**; the candidate built as raw `dir + "/" + cmd` (one separator even
  for a `/`-terminated entry); open-then-stat (read-access semantics, so
  `AccessDenied` is accumulated); `FileNotFound` skip; non-directory +
  `mode & 0o111` match; `seen_eacces` ⇒ `AccessDenied`; otherwise `None`; the
  `PATH_MAX`-bound `PathTooLong`.
- **Faithful adaptation**: the allocator-returning `!?[]u8` →
  `Result<Option<PathBuf>, ExpandError>` (Rust owns the `PathBuf`, no caller
  free); `std.mem.tokenizeScalar(PATH, ':')` → `std::env::split_paths` + an
  empty-entry `continue` (tokenize skips empties); upstream's manual byte
  construction → `OsString::push` concatenation (preserving the exact result
  bytes); `getenvZ` → `std::env::var_os`; the Windows arm dropped (macOS-only).
  The PATH-searching core is extracted as `expand_in(cmd, path_var)` purely as a
  test seam (no behavior change).
- **Deferred**: wiring `expand` into the eventual termio / shell-launch path (no
  termio in roastty yet).
- No C ABI/header/ABI-inventory change (internal Rust). New `os::path` module.

## Changes

1. `roastty/src/os/path.rs` (new): `ExpandError`, `expand`, `expand_in`,
   `is_executable`.
2. `roastty/src/os/mod.rs`: add `pub(crate) mod path;`.
3. Tests (in `path.rs`): port upstream's suite (the macOS arm), plus two
   hermetic tests (using a unique temp dir, not global-env mutation) locking
   down the review fixes —
   - **expand finds a real executable**: `expand("uname")` ⇒ `Some(p)` with
     `p.as_os_str().len() > "uname".len()` (an absolute path).
   - **expand missing**: `expand("thisreallyprobablydoesntexist123")` ⇒
     `Ok(None)`.
   - **expand slash passthrough**: `expand("foo/env")` ⇒
     `Some(PathBuf::from("foo/env"))`, length 7, with no filesystem check.
   - **empty components skipped**: with a temp dir holding an executable `tool`,
     calling `expand_in("tool", ":{tmp}:")` (leading/trailing empty entries) ⇒
     `Some(_)` finding the temp executable — i.e. the empty entries are skipped
     (not treated as the current directory).
   - **trailing-slash entry**: `expand_in("tool", "{tmp}/")` ⇒ `Some(p)` where
     `p`'s bytes contain `//` — i.e. the raw `dir + "/" + cmd` construction is
     preserved (and the file is still found).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty path
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/os/path.rs roastty/src/os/mod.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `os::path::expand` returns the `/`-command as-is, searches `PATH` with
  open-then-stat semantics, matches a non-directory with an execute bit,
  accumulates `AccessDenied`, and returns `None`/`PathTooLong`/`AccessDenied`
  faithfully to `os/path.zig`;
- the tests pass (real executable / missing / slash), and the existing tests
  still pass;
- the termio wiring stays deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the search semantics, the executable test, or the
error set diverges from upstream, an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex's first design review raised **two Required** findings, both now fixed;
the corrected design was **re-reviewed and approved with no findings**.

- **Empty `PATH` components (Required, fixed)**: `std::env::split_paths` yields
  an empty `PathBuf` for leading/trailing/doubled colons (which would search the
  current directory), but upstream `std.mem.tokenizeScalar(PATH, ':')` skips
  empty tokens. Fixed by `continue`-ing on an empty entry.
- **Raw candidate construction (Required, fixed)**: `PathBuf::join` normalizes
  `//`, but upstream builds `search_path + '/' + cmd` by raw byte concatenation
  (preserving a `//` when a `PATH` entry ends with `/`). Fixed by building the
  candidate with `OsString::push`, matching upstream's exact result bytes.
- **(Optional, addressed)**: the PATH-searching core was extracted as
  `expand_in(cmd, path_var)` (a behavior-preserving test seam), and two hermetic
  tests (using a unique temp dir, not global-env mutation) were added to lock
  down the empty-skip and trailing-slash behaviors.

On re-review Codex confirmed both fixes are correct and faithful, the
`expand_in` extraction does not change behavior, and the rest remains sound
(slash passthrough does no filesystem validation, `File::open` mirrors
`openFile`, `mode & 0o111` is correct, `PATH_MAX = 1024` is the right macOS
bound, and `Result<Option<PathBuf>, ExpandError>` is the right Rust shape).

Review artifacts:

- Prompt: `logs/codex-review/20260604-d542-prompt.md` (design),
  `logs/codex-review/20260604-d542b-prompt.md` (design re-review)
- Result: `logs/codex-review/20260604-d542-last-message.md` (design),
  `logs/codex-review/20260604-d542b-last-message.md` (design re-review)
