# AGENTS.md — Nexus patches

Inherit `patches/AGENTS.md`. Source is `forks/nexus`; `ahnexus` consumes
`nexus-common`. Branches use `issue-{ISSUE_ID}-exp{N}-{short-slug}`.

The default is a documented tip pin with no product patch. Once source is
edited, archive the real delta; never create a no-op patch. Do not track the
checkout or its build output, and do not use iced `nexus-client` as the
TermSurf UI library.
