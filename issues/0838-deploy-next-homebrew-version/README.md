+++
status = "open"
opened = "2026-06-22"
+++

# Issue 838: Deploy Next Homebrew Version

## Goal

Deploy the next TermSurf version to Homebrew so users can install the latest
shipped changes with:

```bash
brew tap termsurf/termsurf
brew trust termsurf/termsurf
brew install --cask termsurf
```

## Background

The next issue number is **838**. Issues 834 and 835 are already occupied by
upstream work, Issue 836 is the closed WebTUI top-controls change, and Issue 837
is the closed website revamp issue.

The current Homebrew cask is `homebrew/Casks/termsurf.rb` and currently points
at TermSurf `1.0.0`. The existing release pipeline is:

1. Build all release components with `scripts/build.sh all --release`.
2. Package and publish with `scripts/release.sh <version>`.
3. The release script uploads the tarball to GitHub Releases and updates the
   `termsurf/homebrew-termsurf` cask.

The WebTUI top-controls fix from Issue 836 is in the repo and should be included
in the next Homebrew deployment.

The target version for this deployment is **TermSurf `1.4.0`**. Local tags
already include `v1.0.1` through `v1.3.1`, so `1.4.0` is the next non-colliding
minor version for a release that adds Surfari packaging.

Surfari is a required part of this release. This Homebrew version must ship with
Surfari, not just Roamium plus the WebTUI top-controls fix. Surfari support is
present in the repo and Ghostboard can resolve Surfari via
`TERMSURF_SURFARI_PATH`, but Surfari is not currently wired into the main
release packaging path:

- `scripts/build.sh all` does not build Surfari or `libtermsurf_webkit`.
- `scripts/install.sh all` does not install Surfari.
- `scripts/release.sh` does not package Surfari or `libtermsurf_webkit`.
- `homebrew/Casks/termsurf.rb` does not install Surfari.
- Ghostboard does not currently resolve an installed Surfari path analogous to
  the installed Roamium path.

This machine has not yet built Surfari. Before publishing `1.4.0`, the issue
must bootstrap the local WebKit workspace and prove Surfari builds here:

1. Shallow clone WebKit into `webkit/src`.
2. Fetch and switch to the documented WebKit base commit
   `1452a43959523449099b2616793fd2c5b6a6487e`.
3. Apply TermSurf's WebKit patch archive from `webkit/patches/issue-756/`.
4. Install or verify the required macOS build prerequisites, including full
   Xcode and the Metal toolchain.
5. Build WebKit with `webkit/src/Tools/Scripts/build-webkit --debug`.
6. Build `surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib`.
7. Build the `surfari` Rust binary.
8. Wire Surfari into TermSurf build, install, release, Homebrew, and installed
   Ghostboard browser resolution before deployment.

## Scope

This issue covers the next Homebrew deployment. It may include:

- deploying version `1.4.0`;
- confirming the release contents;
- bootstrapping and building WebKit on this machine;
- building `libtermsurf_webkit` and `surfari`;
- wiring Surfari into build/install/release/Homebrew flows;
- verifying release builds;
- running the release script;
- verifying the generated GitHub release artifact;
- verifying the Homebrew cask update;
- installing the cask from a clean or existing Homebrew install;
- documenting any deployment blockers.

## Major Stages

- [x] **Stage 1: WebKit workspace bootstrap** — shallow clone `webkit/src`,
      switch to `1452a43959523449099b2616793fd2c5b6a6487e`, apply
      `webkit/patches/issue-756/`, and record the resulting WebKit state.
- [x] **Stage 2: WebKit build** — verify Xcode/Metal prerequisites and build
      WebKit with `webkit/src/Tools/Scripts/build-webkit --debug`.
- [ ] **Stage 3: Surfari local build** — build
      `surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib`, run its smoke
      test, and build the `surfari` Rust binary. `libtermsurf_webkit` and the
      Rust binary build, but the smoke test currently fails because focus is not
      observed.
- [ ] **Stage 4: Surfari packaging integration** — wire Surfari into
      `scripts/build.sh`, `scripts/install.sh`, `scripts/release.sh`,
      `homebrew/Casks/termsurf.rb`, and Ghostboard's installed browser
      resolution.
- [ ] **Stage 5: Full release build** — run the full `1.4.0` release build and
      verify Roamium, Surfari, WebTUI, and Ghostboard artifacts are present.
- [ ] **Stage 6: Package-only release validation** — generate the `1.4.0`
      tarball without publishing and inspect it for `TermSurf.app`, `web`,
      Roamium, Surfari, and required runtime resources.
- [ ] **Stage 7: Publish release** — publish GitHub Release `v1.4.0`, update and
      push the Homebrew cask, and record the generated SHA.
- [ ] **Stage 8: Homebrew install verification** — install or upgrade through
      Homebrew and verify WebTUI top controls plus `web --browser surfari`
      without `TERMSURF_SURFARI_PATH`.
- [ ] **Stage 9: Closeout** — record final verification evidence, update docs if
      deployment changed install instructions, close the issue, and regenerate
      `issues/README.md`.

## Experiments

- [Experiment 1: Bootstrap WebKit and Surfari](01-bootstrap-webkit-surfari.md) —
  **Partial** (smoke test fails on focus observation)

## Acceptance Criteria

- The issue records the intended next version number: `1.4.0`.
- Surfari is included in this deployment.
- WebKit is shallow-cloned, patched, and built on this machine.
- `libtermsurf_webkit` builds on this machine.
- The `surfari` Rust binary builds on this machine.
- `scripts/build.sh all --release` includes Surfari.
- Release builds complete successfully.
- The packaged tarball contains the expected installable artifacts, including
  Surfari and its required `libtermsurf_webkit` runtime dependency.
- GitHub Release upload succeeds for the chosen version.
- The Homebrew cask is updated and pushed to the Homebrew tap.
- A Homebrew install or upgrade test succeeds.
- Installed TermSurf includes the WebTUI top-controls fix.
- Installed Surfari can be launched through `web --browser surfari` without
  relying on a development-only `TERMSURF_SURFARI_PATH`.
