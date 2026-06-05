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

# Experiment 545: the RAII temporary directory (os::TempDir)

## Description

Continuing the `os` module (Experiment 544 added the temp-path helpers), this
experiment ports upstream `os/TempDir.zig` — a **temporary directory created on
disk that destroys itself when dropped**. It builds directly on
`os::file::random_basename` / `os::file::tmp_dir`: `init` opens the temp
directory, generates random basenames until one can be created, and records the
new directory; `deinit` deletes it (and its contents). In Rust this is the
idiomatic RAII pattern: a `TempDir` struct whose `Drop` does the recursive
removal.

## Upstream behavior

`os/TempDir.zig`:

```zig
const TempDir = @This();
dir: Dir,                                  // the created directory handle
parent: Dir,                               // its parent (the temp dir)
name_buf: [file.random_basename_len:0]u8,  // the basename

pub fn init() !TempDir {
    var tmp_path_buf: [file.random_basename_len:0]u8 = undefined;
    const dir = dir: {
        const cwd = std.fs.cwd();
        const tmp_dir = try file.allocTmpDir(std.heap.page_allocator);
        defer file.freeTmpDir(std.heap.page_allocator, tmp_dir);
        break :dir try cwd.openDir(tmp_dir, .{});
    };

    while (true) {
        const tmp_path = try file.randomBasename(&tmp_path_buf);
        tmp_path_buf[tmp_path.len] = 0;
        dir.makeDir(tmp_path) catch |err| switch (err) {
            error.PathAlreadyExists => continue,   // retry with a new name
            else => |e| return e,
        };
        return TempDir{ .dir = try dir.openDir(tmp_path, .{}), .parent = dir, .name_buf = tmp_path_buf };
    }
}

/// The basename (not the full path).
pub fn name(self: *TempDir) []const u8 { return std.mem.sliceTo(&self.name_buf, 0); }

/// Delete the directory and all its contents.
pub fn deinit(self: *TempDir) void {
    self.dir.close();
    self.parent.deleteTree(self.name()) catch |err| log.err("error deleting temp dir err={}", .{err});
}
```

- `init`: resolve the temp directory (`file.allocTmpDir`), then loop — make a
  `random_basename` directory inside it, retrying on `PathAlreadyExists`, until
  one is created; keep the new directory and its basename.
- `name`: the basename only.
- `deinit`: recursively delete the directory (logs but does not propagate a
  delete error).

The upstream test: after `init`, the name is non-empty and the directory can be
opened (it exists); after `deinit`, opening it yields `error.FileNotFound`.

## Rust mapping (`roastty/src/os/temp_dir.rs`)

A `TempDir` struct storing the full path and basename; `new` is `init` (the
retry loop on `AlreadyExists`); `Drop` is `deinit` (recursive `remove_dir_all`,
ignoring the error as upstream logs-and-continues — `Drop` cannot fail). Zig's
handle-based `Dir` fields become a `PathBuf` (the idiomatic Rust equivalent —
operations are by path, not open handles):

```rust
//! A temporary directory created on disk that is removed on drop (port of upstream
//! `os/TempDir`).

use std::path::{Path, PathBuf};

use crate::os::file;

/// A temporary directory; removed (with its contents) when dropped.
pub(crate) struct TempDir {
    /// The full path of the created directory.
    path: PathBuf,
    /// The basename of the directory (not the full path).
    name: String,
}

impl TempDir {
    /// Create a fresh temporary directory under the system temp directory (upstream
    /// `TempDir.init`). Loops over random basenames until one can be created.
    pub(crate) fn new() -> std::io::Result<TempDir> {
        let parent = file::tmp_dir();
        loop {
            let name = file::random_basename();
            let mut path = PathBuf::from(&parent);
            path.push(&name);
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(TempDir { path, name }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }
    }

    /// The basename of the directory, not the full path (upstream `TempDir.name`).
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The full path of the directory (the Rust handle-equivalent; upstream holds a `Dir`).
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Delete the directory and all its contents (upstream `deinit`). A failure is
        // ignored — `Drop` cannot propagate, and upstream likewise logs-and-continues.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
```

`create_dir` (single directory) mirrors `makeDir`; `AlreadyExists` ⇒ retry (the
`PathAlreadyExists` arm), other errors propagate. `remove_dir_all` mirrors
`deleteTree` (recursive). The Zig `Dir` handles become a stored `PathBuf`
because Rust's `std::fs` is path-oriented — the observable behavior (a created
directory, removed on drop) is identical.

## Scope / faithfulness notes

- **Ported (bridged)**: `os.TempDir` → `os::temp_dir::TempDir` with `new`
  (`init`), `name`, `path`, and `Drop` (`deinit`).
- **Faithful**: resolve the temp dir via `file::tmp_dir`; the random-basename
  create-retry loop (`AlreadyExists` ⇒ retry, other errors propagate); `name`
  returns the basename only; `Drop` recursively removes the directory and
  ignores a removal error (upstream logs-and-continues).
- **Faithful adaptation**: Zig's handle-based `Dir`/`parent` fields → a stored
  `PathBuf` (Rust `std::fs` is path-oriented); `deinit` → `Drop`; `makeDir` →
  `create_dir`; `deleteTree` → `remove_dir_all`; `init` ! →
  `new() -> io::Result`. `path()` is a Rust convenience accessor (the
  handle-equivalent; upstream exposes only `name`).
- **Deferred**: nothing specific to this file (it is fully ported on the macOS
  arm).
- No C ABI/header/ABI-inventory change (internal Rust). New `os::temp_dir`
  module.

## Changes

1. `roastty/src/os/temp_dir.rs` (new): `TempDir` with `new`, `name`, `path`,
   `Drop`.
2. `roastty/src/os/mod.rs`: add `pub(crate) mod temp_dir;`.
3. Tests (in `temp_dir.rs`): port the upstream test —
   - **create then remove**: `TempDir::new()` succeeds; `name()` is non-empty
     (length `file::RANDOM_BASENAME_LEN`); the directory exists (`path()` is a
     directory) and lives under `file::tmp_dir()`; after the `TempDir` is
     dropped, the path no longer exists.
   - **distinct dirs**: two `TempDir`s have different names/paths (and both
     clean up).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty temp_dir
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/os/temp_dir.rs roastty/src/os/mod.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `TempDir::new` creates a uniquely-named directory under the temp dir (retrying
  on collision), `name` returns the basename, and `Drop` recursively removes it
  — faithful to `os/TempDir.zig`;
- the tests pass (create/exists/remove + distinctness), and the existing tests
  still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the create/retry/remove behavior diverges from
upstream, an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. Codex confirmed `TempDir::new` mirrors `init` (resolve the temp
parent, generate random basenames, `create_dir`, retry on `AlreadyExists`,
propagate other errors), with `create_dir` the correct equivalent of `makeDir`
(not `create_dir_all`); the stored `PathBuf` is an acceptable Rust adaptation of
the open `Dir` handles (identical observable behavior, `remove_dir_all` the
right equivalent of `deleteTree`); ignoring the removal error in `Drop` is fine
since upstream logs but likewise does not propagate; adding `path()` as a Rust
convenience is reasonable and leaves `name()` behavior unchanged; and the tests
cover creation, basename length, location, cleanup, and distinctness adequately.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d545-prompt.md` (design)
- Result: `logs/codex-review/20260604-d545-last-message.md` (design)
