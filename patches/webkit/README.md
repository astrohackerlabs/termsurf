# WebKit Patches (tombstone)

WebKit is **not** a shipped Astrohacker Terminal engine (Issue 26072120115614,
Exp 1). Chromium is the supported product browser engine.

## Current State

- **Product pin:** none (removed from `patches/release-manifest.json`)
- **Live fork:** not required for release (`forks/webkit` may be deleted locally)
- **Historical archives:** remain under `patches/webkit/patches/` as immutable
  records of past product series. Do not mass-delete `issue-*` archives.

## Do not

- Re-add a WebKit release-manifest pin without a new issue
- Ship `ah-webkitd` in Homebrew packaging
