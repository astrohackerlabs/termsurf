# AGENTS.md — Nushell patches

Inherit `patches/AGENTS.md`. Source is `forks/nushell`; branches use
`issue-{ISSUE_ID}-exp{N}-{short-slug}`. Follow `README.md` for next-patch
versus cumulative archive generation.

Reedline is a separate path-pinned fork under `forks/reedline`; edits there
follow its own patch workspace. Never track Nushell source or build output.

Use `ah fork status nushell` and `ah patch check nushell` from the
monorepo checkout; preview preparation with `ah fork prepare nushell --dry-run`.
See `patches/AGENTS.md` for export, explicit pin updates and evidence requirements.
PWD selects local forks; linked branches never borrow ignored clones from main.
