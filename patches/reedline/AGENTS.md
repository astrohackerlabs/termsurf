# AGENTS.md — Reedline patches

Inherit `patches/AGENTS.md`. Source is `forks/reedline`; consumers are the
Nushell fork and `code/termsurf/rs/ahsh`. Branches use
`issue-{ISSUE_ID}-exp{N}-{short-slug}`.

The default is a documented tip pin with no product patch. Once source is
edited, archive the real delta; never create a no-op patch or track checkout
outputs.
