# AGENTS.md — Ghostty patches

Inherit `patches/AGENTS.md`. Source is `forks/ghostty`; branches use
`issue-{ISSUE_ID}-exp{N}-{short-slug}`. Archive the next ordered patch, or the
series specified by `README.md`, under
`patches/ghostty/patches/issue-{ID}/`.

Never track checkout source or build output. Current release builds require Zig
0.16.x; verify `zig version` before diagnosing Ghostty build failures.
