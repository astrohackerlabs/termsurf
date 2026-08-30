# AGENTS.md — Chromium patches

Inherit `patches/AGENTS.md`. Source is `forks/chromium/src`; tooling is
`forks/chromium/depot_tools`. Branches use
`{version}-issue-{ISSUE_ID}-exp{N}-{short-slug}`.

The archive is cumulative from its recorded base: regenerate `base..HEAD`
under `patches/chromium/patches/issue-{ID}/` as directed by `README.md`.
Never track Chromium source, gclient state, tools, or build output. Use
`autoninja`, not raw `ninja`, for direct diagnostic builds.
