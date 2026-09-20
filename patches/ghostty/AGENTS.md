# AGENTS.md — Ghostty patches

Inherit `patches/AGENTS.md`. Source is `forks/ghostty`; branches use
`issue-{ISSUE_ID}-exp{N}-{short-slug}`. Archive the next ordered patch, or the
series specified by `README.md`, under
`patches/ghostty/patches/issue-{ID}/`.

Never track checkout source or build output. Current release builds require Zig
0.16.x; verify `zig version` before diagnosing Ghostty build failures.

Use `ah fork status ghostty` and `ah patch check ghostty` from the
monorepo checkout; preview preparation with `ah fork prepare ghostty --dry-run`.
See `patches/AGENTS.md` for export, explicit pin updates and evidence requirements.
PWD selects local forks; linked branches never borrow ignored clones from main.
