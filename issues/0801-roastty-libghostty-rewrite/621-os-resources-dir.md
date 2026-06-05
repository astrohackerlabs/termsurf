+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 621: os resources directory resolver

## Description

Port `os/resourcesdir.zig` into `roastty/src/os/resources_dir.rs`, adapted to
Roastty naming. This gives later theme, terminfo, i18n, and app-startup slices a
single OS primitive for locating bundled resources.

Upstream finds a resources directory by checking an environment override and
then walking upward from the current executable to find a terminfo sentinel. The
Roastty port preserves the search behavior but uses Roastty-facing names:
`ROASTTY_RESOURCES_DIR`, `roastty`, and `xterm-roastty`.

## Upstream behavior (`os/resourcesdir.zig`)

```zig
pub const ResourcesDir = struct {
    app_path: ?[]const u8 = null,
    host_path: ?[]const u8 = null,

    pub fn app(self: *ResourcesDir) ?[]const u8 {
        return self.app_path;
    }

    pub fn host(self: *ResourcesDir) ?[]const u8 {
        return self.host_path orelse self.app_path;
    }
};

pub fn resourcesDir(alloc: Allocator) !ResourcesDir {
    // Release: environment first.
    if (comptime builtin.mode != .Debug) {
        if (std.process.getEnvVarOwned(alloc, "GHOSTTY_RESOURCES_DIR")) |dir| {
            if (dir.len > 0) return .{ .app_path = dir };
        } else |err| switch (err) {
            error.EnvironmentVariableNotFound => {},
            else => return err,
        }
    }

    const sentinels = switch (comptime builtin.target.os.tag) {
        .macos => .{"terminfo/78/xterm-ghostty"},
        ...
    };

    var exe = std.fs.selfExePath(&exe_buf) catch return .{};
    while (std.fs.path.dirname(exe)) |dir| {
        exe = dir;

        if (comptime builtin.target.os.tag.isDarwin()) {
            if (try maybeDir(&dir_buf, dir, "Contents/Resources", sentinel)) |v| {
                return .{ .app_path = try std.fs.path.join(alloc, &.{ v, "ghostty" }) };
            }
        }

        if (try maybeDir(&dir_buf, dir, "share", sentinel)) |v| {
            return .{ .app_path = try std.fs.path.join(alloc, &.{ v, "ghostty" }) };
        }
    }

    // Debug: environment fallback after detection.
    if (comptime builtin.mode == .Debug) {
        if (std.process.getEnvVarOwned(alloc, "GHOSTTY_RESOURCES_DIR")) |dir| {
            if (dir.len > 0) return .{ .app_path = dir };
        } else |err| switch (err) {
            error.EnvironmentVariableNotFound => {},
            else => return err,
        }
    }

    return .{};
}
```

## Rust mapping (`roastty/src/os/resources_dir.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ResourcesDir {
    app_path: Option<PathBuf>,
    host_path: Option<PathBuf>,
}

impl ResourcesDir {
    pub(crate) fn app(&self) -> Option<&Path> { ... }
    pub(crate) fn host(&self) -> Option<&Path> { ... }
}

pub(crate) fn resources_dir() -> std::io::Result<ResourcesDir> {
    let env_override = std::env::var_os("ROASTTY_RESOURCES_DIR");
    if !cfg!(debug_assertions) {
        if let Some(dir) = non_empty_env(&env_override) {
            return Ok(ResourcesDir::app(dir.into()));
        }
    }

    let exe = std::env::current_exe().ok();
    resolve_resources_dir(exe.as_deref(), env_override, cfg!(debug_assertions))
}

fn resolve_resources_dir(
    exe: Option<&Path>,
    env_override: Option<OsString>,
    debug: bool,
) -> std::io::Result<ResourcesDir> { ... }

fn maybe_dir(base: &Path, sub: &str, sentinel: &str) -> Option<PathBuf> { ... }
```

### Search behavior

- Release builds (`debug == false`): non-empty `ROASTTY_RESOURCES_DIR` wins
  before filesystem detection and before a failed `current_exe()` can make the
  result empty.
- Debug builds (`debug == true`): filesystem detection runs first, then a
  non-empty `ROASTTY_RESOURCES_DIR` fallback is used if detection fails. This
  mirrors upstream's stale-resources avoidance for debug app launches.
- If executable-path discovery fails and release env did not already return, the
  result is empty. Upstream returns immediately on `selfExePath` failure before
  the debug env fallback.
- Detection walks the current executable's ancestors, starting at `exe.parent()`
  (the upstream `dirname(exe)` first step). For each ancestor:
  - macOS app bundle layout:
    `{ancestor}/Contents/Resources/terminfo/78/xterm-roastty` sentinel → app
    path `{ancestor}/Contents/Resources/roastty`;
  - share layout: `{ancestor}/share/terminfo/78/xterm-roastty` sentinel → app
    path `{ancestor}/share/roastty`.
- When both app-bundle and share-layout sentinels exist under the same ancestor,
  app-bundle layout wins because upstream checks `Contents/Resources` before
  `share` on Darwin.
- `maybe_dir` ignores all filesystem access errors and returns `None`, matching
  upstream's "ignore and move on" behavior.
- `ResourcesDir::host()` returns `host_path.or(app_path)`. This experiment does
  not set `host_path`, matching upstream's current function body.

### Naming deviations

- `GHOSTTY_RESOURCES_DIR` → `ROASTTY_RESOURCES_DIR`.
- `ghostty` resource subdirectory → `roastty`.
- `xterm-ghostty` terminfo sentinel → `xterm-roastty`.
- The source notes may mention upstream names for comparison, but Roastty source
  should not contain product-facing `ghostty` strings.

## Changes

- `roastty/src/os/resources_dir.rs` — add `ResourcesDir`, `resources_dir`,
  `resolve_resources_dir`, and `maybe_dir`.
- `roastty/src/os/mod.rs` — expose the new `resources_dir` module.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — new tests cover:
  - `ResourcesDir::host()` falls back to `app()`;
  - release mode prefers a non-empty env override;
  - release mode still returns the env override when executable-path discovery
    is unavailable;
  - debug mode prefers detected resources over an env override;
  - debug mode falls back to a non-empty env override when detection misses;
  - debug mode returns empty if executable-path discovery is unavailable, even
    with an env override;
  - empty env overrides are ignored;
  - app-bundle sentinel resolves to `Contents/Resources/roastty`;
  - share-layout sentinel resolves to `share/roastty`;
  - app-bundle sentinel wins when app-bundle and share-layout sentinels both
    exist under the same ancestor;
  - missing sentinels return an empty `ResourcesDir`;
  - `maybe_dir` ignores missing/inaccessible sentinels.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = Roastty has a tested resources-directory primitive with upstream search
order and Roastty naming, ready for config/theme/i18n slices.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found one Required issue: the first draft called
`std::env::current_exe()` before checking the release-mode env override, but
upstream returns a non-empty env override before executable discovery can fail
in release. The design was changed so release mode checks
`ROASTTY_RESOURCES_DIR` first, then passes `Option<&Path>` into
`resolve_resources_dir` for testable executable-discovery failure.

The review also asked that the ancestor walk and app-bundle precedence be made
explicit. The design now starts at `exe.parent()` (upstream's first
`dirname(exe)` step), checks `Contents/Resources` before `share`, and adds tests
for release env with unavailable executable path and app-bundle-over-share
precedence.

Follow-up review approved the design with no Required findings. Codex confirmed
the Roastty naming substitutions are appropriate and the verification plan
covers the important ordering and missing-sentinel cases.

## Result

**Result:** Pass

`roastty/src/os/resources_dir.rs` now provides `ResourcesDir`,
`resources_dir()`, and a testable `resolve_resources_dir` core. The resolver
uses Roastty-facing names (`ROASTTY_RESOURCES_DIR`, `roastty`, `xterm-roastty`),
preserves upstream's release/debug ordering, starts detection at the executable
parent, checks app-bundle layout before share layout, and ignores missing or
inaccessible sentinels.

The product wrapper checks a non-empty release env override before calling
`current_exe()`, preserving the upstream case where an env override wins even if
executable-path discovery would fail. The injected resolver accepts
`Option<&Path>` so this ordering is directly covered by tests. In debug mode,
the resolver returns empty when executable discovery is unavailable, matching
upstream's immediate return before the debug env fallback.

Gates (all green):

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — **3436 passed / 0 failed** unit tests, plus **1
  passed / 0 failed** ABI harness test.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on `roastty/src/os/resources_dir.rs` and
  `roastty/src/os/mod.rs` — clean.
- `git diff --check` — clean.

## Conclusion

Roastty now has the resources-directory primitive needed by future theme,
terminfo, i18n, and app-startup slices. The port deliberately uses Roastty names
for product-facing env vars, directories, and terminfo sentinels while keeping
upstream's search order and debug/release behavior.

## Completion Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial completion review found one Required issue: debug mode with no
executable path incorrectly fell back to the env override. Upstream returns an
empty resources directory immediately when executable discovery fails, before
the debug env fallback. The resolver was fixed to track `exe_available` and only
apply the debug env fallback when executable discovery succeeded but sentinel
detection missed. The regression test
`debug_missing_exe_path_returns_empty_even_with_env_override` covers this case.

Follow-up review approved the result with no Required findings. Codex confirmed
the debug missing-executable behavior, release env-before-exe ordering, ancestor
walk start, app-bundle precedence, ignored sentinel access failures, Roastty
naming, and result documentation.
