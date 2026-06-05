+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 624: os desktop launch detection

## Description

Port the macOS-relevant parts of upstream `os/desktop.zig` into
`roastty/src/os/desktop.rs`: launch-source detection and desktop-environment
classification.

Upstream uses this helper to distinguish an app launched from Finder/`open` from
one launched from a shell. Roastty needs the same macOS signal for later app
startup, environment, shell, and config behavior. This experiment keeps the port
small and macOS-focused: implement the macOS behavior, rename the Ghostty launch
override to `ROASTTY_MAC_LAUNCH_SOURCE`, and intentionally omit Linux/BSD XDG
desktop-environment logic because Issue 801 is not adding non-macOS product
paths.

## Upstream behavior (`os/desktop.zig`)

```zig
pub fn launchedFromDesktop() bool {
    return switch (builtin.os.tag) {
        .macos => macos: {
            if (build_config.artifact == .lib) lib: {
                const env = "GHOSTTY_MAC_LAUNCH_SOURCE";
                const source = posix.getenv(env) orelse break :lib;
                if (std.mem.eql(u8, source, "app")) break :macos true;
            }

            break :macos c.getppid() == 1;
        },
        .linux, .freebsd => ...,
        .windows => false,
        .ios => true,
        else => @compileError("unsupported platform"),
    };
}

pub const DesktopEnvironment = enum {
    gnome,
    macos,
    other,
    windows,
};

pub fn desktopEnvironment() DesktopEnvironment {
    return switch (comptime builtin.os.tag) {
        .macos => .macos,
        .windows => .windows,
        .linux, .freebsd => ...,
        else => .other,
    };
}
```

## Rust mapping (`roastty/src/os/desktop.rs`)

```rust
const ENV_MAC_LAUNCH_SOURCE: &str = "ROASTTY_MAC_LAUNCH_SOURCE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopEnvironment {
    Macos,
    Other,
}

pub(crate) fn desktop_environment() -> DesktopEnvironment {
    #[cfg(target_os = "macos")]
    {
        DesktopEnvironment::Macos
    }
    #[cfg(not(target_os = "macos"))]
    {
        DesktopEnvironment::Other
    }
}

pub(crate) fn launched_from_desktop() -> bool {
    #[cfg(target_os = "macos")]
    {
        launched_from_desktop_macos_rule(
            current_parent_pid(),
            std::env::var_os(ENV_MAC_LAUNCH_SOURCE).as_deref(),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn current_parent_pid() -> libc::pid_t {
    unsafe { libc::getppid() }
}

fn launched_from_desktop_macos_rule(parent_pid: libc::pid_t, source: Option<&OsStr>) -> bool {
    if source == Some(OsStr::new("app")) {
        return true;
    }
    parent_pid == 1
}
```

### Notes / deviations

- `GHOSTTY_MAC_LAUNCH_SOURCE` becomes `ROASTTY_MAC_LAUNCH_SOURCE`. Issue 801
  explicitly rejects Ghostty compatibility names in new Roastty code.
- Upstream checks the launch-source override only for the lib artifact. Roastty
  is being built as the library foundation, and its Cargo crate exposes
  `rlib`/`cdylib`/`staticlib`, so checking the renamed override unconditionally
  inside the library helper is the direct port for this codebase.
- Only `app` forces a desktop launch. `cli`, `zig_run`, an empty value, missing
  env, or any other value falls back to parent-pid detection.
- `getppid()` is isolated behind the tiny `current_parent_pid` helper and used
  only on macOS, matching upstream's `c.getppid()` call.
- The Linux/BSD `gnome` variants and XDG env parsing are omitted rather than
  retained as dormant platform abstraction. The current Roastty product path is
  macOS-only, and non-macOS builds/tests get `DesktopEnvironment::Other` and
  `launched_from_desktop() == false`.
- The deterministic helper accepts a parent pid and optional launch-source value
  so tests do not rely on whether Cargo was itself launched by Finder, a shell,
  or a CI process.

## Changes

- `roastty/src/os/desktop.rs` — add `DesktopEnvironment`, `desktop_environment`,
  `launched_from_desktop`, and deterministic helper tests.
- `roastty/src/os/mod.rs` — expose the new `desktop` module.

## Verification

- `cargo test -p roastty os::desktop::tests` — new tests cover:
  - `app` launch source always returns true;
  - `cli`, `zig_run`, empty, unknown, and missing launch source fall back to the
    parent pid;
  - parent pid `1` returns true;
  - non-`1` parent pid returns false;
  - public `launched_from_desktop()` returns false on non-macOS hosts;
  - `desktop_environment()` returns `.Macos` on macOS and `.Other` elsewhere.
- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — full Roastty test suite stays green.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = Roastty has the macOS desktop launch signal and environment classifier
needed by later app startup and environment setup slices.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found three Required issues: `launched_from_desktop()` applied
the macOS parent-pid heuristic on non-macOS hosts; the plan referenced
`current_parent_pid()` without specifying the `unsafe { libc::getppid() }`
boundary; and verification did not cover the public non-macOS `false` behavior.

The design now cfg-gates the public helper so macOS uses the renamed
`ROASTTY_MAC_LAUNCH_SOURCE` override plus `getppid() == 1`, while non-macOS
returns `false`. The unsafe PID call is isolated behind `current_parent_pid()`,
and the test plan covers both the deterministic macOS rule and the public
non-macOS shape. Follow-up review approved the corrected design with no
remaining Required changes.

## Result

**Result:** Pass

`roastty/src/os/desktop.rs` now provides the macOS slice of upstream
`os/desktop.zig`. `desktop_environment()` reports `Macos` on macOS and `Other`
on non-macOS test/build hosts. `launched_from_desktop()` is cfg-gated so macOS
uses the renamed `ROASTTY_MAC_LAUNCH_SOURCE` override plus `getppid() == 1`,
while non-macOS returns `false`.

The deterministic `launched_from_desktop_macos_rule` helper covers the upstream
decision table without depending on the real Cargo test parent process. Only the
`app` launch source forces a desktop launch; `cli`, `zig_run`, empty, unknown,
and missing values fall back to the parent pid rule.

Gates (all green):

- `cargo test -p roastty os::desktop::tests` — **5 passed / 0 failed** focused
  tests on this macOS host. The non-macOS public-false test is cfg-gated for
  non-macOS hosts.
- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — **3449 passed / 0 failed** unit tests, plus **1
  passed / 0 failed** ABI harness test and **0** doc tests.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on `roastty/src/os/desktop.rs` and `roastty/src/os/mod.rs` —
  clean.
- `git diff --check` — clean.

## Conclusion

Roastty now has the macOS desktop launch signal needed by later app startup,
environment setup, shell, and config slices. The port keeps the platform shape
explicit: macOS gets Finder/`open` detection, and non-macOS hosts get inert test
scaffolding rather than Linux/BSD desktop behavior.

## Completion Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED — no code or documentation fixes required before the
result commit.

Codex confirmed the implementation matches the approved design:
`desktop_environment()` reports `Macos` on macOS and `Other` elsewhere;
`launched_from_desktop()` uses `ROASTTY_MAC_LAUNCH_SOURCE` plus the isolated
`getppid()` helper only on macOS; non-macOS returns `false`; and the
deterministic helper covers the upstream macOS decision table.

The only Required note was procedural: `roastty/src/os/desktop.rs` was untracked
at review time and must be explicitly staged for the result commit so the new
`os::desktop` module reference is committed with its implementation.
