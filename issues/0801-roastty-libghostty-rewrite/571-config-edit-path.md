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

# Experiment 571: config edit-path selection (configPath)

## Description

This experiment ports the **config-path selection algorithm** from upstream
`config/edit.zig` — `configPath`, which picks which config file to open for
editing from a list of candidate paths (preferring a non-empty file, then any
existing file, then the first candidate). It lands at `config::edit`. The
candidate _generation_ (`configPathCandidates`) and the directory/file creation
(`openPath`) are **deferred**, because the candidates come from the AppSupport /
XDG path helpers (`file_load.zig`) that are blocked on roastty's
config-directory naming decision; the selection algorithm itself is std-only and
faithfully portable now.

## Upstream behavior

`config/edit.zig`'s `configPath(alloc) ![]const u8` (given the candidate list
from `configPathCandidates`):

```zig
assert(paths.len > 0);
var exists: ?[]const u8 = null;
for (paths) |path| {
    const f = std.fs.openFileAbsolute(path, .{}) catch |err| switch (err) {
        error.BadPathName, error.FileNotFound => continue, // doesn't exist → skip
        else => return err,                                 // other error → propagate
    };
    defer f.close();
    const stat = try f.stat();
    if (stat.size > 0) return path;        // first non-empty file wins immediately
    if (exists == null) exists = path;     // remember the first existing (empty) file
}
if (exists) |v| return v;                  // no non-empty file → first existing
return paths[0];                           // nothing exists → first candidate
```

So the precedence is: **the first non-empty candidate** → else **the first
existing (empty) candidate** → else **the first candidate**. A file that doesn't
exist is skipped; any other IO error propagates.

(On macOS the candidates are AppSupport, legacy AppSupport, XDG, legacy XDG —
AppSupport is preferred — but that ordering is produced by
`configPathCandidates`, which this experiment defers.)

## Rust mapping (`roastty/src/config/edit.rs`)

`configPath`'s selection loop ports directly, preserving upstream's
**open-then-stat** order (`File::open` then `file.metadata()`) so that an
unreadable file errors on open exactly as upstream propagates it. The chosen
path is returned as a borrow into the candidate slice; an IO error other than
"doesn't exist / malformed path" propagates.

```rust
//! Selecting which config file to open for editing (port of upstream `config/edit`'s
//! `configPath`).

use std::io;
use std::path::{Path, PathBuf};

/// Choose the config path to open for editing from `candidates` (upstream `configPath`).
///
/// Precedence: the first **non-empty** candidate, else the first **existing** (empty) candidate,
/// else the first candidate. A candidate that does not exist (or whose path is malformed) is
/// skipped; any other IO error propagates. `candidates` must be non-empty.
pub(crate) fn config_path(candidates: &[PathBuf]) -> io::Result<&Path> {
    assert!(!candidates.is_empty(), "config_path requires at least one candidate");

    let mut exists: Option<&Path> = None;
    for path in candidates {
        // Open first (upstream `openFileAbsolute`), then stat — so an unreadable file surfaces as
        // an open error rather than a successful `metadata` probe.
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            // Doesn't exist / malformed path → skip (upstream skips FileNotFound / BadPathName).
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
                ) =>
            {
                continue
            }
            // Any other IO error propagates (upstream `else => return err`).
            Err(err) => return Err(err),
        };

        let meta = file.metadata()?; // upstream `try f.stat()` — propagates errors.
        // First non-empty file wins immediately.
        if meta.len() > 0 {
            return Ok(path);
        }
        // Otherwise remember the first existing (empty) file.
        if exists.is_none() {
            exists = Some(path);
        }
    }

    // No non-empty file → the first existing one; nothing exists → the first candidate.
    Ok(exists.unwrap_or_else(|| candidates[0].as_path()))
}
```

## Scope / faithfulness notes

- **Ported**: `config/edit.zig`'s `configPath` selection logic →
  `config::edit::config_path`.
- **Faithful**: the precedence (first non-empty → first existing → first
  candidate), the open-then-stat order (`File::open` then `file.metadata()`, so
  an unreadable file errors on open exactly as upstream propagates), the
  skip-on-nonexistent / propagate-other-errors behavior, and the non-empty
  assertion are reproduced exactly. The chosen path is a borrow into the
  candidate slice (upstream returns an arena slice; the caller owns the
  candidates here).
- **Deferred**:
  - `configPathCandidates` — produces the AppSupport / legacy-AppSupport / XDG /
    legacy-XDG candidate list via `file_load.zig`'s path helpers, which are
    blocked on roastty's config-directory naming decision (the same deferral as
    `loadDefaultFiles` / `appSupportDir`).
  - `openPath` — creates the config directory and an empty file if missing, then
    returns the duplicated path; it composes `configPathCandidates` +
    `configPath` + filesystem mutation, so it follows once the candidate
    generation lands.
- **Faithful adaptation**: upstream skips both `BadPathName` and `FileNotFound`;
  roastty skips `io::ErrorKind::NotFound` **and** `io::ErrorKind::InvalidInput`
  (the closest analogue of `BadPathName` for a malformed path) and propagates
  the rest.
- No C ABI/header/ABI-inventory change (internal Rust). Adds `config::edit`.

## Changes

1. `roastty/src/config/edit.rs` (new): `config_path` as above.
2. `roastty/src/config/mod.rs`: add `#[allow(dead_code)] mod edit;`
   (alphabetical, after `conditional`).
3. Tests (in `edit.rs`), using a unique temp directory:
   - **first non-empty wins**: candidates `[empty, non_empty_a, non_empty_b]` →
     `non_empty_a`.
   - **first existing (empty) fallback**: candidates
     `[missing, empty_a, empty_b]` → `empty_a`.
   - **nothing exists → first candidate**: candidates `[missing_1, missing_2]` →
     `missing_1`.
   - **a non-empty later candidate still loses to an earlier non-empty one**
     (ordering).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config::edit
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config/edit.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `config_path` reproduces upstream's precedence (first non-empty → first
  existing → first candidate), the skip-nonexistent / propagate-other-error
  behavior, and the non-empty assertion — faithful to `config/edit.zig`'s
  `configPath`;
- the tests pass (non-empty wins / existing fallback / nothing-exists /
  ordering), and the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the selection precedence, the error handling, or the
assertion diverges from upstream, an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex reviewed the design and found **one Required** finding (and one Optional),
both adopted:

- **Required (fixed)**: use **open-then-stat** (`std::fs::File::open` then
  `file.metadata()`), not `std::fs::metadata`. Upstream calls `openFileAbsolute`
  first and propagates non-`FileNotFound` / `BadPathName` open errors before
  `stat`; `metadata` can succeed where opening would fail (notably unreadable
  files), so it could select a path upstream would reject with an IO error.
  Changed to open-before-stat.
- **Optional (adopted)**: also skip `io::ErrorKind::InvalidInput` alongside
  `NotFound` to mirror upstream's `BadPathName` skip for malformed generated
  paths.

Codex confirmed the selection precedence is otherwise correct (first non-empty →
first existing empty → `candidates[0]`), that returning a borrowed `&Path` from
the candidate slice is sound, and that deferring candidate generation /
open-path mutation is appropriately scoped.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d571-prompt.md`
- Result: `logs/codex-review/20260604-d571-last-message.md`

## Result

**Result:** Pass

`config::edit::config_path(candidates: &[PathBuf]) -> io::Result<&Path>` was
added: it asserts the candidate list is non-empty, then for each candidate
**opens** the file (skipping `NotFound` / `InvalidInput`, propagating other IO
errors) and **stats** the open handle — returning the first non-empty candidate
immediately, remembering the first existing (empty) one, and falling back to
that (else `candidates[0]`). Registered via `#[allow(dead_code)] mod edit;` in
`config/mod.rs`. The candidate generation (`configPathCandidates`, needs the
deferred `file_load` AppSupport/XDG helpers) and `openPath` (filesystem
mutation) remain deferred.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3159 passed, 0 failed (four new tests; no
  regressions, up from 3155).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer + config/edit.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The four new tests (each with a per-test temp dir cleaned on `Drop`): first
non-empty wins (`[empty, a, b]` → `a`), first existing-empty fallback
(`[missing, empty_a, empty_b]` → `empty_a`), nothing-exists → first candidate
(`[missing_1, missing_2]` → `missing_1`), and earlier-non-empty beating a later
non-empty (`[a, b]` → `a`).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet in the saved file — added here as part of result recording). Codex
confirmed the implementation now matches upstream's selection loop — opens
first, stats the opened file, skips missing/malformed paths, propagates other IO
errors, returns the first non-empty candidate / else the first existing empty /
else `candidates[0]` — and that the temp-dir tests cover the key precedence
cases cleanly.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r571-prompt.md` (result)
- Result: `logs/codex-review/20260604-r571-last-message.md` (result)

## Conclusion

`config::edit::config_path` ports the config-path selection algorithm from
`config/edit.zig`'s `configPath` — the precedence "first non-empty file → first
existing file → first candidate", with upstream's open-then-stat order preserved
(a Required review fix: `File::open` then `file.metadata()`, so an unreadable
file errors on open rather than being silently selected by a `metadata` probe)
and its skip-set mirrored (`NotFound` + `InvalidInput` ≈ `FileNotFound` +
`BadPathName`). The two pieces that compose around it — `configPathCandidates`
(the AppSupport / XDG candidate list) and `openPath` (directory + file creation)
— stay **deferred**, blocked on the same config-directory naming decision as
`loadDefaultFiles` / `appSupportDir`; once that lands, the candidate generation
and `openPath` follow, and `config_path` slots straight in. This session has now
exhausted most of the cleanly-independent leaf files; the remaining work is
concentrated in the dependency-blocked helpers (regex/oniguruma for
`Link::oniRegex`, a URI parser for `os/uri`, the config-dir naming decision for
`file_load`/`edit`/`loadDefaultFiles`) and the big-ticket subsystems
(`datastruct/split_tree`, 2517 lines; the terminal **search subsystem** coupled
to `PageList` / `Pin` / `Screen` / `Selection` / `PageFormatter`).
`background-image-opacity` stays float-blocked.
