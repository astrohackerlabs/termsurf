# Homebrew

**Canonical user install and operator release documentation** for Astrohacker
on Apple silicon macOS. Shell / direct install (`install.sh`) is **retired** as
a product channel; do not revive public bootstrap install as the primary path.

Full environment variable taxonomy: [`docs/environment.md`](./environment.md).

Astrohacker ships to macOS through the `astrohackerlabs/astrohacker` Homebrew
tap. There is **one desktop download**: the cask `astrohacker`. It installs
Astrohacker TermSurf, Shell, Web, and related helpers as one Astrohacker
bundle. The app lands in **`/Applications/Astrohacker TermSurf.app`**.

## Public command surface

Released PATH names (machine-readable for gates):

<!-- released-wrappers -->
ahterm
ahweb
ahsh
ahcalc
ah-chromiumd
ah-webkitd
ah-ladybirdd
<!-- /released-wrappers -->

Released payload roots (machine-readable for legal/notice gates; top-level
paths in the release tarball besides bare CLI binaries):

<!-- released-payload-roots -->
ahcalc
ah-chromiumd
ah-webkitd
ah-ladybirdd
<!-- /released-payload-roots -->

| Command | Role |
| --- | --- |
| `ahterm` | Astrohacker TermSurf (app executable + PATH launcher) |
| `ahsh` | Astrohacker Shell |
| `ahweb` | Open URLs / browser panes in Terminal |
| `ahcalc` | Scientific calculator TermSurf app (full-pane web UI) |

Reserved (not shipping until the product ships): `ahwallet`.

There is **no** `ah` / `astrohacker` meta CLI dispatcher today.

Engine helpers (implementation; on PATH for packaging/debug):

- `ah-chromiumd`, `ah-webkitd`, `ah-ladybirdd`

**Not released:** `ah-geckod` / gecko.

Engine **selectors** for `ahweb` remain family names: `chromium`, `webkit`,
`ladybird` (future `gecko`).

`TermSurf` remains the **protocol** name (`termsurf.proto`, `libtermsurf_*`,
`TERMSURF_*` env). It is not the product brand and is not the PATH CLI name

Historical cask token `astrohacker-terminal` is retired. Users install
`astrohacker` only. The public GitHub source and release asset host is
`astrohackerlabs/termsurf` (local default `~/dev/termsurf`). Leave the
legacy `astrohackerlabs/astrohacker-terminal` repo alone unless a separate
experiment migrates historical assets.

Astrohacker Wallet is planned for a future update of this **same** cask—not a
second formula.

## Install

```bash
brew tap astrohackerlabs/astrohacker
brew trust astrohackerlabs/astrohacker
brew install --cask astrohacker
```

Upgrade:

```bash
brew update
brew upgrade --cask astrohacker
```

## Signing model

Distribution uses ad-hoc codesign in the cask postflight (quarantine clear +
`codesign --sign -`) until Developer ID notarization is in place.
`brew trust` trusts the tap source; it does not notarize the app with Apple.

Legal files are injected into the app under `Contents/Resources/legal/` during
`scripts/release.sh` packaging (after the app is copied into the stage tree).
That changes sealed app contents relative to any earlier signature; the
**cask postflight ad-hoc re-sign is the intentional installed contract** for
those Resources until Developer ID notarization lands.

Normal install/reinstall/uninstall of Astrohacker-owned opt artifacts must not
require `sudo` (helpers are Homebrew `artifact`s).

## Installed layout

- `Astrohacker TermSurf.app` → `/Applications/Astrohacker TermSurf.app`
  (executable `Contents/MacOS/ahterm`)
- **Legal (authoritative for installed users):**
  `/Applications/Astrohacker TermSurf.app/Contents/Resources/legal/`
  (`LICENSE`, `NOTICE`, `TRADEMARKS.md`, `third_party/...`)
- PATH: `ahterm`, `ahweb`, `ahsh`, `ahcalc`, engine helpers
- Chromium / WebKit / Ladybird trees →
  `/opt/homebrew/opt/astrohacker-terminal-ah-{chromiumd,webkitd,ladybirdd}/`
- ahcalc package payload →
  `/opt/homebrew/opt/astrohacker-terminal-ahcalc/` (when installed as artifact)
  or under Caskroom stage `ahcalc/` (binary links `ahcalc/dist/ahcalc`)


## Release tarball contract

Asset name: `astrohacker-<version>-aarch64-apple-darwin.tar.gz`

Top-level contents:

- `Astrohacker TermSurf.app/` (with `Contents/MacOS/ahterm` and
  `Contents/Resources/legal/`)
- `LICENSE`, `NOTICE`, `TRADEMARKS.md` (tarball root mirror of product legal)
- `legal/third_party/` (Chromium credits/LICENSE, Ladybird LICENSE + vcpkg
  copyrights, Nushell/Reedline LICENSE copies)
- `ahweb`, `ahsh`
- `ahcalc/` (payload: `dist/ahcalc`, `dist/browser/`, `public/`)
- `ah-chromiumd/`, `ah-webkitd/`, `ah-ladybirdd/`

Gate before publish: `scripts/check-release-legal-notices.sh` (NOTICE
legal-manifest vs released wrappers + payload roots).

## Release / publish (agents and humans)

Canonical three-repository Homebrew release flow. Packaging scripts live in the
**private** monorepo; they are not synced to the public source tree.

### Topology

| Role | Local default | GitHub |
| --- | --- | --- |
| Private monorepo | this repo | private business monorepo |
| Public TermSurf source | `~/dev/termsurf` | `astrohackerlabs/termsurf` |
| Homebrew tap | `~/dev/homebrew-astrohacker` | `astrohackerlabs/homebrew-astrohacker` |

Cask file: `~/dev/homebrew-astrohacker/Casks/astrohacker.rb`

Env overrides: `ASTROHACKER_TERMINAL_PUBLIC_REPO`,
`ASTROHACKER_TERMINAL_PUBLIC_GITHUB_REPO`,
`ASTROHACKER_TERMINAL_HOMEBREW_TAP_REPO` (legacy `TERMSURF_*` aliases still
accepted by scripts).

### Scripts

| Script | Role |
| --- | --- |
| `scripts/release-homebrew.py` | Canonical fork verification, incremental release build, package, publish, and local cask installation transaction |
| `scripts/lib/release_forks.py` | Enforce `patches/release-manifest.json` and apply a missing exact cumulative series only from its recorded base |
| `scripts/build.sh` | Build components / `all --release` |
| `scripts/release.sh` | Lower-level package/publish helper used by the canonical command |
| `scripts/sync-public-source.sh` | Sync allowlisted paths into public checkout |

### Canonical release command

The human release operator runs one command from the clean private monorepo:

```sh
scripts/release-homebrew.py
```

With no option it selects the next patch version after the greatest strict
version found across public releases, public tags, and the remote cask. To
select a higher unused version:

```sh
scripts/release-homebrew.py --version 0.2.0
```

#### Build cache preservation

“Clean private monorepo” means a clean Git worktree, not an empty build
directory. Release builds are incremental by default. `--release` selects
release-mode artifacts and must never imply `--clean`; normal dependency
tracking rebuilds only the inputs that changed.

Preserve valid outputs and caches for every shipped project, especially:

- Chromium `forks/chromium/src/out/Default`;
- WebKit `forks/webkit/src/WebKitBuild`;
- Ladybird `forks/ladybird/Build`;
- Ghostty Zig and Xcode outputs under `forks/ghostty`;
- Rust/Cargo target directories; and

Never remove these for a routine release. A clean is allowed only after a stale
or corrupt artifact is diagnosed, must be scoped to that component, and
requires an explicit rebuild-cost warning and user approval before deletion.
The approval to destroy build state is separate from the operator's publication
confirmation.

If an external step fails after packaging, the command records the exact
version, archive, and digest under ignored `dist/` state. Running the command
again resumes those same bytes; `--resume X.Y.Z` may state the expected saved
version explicitly. It never invents another recovery version.

Before confirmation the command performs read-only version, repository, tool,
and credential discovery. After the operator types the exact confirmation, it:

1. sets first-party product Cargo package versions to the selected release
   version (`ahsh`, `ahweb`, `ah-chromiumd`, `ah-webkitd`,
   `ah-ladybirdd` only), refreshes their `Cargo.lock` files, commits that bump
   on private `main` when needed, and pushes it so the monorepo stays aligned
   with `origin/main`. This step never rewrites anything under `forks/`
   (fork trees are out of scope; `ahterm` still gets the
   release stamp from `ASTROHACKER_VERSION` / `TERMSURF_VERSION`);
2. proves or reconstructs all released fork inputs from the tracked cumulative
   patch manifest (Ghostty, Nushell, Reedline, Chromium, WebKit, and
   Ladybird; Gecko is excluded; editor fork excluded);
3. incrementally builds every shipped component in release mode with one
   version while preserving valid build outputs and caches;
4. packages one archive and freezes its SHA-256;
5. syncs and pushes the allowlisted public source when it changed;
6. creates or safely resumes the matching tag, GitHub release asset, and cask
   without deleting or overwriting a conflict;
7. refreshes Homebrew and installs or reinstalls the published cask; and
8. prints the exact release identities and asks the operator to test the app
   manually.

The release command runs no product tests, smokes, browser checks, screenshots,
or UI automation. Publication and product qualification are separate. Agents
may implement or review the command, but the human operator owns its publishing
confirmation and the resulting app acceptance.

### Lower-level helpers

Flags for `scripts/release.sh <version>`:

- Package only (default if publish unset):
  `ASTROHACKER_TERMINAL_RELEASE_PACKAGE_ONLY=1`
  or simply omit `ASTROHACKER_TERMINAL_RELEASE_PUBLISH=1`
- Publish:
  `ASTROHACKER_TERMINAL_RELEASE_PUBLISH=1`
- Publish an archive already created by package-only mode, without restaging or
  retarring it:
  `ASTROHACKER_TERMINAL_RELEASE_USE_EXISTING_PACKAGE=1` plus
  `ASTROHACKER_TERMINAL_RELEASE_EXPECTED_SHA256=<sha256>`
- The canonical command sets
  `ASTROHACKER_TERMINAL_RELEASE_SKIP_PRODUCT_QUALIFICATION=1`; lower-level
  packaging then omits executable version/help checks and Ladybird
  resource-root smokes while retaining artifact-presence, dependency,
  topology, and legal-integrity assertions needed to construct the archive.

Publish mode requires **clean** public and tap worktrees. It only rewrites cask
`version` and `sha256`. Commit any binary/postflight content changes on the tap
**before** publish mode.

### Lower-level manual flow

The following describes the components orchestrated by
`scripts/release-homebrew.py`. It is recovery/reference material, not the
normal operator interface.

1. **Preflight version** (remote-facing):

   ```sh
   gh release list --repo astrohackerlabs/termsurf --limit 5
   git -C ~/dev/termsurf ls-remote origin 'refs/heads/main' 'refs/tags/v*'
   git -C ~/dev/homebrew-astrohacker fetch origin
   git -C ~/dev/homebrew-astrohacker show origin/main:Casks/astrohacker.rb | grep -E 'version |sha256 '
   ```

   Choose next version from max(public release, tag, remote cask).

2. **Land product changes** in private monorepo; push tap **content** changes
   (not version/sha) if needed so the tap is clean for publish.

3. **Full release build** (`scripts/build.sh all` ships Terminal components
   only — no editor; preserve all valid incremental build outputs):

   ```sh
   TERMSURF_VERSION=<version> \
   ASTROHACKER_VERSION=<version> \
     scripts/build.sh all --release
   ```

   Version contract:

   - First-party product crate package versions under the monorepo root  track the
     Homebrew release version. The canonical command rewrites and commits those
     manifests before building so `CARGO_PKG_VERSION` matches the cask. It also
     rewrites `bun/ahcalc/package.json` `"version"` and
     `bun/ahcalc/app/cli/embedded-version.ts` to the same X.Y.Z so the ahcalc
     compile-time stamp matches the cask and the post-build tree stays clean.
     Do not leave those packages stuck at a placeholder such as `0.1.0` across
     releases. Do not rewrite package versions under `forks/`.
   - `TERMSURF_VERSION=<version>` is the `ahterm` app/helper version input.
     `ahterm` is the only shipped wrapper that uses the terminal helper/action
     convention: `ahterm +version` and `ahterm +help`.
   - `ASTROHACKER_VERSION=<version>` is the release version input for Rust
     product/helper binaries (first-party crate versions are also aligned to
     the same release).
   - Every shipped non-`ahterm` wrapper must support `--version` and `--help`.
     The first `--version` line must use the same `<version>`:

     | Wrapper | Expected first line |
     | --- | --- |
     | `ahweb --version` | `Astrohacker Web <version>` |
     | `ahsh --version` | `Astrohacker Shell <version>` |
     | `ahcalc --version` | `Astrohacker Calc <version>` |
     | `ah-chromiumd --version` | `Astrohacker Chromium Engine <version>` |
     | `ah-webkitd --version` | `Astrohacker WebKit Engine <version>` |
     | `ah-ladybirdd --version` | `Astrohacker Ladybird Engine <version>` |

     Runtime/component versions, such as Nushell or browser ABI versions, may be
     shown only as secondary detail after the product release line.

4. **Package-only**:

   ```sh
   ASTROHACKER_TERMINAL_RELEASE_PACKAGE_ONLY=1 \
   ASTROHACKER_TERMINAL_RELEASE_PUBLISH=0 \
     scripts/release.sh <version>
   ```

   Inspect `dist/release` and
   `dist/astrohacker-<version>-aarch64-apple-darwin.tar.gz`.

5. **Public source sync** (private monorepo → public checkout), then commit on
   public `main` so the tree is clean:

   ```sh
   scripts/sync-public-source.sh
   # commit in ~/dev/termsurf
   ```

6. **Publish** (a direct helper invocation repackages; the canonical command
   instead uses the exact existing-package mode documented above):

   ```sh
   ASTROHACKER_TERMINAL_RELEASE_PUBLISH=1 scripts/release.sh <version>
   ```

   Creates/pushes `v<version>`, GitHub release asset, tap commit `v<version>`
   with authoritative SHA256.

### Product qualification (separate)

The following historical/active harnesses may be useful in an issue whose goal
is product qualification. They are not publication gates and are never invoked
by `scripts/release-homebrew.py`:

| Script | Role |
| --- | --- |
| `scripts/test-issue-26062812000869-installed-homebrew-browser-smoke.sh` | installed three-engine browser smoke |
| `scripts/test-issue-26070112000882-installed-cold-start.sh` | cold-start + warmup |
| `scripts/test-issue-26062812000867-release-no-env-browser-discovery.sh` | useful discovery check |
| Older Surfari-named 871/872 harnesses | historical; not current gates until updated |

Example:

```sh
ASTROHACKER_TERMINAL_SMOKE_VERSION=<version> \
  scripts/test-issue-26062812000869-installed-homebrew-browser-smoke.sh
```

### Traps

- Dirty tap or public repo aborts publish mode.
- The canonical command packages once and requires the identical archive SHA in
  publish-existing mode. A direct lower-level publish may repackage instead.
- Partial publish: inspect tag/asset/tap; rerun same version; do not invent a
  new version just to recover.
  incremental dependency graph. Do not use `--clean` for a routine release.
- Do not revive cask token `astrohacker-terminal`.

### Agent checklist

1. Keep the private monorepo, public source, and tap clean and pushed.
2. Keep `patches/release-manifest.json` current whenever a released fork patch
   series changes.
3. Do not invoke the publishing command; hand it to the human release operator.
4. Do not add product qualification to the release transaction.
5. Preserve valid build outputs and caches; never turn release mode into an
   implicit clean build.

## Installed smoke expectations

After install, from inside Astrohacker TermSurf:

- `ahweb --browser chromium https://example.com`
- `ahweb --browser webkit https://example.com`
- `ahweb --browser ladybird http://127.0.0.1:<fixture>/`

Helpers resolve under `/opt/homebrew/opt/astrohacker-terminal-ah-*` without
browser path env overrides.

Ladybird is a prototype packaging surface, not production browser parity.

## Engine path environment variables

Primary product overrides (preferred):

- `ASTROHACKER_CHROMIUM_PATH`
- `ASTROHACKER_WEBKIT_PATH`
- `ASTROHACKER_LADYBIRD_PATH`

Legacy dual-read aliases (deprecated; still accepted):

- `TERMSURF_ROAMIUM_PATH` / `TERMSURF_INSTALLED_ROAMIUM_PATH`
- `TERMSURF_SURFARI_PATH` / `TERMSURF_INSTALLED_SURFARI_PATH`
- `TERMSURF_GIRLBAT_PATH` / `TERMSURF_INSTALLED_GIRLBAT_PATH`

Values must be nonempty absolute paths. Protocol vars such as `TERMSURF_SOCKET`
and `TERMSURF_PANE_ID` are unchanged.
