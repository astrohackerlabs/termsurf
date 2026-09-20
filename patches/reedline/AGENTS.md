# AGENTS.md — Reedline patches

Inherit `patches/AGENTS.md`. Source is `forks/reedline`; consumers are the
Nushell fork and `code/nutorch/rs/shell`. Branches use
`issue-{ISSUE_ID}-exp{N}-{short-slug}`.

A zero-patch tip pin is valid only when no product edit exists; consult the
current manifest. Once source is edited, archive the real delta; never create a no-op patch or track checkout
outputs.

Use `ah fork status reedline` and `ah patch check reedline` from the
monorepo checkout; preview preparation with `ah fork prepare reedline --dry-run`.
See `patches/AGENTS.md` for export, explicit pin updates and evidence requirements.
PWD selects local forks; linked branches never borrow ignored clones from main.
