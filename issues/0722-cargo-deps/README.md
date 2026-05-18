+++
status = "closed"
opened = "2026-03-07"
closed = "2026-03-07"
+++

# Issue 722: Update outdated cargo dependencies

## Goal

Bring all cross-platform cargo dependencies in Wezboard up to their latest
versions. Skip Windows-only dependencies (can't test them).

## Background

After completing the wgpu 25→28 upgrade (Issue 721), a full
`cargo outdated --depth 9999` audit reveals additional outdated dependencies.
Most are minor or patch bumps that resolve with a simple `cargo update`. One
requires code changes: `env_logger` in `env-bootstrap` is pinned to 0.10 because
it relies on `filter::Builder`, which was removed in 0.11.

## Outdated dependencies

### Cross-platform (affects macOS)

| Dependency                   | Current | Latest | Bump  | How to update                                                                                                         |
| ---------------------------- | ------- | ------ | ----- | --------------------------------------------------------------------------------------------------------------------- |
| `bitflags`                   | 2.10.0  | 2.11.0 | Minor | `cargo update` (used by termwiz, wezboard-escape-parser, wezboard-surface, plus ~25 transitive consumers)             |
| `env_logger`                 | 0.11.8  | 0.11.9 | Patch | `cargo update` (workspace version "0.11")                                                                             |
| `env_logger` (env-bootstrap) | 0.10.2  | 0.11.9 | Minor | Cargo.toml change + code migration. Pinned to "0.10" with comment: "we rely on filter::Builder which is gone in 0.11" |
| `env_filter`                 | 0.1.4   | 1.0.0  | Major | Transitive dep of `env_logger` — updates automatically                                                                |
| `thiserror`                  | 2.0.17  | 2.0.18 | Patch | `cargo update` (used by wezboard-dynamic, wezboard-escape-parser)                                                     |
| `thiserror-impl`             | 2.0.17  | 2.0.18 | Patch | Transitive proc macro — updates with `thiserror`                                                                      |

### Windows only (skip — can't test)

| Dependency          | Current | Latest | Notes                                       |
| ------------------- | ------- | ------ | ------------------------------------------- |
| `windows`           | 0.58.0  | 0.62.2 | Transitive dep from wgpu-hal (DX12 backend) |
| `windows-core`      | 0.58.0  | 0.62.2 | Transitive                                  |
| `windows-implement` | 0.58.0  | 0.60.2 | Transitive                                  |
| `windows-interface` | 0.58.0  | 0.59.3 | Transitive                                  |
| `windows-result`    | 0.2.0   | 0.4.1  | Transitive                                  |
| `windows-strings`   | 0.1.0   | 0.5.1  | Transitive                                  |

## Analysis

The update splits naturally into two steps:

1. **`cargo update`** — Bumps `bitflags`, `env_logger` (0.11.8→0.11.9),
   `env_filter`, `thiserror`, and `thiserror-impl`. These are all
   semver-compatible and require zero code changes.

2. **`env-bootstrap` env_logger 0.10→0.11** — This is the only dependency that
   requires code changes. The `filter::Builder` API was removed in env_logger
   0.11. Need to read the env-bootstrap source to understand what it uses and
   find the 0.11 equivalent.

## Files affected

| File                                    | Changes                         |
| --------------------------------------- | ------------------------------- |
| `wezboard/Cargo.lock`                   | Updated by `cargo update`       |
| `wezboard/env-bootstrap/Cargo.toml`     | `env_logger` version bump       |
| `wezboard/env-bootstrap/src/ringlog.rs` | Migrate `filter::Builder` usage |

## Experiments

### Experiment 1: Update all cross-platform dependencies

Update all outdated cross-platform dependencies in a single experiment. This
combines the `cargo update` patch bumps with the env-bootstrap env_logger
0.10→0.11 migration, since they're all small changes that belong together.

#### Changes

1. **`cargo update`** — Run `cargo update` in `wezboard/` to bump
   semver-compatible deps: `bitflags` 2.10.0→2.11.0, `env_logger` 0.11.8→0.11.9,
   `env_filter` 0.1.4→1.0.0, `thiserror` 2.0.17→2.0.18, `thiserror-impl`
   2.0.17→2.0.18. Zero code changes needed.

2. **`wezboard/env-bootstrap/Cargo.toml`** (line 17): Change
   `env_logger = "0.10"` to `env_logger = { workspace = true }` (workspace
   version is "0.11"). Add `env_filter = "1.0"` as a new direct dependency.

3. **`wezboard/env-bootstrap/src/ringlog.rs`** (line 7): Change import from
   `env_logger::filter::{Builder as FilterBuilder, Filter}` to
   `env_filter::{Builder as FilterBuilder, Filter}`. The `env_filter` crate
   provides identical `Builder` and `Filter` types with the same API
   (`filter_module`, `parse`, `filter_level`, `build`, `matches`, `enabled`,
   `filter`).

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. `cargo run --bin wezboard-gui` — app launches and renders

**Result:** Pass

`cargo update` bumped 206 packages to their latest compatible versions. The
env-bootstrap migration from `env_logger::filter` to `env_filter` compiled
cleanly — the `env_filter` crate exports identical `Builder` and `Filter` types.
Build succeeded with zero errors, app launched and rendered correctly.

#### Conclusion

All cross-platform dependencies are up to date. The env_logger 0.10→0.11
migration was straightforward: add `env_filter = "1.0"` as a direct dependency
and change one import line. The Windows-only deps remain untouched (can't test).

## Conclusion

All cross-platform cargo dependencies in Wezboard are now up to date. The
`cargo update` bumped 206 packages to their latest semver-compatible versions,
and the env-bootstrap `env_logger` 0.10→0.11 migration required only a new
`env_filter` dependency and one import change. Windows-only deps (`windows`,
`windows-core`, etc.) were intentionally skipped since they can't be tested on
macOS.
