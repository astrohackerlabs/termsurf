+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 761: Config Recursive File Loading

## Description

Add internal recursive `config-file` loading to the typed Rust config model.
Experiments 759 and 760 added storage, parsing, formatting, and path expansion
for `config-file` entries. The next faithful step is to load those expanded
files in order, allowing loaded files to append more `config-file` entries while
the iteration is in progress.

This experiment stays inside `roastty/src/config/mod.rs`. It does not wire
`roastty_config_load_recursive_files`, add replay-step behavior, add C ABI
diagnostic exposure for recursive reports, or implement a general config
diagnostic list beyond the returned report.

## Upstream Behavior

In `vendor/ghostty/src/config/Config.zig`, `loadRecursiveFiles`:

- returns immediately when there are no `config-file` entries;
- iterates with a `while` loop rather than a fixed snapshot because loaded files
  may append more `config-file` entries;
- skips empty paths;
- assumes paths have already been expanded to absolute paths;
- tracks loaded paths and reports a cycle when a path appears again;
- suppresses missing-file diagnostics for optional paths;
- records diagnostics for required missing files and other open/type errors;
- loads each readable config file with normal config parsing, so child files can
  override parent settings and append more recursive file entries.

## Changes

- `roastty/src/config/mod.rs`
  - Add `ConfigRecursiveLoadReport` with:
    - `loaded: Vec<ConfigRecursiveFileLoad>` carrying path and line diagnostics;
    - `errors: Vec<ConfigRecursiveFileError>` for required missing files and
      other load errors;
    - `cycles: Vec<PathBuf>` for repeated paths skipped as cycles.
  - Add `Config::load_recursive_files_from_config()` that:
    - iterates `self.config_file.list` by index so appended entries are visited;
    - clones each current entry's path/optional flag before loading;
    - skips empty paths;
    - rejects unexpanded relative paths by recording an error and not resolving
      them against the process current working directory;
    - tracks paths in a `HashSet<PathBuf>` and records cycles before loading;
    - suppresses `NotFound` errors for optional paths;
    - records other errors, including optional non-file/directory paths, without
      aborting;
    - uses `Config::load_file` for successful loads so child `config-file`
      entries are expanded relative to the child file's directory;
    - preserves upstream ordering where a child file's settings override the
      parent after the parent has fully loaded.
- Tests in `roastty/src/config/mod.rs`
  - no entries returns an empty report;
  - required child file loads after parent and overrides a parent setting;
  - a child file can append a grandchild, and the grandchild is loaded by the
    same while-loop iteration;
  - optional missing files are suppressed;
  - required missing files are recorded;
  - manually stored relative paths are recorded as errors and do not load from
    cwd;
  - required and optional directory/non-file paths are recorded as errors;
  - repeated paths are reported as cycles and loaded only once;
  - line diagnostics from loaded recursive files are recorded while later valid
    settings still apply.

## Verification

- `cargo test -p roastty recursive -- --nocapture --test-threads=1`
- `cargo test -p roastty config_file -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

The experiment passes if recursive file loading follows the expanded
`config-file` list in order, visits entries appended by child files, records
cycles/errors without aborting, suppresses optional missing files, and keeps the
C ABI recursive load function deferred.

## Design Review

Codex reviewed the first design draft and found two blockers. First, the
recursive loader needs an explicit guard for unexpanded relative entries so a
manually stored relative path cannot silently load from the process current
working directory. Second, non-file/type errors need verification for both
required and optional paths because upstream suppresses only optional
`FileNotFound`, not optional directories or other errors. The design was updated
to record relative-path and non-file errors in the recursive report.

Codex reviewed the updated design and approved it with no remaining blocking
findings. The follow-up review confirmed that the scope covers the prior
relative-path and optional non-file blockers while keeping replay and C ABI work
deferred.

## Result

**Result:** Pass

Implemented internal recursive `config-file` loading in
`roastty/src/config/mod.rs`.

`Config::load_recursive_files_from_config` now walks the expanded `config_file`
list by index so files loaded during recursion can append more entries and have
them visited in the same pass. The loader skips empty paths, reports unexpanded
relative paths instead of resolving them against cwd, tracks loaded paths for
cycle detection, suppresses optional `NotFound`, records required missing and
other IO errors, and uses `Config::load_file` for child loads so child settings
apply normally and child `config-file` entries expand relative to the child
file.

Verification passed:

- `cargo test -p roastty recursive -- --nocapture --test-threads=1`
- `cargo test -p roastty config_file -- --nocapture --test-threads=1`
- `cargo test -p roastty config_ -- --nocapture --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Completion Review

Codex reviewed the completed implementation and found no blocking findings. The
review confirmed that index-based iteration handles appended recursive entries,
relative stored paths are reported rather than resolved from cwd, cycles are
detected before loading, optional `NotFound` is suppressed, optional non-file
errors are retained, and C ABI/replay wiring remains deferred.

Non-blocking follow-ups from the review: strengthen the non-file test to assert
directory errors are not `NotFound`, and add a regression test documenting that
repeated optional missing paths become cycles on the second occurrence.

## Conclusion

Roastty now has the internal recursive `config-file` loader needed before the C
ABI recursive entry point can be wired. Replay-step behavior and public ABI
diagnostic surfacing remain deferred to later slices.
