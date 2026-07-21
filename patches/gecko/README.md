# Gecko Patches (tombstone)

Gecko is **not** a shipped or active Astrohacker Terminal engine (Issue
26072121272459). It was never part of the Homebrew ship set or
`patches/release-manifest.json` product pin list. Chromium is the supported
product browser engine.

## Current State

- **Product pin:** none (do not invent a release-manifest gecko entry)
- **Live fork:** not required for release (`forks/gecko` may be deleted locally)
- **Product crate:** removed (`rust/ah-geckod` / `libtermsurf_gecko` deleted with
  Issue 26072121272459 Exp 1)
- **Historical archives:** remain under `patches/gecko/patches/` as immutable
  records of past experimental series. Do not mass-delete `issue-*` archives.

## Last experimental tip (before product scrub)

Frozen for reconstruction if a future issue reintroduces Gecko. Values from the
pre-tombstone living README tip:

| Field | Value |
| --- | --- |
| Checkout (historical) | `forks/gecko` |
| Upstream | `https://github.com/mozilla-firefox/firefox.git` (`main`) |
| Base | `0ae9827c4d7bc8b28ccbfa58324ded73b68dccf6` |
| Branch | `ffe9a294-issue-26071212001982-exp1` |
| Archive dirs | `issue-26071112000932`, `issue-26071212001982`, and others under `patches/` |

Reconstruct only under a **new** issue before any product pin or Homebrew ship.

## Do not

- Re-add a Gecko release-manifest pin without a new issue
- Ship `ah-geckod` in Homebrew packaging
- Revive `rust/ah-geckod` as a workspace member without a new issue
