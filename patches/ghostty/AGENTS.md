# AGENTS.md — Ghostty patches

Inherit `patches/AGENTS.md`. Source is `forks/ghostty`; branches use
`issue-{ISSUE_ID}-exp{N}-{short-slug}`. Archive the next ordered patch, or the
series specified by `README.md`, under
`patches/ghostty/patches/issue-{ID}/`.

Never track checkout source or build output.

The required Zig is the checkout's `build.zig.zon` `minimum_zig_version`: same
major.minor, patch at least that. Do not record a version number here.
Homebrew's plain `zig` formula tracks the newest release and may not match.
`scripts/build.nu` selects the keg-only `zig@<major>.<minor>` formula, or a
PATH `zig` that matches, and stops with the `brew install` command when none
matches. Never `brew link` the keg over `zig`. For direct commands, never type
a bare `zig`. From the monorepo root, `nu scripts/lib/zig-toolchain.nu` prints
the selected path (default checkout `forks/ghostty`), for example
`cd forks/ghostty; ^(nu ../../scripts/lib/zig-toolchain.nu) build test`.
When an upstream update changes `minimum_zig_version`, install the matching
formula; no script or guidance edit is needed.

Use `ah fork status ghostty` and `ah patch check ghostty` from the
monorepo checkout; preview preparation with `ah fork prepare ghostty --dry-run`.
See `patches/AGENTS.md` for export, explicit pin updates and evidence requirements.
PWD selects local forks; linked branches never borrow ignored clones from main.
