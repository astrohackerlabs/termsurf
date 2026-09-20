# AGENTS.md — Nexus patches

Inherit `patches/AGENTS.md`. Source is `forks/nexus`; `ahnexus` consumes
`nexus-common`. Branches use `issue-{ISSUE_ID}-exp{N}-{short-slug}`.

A zero-patch tip pin is valid only when no product edit exists; consult the
current manifest. Once source is edited, archive the real delta; never create a no-op patch. Do not track the
checkout or its build output, and do not use iced `nexus-client` as the
TermSurf UI library.

Use `ah fork status nexus` and `ah patch check nexus` from the
monorepo checkout; preview preparation with `ah fork prepare nexus --dry-run`.
See `patches/AGENTS.md` for export, explicit pin updates and evidence requirements.
PWD selects local forks; linked branches never borrow ignored clones from main.
